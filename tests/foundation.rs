mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::{EXTENSION_ORIGIN, TestApp};
use liotek::db::ReadOnlyDb;

#[tokio::test]
async fn health_reports_build_and_schema_identity() {
    let app = TestApp::start().await;

    let health = app.get("/api/health").await.data();

    assert_eq!(health["schema_version"], liotek::db::SUPPORTED_SCHEMA_VERSION);
    assert_eq!(health["environment"], "development");
    assert!(health["revision"].is_string());
}

#[tokio::test]
async fn unknown_api_routes_fail_in_the_error_envelope() {
    let app = TestApp::start().await;

    let response = app.get("/api/no-such-thing").await;

    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert_eq!(response.error_code(), "not_found");
}

#[tokio::test]
async fn cors_admits_only_configured_extension_origins() {
    let app = TestApp::start().await;
    let from = |origin: &'static str| {
        Request::get("/api/health")
            .header(header::ORIGIN, origin)
            .body(Body::empty())
            .unwrap()
    };

    let allowed = app.send(from(EXTENSION_ORIGIN)).await;
    let refused = app.send(from("https://evil.example")).await;

    assert_eq!(
        allowed.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        EXTENSION_ORIGIN
    );
    assert!(!refused.headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
}

#[tokio::test]
async fn export_is_one_restorable_file_without_operational_state() {
    let app = TestApp::start().await;
    let tx = app.state.db.begin().await.unwrap();
    tx.execute(
        "INSERT INTO documents (kind, title) VALUES ('pdf', 'Our Sister Killjoy')",
        (),
    )
    .await
    .unwrap();
    tx.execute(
        "INSERT INTO import_runs (source, stats_json) VALUES ('light:aaa', '{}')",
        (),
    )
    .await
    .unwrap();
    tx.execute(
        "INSERT INTO upload_sessions (id, status, provider_upload_id, object_key, original_filename,
                                      file_size, part_size, expires_at)
         VALUES ('u1', 'uploading', 'p1', 'local/u1.pdf', 'u1.pdf', 10, 5, '2099-01-01T00:00:00Z')",
        (),
    )
    .await
    .unwrap();
    tx.execute(
        "INSERT INTO upload_parts (upload_session_id, part_number, size, etag) VALUES ('u1', 1, 5, 'e')",
        (),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let output = app.dir.path().join("library-export.sqlite");

    let summary = liotek::export::export(&app.state.db, &output).await.unwrap();

    assert_eq!(summary.counts.documents, 1);
    for companion in ["sqlite-wal", "sqlite-shm", "sqlite-journal"] {
        assert!(
            !output.with_extension(companion).exists(),
            "the export has no {companion} companion"
        );
    }

    let exported = ReadOnlyDb::open(&output).await.unwrap();
    assert_eq!(
        exported.schema_status().await.unwrap().pending,
        Vec::<i64>::new(),
        "the export is a liotek database at this build's schema"
    );
    assert_eq!(exported.integrity_problems().await.unwrap(), Vec::<String>::new());
    for table in ["upload_parts", "upload_sessions", "import_runs"] {
        let mut rows = exported
            .query(&format!("SELECT count(*) FROM {table}"), ())
            .await
            .unwrap_or_else(|error| panic!("{table} is kept: {error}"));
        let rows = rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap();
        assert_eq!(rows, 0, "{table} is emptied");
    }

    let live = app.state.db.read().await.unwrap();
    let mut sessions = live
        .query("SELECT count(*) FROM upload_sessions", ())
        .await
        .unwrap();
    assert_eq!(
        sessions.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        1,
        "the library itself keeps its operational state"
    );

    let again = liotek::export::export(&app.state.db, &output).await;
    assert!(again.is_err(), "an existing export is never overwritten");
}

#[tokio::test]
async fn writes_without_the_admin_token_are_refused() {
    let app = TestApp::start().await;

    for (method, path) in [
        (Method::POST, "/api/uploads"),
        (Method::PATCH, "/api/documents/1"),
        (Method::POST, "/api/imports/research"),
        (Method::POST, "/api/backups"),
    ] {
        let response = app.anonymous(method.clone(), path, None).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert_eq!(response.error_code(), "admin_token_required");
    }
}
