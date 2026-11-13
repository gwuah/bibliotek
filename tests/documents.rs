mod common;

use axum::http::{Method, StatusCode, header};
use common::TestApp;
use libsql::params;
use serde_json::{Value, json};

use liotek::blob::ObjectKey;
use liotek::domain::documents::{self, DocumentKind, NewDocument};

async fn seed(app: &TestApp, new: NewDocument) -> i64 {
    let tx = app.state.db.begin().await.unwrap();
    let id = documents::create(&tx, &new).await.unwrap();
    tx.commit().await.unwrap();
    id
}

fn book(title: &str) -> NewDocument {
    NewDocument::new(DocumentKind::Pdf, title)
}

async fn execute(app: &TestApp, sql: &str, values: impl libsql::params::IntoParams) {
    let tx = app.state.db.begin().await.unwrap();
    tx.execute(sql, values).await.unwrap();
    tx.commit().await.unwrap();
}

fn titles(page: &Value) -> Vec<String> {
    page["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|document| document["title"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn listing_is_paginated_newest_first_with_a_total() {
    let app = TestApp::start().await;
    for n in 1..=55 {
        seed(&app, book(&format!("Volume {n}"))).await;
    }

    let first = app.get("/api/documents").await.data();
    let second = app.get("/api/documents?page=2").await.data();

    assert_eq!(first["total"], 55);
    assert_eq!(first["page"], 1);
    assert_eq!(first["page_size"], 50);
    assert_eq!(first["documents"].as_array().unwrap().len(), 50);
    assert_eq!(titles(&first)[0], "Volume 55");
    assert_eq!(
        titles(&second),
        ["Volume 5", "Volume 4", "Volume 3", "Volume 2", "Volume 1"]
    );
    assert_eq!(second["total"], 55);

    let zero = app.get("/api/documents?page=0").await;
    assert_eq!(zero.status, StatusCode::BAD_REQUEST);
    assert_eq!(zero.error_code(), "invalid_page");
    let garbled = app.get("/api/documents?page=two").await;
    assert_eq!(garbled.error_code(), "invalid_query");
}

#[tokio::test]
async fn search_matches_title_description_and_author_names() {
    let app = TestApp::start().await;
    seed(
        &app,
        NewDocument {
            authors: vec!["Chinua Achebe".into()],
            ..book("Things Fall Apart")
        },
    )
    .await;
    seed(
        &app,
        NewDocument {
            description: Some("A novel about falling empires".into()),
            ..book("Anthills of the Savannah")
        },
    )
    .await;
    seed(&app, book("Petals of Blood")).await;

    let by_title = app.get("/api/documents?q=fall").await.data();
    let by_author = app.get("/api/documents?q=achebe").await.data();
    let blank = app.get("/api/documents?q=%20").await.data();

    assert_eq!(
        titles(&by_title),
        ["Anthills of the Savannah", "Things Fall Apart"]
    );
    assert_eq!(titles(&by_author), ["Things Fall Apart"]);
    assert_eq!(by_author["total"], 1);
    assert_eq!(blank["total"], 3, "a blank query matches everything");
}

#[tokio::test]
async fn filters_by_kind_author_subject_and_tag() {
    let app = TestApp::start().await;
    seed(
        &app,
        NewDocument {
            authors: vec!["Walter Rodney".into()],
            subjects: vec!["History".into()],
            ..book("How Europe Underdeveloped Africa")
        },
    )
    .await;
    seed(
        &app,
        NewDocument {
            tags: vec!["History".into()],
            ..NewDocument::new(DocumentKind::Web, "A blog post tagged history")
        },
    )
    .await;
    seed(
        &app,
        NewDocument {
            subjects: vec!["West African History".into()],
            ..book("The Akan Kingdoms")
        },
    )
    .await;
    execute(
        &app,
        "UPDATE labels SET parent_id = (SELECT id FROM labels WHERE kind = 'subject' AND name = 'History')
         WHERE name = 'West African History'",
        (),
    )
    .await;

    let web = app.get("/api/documents?kind=web").await.data();
    let by_author = app.get("/api/documents?author=walter%20rodney").await.data();
    let subject = app.get("/api/documents?subject=History").await.data();
    let tag = app.get("/api/documents?tag=History").await.data();
    let combined = app.get("/api/documents?subject=History&kind=web").await.data();

    assert_eq!(titles(&web), ["A blog post tagged history"]);
    assert_eq!(titles(&by_author), ["How Europe Underdeveloped Africa"]);
    assert_eq!(
        titles(&subject),
        ["The Akan Kingdoms", "How Europe Underdeveloped Africa"],
        "a subject includes its sub-subjects, but never a tag of the same name"
    );
    assert_eq!(titles(&tag), ["A blog post tagged history"]);
    assert_eq!(combined["total"], 0);

    let bad_kind = app.get("/api/documents?kind=video").await;
    assert_eq!(bad_kind.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad_kind.error_code(), "invalid_query");
}

#[tokio::test]
async fn filters_by_whether_a_document_has_a_file() {
    let app = TestApp::start().await;
    let uploaded = seed(&app, book("Our Sister Killjoy")).await;
    seed(&app, book("A Research PDF without a file")).await;
    seed(
        &app,
        NewDocument::new(DocumentKind::Web, "https://example.com/essay"),
    )
    .await;
    execute(
        &app,
        "UPDATE documents SET object_key = 'local/killjoy.pdf' WHERE id = ?",
        [uploaded],
    )
    .await;

    let with_file = app.get("/api/documents?has_file=true").await.data();
    let without_file = app.get("/api/documents?has_file=false").await.data();

    assert_eq!(titles(&with_file), ["Our Sister Killjoy"]);
    assert_eq!(without_file["total"], 2);
}

#[tokio::test]
async fn summaries_keep_author_order_and_count_annotations() {
    let app = TestApp::start().await;
    let id = seed(
        &app,
        NewDocument {
            authors: vec!["Zadie Smith".into(), "Ama Ata Aidoo".into(), "Mia Couto".into()],
            subjects: vec!["Fiction".into()],
            tags: vec!["essays".into(), "anthology".into()],
            object_key: Some(ObjectKey::parse("local/anthology.pdf").unwrap()),
            ..book("An Anthology")
        },
    )
    .await;
    annotate(&app, id).await;

    let summary = app.get("/api/documents").await.data()["documents"][0].clone();

    assert_eq!(
        summary["authors"],
        json!(["Zadie Smith", "Ama Ata Aidoo", "Mia Couto"])
    );
    assert_eq!(summary["subjects"], json!(["Fiction"]));
    assert_eq!(summary["tags"], json!(["anthology", "essays"]));
    assert_eq!(summary["has_file"], true);
    assert_eq!(summary["kind"], "pdf");
    assert_eq!(summary["highlight_count"], 2);
    assert_eq!(summary["note_count"], 1);
}

async fn annotate(app: &TestApp, document_id: i64) {
    let tx = app.state.db.begin().await.unwrap();
    let source_id = documents::add_source(&tx, document_id, "research", &format!("item-{document_id}"))
        .await
        .unwrap();
    for (external_id, text) in [("h1", "first passage"), ("h2", "second passage")] {
        tx.execute(
            "INSERT INTO highlights (document_id, document_source_id, external_id, text, color, location_json)
             VALUES (?, ?, ?, ?, 'yellow', '{\"version\":1,\"kind\":\"pdf\",\"payload\":{\"page\":4}}')",
            params![document_id, source_id, external_id, text],
        )
        .await
        .unwrap();
    }
    for (external_id, body) in [("c1", "first comment"), ("c2", "second comment")] {
        tx.execute(
            "INSERT INTO comments (highlight_id, external_id, body)
             SELECT id, ?, ? FROM highlights WHERE document_source_id = ? AND external_id = 'h2'",
            params![external_id, body, source_id],
        )
        .await
        .unwrap();
    }
    tx.execute(
        "INSERT INTO notes (document_id, document_source_id, external_id, body) VALUES (?, ?, 'n1', 'a note')",
        params![document_id, source_id],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn detail_returns_the_whole_document_in_one_response() {
    let app = TestApp::start().await;
    let id = seed(
        &app,
        NewDocument {
            canonical_url: Some("https://example.com/devil".into()),
            original_filename: Some("devil.pdf".into()),
            authors: vec!["Ngũgĩ wa Thiong'o".into()],
            subjects: vec!["Fiction".into()],
            tags: vec!["gikuyu".into()],
            ..book("Devil on the Cross")
        },
    )
    .await;
    annotate(&app, id).await;
    app.admin(
        Method::PATCH,
        &format!("/api/documents/{id}"),
        Some(json!({"sections": [{"title": "One", "start_page": 1, "end_page": 20}]})),
    )
    .await
    .data();

    let document = app.get(&format!("/api/documents/{id}")).await.data();

    assert_eq!(document["title"], "Devil on the Cross");
    assert_eq!(document["canonical_url"], "https://example.com/devil");
    assert_eq!(document["original_filename"], "devil.pdf");
    assert_eq!(document["page_offset"], 0);
    assert_eq!(document["has_file"], false);
    assert_eq!(document["authors"], json!(["Ngũgĩ wa Thiong'o"]));
    assert_eq!(document["subjects"], json!(["Fiction"]));
    assert_eq!(document["tags"], json!(["gikuyu"]));
    assert_eq!(document["sources"][0]["source"], "research");
    assert_eq!(document["sources"][0]["external_id"], format!("item-{id}"));
    assert!(document["sources"][0]["last_seen_at"].is_string());
    assert_eq!(
        document["sections"],
        json!([{"title": "One", "start_page": 1, "end_page": 20}])
    );

    let highlights = document["highlights"].as_array().unwrap();
    assert_eq!(highlights.len(), 2);
    assert_eq!(highlights[0]["text"], "first passage");
    assert_eq!(highlights[0]["source"], "research");
    assert_eq!(highlights[0]["color"], "yellow");
    assert_eq!(highlights[0]["location"]["payload"]["page"], 4);
    assert_eq!(highlights[0]["comments"], json!([]));
    let comments: Vec<&str> = highlights[1]["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|comment| comment["body"].as_str().unwrap())
        .collect();
    assert_eq!(comments, ["first comment", "second comment"]);
    assert_eq!(document["notes"][0]["body"], "a note");
    assert_eq!(document["notes"][0]["source"], "research");

    let missing = app.get("/api/documents/9999").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert_eq!(missing.error_code(), "not_found");
}

#[tokio::test]
async fn patch_changes_only_the_fields_it_names() {
    let app = TestApp::start().await;
    let id = seed(
        &app,
        NewDocument {
            description: Some("Original description".into()),
            authors: vec!["Ayi Kwei Armah".into()],
            subjects: vec!["Fiction".into()],
            tags: vec!["ghana".into()],
            ..book("The Beautyful Ones")
        },
    )
    .await;
    execute(
        &app,
        "UPDATE documents SET updated_at = '2000-01-01T00:00:00.000Z'",
        (),
    )
    .await;
    let path = format!("/api/documents/{id}");

    let retitled = app
        .admin(
            Method::PATCH,
            &path,
            Some(json!({"title": "  The Beautyful Ones Are Not Yet Born ", "page_offset": 4})),
        )
        .await
        .data();
    assert_eq!(retitled["title"], "The Beautyful Ones Are Not Yet Born");
    assert_eq!(retitled["page_offset"], 4);
    assert_eq!(retitled["description"], "Original description");
    assert_eq!(retitled["authors"], json!(["Ayi Kwei Armah"]));
    assert_eq!(retitled["tags"], json!(["ghana"]));
    assert_ne!(retitled["updated_at"], "2000-01-01T00:00:00.000Z");

    let relabelled = app
        .admin(
            Method::PATCH,
            &path,
            Some(json!({
                "description": null,
                "authors": ["Second Author", "Ayi Kwei Armah"],
                "tags": [],
            })),
        )
        .await
        .data();
    assert_eq!(relabelled["description"], Value::Null);
    assert_eq!(relabelled["authors"], json!(["Second Author", "Ayi Kwei Armah"]));
    assert_eq!(relabelled["tags"], json!([]));
    assert_eq!(
        relabelled["subjects"],
        json!(["Fiction"]),
        "subjects were not sent"
    );
    assert_eq!(relabelled["title"], "The Beautyful Ones Are Not Yet Born");

    let blank_title = app.admin(Method::PATCH, &path, Some(json!({"title": " "}))).await;
    assert_eq!(blank_title.error_code(), "title_required");
    let unknown_field = app
        .admin(Method::PATCH, &path, Some(json!({"subject": ["x"]})))
        .await;
    assert_eq!(unknown_field.error_code(), "invalid_request_body");
    let missing = app
        .admin(Method::PATCH, "/api/documents/9999", Some(json!({"title": "x"})))
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn anonymous_patch_is_refused_and_changes_nothing() {
    let app = TestApp::start().await;
    let id = seed(&app, book("Changes")).await;

    let response = app
        .anonymous(
            Method::PATCH,
            &format!("/api/documents/{id}"),
            Some(json!({"title": "Vandalised"})),
        )
        .await;

    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        app.get(&format!("/api/documents/{id}")).await.data()["title"],
        "Changes"
    );
}

#[tokio::test]
async fn sections_are_validated_then_replaced_whole() {
    let app = TestApp::start().await;
    let id = seed(&app, book("Sectioned")).await;
    let path = format!("/api/documents/{id}");
    let patch_sections =
        |sections: Value| app.admin(Method::PATCH, &path, Some(json!({"sections": sections})));

    let saved = patch_sections(json!([
        {"title": "Part One", "start_page": 1, "end_page": 40},
        {"title": "Part Two", "start_page": 41, "end_page": 90},
    ]))
    .await
    .data();
    assert_eq!(saved["sections"].as_array().unwrap().len(), 2);

    for (sections, code) in [
        (
            json!([{"title": "A", "start_page": 1, "end_page": 10}, {"title": "B", "start_page": 10, "end_page": 20}]),
            "sections_overlap",
        ),
        (
            json!([{"title": "A", "start_page": 30, "end_page": 20}]),
            "section_range_inverted",
        ),
        (
            json!([{"title": " ", "start_page": 1, "end_page": 2}]),
            "section_title_required",
        ),
        (
            json!([{"title": "B", "start_page": 50, "end_page": 60}, {"title": "A", "start_page": 1, "end_page": 5}]),
            "sections_out_of_order",
        ),
    ] {
        let response = patch_sections(sections).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST);
        assert_eq!(response.error_code(), code);
    }
    assert_eq!(
        app.get(&path).await.data()["sections"],
        saved["sections"],
        "a rejected edit changes nothing"
    );

    let replaced = patch_sections(json!([{"title": "Whole", "start_page": 1, "end_page": 90}]))
        .await
        .data();
    assert_eq!(
        replaced["sections"],
        json!([{"title": "Whole", "start_page": 1, "end_page": 90}])
    );
}

#[tokio::test]
async fn file_streams_from_the_filesystem_store_and_404s_without_one() {
    let app = TestApp::start().await;
    let objects = app.dir.path().join("blobs/objects/local");
    std::fs::create_dir_all(&objects).unwrap();
    std::fs::write(objects.join("devil.pdf"), b"%PDF-1.7 bytes").unwrap();
    let with_file = seed(
        &app,
        NewDocument {
            object_key: Some(ObjectKey::parse("local/devil.pdf").unwrap()),
            original_filename: Some("Devil on the Cross.pdf".into()),
            ..book("Devil on the Cross")
        },
    )
    .await;
    let without_file = seed(&app, book("No file")).await;

    let served = app.get(&format!("/api/documents/{with_file}/file")).await;
    let absent = app.get(&format!("/api/documents/{without_file}/file")).await;

    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.bytes.as_ref(), b"%PDF-1.7 bytes");
    assert_eq!(served.headers[header::CONTENT_TYPE], "application/pdf");
    assert!(
        served.headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("inline; filename=\"Devil on the Cross.pdf\"")
    );
    assert_eq!(absent.status, StatusCode::NOT_FOUND);
    assert_eq!(absent.error_code(), "not_found");
}

#[tokio::test]
async fn facets_count_documents_and_describe_the_subject_hierarchy() {
    let app = TestApp::start().await;
    seed(
        &app,
        NewDocument {
            authors: vec!["Chinua Achebe".into()],
            subjects: vec!["Nigerian Fiction".into()],
            tags: vec!["igbo".into()],
            ..book("Arrow of God")
        },
    )
    .await;
    seed(
        &app,
        NewDocument {
            authors: vec!["Chinua Achebe".into()],
            subjects: vec!["Fiction".into()],
            tags: vec!["igbo".into(), "colonialism".into()],
            ..book("Things Fall Apart")
        },
    )
    .await;
    execute(
        &app,
        "UPDATE labels SET parent_id = (SELECT id FROM labels WHERE kind = 'subject' AND name = 'Fiction')
         WHERE name = 'Nigerian Fiction'",
        (),
    )
    .await;

    let facets = app.get("/api/facets").await.data();

    assert_eq!(facets["authors"][0]["name"], "Chinua Achebe");
    assert_eq!(facets["authors"][0]["document_count"], 2);
    let tags: Vec<(String, i64)> = facets["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tag| {
            (
                tag["name"].as_str().unwrap().into(),
                tag["document_count"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(tags, [("colonialism".into(), 1), ("igbo".into(), 2)]);

    let subjects = facets["subjects"].as_array().unwrap();
    let fiction = subjects
        .iter()
        .find(|subject| subject["name"] == "Fiction")
        .unwrap();
    let nigerian = subjects
        .iter()
        .find(|subject| subject["name"] == "Nigerian Fiction")
        .unwrap();
    assert_eq!(fiction["parent"], Value::Null);
    assert_eq!(
        fiction["document_count"], 2,
        "a subject counts its sub-subjects' documents"
    );
    assert_eq!(nigerian["parent"], "Fiction");
    assert_eq!(nigerian["parent_id"], fiction["id"]);
    assert_eq!(nigerian["document_count"], 1);
}
