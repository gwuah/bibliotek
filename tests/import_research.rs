mod common;
#[path = "common/fixtures.rs"]
mod fixtures;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use common::{ADMIN_TOKEN, TestApp};
use fixtures::{ResearchDb, count, create_database, execute};
use liotek::http::{self, AppState};

fn router_reading(app: &TestApp, path: Option<PathBuf>) -> Router {
    let mut config = (*app.state.config).clone();
    config.research_database = path;
    http::router(AppState {
        config: Arc::new(config),
        ..app.state.clone()
    })
}

async fn import(router: &Router, query: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/imports/research{query}"))
        .header(header::AUTHORIZATION, format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn imported(router: &Router) -> Value {
    let (status, body) = import(router, "").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

async fn refused(router: &Router, code: &str) {
    let (status, body) = import(router, "").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], code);
}

async fn library_count(app: &TestApp, sql: &str) -> i64 {
    count(&app.state.db.read().await.unwrap(), sql).await
}

async fn library_state(app: &TestApp) -> Vec<i64> {
    let mut state = Vec::new();
    for table in ["documents", "document_sources", "highlights", "comments", "notes"] {
        state.push(library_count(app, &format!("SELECT count(*) FROM {table}")).await);
    }
    state
}

async fn research(app: &TestApp) -> ResearchDb {
    let db = ResearchDb::create(app.dir.path().join("research.sqlite")).await;
    db.item("item-1", "Decolonising the Mind").await;
    db.item("item-2", "Some Paper").await;
    db.annotation("ann-1", "item-1", "Language is a carrier of culture", 3)
        .await;
    db.annotation("ann-2", "item-1", "The bullet was the means", 12)
        .await;
    db.comment("com-1", "ann-1", "on language").await;
    db.note("note-1", "item-1", "A long note").await;
    db
}

#[tokio::test]
async fn first_import_creates_documents_highlights_comments_and_notes() {
    let app = TestApp::start().await;
    let db = research(&app).await;
    let router = router_reading(&app, Some(db.path.clone()));

    let stats = imported(&router).await;

    assert_eq!(stats["source"], "research");
    assert_eq!(stats["documents"]["created"], 2);
    assert_eq!(stats["highlights"]["created"], 2);
    assert_eq!(stats["comments"]["created"], 1);
    assert_eq!(stats["notes"]["created"], 1);
    let location: Value = serde_json::from_str(
        &app.state
            .db
            .read()
            .await
            .unwrap()
            .query(
                "SELECT location_json FROM highlights WHERE external_id = 'ann-2'",
                (),
            )
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .unwrap()
            .get::<String>(0)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(location["version"], 1);
    assert_eq!(location["kind"], "pdf_region");
    assert_eq!(location["payload"]["page_number"], 12);
    assert_eq!(location["payload"]["position"]["boundingRect"]["pageNumber"], 12);
}

#[tokio::test]
async fn repeating_an_import_changes_nothing() {
    let app = TestApp::start().await;
    let db = research(&app).await;
    let router = router_reading(&app, Some(db.path.clone()));
    imported(&router).await;
    let before = library_state(&app).await;

    let again = imported(&router).await;

    assert_eq!(again["documents"]["unchanged"], 2);
    assert_eq!(again["highlights"]["unchanged"], 2);
    assert_eq!(again["comments"]["unchanged"], 1);
    assert_eq!(again["notes"]["unchanged"], 1);
    for kind in ["highlights", "comments", "notes"] {
        assert_eq!(again[kind]["created"], 0);
        assert_eq!(again[kind]["updated"], 0);
        assert_eq!(again[kind]["deleted"], 0);
    }
    assert_eq!(library_state(&app).await, before);
}

#[tokio::test]
async fn edits_in_research_update_rows_in_place_but_never_retitle_a_document() {
    let app = TestApp::start().await;
    let db = research(&app).await;
    let router = router_reading(&app, Some(db.path.clone()));
    imported(&router).await;

    db.run("UPDATE annotations SET color = '#00ff00' WHERE id = 'ann-1'")
        .await;
    db.run("UPDATE comments SET content = 'on language, revised' WHERE id = 'com-1'")
        .await;
    db.run("UPDATE notes SET content = 'A longer note' WHERE id = 'note-1'")
        .await;
    db.run("UPDATE items SET title = 'Renamed in Research' WHERE id = 'item-1'")
        .await;
    let stats = imported(&router).await;

    assert_eq!(stats["highlights"]["updated"], 1);
    assert_eq!(stats["comments"]["updated"], 1);
    assert_eq!(stats["notes"]["updated"], 1);
    assert_eq!(
        library_count(
            &app,
            "SELECT count(*) FROM documents WHERE title = 'Decolonising the Mind'"
        )
        .await,
        1,
        "the title belongs to the library once imported"
    );
    assert_eq!(library_count(&app, "SELECT count(*) FROM highlights").await, 2);
}

#[tokio::test]
async fn removals_in_research_are_removed_here_and_can_reappear() {
    let app = TestApp::start().await;
    let db = research(&app).await;
    let router = router_reading(&app, Some(db.path.clone()));
    imported(&router).await;

    db.run("DELETE FROM comments WHERE id = 'com-1'").await;
    db.run("DELETE FROM annotations WHERE id = 'ann-1'").await;
    db.run("DELETE FROM notes").await;
    db.run("UPDATE items SET deleted_at = '2025-06-01T00:00:00Z' WHERE id = 'item-2'")
        .await;
    let removed = imported(&router).await;

    assert_eq!(removed["highlights"]["deleted"], 1);
    assert_eq!(removed["comments"]["deleted"], 1);
    assert_eq!(removed["notes"]["deleted"], 1);
    assert_eq!(
        removed["documents"]["deleted"], 1,
        "the soft-deleted item, which held nothing local"
    );
    assert_eq!(library_count(&app, "SELECT count(*) FROM documents").await, 1);

    db.annotation("ann-1", "item-1", "Language is a carrier of culture", 3)
        .await;
    db.run("UPDATE items SET deleted_at = NULL WHERE id = 'item-2'")
        .await;
    let reappeared = imported(&router).await;
    assert_eq!(reappeared["highlights"]["created"], 1);
    assert_eq!(reappeared["documents"]["created"], 1);
}

#[tokio::test]
async fn a_missing_or_unconfigured_database_is_refused() {
    let app = TestApp::start().await;

    refused(&router_reading(&app, None), "research_not_configured").await;
    refused(
        &router_reading(&app, Some(app.dir.path().join("nowhere.sqlite"))),
        "research_database_missing",
    )
    .await;
}

#[tokio::test]
async fn a_failed_read_changes_nothing() {
    let app = TestApp::start().await;
    let db = research(&app).await;
    let router = router_reading(&app, Some(db.path.clone()));
    imported(&router).await;
    let before = library_state(&app).await;

    let moved = app.dir.path().join("moved.sqlite");
    std::fs::rename(&db.path, &moved).unwrap();
    refused(&router, "research_database_missing").await;
    std::fs::rename(&moved, &db.path).unwrap();

    db.comment("com-orphan", "ann-missing", "dangling").await;
    refused(&router, "research_missing_parent").await;
    db.run("DELETE FROM comments WHERE id = 'com-orphan'").await;

    db.run("UPDATE annotations SET position = '{not json' WHERE id = 'ann-2'")
        .await;
    let (status, body) = import(&router, "").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "research_malformed");
    db.run(&format!(
        "UPDATE annotations SET position = '{}' WHERE id = 'ann-2'",
        fixtures::research_position(12)
    ))
    .await;

    db.run("DELETE FROM comments").await;
    db.run("DELETE FROM annotations").await;
    db.run("DELETE FROM notes").await;
    refused(&router, "empty_snapshot").await;

    assert_eq!(library_state(&app).await, before);
    let (status, body) = import(&router, "?confirm_empty=true").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["highlights"]["deleted"], 2);
}

#[tokio::test]
async fn an_incompatible_schema_is_refused_and_changes_nothing() {
    let app = TestApp::start().await;
    let db = research(&app).await;
    let router = router_reading(&app, Some(db.path.clone()));
    imported(&router).await;
    let before = library_state(&app).await;

    let other = app.dir.path().join("other.sqlite");
    let conn = create_database(&other).await;
    execute(&conn, "CREATE TABLE items (id TEXT PRIMARY KEY, name TEXT)", ()).await;
    refused(&router_reading(&app, Some(other)), "research_schema_incompatible").await;

    let not_sqlite = app.dir.path().join("not.sqlite");
    std::fs::write(&not_sqlite, b"this is not a database").unwrap();
    refused(
        &router_reading(&app, Some(not_sqlite)),
        "research_database_unreadable",
    )
    .await;

    assert_eq!(library_state(&app).await, before);
}

#[tokio::test]
async fn local_notes_and_metadata_keep_a_document_research_removed() {
    let app = TestApp::start().await;
    let db = research(&app).await;
    let router = router_reading(&app, Some(db.path.clone()));
    imported(&router).await;
    let tx = app.state.db.begin().await.unwrap();
    execute(
        &tx,
        "INSERT INTO notes (document_id, body)
         SELECT document_id, 'my own note' FROM document_sources WHERE external_id = 'item-1'",
        (),
    )
    .await;
    execute(
        &tx,
        "UPDATE documents SET description = 'kept' WHERE id = (SELECT document_id FROM document_sources WHERE external_id = 'item-2')",
        (),
    )
    .await;
    tx.commit().await.unwrap();

    db.run("UPDATE items SET deleted_at = '2025-06-01T00:00:00Z'")
        .await;
    let (status, body) = import(&router, "?confirm_empty=true").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["documents"]["unlinked"], 2);
    assert_eq!(body["data"]["documents"]["deleted"], 0);
    assert_eq!(library_count(&app, "SELECT count(*) FROM documents").await, 2);
    assert_eq!(
        library_count(&app, "SELECT count(*) FROM notes WHERE body = 'my own note'").await,
        1
    );
    assert_eq!(library_count(&app, "SELECT count(*) FROM highlights").await, 0);
}
