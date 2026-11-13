//! Sessions use a 4-byte part size, so the 13-byte "PDF" has four parts (4 + 4 + 4 + 1).

mod common;
#[path = "common/flaky_store.rs"]
mod flaky_store;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use libsql::params::IntoParams;
use serde_json::{Value, json};

use common::{ADMIN_TOKEN, TestApp, TestResponse};
use flaky_store::{Failure, FlakyStore, Operation};
use liotek::blob::filesystem::FilesystemStore;
use liotek::blob::{BlobAccess, BlobError, Multipart, ObjectKey};
use liotek::domain::uploads::{self, NewUpload, UploadMetadata};

const FILE: &[u8] = b"%PDF-1.4 tiny";
const PART_SIZE: u64 = 4;

fn part(n: usize) -> &'static [u8] {
    let start = (n - 1) * PART_SIZE as usize;
    &FILE[start..(start + PART_SIZE as usize).min(FILE.len())]
}

fn new_upload(resume_key: &str) -> NewUpload {
    NewUpload {
        filename: "Our Sister Killjoy.pdf".into(),
        size: FILE.len() as u64,
        resume_key: Some(resume_key.into()),
        metadata: UploadMetadata {
            title: None,
            description: Some("Reflections from a black-eyed squint".into()),
            authors: vec!["Ama Ata Aidoo".into()],
            subjects: vec!["Literature".into()],
            tags: vec!["novel".into(), "ghana".into()],
        },
    }
}

async fn start_upload(app: &TestApp, resume_key: &str) -> String {
    let upload = uploads::create(
        &app.state.db,
        app.state.blobs.as_ref(),
        &new_upload(resume_key),
        PART_SIZE,
    )
    .await
    .unwrap();
    upload.id
}

async fn put_part(app: &TestApp, id: &str, part_number: &str, bytes: &[u8]) -> TestResponse {
    app.send(
        Request::put(format!("/api/uploads/{id}/parts/{part_number}"))
            .header(header::AUTHORIZATION, format!("Bearer {ADMIN_TOKEN}"))
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(bytes.to_vec()))
            .unwrap(),
    )
    .await
}

async fn put_parts(app: &TestApp, id: &str, part_numbers: &[usize]) {
    for &n in part_numbers {
        put_part(app, id, &n.to_string(), part(n)).await.data();
    }
}

async fn complete(app: &TestApp, id: &str) -> TestResponse {
    app.admin(Method::POST, &format!("/api/uploads/{id}/complete"), None)
        .await
}

async fn abort(app: &TestApp, id: &str) -> TestResponse {
    app.admin(Method::DELETE, &format!("/api/uploads/{id}"), None)
        .await
}

async fn show(app: &TestApp, id: &str) -> Value {
    app.get(&format!("/api/uploads/{id}")).await.data()
}

async fn start_with_flaky_store() -> (TestApp, Arc<FlakyStore>) {
    let dir = tempfile::tempdir().unwrap();
    let filesystem = FilesystemStore::open(&dir.path().join("blobs")).await.unwrap();
    let flaky = Arc::new(FlakyStore::wrap(Arc::new(filesystem)));
    let app = TestApp::start_in(dir, flaky.clone()).await;
    (app, flaky)
}

async fn query_strings(app: &TestApp, sql: &str, id: impl Into<libsql::Value>) -> Vec<String> {
    let conn = app.state.db.read().await.unwrap();
    let mut rows = conn.query(sql, [id.into()]).await.unwrap();
    let mut values = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        values.push(row.get::<String>(0).unwrap());
    }
    values
}

async fn count(app: &TestApp, sql: &str) -> i64 {
    let conn = app.state.db.read().await.unwrap();
    let mut rows = conn.query(sql, ()).await.unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

async fn execute(app: &TestApp, sql: &str, params: impl IntoParams) {
    let tx = app.state.db.begin().await.unwrap();
    tx.execute(sql, params).await.unwrap();
    tx.commit().await.unwrap();
}

async fn force_status(app: &TestApp, id: &str, status: &str) {
    execute(
        app,
        "UPDATE upload_sessions SET status = ? WHERE id = ?",
        [status, id],
    )
    .await;
}

async fn make_overdue(app: &TestApp, id: &str) {
    execute(
        app,
        "UPDATE upload_sessions SET expires_at = '2000-01-01T00:00:00.000Z' WHERE id = ?",
        [id],
    )
    .await;
}

async fn object_key(app: &TestApp, id: &str) -> ObjectKey {
    let keys = query_strings(app, "SELECT object_key FROM upload_sessions WHERE id = ?", id).await;
    ObjectKey::parse(keys[0].clone()).unwrap()
}

async fn provider_upload_id(app: &TestApp, id: &str) -> String {
    query_strings(
        app,
        "SELECT provider_upload_id FROM upload_sessions WHERE id = ?",
        id,
    )
    .await
    .remove(0)
}

async fn assemble_behind_the_servers_back(app: &TestApp, id: &str) {
    let key = object_key(app, id).await;
    let upload_id = provider_upload_id(app, id).await;
    let upload = Multipart {
        key: &key,
        upload_id: &upload_id,
    };
    let parts = app.state.blobs.list_parts(upload).await.unwrap();
    app.state.blobs.complete_multipart(upload, &parts).await.unwrap();
}

async fn stored_bytes(app: &TestApp, key: &ObjectKey) -> Vec<u8> {
    match app
        .state
        .blobs
        .access(key, Duration::from_secs(60))
        .await
        .unwrap()
    {
        BlobAccess::LocalFile(path) => std::fs::read(path).unwrap(),
        BlobAccess::Redirect(_) => panic!("the filesystem store serves local files"),
    }
}

async fn assert_one_document_for(app: &TestApp, upload: &Value) -> i64 {
    assert_eq!(upload["status"], "committed");
    let document_id = upload["document_id"]
        .as_i64()
        .expect("a committed upload names its document");
    assert_eq!(count(app, "SELECT count(*) FROM documents").await, 1);

    let sources = query_strings(
        app,
        "SELECT source || ':' || external_id FROM document_sources WHERE document_id = ?",
        document_id,
    )
    .await;
    assert_eq!(
        sources,
        vec![format!("upload:{}", upload["id"].as_str().unwrap())]
    );

    let keys = query_strings(app, "SELECT object_key FROM documents WHERE id = ?", document_id).await;
    let key = ObjectKey::parse(keys[0].clone()).unwrap();
    assert_eq!(
        stored_bytes(app, &key).await,
        FILE,
        "the object holds the whole file, in order"
    );
    document_id
}

#[tokio::test]
async fn a_complete_upload_becomes_a_document_with_its_metadata_and_source() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;

    put_parts(&app, &id, &[3, 1, 4, 2]).await;
    let upload = complete(&app, &id).await.data();

    let document_id = assert_one_document_for(&app, &upload).await;
    assert_eq!(
        query_strings(
            &app,
            "SELECT title || '|' || original_filename || '|' || kind || '|' || description
             FROM documents WHERE id = ?",
            document_id
        )
        .await,
        vec!["Our Sister Killjoy|Our Sister Killjoy.pdf|pdf|Reflections from a black-eyed squint"],
        "the title falls back to the filename without its extension"
    );
    assert_eq!(
        query_strings(
            &app,
            "SELECT a.name FROM document_authors da JOIN authors a ON a.id = da.author_id
             WHERE da.document_id = ? ORDER BY da.position",
            document_id
        )
        .await,
        vec!["Ama Ata Aidoo"]
    );
    assert_eq!(
        query_strings(
            &app,
            "SELECT l.kind || ':' || l.name FROM document_labels dl JOIN labels l ON l.id = dl.label_id
             WHERE dl.document_id = ? ORDER BY l.kind, l.name",
            document_id
        )
        .await,
        vec!["subject:Literature", "tag:ghana", "tag:novel"]
    );

    let key = ObjectKey::parse(
        query_strings(&app, "SELECT object_key FROM documents WHERE id = ?", document_id)
            .await
            .remove(0),
    )
    .unwrap();
    assert!(
        key.as_str().starts_with(app.state.blobs.namespace()) && !key.as_str().contains("Killjoy"),
        "the server chose the key; the filename is only metadata: {key}"
    );
}

#[tokio::test]
async fn creating_an_upload_twice_with_one_resume_key_returns_the_same_session() {
    let app = TestApp::start().await;
    let body = json!({
        "filename": "killjoy.pdf",
        "size": 13,
        "resume_key": "killjoy.pdf:13:1700000000",
        "metadata": {"authors": ["Ama Ata Aidoo"]}
    });

    let first = app
        .admin(Method::POST, "/api/uploads", Some(body.clone()))
        .await
        .data();
    let again = app
        .admin(Method::POST, "/api/uploads", Some(body.clone()))
        .await
        .data();
    let other_size = app
        .admin(
            Method::POST,
            "/api/uploads",
            Some(json!({"filename": "killjoy.pdf", "size": 14, "resume_key": "killjoy.pdf:13:1700000000"})),
        )
        .await
        .data();

    assert_eq!(first["status"], "created");
    assert_eq!(first["part_size"], uploads::DEFAULT_PART_SIZE);
    assert_eq!(first["total_parts"], 1);
    assert_eq!(first["uploaded_parts"], json!([]));
    assert_eq!(first["metadata"]["authors"], json!(["Ama Ata Aidoo"]));
    assert_eq!(again["id"], first["id"]);
    assert_ne!(
        other_size["id"], first["id"],
        "a different size is a different file"
    );

    abort(&app, first["id"].as_str().unwrap()).await.data();
    let after_abort = app.admin(Method::POST, "/api/uploads", Some(body)).await.data();
    assert_ne!(
        after_abort["id"], first["id"],
        "an aborted session is never resumed"
    );
}

#[tokio::test]
async fn create_rejects_an_empty_file_or_a_missing_filename() {
    let app = TestApp::start().await;

    let empty = app
        .admin(
            Method::POST,
            "/api/uploads",
            Some(json!({"filename": "a.pdf", "size": 0})),
        )
        .await;
    let nameless = app
        .admin(
            Method::POST,
            "/api/uploads",
            Some(json!({"filename": " ", "size": 5})),
        )
        .await;

    assert_eq!(empty.status, StatusCode::BAD_REQUEST);
    assert_eq!(empty.error_code(), "invalid_file_size");
    assert_eq!(nameless.error_code(), "filename_required");
}

#[tokio::test]
async fn uploaded_parts_list_part_numbers_so_gaps_show() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;

    put_parts(&app, &id, &[4, 1]).await;

    let upload = show(&app, &id).await;
    assert_eq!(upload["status"], "uploading");
    assert_eq!(upload["total_parts"], 4);
    assert_eq!(upload["uploaded_parts"], json!([1, 4]));

    let listed = app.get("/api/uploads").await.data();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["uploaded_parts"], json!([1, 4]));
}

#[tokio::test]
async fn the_part_list_is_reconciled_with_the_store() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;
    put_parts(&app, &id, &[1, 2]).await;
    execute(
        &app,
        "DELETE FROM upload_parts WHERE upload_session_id = ? AND part_number = 2",
        [id.as_str()],
    )
    .await;
    execute(
        &app,
        "INSERT INTO upload_parts (upload_session_id, part_number, size, etag) VALUES (?, 3, 4, 'x')",
        [id.as_str()],
    )
    .await;

    assert_eq!(show(&app, &id).await["uploaded_parts"], json!([1, 2]));
    assert_eq!(
        app.get("/api/uploads").await.data()[0]["uploaded_parts"],
        json!([1, 2])
    );
}

#[tokio::test]
async fn a_restarted_server_resumes_where_the_upload_left_off() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;
    put_parts(&app, &id, &[1, 3]).await;

    let TestApp { dir, .. } = app;
    let blobs = Arc::new(FilesystemStore::open(&dir.path().join("blobs")).await.unwrap());
    let restarted = TestApp::start_in(dir, blobs).await;

    let recovered = restarted
        .admin(
            Method::POST,
            "/api/uploads",
            Some(json!({"filename": "Our Sister Killjoy.pdf", "size": 13, "resume_key": "killjoy"})),
        )
        .await
        .data();
    assert_eq!(recovered["id"], id.as_str());
    assert_eq!(
        recovered["part_size"], PART_SIZE,
        "the session keeps its own part size"
    );
    assert_eq!(show(&restarted, &id).await["uploaded_parts"], json!([1, 3]));
    assert_eq!(
        recovered["metadata"]["authors"],
        json!(["Ama Ata Aidoo"]),
        "metadata survives on the server"
    );

    put_parts(&restarted, &id, &[2, 4]).await;
    let upload = complete(&restarted, &id).await.data();
    assert_one_document_for(&restarted, &upload).await;
}

#[tokio::test]
async fn completing_with_a_missing_part_is_a_conflict_that_names_it() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;
    put_parts(&app, &id, &[1, 2, 4]).await;

    let response = complete(&app, &id).await;

    assert_eq!(response.status, StatusCode::CONFLICT);
    assert_eq!(response.error_code(), "upload_incomplete");
    assert_eq!(
        response.json()["error"]["message"],
        "part 3 has not been uploaded"
    );
    assert_eq!(count(&app, "SELECT count(*) FROM documents").await, 0);
    assert_eq!(
        show(&app, &id).await["status"],
        "uploading",
        "the upload can still be finished"
    );

    put_parts(&app, &id, &[3]).await;
    let upload = complete(&app, &id).await.data();
    assert_one_document_for(&app, &upload).await;
}

#[tokio::test]
async fn a_resent_part_replaces_the_earlier_bytes() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;

    put_part(&app, &id, "2", b"XXXX").await.data();
    put_parts(&app, &id, &[1, 2, 3, 4]).await;

    let upload = complete(&app, &id).await.data();
    assert_one_document_for(&app, &upload).await;
}

#[tokio::test]
async fn parts_of_the_wrong_number_or_size_are_rejected() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;

    for (part_number, bytes, code) in [
        ("0", part(1), "invalid_part_number"),
        ("5", part(1), "invalid_part_number"),
        ("one", part(1), "invalid_part_number"),
        ("-1", part(1), "invalid_part_number"),
        ("1", &b"abc"[..], "part_size_mismatch"),
        ("1", &b"abcde"[..], "part_size_mismatch"),
        ("4", part(1), "part_size_mismatch"), // the last part is the 1-byte remainder
    ] {
        let response = put_part(&app, &id, part_number, bytes).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "part {part_number}");
        assert_eq!(response.error_code(), code, "part {part_number}");
    }
    assert_eq!(show(&app, &id).await["uploaded_parts"], json!([]));
}

#[tokio::test]
async fn parts_may_exceed_the_default_body_limit_but_not_the_part_size() {
    let app = TestApp::start().await;
    let three_mib = 3 * 1024 * 1024;
    let mut new = new_upload("large");
    new.size = three_mib;
    let large = uploads::create(&app.state.db, app.state.blobs.as_ref(), &new, three_mib)
        .await
        .unwrap();

    let accepted = put_part(&app, &large.id, "1", &vec![b'x'; three_mib as usize]).await;
    let oversized = put_part(
        &app,
        &large.id,
        "1",
        &vec![b'x'; uploads::DEFAULT_PART_SIZE as usize + 1],
    )
    .await;

    assert_eq!(accepted.data()["uploaded_parts"], json!([1]));
    assert_eq!(oversized.status, StatusCode::BAD_REQUEST);
    assert_eq!(oversized.error_code(), "invalid_part_body");
}

#[tokio::test]
async fn unknown_uploads_are_not_found() {
    let app = TestApp::start().await;

    for response in [
        app.get("/api/uploads/nope").await,
        put_part(&app, "nope", "1", b"abcd").await,
        complete(&app, "nope").await,
        abort(&app, "nope").await,
    ] {
        assert_eq!(response.status, StatusCode::NOT_FOUND);
        assert_eq!(response.error_code(), "not_found");
    }
}

#[tokio::test]
async fn completing_again_returns_the_same_document() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;
    put_parts(&app, &id, &[1, 2, 3, 4]).await;

    let first = complete(&app, &id).await.data();
    let again = complete(&app, &id).await.data();

    assert_eq!(again["document_id"], first["document_id"]);
    assert_one_document_for(&app, &again).await;
    assert!(
        app.get("/api/uploads")
            .await
            .data()
            .as_array()
            .unwrap()
            .is_empty(),
        "a committed upload leaves the queue"
    );

    let abort_committed = abort(&app, &id).await;
    assert_eq!(abort_committed.status, StatusCode::CONFLICT);
    assert_eq!(abort_committed.error_code(), "upload_committed");
}

#[tokio::test]
async fn abort_is_idempotent_and_refuses_later_parts() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;
    put_parts(&app, &id, &[1]).await;

    let first = abort(&app, &id).await.data();
    let again = abort(&app, &id).await.data();
    let late_part = put_part(&app, &id, "2", part(2)).await;
    let late_complete = complete(&app, &id).await;

    assert_eq!(first["status"], "aborted");
    assert_eq!(again["status"], "aborted");
    assert_eq!(late_part.status, StatusCode::CONFLICT);
    assert_eq!(late_part.error_code(), "upload_not_resumable");
    assert_eq!(late_complete.error_code(), "upload_not_resumable");
    assert!(
        app.get("/api/uploads")
            .await
            .data()
            .as_array()
            .unwrap()
            .is_empty()
    );

    let key = object_key(&app, &id).await;
    let upload_id = provider_upload_id(&app, &id).await;
    assert!(matches!(
        app.state
            .blobs
            .list_parts(Multipart {
                key: &key,
                upload_id: &upload_id
            })
            .await,
        Err(BlobError::UploadNotFound)
    ));
}

#[tokio::test]
async fn an_interrupted_abort_can_be_retried() {
    let (app, flaky) = start_with_flaky_store().await;
    let id = start_upload(&app, "killjoy").await;
    flaky.fail(Operation::AbortMultipart, Failure::Before);

    let failed = abort(&app, &id).await;
    assert_eq!(failed.status, StatusCode::BAD_GATEWAY);
    let stuck = show(&app, &id).await;
    assert_eq!(stuck["status"], "aborting");
    assert!(stuck["last_error"].as_str().unwrap().contains("injected"));
    assert_eq!(
        put_part(&app, &id, "1", part(1)).await.error_code(),
        "upload_not_resumable"
    );

    flaky.heal(Operation::AbortMultipart);
    assert_eq!(abort(&app, &id).await.data()["status"], "aborted");
}

#[tokio::test]
async fn a_store_failure_while_assembling_is_retryable() {
    let (app, flaky) = start_with_flaky_store().await;
    let id = start_upload(&app, "killjoy").await;
    put_parts(&app, &id, &[1, 2, 3, 4]).await;
    flaky.fail(Operation::CompleteMultipart, Failure::Before);

    let failed = complete(&app, &id).await;

    assert_eq!(failed.status, StatusCode::BAD_GATEWAY);
    assert_eq!(failed.error_code(), "object_store_failed");
    let session = show(&app, &id).await;
    assert_eq!(session["status"], "failed");
    assert!(session["last_error"].as_str().unwrap().contains("injected"));
    assert_eq!(count(&app, "SELECT count(*) FROM documents").await, 0);

    flaky.heal(Operation::CompleteMultipart);
    let upload = complete(&app, &id).await.data();
    assert_one_document_for(&app, &upload).await;
    assert!(
        upload["last_error"].is_null(),
        "success clears the recorded failure"
    );
}

#[tokio::test]
async fn a_lost_reply_after_the_store_assembled_the_object_is_repaired() {
    let (app, flaky) = start_with_flaky_store().await;
    let id = start_upload(&app, "killjoy").await;
    put_parts(&app, &id, &[1, 2, 3, 4]).await;
    flaky.fail(Operation::CompleteMultipart, Failure::After);

    assert_eq!(complete(&app, &id).await.status, StatusCode::BAD_GATEWAY);
    assert_eq!(show(&app, &id).await["status"], "failed");

    // The multipart upload is gone, but the object exists; the retry must notice.
    flaky.heal(Operation::CompleteMultipart);
    let upload = complete(&app, &id).await.data();
    assert_one_document_for(&app, &upload).await;
}

#[tokio::test]
async fn a_crash_while_completing_is_repaired_by_asking_the_store() {
    let app = TestApp::start().await;
    let assembled = start_upload(&app, "assembled").await;
    let not_assembled = start_upload(&app, "not-assembled").await;
    put_parts(&app, &assembled, &[1, 2, 3, 4]).await;
    put_parts(&app, &not_assembled, &[1, 2, 3, 4]).await;

    // Both crashed after recording `completing`; only one reached the store first.
    assemble_behind_the_servers_back(&app, &assembled).await;
    force_status(&app, &assembled, "completing").await;
    force_status(&app, &not_assembled, "completing").await;

    for id in [&assembled, &not_assembled] {
        let upload = complete(&app, id).await.data();
        assert_eq!(upload["status"], "committed", "{id}");
    }
    assert_eq!(count(&app, "SELECT count(*) FROM documents").await, 2);
}

#[tokio::test]
async fn a_crash_after_the_object_was_ready_is_repaired_by_retrying() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;
    put_parts(&app, &id, &[1, 2, 3, 4]).await;
    assemble_behind_the_servers_back(&app, &id).await;
    force_status(&app, &id, "object_ready").await;

    let queued = app.get("/api/uploads").await.data();
    assert_eq!(
        queued[0]["status"], "object_ready",
        "the queue can see it needs finishing"
    );

    let upload = complete(&app, &id).await.data();
    assert_one_document_for(&app, &upload).await;
}

#[tokio::test]
async fn a_failed_document_commit_leaves_the_upload_ready_to_retry() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;
    put_parts(&app, &id, &[1, 2, 3, 4]).await;
    // Something already claims the object key, so the document insert must fail.
    execute(
        &app,
        "INSERT INTO documents (kind, title, object_key)
         SELECT 'pdf', 'squatter', object_key FROM upload_sessions WHERE id = ?",
        [id.as_str()],
    )
    .await;

    let failed = complete(&app, &id).await;

    assert_eq!(failed.status, StatusCode::INTERNAL_SERVER_ERROR);
    let session = show(&app, &id).await;
    assert_eq!(session["status"], "object_ready");
    assert!(session["document_id"].is_null());
    assert!(session["last_error"].is_string());
    assert_eq!(
        count(&app, "SELECT count(*) FROM document_sources").await,
        0,
        "nothing of the failed commit remains"
    );

    execute(&app, "DELETE FROM documents WHERE title = 'squatter'", ()).await;
    let upload = complete(&app, &id).await.data();
    assert_one_document_for(&app, &upload).await;
}

#[tokio::test]
async fn gc_dry_run_lists_overdue_sessions_without_changing_them() {
    let app = TestApp::start().await;
    let overdue = start_upload(&app, "overdue").await;
    let fresh = start_upload(&app, "fresh").await;
    put_parts(&app, &overdue, &[1]).await;
    make_overdue(&app, &overdue).await;

    let report = uploads::collect_garbage(&app.state.db, app.state.blobs.as_ref(), true)
        .await
        .unwrap();

    assert!(report.dry_run);
    assert_eq!(
        report
            .expired
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec![overdue.as_str()]
    );
    let untouched = show(&app, &overdue).await;
    assert_eq!(untouched["status"], "uploading");
    assert_eq!(
        untouched["uploaded_parts"],
        json!([1]),
        "the store still holds its parts"
    );
    assert_eq!(show(&app, &fresh).await["status"], "created");
}

#[tokio::test]
async fn gc_expires_and_aborts_overdue_sessions_but_keeps_assembled_objects() {
    let app = TestApp::start().await;
    let abandoned = start_upload(&app, "abandoned").await;
    let assembled = start_upload(&app, "assembled").await;
    let fresh = start_upload(&app, "fresh").await;
    put_parts(&app, &abandoned, &[1]).await;
    put_parts(&app, &assembled, &[1, 2, 3, 4]).await;
    assemble_behind_the_servers_back(&app, &assembled).await;
    force_status(&app, &assembled, "completing").await;
    make_overdue(&app, &abandoned).await;
    make_overdue(&app, &assembled).await;

    let report = uploads::collect_garbage(&app.state.db, app.state.blobs.as_ref(), false)
        .await
        .unwrap();

    assert!(!report.dry_run);
    assert_eq!(report.expired.len(), 1);
    assert_eq!(report.expired[0].id, abandoned);
    assert_eq!(report.needs_completion.len(), 1);
    assert_eq!(report.needs_completion[0].id, assembled);
    assert!(report.failed.is_empty());

    assert_eq!(show(&app, &abandoned).await["status"], "expired");
    let key = object_key(&app, &abandoned).await;
    let upload_id = provider_upload_id(&app, &abandoned).await;
    assert!(matches!(
        app.state
            .blobs
            .list_parts(Multipart {
                key: &key,
                upload_id: &upload_id
            })
            .await,
        Err(BlobError::UploadNotFound)
    ));
    assert_eq!(
        put_part(&app, &abandoned, "2", part(2)).await.error_code(),
        "upload_not_resumable"
    );
    assert_eq!(show(&app, &fresh).await["status"], "created");

    let upload = complete(&app, &assembled).await.data();
    assert_eq!(
        upload["status"], "committed",
        "the kept session still becomes a document"
    );

    let again = uploads::collect_garbage(&app.state.db, app.state.blobs.as_ref(), false)
        .await
        .unwrap();
    assert!(again.expired.is_empty() && again.needs_completion.is_empty());
}

#[tokio::test]
async fn upload_writes_require_the_admin_token() {
    let app = TestApp::start().await;
    let id = start_upload(&app, "killjoy").await;

    for (method, path) in [
        (Method::POST, "/api/uploads".to_string()),
        (Method::PUT, format!("/api/uploads/{id}/parts/1")),
        (Method::POST, format!("/api/uploads/{id}/complete")),
        (Method::DELETE, format!("/api/uploads/{id}")),
    ] {
        let response = app.anonymous(method.clone(), &path, None).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert_eq!(response.error_code(), "admin_token_required");
    }
    assert_eq!(show(&app, &id).await["status"], "created", "reads stay open");
}
