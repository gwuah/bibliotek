mod common;

use axum::http::{Method, StatusCode, header};
use common::TestApp;
use libsql::params;
use serde_json::{Value, json};

use liotek::blob::ObjectKey;
use liotek::domain::documents::{self, DocumentKind, NewDocument, Section, Sections};

async fn seed(app: &TestApp, new: NewDocument) -> i64 {
    let tx = app.state.db.begin().await.unwrap();
    let id = documents::create(&tx, &new).await.unwrap();
    tx.commit().await.unwrap();
    id
}

fn book(title: &str) -> NewDocument {
    NewDocument::new(DocumentKind::Pdf, title)
}

fn pair(a: i64, b: i64) -> String {
    format!("{}-{}", a.min(b), a.max(b))
}

async fn candidate_ids(app: &TestApp) -> Vec<String> {
    app.get("/api/document-matches")
        .await
        .data()
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| candidate["id"].as_str().unwrap().to_string())
        .collect()
}

async fn merge(app: &TestApp, a: i64, b: i64, survivor_id: i64) -> common::TestResponse {
    app.admin(
        Method::POST,
        &format!("/api/document-matches/{}/merge", pair(a, b)),
        Some(json!({ "survivor_id": survivor_id })),
    )
    .await
}

async fn annotate(app: &TestApp, document_id: i64, source: &str) {
    let tx = app.state.db.begin().await.unwrap();
    let source_id = documents::add_source(&tx, document_id, source, &format!("{source}-{document_id}"))
        .await
        .unwrap();
    tx.execute(
        "INSERT INTO highlights (document_id, document_source_id, external_id, text) VALUES (?, ?, 'h', ?)",
        params![document_id, source_id, format!("passage from {source}")],
    )
    .await
    .unwrap();
    tx.execute(
        "INSERT INTO comments (highlight_id, external_id, body)
         SELECT id, 'c', 'a comment' FROM highlights WHERE document_source_id = ?",
        [source_id],
    )
    .await
    .unwrap();
    tx.execute(
        "INSERT INTO notes (document_id, document_source_id, external_id, body) VALUES (?, ?, 'n', 'a note')",
        params![document_id, source_id],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn totals(app: &TestApp) -> [i64; 4] {
    let conn = app.state.db.read().await.unwrap();
    let mut totals = [0; 4];
    for (slot, table) in ["document_sources", "highlights", "comments", "notes"]
        .iter()
        .enumerate()
    {
        let mut rows = conn
            .query(&format!("SELECT count(*) FROM {table}"), ())
            .await
            .unwrap();
        totals[slot] = rows.next().await.unwrap().unwrap().get(0).unwrap();
    }
    totals
}

#[tokio::test]
async fn candidates_are_documents_with_equal_normalized_titles() {
    let app = TestApp::start().await;
    let upload = seed(
        &app,
        NewDocument {
            authors: vec!["Ama Ata Aidoo".into()],
            original_filename: Some("killjoy.pdf".into()),
            ..book("Our Sister Killjoy")
        },
    )
    .await;
    let research = seed(
        &app,
        NewDocument {
            authors: vec!["ama ata aidoo".into()],
            ..book("our sister   killjoy!")
        },
    )
    .await;
    annotate(&app, research, "research").await;
    seed(&app, book("Our Sister Killjoy, Revisited")).await;

    let candidates = app.get("/api/document-matches").await.data();

    let candidates = candidates.as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(candidate["id"], pair(upload, research));
    assert_eq!(candidate["basis"], "our sister killjoy");
    assert_eq!(candidate["evidence"]["shared_authors"], json!(["Ama Ata Aidoo"]));
    assert_eq!(candidate["evidence"]["same_filename"], false);
    let [first, second] = [&candidate["documents"][0], &candidate["documents"][1]];
    assert_eq!(first["id"], upload);
    assert_eq!(first["original_filename"], "killjoy.pdf");
    assert_eq!(first["has_file"], false);
    assert_eq!(first["kind"], "pdf");
    assert_eq!(
        second["authors"],
        json!(["Ama Ata Aidoo"]),
        "author names are shared, ignoring case"
    );
    assert_eq!(second["sources"][0]["source"], "research");
    assert_eq!(second["highlight_count"], 1);
    assert_eq!(second["comment_count"], 1);
    assert_eq!(second["note_count"], 1);
}

#[tokio::test]
async fn a_dismissed_pair_stays_hidden_until_its_title_changes() {
    let app = TestApp::start().await;
    let a = seed(&app, book("Arrow of God")).await;
    let b = seed(&app, book("Arrow of God.")).await;
    let dismiss_path = format!("/api/document-matches/{}/dismiss", pair(a, b));
    let dismiss = || app.admin(Method::POST, &dismiss_path, None);

    let dismissal = dismiss().await.data();
    assert_eq!(dismissal, json!({"id": pair(a, b), "basis": "arrow of god"}));
    dismiss().await.data();
    assert!(candidate_ids(&app).await.is_empty());

    // A cosmetic edit leaves the match basis unchanged.
    for id in [a, b] {
        app.admin(
            Method::PATCH,
            &format!("/api/documents/{id}"),
            Some(json!({"title": "ARROW OF GOD"})),
        )
        .await
        .data();
    }
    assert!(candidate_ids(&app).await.is_empty());

    for id in [a, b] {
        app.admin(
            Method::PATCH,
            &format!("/api/documents/{id}"),
            Some(json!({"title": "Arrow of God: A Novel"})),
        )
        .await
        .data();
    }
    assert_eq!(candidate_ids(&app).await, [pair(a, b)]);

    let anonymous = app
        .anonymous(
            Method::POST,
            &format!("/api/document-matches/{}/dismiss", pair(a, b)),
            None,
        )
        .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn merge_moves_every_relationship_onto_the_survivor() {
    let app = TestApp::start().await;
    let survivor = seed(
        &app,
        NewDocument {
            authors: vec!["Chinua Achebe".into()],
            subjects: vec!["Fiction".into()],
            tags: vec!["igbo".into()],
            object_key: Some(ObjectKey::parse("local/things.pdf").unwrap()),
            original_filename: Some("things.pdf".into()),
            ..book("Things Fall Apart")
        },
    )
    .await;
    let retired = seed(
        &app,
        NewDocument {
            description: Some("From Research".into()),
            authors: vec!["chinua achebe".into(), "An Editor".into()],
            subjects: vec!["Nigerian Literature".into()],
            tags: vec!["igbo".into(), "classic".into()],
            ..book("Things Fall Apart")
        },
    )
    .await;
    annotate(&app, survivor, "upload").await;
    annotate(&app, retired, "research").await;
    annotate(&app, retired, "light:laptop").await;
    let tx = app.state.db.begin().await.unwrap();
    let sections = Sections::new(vec![Section {
        title: "Part One".into(),
        start_page: 1,
        end_page: 50,
    }])
    .unwrap();
    documents::replace_sections(&tx, retired, &sections)
        .await
        .unwrap();
    tx.execute("UPDATE documents SET page_offset = 6 WHERE id = ?", [retired])
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let before = totals(&app).await;

    let merged = merge(&app, survivor, retired, survivor).await.data();

    assert_eq!(merged, json!({"survivor_id": survivor, "retired_id": retired}));
    assert_eq!(
        totals(&app).await,
        before,
        "no source, highlight, comment, or note was lost"
    );
    let document = app.get(&format!("/api/documents/{survivor}")).await.data();
    assert_eq!(document["highlights"].as_array().unwrap().len(), 3);
    assert_eq!(document["notes"].as_array().unwrap().len(), 3);
    assert_eq!(document["sources"].as_array().unwrap().len(), 3);
    let comments: usize = document["highlights"]
        .as_array()
        .unwrap()
        .iter()
        .map(|highlight| highlight["comments"].as_array().unwrap().len())
        .sum();
    assert_eq!(comments, 3);
    assert_eq!(document["authors"], json!(["Chinua Achebe", "An Editor"]));
    assert_eq!(document["subjects"], json!(["Fiction", "Nigerian Literature"]));
    assert_eq!(document["tags"], json!(["classic", "igbo"]));
    assert_eq!(document["description"], "From Research", "gaps are filled");
    assert_eq!(document["original_filename"], "things.pdf");
    assert_eq!(document["has_file"], true);
    assert_eq!(
        document["sections"],
        json!([{"title": "Part One", "start_page": 1, "end_page": 50}]),
        "a survivor without sections adopts the retired document's"
    );
    assert_eq!(document["page_offset"], 6, "the offset travels with its sections");
    assert!(candidate_ids(&app).await.is_empty());
}

#[tokio::test]
async fn a_survivor_keeps_its_own_sections() {
    let app = TestApp::start().await;
    let survivor = seed(&app, book("Sections")).await;
    let retired = seed(&app, book("Sections")).await;
    for (id, title) in [(survivor, "Survivor's"), (retired, "Retired's")] {
        app.admin(
            Method::PATCH,
            &format!("/api/documents/{id}"),
            Some(json!({"sections": [{"title": title, "start_page": 1, "end_page": 9}]})),
        )
        .await
        .data();
    }

    merge(&app, survivor, retired, survivor).await.data();

    let document = app.get(&format!("/api/documents/{survivor}")).await.data();
    assert_eq!(
        document["sections"],
        json!([{"title": "Survivor's", "start_page": 1, "end_page": 9}])
    );
}

#[tokio::test]
async fn a_retired_id_redirects_to_its_survivor_even_through_chains() {
    let app = TestApp::start().await;
    let first = seed(&app, book("Copy")).await;
    let second = seed(&app, book("Copy")).await;
    let third = seed(
        &app,
        NewDocument {
            object_key: Some(ObjectKey::parse("local/copy.pdf").unwrap()),
            ..book("Copy")
        },
    )
    .await;

    merge(&app, first, second, second).await.data();
    let redirect = app.get(&format!("/api/documents/{first}")).await;
    assert_eq!(redirect.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        redirect.headers[header::LOCATION],
        format!("/api/documents/{second}").as_str()
    );

    merge(&app, second, third, third).await.data();
    for retired in [first, second] {
        let redirect = app.get(&format!("/api/documents/{retired}")).await;
        assert_eq!(redirect.status, StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            redirect.headers[header::LOCATION],
            format!("/api/documents/{third}").as_str(),
            "aliases of a retired document are re-pointed"
        );
    }
    let file = app.get(&format!("/api/documents/{first}/file")).await;
    assert_eq!(
        file.headers[header::LOCATION],
        format!("/api/documents/{third}/file").as_str()
    );

    let edit = app
        .admin(
            Method::PATCH,
            &format!("/api/documents/{first}"),
            Some(json!({"title": "x"})),
        )
        .await;
    assert_eq!(edit.status, StatusCode::NOT_FOUND, "edits are never redirected");
}

#[tokio::test]
async fn merging_two_documents_with_files_is_refused_and_changes_nothing() {
    let app = TestApp::start().await;
    let a = seed(
        &app,
        NewDocument {
            object_key: Some(ObjectKey::parse("local/a.pdf").unwrap()),
            authors: vec!["Author A".into()],
            ..book("Two Files")
        },
    )
    .await;
    let b = seed(
        &app,
        NewDocument {
            object_key: Some(ObjectKey::parse("local/b.pdf").unwrap()),
            authors: vec!["Author B".into()],
            ..book("Two Files")
        },
    )
    .await;
    annotate(&app, b, "research").await;
    let before = (
        totals(&app).await,
        snapshot(&app, a).await,
        snapshot(&app, b).await,
    );

    let response = merge(&app, a, b, a).await;

    assert_eq!(response.status, StatusCode::CONFLICT);
    assert_eq!(response.error_code(), "both_have_files");
    let after = (
        totals(&app).await,
        snapshot(&app, a).await,
        snapshot(&app, b).await,
    );
    assert_eq!(after, before);
    assert_eq!(candidate_ids(&app).await, [pair(a, b)]);
}

async fn snapshot(app: &TestApp, id: i64) -> Value {
    app.get(&format!("/api/documents/{id}")).await.data()
}

#[tokio::test]
async fn merges_of_impossible_pairs_are_rejected() {
    let app = TestApp::start().await;
    let a = seed(&app, book("Alone")).await;
    let b = seed(&app, book("Different")).await;
    let post = async |path: String, survivor_id: i64| {
        app.admin(Method::POST, &path, Some(json!({ "survivor_id": survivor_id })))
            .await
    };

    let itself = post(format!("/api/document-matches/{a}-{a}/merge"), a).await;
    assert_eq!(itself.status, StatusCode::BAD_REQUEST);
    assert_eq!(itself.error_code(), "same_document");

    let missing = post(format!("/api/document-matches/{a}-9999/merge"), a).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    let stranger = post(format!("/api/document-matches/{}/merge", pair(a, b)), 9999).await;
    assert_eq!(stranger.error_code(), "survivor_not_in_pair");

    let backwards = post(format!("/api/document-matches/{b}-{a}/merge"), a).await;
    assert_eq!(backwards.error_code(), "invalid_pair_id");

    let not_candidates = app
        .admin(
            Method::POST,
            &format!("/api/document-matches/{}/dismiss", pair(a, b)),
            None,
        )
        .await;
    assert_eq!(not_candidates.error_code(), "not_a_candidate");

    post(format!("/api/document-matches/{}/merge", pair(a, b)), b)
        .await
        .data();
    assert_eq!(
        app.get(&format!("/api/documents/{a}")).await.status,
        StatusCode::PERMANENT_REDIRECT
    );
}
