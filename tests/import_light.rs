mod common;
#[path = "common/fixtures.rs"]
mod fixtures;

use axum::http::{Method, StatusCode};
use liotek::domain::documents::{self, LabelKind};
use serde_json::{Value, json};

use common::{TestApp, TestResponse};
use fixtures::{count, execute};

const SHARED_URL: &str = "https://shared.example/essay";
const INSTALLATIONS: [&str; 3] = ["aaa", "bbb", "ccc"];

fn highlight(group_id: i64, text: &str, comments: Value) -> Value {
    json!({
        "groupID": group_id,
        "repr": text,
        "chunks": [text],
        "url": SHARED_URL,
        "date": "2025-05-01T09:00:00.000Z",
        "comments": comments,
    })
}

fn payload(pages: Value) -> Value {
    json!({ "highlights": pages })
}

async fn sync(app: &TestApp, installation: &str, body: Value) -> TestResponse {
    app.admin(
        Method::PUT,
        &format!("/api/imports/light/{installation}"),
        Some(body),
    )
    .await
}

async fn sync_confirming_empty(app: &TestApp, installation: &str, body: Value) -> TestResponse {
    app.admin(
        Method::PUT,
        &format!("/api/imports/light/{installation}?confirm_empty=true"),
        Some(body),
    )
    .await
}

async fn library_count(app: &TestApp, sql: &str) -> i64 {
    count(&app.state.db.read().await.unwrap(), sql).await
}

async fn library_state(app: &TestApp) -> Vec<i64> {
    let mut state = Vec::new();
    for table in [
        "documents",
        "document_sources",
        "highlights",
        "comments",
        "notes",
        "import_runs",
    ] {
        state.push(library_count(app, &format!("SELECT count(*) FROM {table}")).await);
    }
    state.push(library_count(app, "SELECT coalesce(sum(length(text)), 0) FROM highlights").await);
    state
}

async fn sync_three_installations(app: &TestApp) {
    for (index, installation) in INSTALLATIONS.iter().enumerate() {
        let group = 1_000 + index as i64;
        let mut pages = json!({ SHARED_URL: [highlight(group, "shared words", json!([]))] });
        if *installation == "aaa" {
            pages = json!({
                SHARED_URL: [
                    highlight(1_000, "shared words", json!([{ "id": "c1", "text": "a first thought", "created": "2025-05-01T09:01:00.000Z", "updated": "2025-05-01T09:01:00.000Z" }])),
                    highlight(1_100, "more words", json!([{ "id": "c2", "text": "another" }])),
                ],
                "https://aaa-only.example/": [highlight(1_200, "mine alone", json!([]))],
            });
        }
        sync(app, installation, payload(pages)).await.data();
    }
}

#[tokio::test]
async fn first_sync_creates_and_three_installations_share_one_document_per_url() {
    let app = TestApp::start().await;

    let first = sync(
        &app,
        "aaa",
        payload(json!({ SHARED_URL: [highlight(1, "words", json!([{ "id": "c1", "text": "a thought" }]))] })),
    )
    .await
    .data();
    assert_eq!(first["source"], "light:aaa");
    assert_eq!(first["documents"]["created"], 1);
    assert_eq!(first["highlights"]["created"], 1);
    assert_eq!(first["comments"]["created"], 1);
    assert!(first["import_run_id"].as_i64().unwrap() > 0);

    for installation in ["bbb", "ccc"] {
        let stats = sync(
            &app,
            installation,
            payload(json!({ SHARED_URL: [highlight(1, "words", json!([]))] })),
        )
        .await
        .data();
        assert_eq!(
            stats["documents"]["linked"], 1,
            "{installation} joins the existing page"
        );
    }

    assert_eq!(library_count(&app, "SELECT count(*) FROM documents").await, 1);
    assert_eq!(
        library_count(&app, "SELECT count(*) FROM document_sources").await,
        3
    );
    assert_eq!(library_count(&app, "SELECT count(*) FROM highlights").await, 3);
    assert_eq!(
        library_count(&app, "SELECT count(*) FROM documents WHERE title = canonical_url").await,
        1,
        "a Light page is titled by its URL when created"
    );
}

#[tokio::test]
async fn repeating_a_sync_changes_nothing() {
    let app = TestApp::start().await;
    sync_three_installations(&app).await;
    let before = library_state(&app).await;

    let again = sync(
        &app,
        "aaa",
        payload(json!({
            SHARED_URL: [
                highlight(1_000, "shared words", json!([{ "id": "c1", "text": "a first thought", "created": "2025-05-01T09:01:00.000Z", "updated": "2025-05-01T09:01:00.000Z" }])),
                highlight(1_100, "more words", json!([{ "id": "c2", "text": "another" }])),
            ],
            "https://aaa-only.example/": [highlight(1_200, "mine alone", json!([]))],
        })),
    )
    .await
    .data();

    assert_eq!(
        again["documents"],
        json!({ "created": 0, "linked": 0, "unchanged": 2, "unlinked": 0, "deleted": 0 })
    );
    assert_eq!(
        again["highlights"],
        json!({ "created": 0, "updated": 0, "unchanged": 3, "deleted": 0 })
    );
    assert_eq!(
        again["comments"],
        json!({ "created": 0, "updated": 0, "unchanged": 2, "deleted": 0 })
    );
    let mut after = library_state(&app).await;
    after[5] -= 1; // the repeat is itself recorded as an import run
    assert_eq!(after, before);
}

#[tokio::test]
async fn edited_highlights_and_comments_are_updated_in_place() {
    let app = TestApp::start().await;
    sync_three_installations(&app).await;

    let stats = sync(
        &app,
        "aaa",
        payload(json!({
            SHARED_URL: [
                highlight(1_000, "shared words, re-selected", json!([{ "id": "c1", "text": "a second thought", "created": "2025-05-01T09:01:00.000Z", "updated": "2025-05-02T09:01:00.000Z" }])),
                highlight(1_100, "more words", json!([{ "id": "c2", "text": "another" }])),
            ],
            "https://aaa-only.example/": [highlight(1_200, "mine alone", json!([]))],
        })),
    )
    .await
    .data();

    assert_eq!(stats["highlights"]["updated"], 1);
    assert_eq!(stats["comments"]["updated"], 1);
    assert_eq!(
        library_count(&app, "SELECT count(*) FROM comments WHERE body = 'a second thought' AND updated_at = '2025-05-02T09:01:00.000Z'").await,
        1
    );
    assert_eq!(
        library_count(&app, "SELECT count(*) FROM highlights").await,
        5,
        "nothing duplicated"
    );
}

#[tokio::test]
async fn a_removed_highlight_goes_with_its_comments_and_can_reappear() {
    let app = TestApp::start().await;
    sync_three_installations(&app).await;

    let removed = sync(
        &app,
        "aaa",
        payload(json!({
            SHARED_URL: [highlight(1_100, "more words", json!([]))],
            "https://aaa-only.example/": [highlight(1_200, "mine alone", json!([]))],
        })),
    )
    .await
    .data();
    assert_eq!(removed["highlights"]["deleted"], 1);
    assert_eq!(
        removed["comments"]["deleted"], 2,
        "c1 with its highlight, c2 removed from its highlight"
    );
    assert_eq!(library_count(&app, "SELECT count(*) FROM comments").await, 0);

    let reappeared = sync(
        &app,
        "aaa",
        payload(json!({
            SHARED_URL: [highlight(1_000, "shared words", json!([{ "id": "c1", "text": "back again" }])), highlight(1_100, "more words", json!([]))],
            "https://aaa-only.example/": [highlight(1_200, "mine alone", json!([]))],
        })),
    )
    .await
    .data();
    assert_eq!(reappeared["highlights"]["created"], 1);
    assert_eq!(reappeared["comments"]["created"], 1);
}

#[tokio::test]
async fn one_installations_removals_never_touch_another_installations_rows() {
    let app = TestApp::start().await;
    sync_three_installations(&app).await;

    let emptied = sync_confirming_empty(&app, "bbb", payload(json!({})))
        .await
        .data();

    assert_eq!(emptied["highlights"]["deleted"], 1);
    assert_eq!(
        emptied["documents"]["unlinked"], 1,
        "the shared page is still held by aaa and ccc"
    );
    assert_eq!(
        library_count(
            &app,
            "SELECT count(*) FROM document_sources WHERE source = 'light:bbb'"
        )
        .await,
        0
    );
    assert_eq!(
        library_count(&app, "SELECT count(*) FROM highlights h JOIN document_sources s ON s.id = h.document_source_id WHERE s.source = 'light:aaa'").await,
        3
    );
    assert_eq!(
        library_count(&app, "SELECT count(*) FROM highlights h JOIN document_sources s ON s.id = h.document_source_id WHERE s.source = 'light:ccc'").await,
        1
    );
    assert_eq!(library_count(&app, "SELECT count(*) FROM comments").await, 2);
}

#[tokio::test]
async fn an_unexpectedly_empty_snapshot_is_refused_and_changes_nothing() {
    let app = TestApp::start().await;
    sync_three_installations(&app).await;
    let before = library_state(&app).await;

    for empty in [json!({}), json!({ SHARED_URL: [] })] {
        let response = sync(&app, "aaa", payload(empty)).await;
        assert_eq!(response.status, StatusCode::CONFLICT);
        assert_eq!(response.error_code(), "empty_snapshot");
    }

    assert_eq!(library_state(&app).await, before);
}

#[tokio::test]
async fn a_confirmed_empty_snapshot_removes_the_installation_and_its_unheld_pages() {
    let app = TestApp::start().await;
    sync_three_installations(&app).await;

    let stats = sync_confirming_empty(&app, "aaa", payload(json!({})))
        .await
        .data();

    assert_eq!(stats["highlights"]["deleted"], 3);
    assert_eq!(stats["comments"]["deleted"], 2);
    assert_eq!(stats["documents"]["deleted"], 1, "the page only aaa had");
    assert_eq!(stats["documents"]["unlinked"], 1, "the shared page");
    assert_eq!(library_count(&app, "SELECT count(*) FROM documents").await, 1);
    assert_eq!(library_count(&app, "SELECT count(*) FROM highlights").await, 2);
}

#[tokio::test]
async fn a_malformed_or_partial_payload_is_refused_and_changes_nothing() {
    let app = TestApp::start().await;
    sync_three_installations(&app).await;
    let before = library_state(&app).await;

    let missing_text = json!({ "highlights": { SHARED_URL: [{ "groupID": 1, "chunks": [], "url": SHARED_URL, "date": "x" }] } });
    let scoped = json!({ "scope": SHARED_URL, "highlights": { SHARED_URL: [highlight(1_000, "shared words", json!([]))] } });
    let duplicate =
        payload(json!({ SHARED_URL: [highlight(7, "a", json!([])), highlight(7, "b", json!([]))] }));
    let blank_comment =
        payload(json!({ SHARED_URL: [highlight(7, "a", json!([{ "id": "c", "text": "  " }]))] }));
    for (body, code) in [
        (missing_text, "invalid_request_body"),
        (scoped, "invalid_request_body"),
        (duplicate, "snapshot_duplicate_id"),
        (blank_comment, "invalid_light_snapshot"),
    ] {
        let response = sync(&app, "aaa", body).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST);
        assert_eq!(response.error_code(), code);
    }
    let bad_installation = sync(&app, "a%20b", payload(json!({}))).await;
    assert_eq!(bad_installation.error_code(), "invalid_light_installation");

    assert_eq!(library_state(&app).await, before);
}

#[tokio::test]
async fn local_notes_and_edits_survive_every_sync_and_keep_the_page_alive() {
    let app = TestApp::start().await;
    sync_three_installations(&app).await;
    let tx = app.state.db.begin().await.unwrap();
    let only_aaa = "SELECT id FROM documents WHERE canonical_url = 'https://aaa-only.example/'";
    let document_id = count(&tx, only_aaa).await;
    execute(
        &tx,
        "UPDATE documents SET title = 'Retitled', description = 'Mine', updated_at = '2030-01-01T00:00:00.000Z' WHERE id = ?",
        [document_id],
    )
    .await;
    documents::replace_labels(&tx, document_id, LabelKind::Tag, &["keep".to_string()])
        .await
        .unwrap();
    execute(
        &tx,
        "INSERT INTO notes (document_id, body) VALUES (?, 'written here')",
        [document_id],
    )
    .await;
    tx.commit().await.unwrap();

    sync_three_installations(&app).await;
    sync_confirming_empty(&app, "aaa", payload(json!({})))
        .await
        .data();

    let conn = app.state.db.read().await.unwrap();
    assert_eq!(
        count(&conn, &format!("SELECT count(*) FROM documents WHERE id = {document_id} AND title = 'Retitled' AND description = 'Mine'")).await,
        1
    );
    assert_eq!(
        count(&conn, "SELECT count(*) FROM notes WHERE body = 'written here'").await,
        1
    );
    assert_eq!(
        count(
            &conn,
            &format!("SELECT count(*) FROM document_labels WHERE document_id = {document_id}")
        )
        .await,
        1
    );
}

#[tokio::test]
async fn a_title_edit_alone_keeps_a_page_whose_source_is_gone() {
    let app = TestApp::start().await;
    sync_three_installations(&app).await;
    let tx = app.state.db.begin().await.unwrap();
    execute(
        &tx,
        "UPDATE documents SET title = 'A page I named', updated_at = '2030-01-01T00:00:00.000Z'
         WHERE canonical_url = 'https://aaa-only.example/'",
        (),
    )
    .await;
    tx.commit().await.unwrap();

    sync_confirming_empty(&app, "aaa", payload(json!({})))
        .await
        .data();

    assert_eq!(
        library_count(
            &app,
            "SELECT count(*) FROM documents WHERE title = 'A page I named'"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn a_local_comment_is_kept_as_a_note_when_its_highlight_is_removed() {
    let app = TestApp::start().await;
    sync_three_installations(&app).await;
    let tx = app.state.db.begin().await.unwrap();
    execute(
        &tx,
        "INSERT INTO comments (highlight_id, body) SELECT id, 'my own aside' FROM highlights WHERE text = 'mine alone'",
        (),
    )
    .await;
    tx.commit().await.unwrap();

    let stats = sync(
        &app,
        "aaa",
        payload(json!({ SHARED_URL: [highlight(1_000, "shared words", json!([]))] })),
    )
    .await
    .data();

    assert_eq!(stats["comments_rehomed_as_notes"], 1);
    assert_eq!(
        stats["documents"]["deleted"], 0,
        "the rehomed note keeps the page"
    );
    assert_eq!(
        library_count(&app, "SELECT count(*) FROM notes WHERE document_source_id IS NULL AND body = '> mine alone\n\nmy own aside'").await,
        1
    );
}

#[tokio::test]
async fn a_light_sync_needs_the_admin_token() {
    let app = TestApp::start().await;

    let response = app
        .anonymous(Method::PUT, "/api/imports/light/aaa", Some(payload(json!({}))))
        .await;

    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_snapshot_larger_than_the_default_body_limit_is_accepted() {
    let app = TestApp::start().await;
    let long = "x".repeat(4096);
    let highlights: Vec<Value> = (0..800).map(|group| highlight(group, &long, json!([]))).collect();

    let stats = sync(&app, "aaa", payload(json!({ SHARED_URL: highlights })))
        .await
        .data();

    assert_eq!(stats["highlights"]["created"], 800);
}
