mod common;
#[path = "common/flaky_store.rs"]
mod flaky_store;

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::{Method, StatusCode, header};
use time::OffsetDateTime;
use time::macros::datetime;

use common::{TestApp, config_in};
use flaky_store::{Failure, FlakyStore, Operation};
use liotek::backup::{self, Backup, BackupName, RestoreSource, Trigger};
use liotek::blob::filesystem::FilesystemStore;
use liotek::db::{self, Db, DbError, ReadOnlyDb};
use liotek::error::Error;
use liotek::export::{self, LibraryCounts};

async fn seed_library(app: &TestApp) {
    let tx = app.state.db.begin().await.unwrap();
    tx.execute_batch(
        "INSERT INTO documents (id, kind, title) VALUES (1, 'pdf', 'Our Sister Killjoy'), (2, 'web', 'An essay');
         INSERT INTO document_sources (id, document_id, source, external_id) VALUES (1, 1, 'upload', 'u1');
         INSERT INTO highlights (id, document_id, document_source_id, external_id, text)
             VALUES (1, 1, 1, 'h1', 'a black-eyed squint'), (2, 1, 1, 'h2', 'a second highlight');
         INSERT INTO comments (highlight_id, body) VALUES (1, 'a comment');
         INSERT INTO notes (document_id, body) VALUES (1, 'a note'), (2, 'another note'), (2, 'a third');",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

const SEEDED: LibraryCounts = LibraryCounts {
    documents: 2,
    highlights: 2,
    comments: 1,
    notes: 3,
};

fn manual_at(time: OffsetDateTime) -> BackupName {
    BackupName::at(Trigger::Manual, time)
}

fn scheduled_at(time: OffsetDateTime) -> BackupName {
    BackupName::at(Trigger::Scheduled, time)
}

async fn create(app: &TestApp, name: BackupName) -> Backup {
    backup::create(
        &app.state.db,
        app.state.blobs.as_ref(),
        &app.state.config.temp_dir(),
        name,
    )
    .await
    .unwrap()
}

async fn names(app: &TestApp) -> Vec<String> {
    backup::list(app.state.blobs.as_ref())
        .await
        .unwrap()
        .iter()
        .map(|backup| backup.name.to_string())
        .collect()
}

async fn counts_in(path: &Path) -> LibraryCounts {
    let db = ReadOnlyDb::open(path).await.unwrap();
    LibraryCounts::read(&db, "main").await.unwrap()
}

fn staged_files(data_dir: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(data_dir.join("tmp")) {
        Ok(entries) => entries.map(|entry| entry.unwrap().path()).collect(),
        Err(_) => Vec::new(),
    }
}

fn open_uploads(app: &TestApp) -> usize {
    std::fs::read_dir(app.dir.path().join("blobs/multipart"))
        .unwrap()
        .count()
}

async fn start_with_flaky_store() -> (TestApp, Arc<FlakyStore>) {
    let dir = tempfile::tempdir().unwrap();
    let filesystem = FilesystemStore::open(&dir.path().join("blobs")).await.unwrap();
    let flaky = Arc::new(FlakyStore::wrap(Arc::new(filesystem)));
    let app = TestApp::start_in(dir, flaky.clone()).await;
    (app, flaky)
}

async fn export_of(app: &TestApp, dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("export.sqlite");
    export::export(&app.state.db, &path).await.unwrap();
    path
}

#[tokio::test]
async fn a_manual_backup_appears_in_the_list_with_its_trigger_and_size() {
    let app = TestApp::start().await;
    seed_library(&app).await;

    let made = app.admin(Method::POST, "/api/backups", None).await.data();
    let listed = app.admin(Method::GET, "/api/backups", None).await.data();

    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0], made);
    assert_eq!(made["trigger"], "manual");
    let name = made["name"].as_str().unwrap();
    assert!(name.starts_with("manual-"), "{name}");
    let key = app
        .dir
        .path()
        .join("blobs/objects/local/backups/manual")
        .join(format!("{}.sqlite", name.trim_start_matches("manual-")));
    assert_eq!(made["size"], std::fs::metadata(&key).unwrap().len());
    assert!(made["created_at"].as_str().unwrap().ends_with('Z'));
    assert!(
        staged_files(app.dir.path()).is_empty(),
        "the staged export is gone"
    );
}

#[tokio::test]
async fn backups_are_listed_newest_first_from_the_store_alone() {
    let app = TestApp::start().await;
    create(&app, scheduled_at(datetime!(2026-11-01 02:00 UTC))).await;
    create(&app, manual_at(datetime!(2026-11-03 09:30 UTC))).await;
    create(&app, scheduled_at(datetime!(2026-11-02 02:00 UTC))).await;

    let fresh = tempfile::tempdir().unwrap();
    let restarted = TestApp::start_in(fresh, app.state.blobs.clone()).await;

    assert_eq!(
        names(&restarted).await,
        vec![
            "manual-20261103T093000Z",
            "scheduled-20261102T020000Z",
            "scheduled-20261101T020000Z"
        ]
    );
}

#[tokio::test]
async fn a_second_backup_with_the_same_name_is_refused() {
    let app = TestApp::start().await;
    let name = manual_at(datetime!(2026-11-13 02:03:04 UTC));
    create(&app, name).await;

    let again = backup::create(
        &app.state.db,
        app.state.blobs.as_ref(),
        &app.state.config.temp_dir(),
        name,
    )
    .await;

    assert!(matches!(
        again,
        Err(Error::Conflict {
            code: "backup_exists",
            ..
        })
    ));
}

#[tokio::test]
async fn downloading_a_backup_reproduces_the_library() {
    let app = TestApp::start().await;
    seed_library(&app).await;
    let made = app.admin(Method::POST, "/api/backups", None).await.data();
    let name = made["name"].as_str().unwrap();

    let response = app
        .admin(Method::GET, &format!("/api/backups/{name}/file"), None)
        .await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.headers[header::CONTENT_TYPE], "application/vnd.sqlite3");
    assert!(
        response.headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with(&format!("attachment; filename=\"liotek-{name}.sqlite\""))
    );
    let downloaded = tempfile::tempdir().unwrap();
    let path = downloaded.path().join("backup.sqlite");
    std::fs::write(&path, &response.bytes).unwrap();
    assert_eq!(counts_in(&path).await, SEEDED);
}

#[tokio::test]
async fn backup_files_are_found_only_by_well_formed_names() {
    let app = TestApp::start().await;

    let missing = app
        .admin(Method::GET, "/api/backups/manual-20200101T000000Z/file", None)
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    for bad in [
        "..",
        "manual-20200101T000000Z.sqlite",
        "weekly-20200101T000000Z",
        "%2e%2e%2fx",
    ] {
        let response = app
            .admin(Method::GET, &format!("/api/backups/{bad}/file"), None)
            .await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(response.error_code(), "invalid_backup_name", "{bad}");
    }
}

#[tokio::test]
async fn every_backup_route_requires_the_admin_token() {
    let app = TestApp::start().await;
    let name = manual_at(datetime!(2026-11-13 02:03:04 UTC));
    create(&app, name).await;

    for (method, path) in [
        (Method::GET, "/api/backups".to_string()),
        (Method::POST, "/api/backups".to_string()),
        (Method::GET, format!("/api/backups/{name}/file")),
    ] {
        let response = app.anonymous(method.clone(), &path, None).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert_eq!(response.error_code(), "admin_token_required");
    }
    assert_eq!(names(&app).await.len(), 1, "no anonymous backup was made");
}

#[tokio::test]
async fn pruning_keeps_the_newest_scheduled_backups_and_every_manual_one() {
    let app = TestApp::start().await;
    create(&app, manual_at(datetime!(2026-10-01 00:00 UTC))).await;
    for day in [1, 2, 3, 4] {
        let time = datetime!(2026-11-01 02:00 UTC) + time::Duration::days(day);
        create(&app, scheduled_at(time)).await;
    }
    create(&app, manual_at(datetime!(2026-11-30 00:00 UTC))).await;

    let pruned = backup::prune(app.state.blobs.as_ref(), NonZeroUsize::new(2).unwrap())
        .await
        .unwrap();

    assert_eq!(
        pruned
            .iter()
            .map(|backup| backup.name.to_string())
            .collect::<Vec<_>>(),
        vec!["scheduled-20261103T020000Z", "scheduled-20261102T020000Z"]
    );
    assert_eq!(
        names(&app).await,
        vec![
            "manual-20261130T000000Z",
            "scheduled-20261105T020000Z",
            "scheduled-20261104T020000Z",
            "manual-20261001T000000Z"
        ]
    );
}

#[tokio::test]
async fn a_failed_backup_leaves_nothing_behind_and_deletes_nothing() {
    let (app, flaky) = start_with_flaky_store().await;
    seed_library(&app).await;
    let old = datetime!(2026-11-01 02:00 UTC);
    for day in 0..3 {
        create(&app, scheduled_at(old + time::Duration::days(day))).await;
    }
    let before = names(&app).await;

    for operation in [
        Operation::BeginMultipart,
        Operation::PutPart,
        Operation::CompleteMultipart,
    ] {
        flaky.fail(operation, Failure::Before);
        let run = backup::run_scheduled(
            &app.state.db,
            app.state.blobs.as_ref(),
            &app.state.config.temp_dir(),
            NonZeroUsize::new(1).unwrap(),
        )
        .await;
        flaky.heal(operation);

        assert!(run.is_err(), "{operation:?}");
        assert!(staged_files(app.dir.path()).is_empty(), "{operation:?}");
        assert_eq!(open_uploads(&app), 0, "{operation:?}: the upload was aborted");
        assert_eq!(names(&app).await, before, "{operation:?}: nothing was pruned");
    }
}

#[tokio::test]
async fn a_scheduled_run_prunes_once_its_backup_is_stored() {
    let app = TestApp::start().await;
    for day in 0..3 {
        create(
            &app,
            scheduled_at(datetime!(2026-11-01 02:00 UTC) + time::Duration::days(day)),
        )
        .await;
    }

    let run = backup::run_scheduled(
        &app.state.db,
        app.state.blobs.as_ref(),
        &app.state.config.temp_dir(),
        NonZeroUsize::new(2).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(run.backup.name.trigger, Trigger::Scheduled);
    assert_eq!(run.pruned.len(), 2);
    assert_eq!(
        names(&app).await,
        vec![
            run.backup.name.to_string(),
            "scheduled-20261103T020000Z".to_string()
        ]
    );
}

#[tokio::test]
async fn restoring_a_backup_into_a_fresh_data_directory_round_trips_the_library() {
    let app = TestApp::start().await;
    seed_library(&app).await;
    let name = manual_at(datetime!(2026-11-13 02:03:04 UTC));
    create(&app, name).await;

    let fresh = tempfile::tempdir().unwrap();
    let mut config = config_in(fresh.path());
    config.blob_store = app.state.config.blob_store.clone();
    let restored = backup::restore(&config, app.state.blobs.as_ref(), &RestoreSource::Backup(name))
        .await
        .unwrap();

    assert_eq!(restored.counts, SEEDED);
    assert_eq!(restored.database, config.database.path);
    assert!(staged_files(fresh.path()).is_empty());

    let db = Db::open(&config.database).await.unwrap();
    assert_eq!(
        db.migrate().await.unwrap(),
        Vec::<i64>::new(),
        "already up to date"
    );
    assert_eq!(
        LibraryCounts::read(&db.read().await.unwrap(), "main")
            .await
            .unwrap(),
        SEEDED
    );
    let conn = db.read().await.unwrap();
    let mut rows = conn.query("SELECT body FROM comments", ()).await.unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<String>(0).unwrap(),
        "a comment"
    );
}

#[tokio::test]
async fn restoring_a_local_export_round_trips_the_library() {
    let app = TestApp::start().await;
    seed_library(&app).await;
    let elsewhere = tempfile::tempdir().unwrap();
    let export = export_of(&app, &elsewhere).await;

    let fresh = tempfile::tempdir().unwrap();
    let config = config_in(fresh.path());
    let restored = backup::restore(
        &config,
        app.state.blobs.as_ref(),
        &RestoreSource::File(export.clone()),
    )
    .await
    .unwrap();

    assert_eq!(restored.counts, SEEDED);
    assert_eq!(counts_in(&config.database.path).await, SEEDED);
    assert!(export.exists(), "the source file is left as it was");
}

#[tokio::test]
async fn restore_never_replaces_an_existing_database() {
    let app = TestApp::start().await;
    seed_library(&app).await;
    let elsewhere = tempfile::tempdir().unwrap();
    let export = export_of(&app, &elsewhere).await;
    let database = &app.state.config.database.path;
    let before = std::fs::read(database).unwrap();

    let refused = backup::restore(
        &app.state.config,
        app.state.blobs.as_ref(),
        &RestoreSource::File(export),
    )
    .await;

    assert!(matches!(
        refused,
        Err(Error::Conflict {
            code: "database_exists",
            ..
        })
    ));
    assert_eq!(std::fs::read(database).unwrap(), before);
}

#[tokio::test]
async fn restore_refuses_a_stale_write_ahead_log_where_the_database_would_go() {
    let app = TestApp::start().await;
    let elsewhere = tempfile::tempdir().unwrap();
    let export = export_of(&app, &elsewhere).await;
    let fresh = tempfile::tempdir().unwrap();
    let config = config_in(fresh.path());
    std::fs::write(fresh.path().join("liotek.db-wal"), b"stale").unwrap();

    let refused = backup::restore(&config, app.state.blobs.as_ref(), &RestoreSource::File(export)).await;

    assert!(matches!(
        refused,
        Err(Error::Conflict {
            code: "database_exists",
            ..
        })
    ));
    assert!(!config.database.path.exists());
}

#[tokio::test]
async fn restore_refuses_a_damaged_file() {
    let app = TestApp::start().await;
    seed_library(&app).await;
    let elsewhere = tempfile::tempdir().unwrap();
    let export = export_of(&app, &elsewhere).await;

    let mut damaged = std::fs::read(&export).unwrap();
    let page = 4096;
    damaged[page..2 * page].fill(0xA5);
    let damaged_path = elsewhere.path().join("damaged.sqlite");
    std::fs::write(&damaged_path, damaged).unwrap();
    let garbage_path = elsewhere.path().join("garbage.sqlite");
    std::fs::write(&garbage_path, vec![0x5A; 3 * page]).unwrap();

    for path in [damaged_path, garbage_path] {
        let fresh = tempfile::tempdir().unwrap();
        let config = config_in(fresh.path());

        let refused = backup::restore(
            &config,
            app.state.blobs.as_ref(),
            &RestoreSource::File(path.clone()),
        )
        .await;

        assert!(
            matches!(
                refused,
                Err(Error::Invalid {
                    code: "restore_source_corrupt",
                    ..
                })
            ),
            "{}: {refused:?}",
            path.display()
        );
        assert!(!config.database.path.exists());
        assert!(staged_files(fresh.path()).is_empty());
    }
}

#[tokio::test]
async fn restore_refuses_a_library_from_a_newer_build() {
    let app = TestApp::start().await;
    let elsewhere = tempfile::tempdir().unwrap();
    let export = export_of(&app, &elsewhere).await;
    let conn = db::open_file(&export).await.unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (version, name, checksum, build_revision) VALUES (99, 'x', 'x', 'x')",
        (),
    )
    .await
    .unwrap();

    let fresh = tempfile::tempdir().unwrap();
    let config = config_in(fresh.path());
    let refused = backup::restore(&config, app.state.blobs.as_ref(), &RestoreSource::File(export)).await;

    assert!(matches!(
        refused,
        Err(Error::Database(DbError::SchemaTooNew { found: 99, .. }))
    ));
    assert!(!config.database.path.exists());
    assert!(staged_files(fresh.path()).is_empty());
}

#[tokio::test]
async fn restore_refuses_a_sqlite_file_that_is_not_a_library() {
    let elsewhere = tempfile::tempdir().unwrap();
    let foreign = elsewhere.path().join("foreign.sqlite");
    let conn = db::open_file(&foreign).await.unwrap();
    conn.execute("CREATE TABLE things (id INTEGER)", ())
        .await
        .unwrap();
    let app = TestApp::start().await;

    let fresh = tempfile::tempdir().unwrap();
    let refused = backup::restore(
        &config_in(fresh.path()),
        app.state.blobs.as_ref(),
        &RestoreSource::File(foreign),
    )
    .await;

    assert!(matches!(
        refused,
        Err(Error::Invalid {
            code: "restore_source_not_liotek",
            ..
        })
    ));
}

#[tokio::test]
async fn restore_refuses_while_something_listens_on_the_configured_address() {
    let app = TestApp::start().await;
    let elsewhere = tempfile::tempdir().unwrap();
    let export = export_of(&app, &elsewhere).await;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();

    let fresh = tempfile::tempdir().unwrap();
    let mut config = config_in(fresh.path());
    config.bind = listener.local_addr().unwrap();
    let refused = backup::restore(&config, app.state.blobs.as_ref(), &RestoreSource::File(export)).await;

    assert!(matches!(
        refused,
        Err(Error::Conflict {
            code: "daemon_running",
            ..
        })
    ));
    assert!(!config.database.path.exists());
}

async fn library_with_one_document(dir: &Path) -> liotek::config::Config {
    let config = config_in(dir);
    let db = Db::open(&config.database).await.unwrap();
    db.migrate().await.unwrap();
    let tx = db.begin().await.unwrap();
    tx.execute("INSERT INTO documents (kind, title) VALUES ('pdf', 'Anowa')", ())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    config
}

#[tokio::test]
async fn restore_can_set_the_current_database_aside_instead_of_refusing() {
    let app = TestApp::start().await;
    seed_library(&app).await;
    let elsewhere = tempfile::tempdir().unwrap();
    let export = export_of(&app, &elsewhere).await;
    let target = tempfile::tempdir().unwrap();
    let config = library_with_one_document(target.path()).await;

    let restored =
        backup::restore_setting_aside(&config, app.state.blobs.as_ref(), &RestoreSource::File(export))
            .await
            .unwrap();

    assert_eq!(restored.counts, SEEDED);
    let set_aside = restored.set_aside.expect("the old database was set aside");
    assert!(set_aside.starts_with(target.path().join("set-aside")));
    let old = ReadOnlyDb::open(&set_aside.join("liotek.db")).await.unwrap();
    assert_eq!(LibraryCounts::read(&old, "main").await.unwrap().documents, 1);
}

#[tokio::test]
async fn a_damaged_backup_never_displaces_the_current_database() {
    let app = TestApp::start().await;
    let elsewhere = tempfile::tempdir().unwrap();
    let garbage = elsewhere.path().join("garbage.sqlite");
    std::fs::write(&garbage, vec![0x5A; 3 * 4096]).unwrap();
    let target = tempfile::tempdir().unwrap();
    let config = library_with_one_document(target.path()).await;
    let before = std::fs::read(&config.database.path).unwrap();

    let refused =
        backup::restore_setting_aside(&config, app.state.blobs.as_ref(), &RestoreSource::File(garbage)).await;

    assert!(matches!(
        refused,
        Err(Error::Invalid {
            code: "restore_source_corrupt",
            ..
        })
    ));
    assert_eq!(std::fs::read(&config.database.path).unwrap(), before);
    assert!(!target.path().join("set-aside").exists());
}
