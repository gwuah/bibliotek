mod common;
#[path = "common/fixtures.rs"]
mod fixtures;

use axum::http::Method;
use liotek::error::Error;
use liotek::import::research;
use liotek::import::snapshot::{self, EmptySnapshot};
use liotek::import::v1::{self, MigrationReport, TableCounts};
use serde_json::json;

use common::TestApp;
use fixtures::{ResearchDb, V1Db, count, standard_v1};

async fn standard_snapshot(app: &TestApp) -> V1Db {
    let db = V1Db::create(app.dir.path().join("v1-snapshot.db")).await;
    standard_v1(&db).await;
    db
}

async fn library_count(app: &TestApp, sql: &str) -> i64 {
    count(&app.state.db.read().await.unwrap(), sql).await
}

fn table(report: &MigrationReport, name: &str) -> TableCounts {
    *report
        .tables
        .get(name)
        .unwrap_or_else(|| panic!("no counts for {name}"))
}

fn counts(read: u64, written: u64, merged: u64, skipped: u64, already_deleted: u64) -> TableCounts {
    TableCounts {
        read,
        written,
        merged,
        skipped,
        already_deleted,
        failed: 0,
    }
}

#[tokio::test]
async fn every_table_of_a_snapshot_is_accounted_for_and_nothing_is_lost() {
    let app = TestApp::start().await;
    let snapshot = standard_snapshot(&app).await;

    let report = v1::run(&app.state.db, &snapshot.path, false).await.unwrap();

    assert_eq!(report.unexplained_losses, Vec::<String>::new());
    assert!(report.succeeded && report.committed && !report.dry_run);
    assert_eq!(report.snapshot.migrations.len(), fixtures::V1_MIGRATIONS.len());
    assert_eq!(report.snapshot.sha256.len(), 64);

    assert_eq!(table(&report, "books"), counts(3, 3, 0, 0, 0));
    assert_eq!(table(&report, "authors"), counts(4, 4, 0, 0, 0));
    assert_eq!(table(&report, "book_authors"), counts(4, 4, 0, 0, 0));
    assert_eq!(table(&report, "categories"), counts(5, 5, 0, 0, 0));
    assert_eq!(table(&report, "book_categories"), counts(4, 4, 0, 0, 0));
    assert_eq!(table(&report, "tags"), counts(4, 4, 0, 0, 0));
    assert_eq!(table(&report, "book_tags"), counts(4, 4, 0, 0, 0));
    assert_eq!(table(&report, "resources"), counts(6, 4, 0, 1, 1));
    assert_eq!(table(&report, "annotations"), counts(7, 5, 0, 0, 2));
    assert_eq!(table(&report, "comments"), counts(4, 3, 0, 0, 1));
    assert_eq!(table(&report, "notes"), counts(3, 2, 0, 0, 1));
    assert_eq!(table(&report, "words"), counts(0, 0, 0, 0, 0));
    assert_eq!(table(&report, "reader_settings"), counts(1, 0, 0, 1, 0));
    assert_eq!(table(&report, "page_texts"), counts(0, 0, 0, 0, 0));
    assert!(
        report
            .not_imported
            .iter()
            .any(|exclusion| exclusion.table == "reader_settings")
    );
    assert!(
        report
            .not_imported
            .iter()
            .any(|exclusion| exclusion.table == "research_config")
    );

    assert_eq!(report.relationships.subject_parents.written, 3);
    assert_eq!(report.parents.highlights_verified, 5);
    assert_eq!(report.parents.comments_verified, 3);
    assert_eq!(report.parents.notes_verified, 2);
    assert!(report.parents.mismatches.is_empty());
    assert_eq!(report.id_mapping.entries, 3 + 4 + 5 + 3 + 2);
    assert_eq!(report.object_references.parsed, 3);
    assert!(report.object_references.unparsed.is_empty());
    assert_eq!(report.sections.sections_written, 2);
    assert!(report.sections.problems.is_empty());
    assert_eq!(
        report.light_installations,
        [
            ("aaa".to_string(), 1),
            ("bbb".to_string(), 1),
            ("ccc".to_string(), 1)
        ]
        .into()
    );

    let suspicious: Vec<&str> = report
        .suspicious_labels
        .iter()
        .map(|label| label.name.as_str())
        .collect();
    assert_eq!(suspicious, ["undefined", "HP LaserJet 600 dpi"]);
    assert_eq!(report.merge_candidates.len(), 1);
    assert_eq!(report.merge_candidates[0].title, fixtures::v1::RESEARCH_TITLE);
    assert_ne!(
        report.merge_candidates[0].book_document_id, report.merge_candidates[0].resource_document_id,
        "a same-titled Research PDF is never merged automatically"
    );
}

#[tokio::test]
async fn imported_rows_keep_their_shape_order_and_timestamps() {
    let app = TestApp::start().await;
    let snapshot = standard_snapshot(&app).await;

    v1::run(&app.state.db, &snapshot.path, false).await.unwrap();

    for (sql, expected) in [
        (
            "SELECT count(*) FROM documents WHERE object_key = '1111111111111111_The Trial & After.pdf'
             AND original_filename = 'The Trial & After.pdf' AND created_at = '2024-02-01T00:00:00.000Z'",
            1,
        ),
        ("SELECT count(*) FROM documents WHERE object_key = '2222222222222222_Decolonising the Mind.pdf'", 1),
        ("SELECT count(*) FROM document_sources WHERE source = 'bibliotek-v1' AND external_id IN ('1', '2', '3')", 3),
        ("SELECT count(*) FROM documents WHERE description = 'A novel in four movements'", 1),
        ("SELECT count(*) FROM documents WHERE kind = 'pdf' AND object_key IS NOT NULL AND description = ''", 0),
        (
            "SELECT count(*) FROM document_authors da JOIN authors a ON a.id = da.author_id
             JOIN documents d ON d.id = da.document_id
             WHERE d.title = 'The Trial of Dedan Kimathi'
               AND ((a.name = 'Ngũgĩ wa Thiong''o' AND da.position = 0) OR (a.name = 'Micere Githae Mugo' AND da.position = 1))",
            2,
        ),
        ("SELECT count(*) FROM authors", 4),
        (
            "SELECT count(*) FROM labels child JOIN labels parent ON parent.id = child.parent_id
             JOIN labels grand ON grand.id = parent.parent_id
             WHERE child.name = 'african literature' AND parent.name = 'literature' AND grand.name = 'humanities'",
            1,
        ),
        ("SELECT count(*) FROM labels WHERE kind = 'tag' AND name = 'undefined'", 1),
        ("SELECT count(*) FROM documents WHERE canonical_url = 'https://example.com/essay'", 1),
        ("SELECT count(*) FROM document_sources WHERE external_id = 'https://example.com/essay'", 2),
        ("SELECT count(*) FROM document_sources WHERE source = 'light:ccc'", 1),
        ("SELECT count(*) FROM documents WHERE canonical_url IN ('https://example.com/empty', 'https://example.com/gone')", 0),
        (
            "SELECT count(*) FROM highlights WHERE external_id = '1700000000001'
             AND created_at = '2025-03-01T10:00:00.000Z' AND color IS NULL
             AND json_extract(location_json, '$.kind') = 'web_text'",
            1,
        ),
        (
            "SELECT count(*) FROM highlights WHERE external_id = 'ann-1' AND color = '#ffd400'
             AND json_extract(location_json, '$.payload.page_number') = 3",
            1,
        ),
        ("SELECT count(*) FROM comments WHERE external_id IS NULL AND body = 'written by hand in Bibliotek'", 1),
        ("SELECT count(*) FROM notes WHERE document_source_id IS NULL AND body = 'A note written in Bibliotek'", 1),
        ("SELECT count(*) FROM notes WHERE body = 'A note Research deleted'", 0),
        (
            "SELECT count(*) FROM sections s JOIN document_sources ds ON ds.document_id = s.document_id
             WHERE ds.external_id = 'item-1'
               AND ((s.position = 0 AND s.start_page = 1 AND s.end_page = 9)
                 OR (s.position = 1 AND s.start_page = 10 AND s.end_page = 12))",
            2,
        ),
        ("SELECT count(*) FROM sqlite_master WHERE name IN ('reader_settings', 'page_texts', 'words')", 0),
    ] {
        assert_eq!(library_count(&app, sql).await, expected, "{sql}");
    }
}

#[tokio::test]
async fn a_dry_run_reports_everything_and_keeps_nothing() {
    let app = TestApp::start().await;
    let snapshot = standard_snapshot(&app).await;

    let report = v1::run(&app.state.db, &snapshot.path, true).await.unwrap();

    assert!(report.succeeded && report.dry_run && !report.committed);
    assert_eq!(table(&report, "annotations").written, 5);
    for table in [
        "documents",
        "labels",
        "authors",
        "highlights",
        "notes",
        "import_runs",
    ] {
        assert_eq!(
            library_count(&app, &format!("SELECT count(*) FROM {table}")).await,
            0,
            "{table}"
        );
    }
}

#[tokio::test]
async fn a_second_import_into_a_non_empty_library_is_refused() {
    let app = TestApp::start().await;
    let snapshot = standard_snapshot(&app).await;
    v1::run(&app.state.db, &snapshot.path, false).await.unwrap();

    let again = v1::run(&app.state.db, &snapshot.path, false).await;

    assert!(matches!(
        again,
        Err(Error::Conflict {
            code: "target_not_empty",
            ..
        })
    ));
    assert_eq!(library_count(&app, "SELECT count(*) FROM highlights").await, 5);
}

#[tokio::test]
async fn an_unexplained_loss_fails_the_import_and_keeps_nothing() {
    let app = TestApp::start().await;
    let snapshot = standard_snapshot(&app).await;
    snapshot
        .run(
            "INSERT INTO annotations (resource_id, text, external_id) VALUES (1, 'typed by hand', NULL)",
            (),
        )
        .await;
    snapshot
        .run(
            "UPDATE books SET url = 'https://elsewhere.example/a.pdf' WHERE id = 1",
            (),
        )
        .await;

    let report = v1::run(&app.state.db, &snapshot.path, false).await.unwrap();

    assert!(!report.succeeded && !report.committed);
    assert_eq!(table(&report, "annotations").failed, 1);
    assert_eq!(report.object_references.unparsed.len(), 1);
    assert_eq!(
        report.unexplained_losses.len(),
        2,
        "{:?}",
        report.unexplained_losses
    );
    assert_eq!(library_count(&app, "SELECT count(*) FROM documents").await, 0);
}

#[tokio::test]
async fn a_database_that_is_not_bibliotek_is_refused() {
    let app = TestApp::start().await;
    let path = app.dir.path().join("other.db");
    let conn = fixtures::create_database(&path).await;
    fixtures::execute(&conn, "CREATE TABLE books (id INTEGER PRIMARY KEY)", ()).await;

    let result = v1::run(&app.state.db, &path, false).await;

    assert!(matches!(
        result,
        Err(Error::Invalid {
            code: "v1_schema_incompatible",
            ..
        })
    ));
}

#[tokio::test]
async fn the_first_live_syncs_after_cutover_match_the_imported_rows() {
    let app = TestApp::start().await;
    let snapshot = standard_snapshot(&app).await;
    v1::run(&app.state.db, &snapshot.path, false).await.unwrap();
    let highlights_before = library_count(&app, "SELECT count(*) FROM highlights").await;

    let light = app
        .admin(
            Method::PUT,
            "/api/imports/light/aaa",
            Some(json!({ "highlights": { fixtures::v1::ESSAY_URL: [{
                "groupID": 1700000000001_i64,
                "repr": fixtures::v1::ESSAY_TEXT,
                "chunks": fixtures::v1::ESSAY_CHUNKS,
                "url": fixtures::v1::ESSAY_URL,
                "date": fixtures::v1::ESSAY_DATE,
                "comments": [{ "id": "c1", "text": fixtures::v1::ESSAY_COMMENT }],
            }] } })),
        )
        .await
        .data();
    assert_eq!(light["documents"]["unchanged"], 1);
    assert_eq!(
        light["highlights"],
        json!({ "created": 0, "updated": 0, "unchanged": 1, "deleted": 0 })
    );
    assert_eq!(
        light["comments"],
        json!({ "created": 0, "updated": 0, "unchanged": 1, "deleted": 0 })
    );

    let research_db = ResearchDb::create(app.dir.path().join("research.sqlite")).await;
    fixtures::research_matching_standard_v1(&research_db).await;
    let research_snapshot = research::read(&research_db.path).await.unwrap();
    let tx = app.state.db.begin().await.unwrap();
    let stats = snapshot::apply(&tx, &research_snapshot, EmptySnapshot::Refuse)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(stats.documents.unchanged, 2);
    assert_eq!(
        (
            stats.highlights.created,
            stats.highlights.updated,
            stats.highlights.unchanged
        ),
        (0, 0, 2)
    );
    assert_eq!(
        (
            stats.comments.created,
            stats.comments.updated,
            stats.comments.unchanged
        ),
        (0, 0, 1)
    );
    assert_eq!(
        (stats.notes.created, stats.notes.updated, stats.notes.unchanged),
        (0, 0, 1)
    );

    assert_eq!(
        library_count(&app, "SELECT count(*) FROM highlights").await,
        highlights_before
    );
    assert_eq!(
        library_count(&app, "SELECT count(*) FROM comments WHERE external_id IS NULL").await,
        1,
        "the comment written in Bibliotek is untouched by the live sync"
    );
}
