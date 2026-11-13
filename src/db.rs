use std::ops::Deref;
use std::path::Path;
use std::sync::Mutex as SyncMutex;
use std::time::Duration;

use libsql::{Builder, Connection, Database, OpenFlags, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, MutexGuard, Semaphore, SemaphorePermit};

use crate::build;
use crate::config::DatabaseConfig;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

pub const SNAPSHOT_SCHEMA: &str = "snapshot";

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    sql: &'static str,
}

/// Append only: a released migration is never edited, reordered, or removed, and none may
/// drop user data.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "documents",
        sql: include_str!("../migrations/0001_documents.sql"),
    },
    Migration {
        version: 2,
        name: "annotations",
        sql: include_str!("../migrations/0002_annotations.sql"),
    },
    Migration {
        version: 3,
        name: "operations",
        sql: include_str!("../migrations/0003_operations.sql"),
    },
];

pub const SUPPORTED_SCHEMA_VERSION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].version;

const CREATE_MIGRATIONS_TABLE: &str = "
    CREATE TABLE IF NOT EXISTS schema_migrations (
        version        INTEGER PRIMARY KEY,
        name           TEXT NOT NULL,
        checksum       TEXT NOT NULL,
        build_revision TEXT NOT NULL,
        applied_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    )";

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(
        "the database is at schema version {found}, but this build only supports up to {supported}; \
         run a newer liotek"
    )]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("migration {version} ({name}) was applied with different contents than this build carries")]
    ChecksumMismatch { version: i64, name: String },
    #[error(transparent)]
    Sqlite(#[from] libsql::Error),
}

#[derive(Debug, PartialEq, Eq)]
pub struct SchemaStatus {
    pub current: i64,
    pub pending: Vec<i64>,
}

const READ_CONNECTIONS: usize = 4;

pub struct Db {
    readers: ReaderPool,
    writer: Connection,
    write_gate: Mutex<()>,
    _database: Database,
}

impl Db {
    pub async fn open(config: &DatabaseConfig) -> Result<Db, DbError> {
        let database = Builder::new_local(&config.path).build().await?;
        let writer = connect(&database).await?;
        // `journal_mode` answers with a row, so it must be queried rather than executed.
        writer.query("PRAGMA journal_mode = WAL", ()).await?;
        let mut readers = Vec::with_capacity(READ_CONNECTIONS);
        for _ in 0..READ_CONNECTIONS {
            readers.push(connect(&database).await?);
        }
        Ok(Db {
            readers: ReaderPool::new(readers),
            writer,
            write_gate: Mutex::new(()),
            _database: database,
        })
    }

    /// One caller per connection: all statements on a SQLite connection share one snapshot, so
    /// a shared reader would show one request's stale view to another.
    pub async fn read(&self) -> Result<ReadConn<'_>, DbError> {
        Ok(self.readers.borrow().await)
    }

    pub async fn begin(&self) -> Result<WriteTx<'_>, DbError> {
        let gate = self.write_gate.lock().await;
        let tx = self
            .writer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await?;
        Ok(WriteTx { tx, _gate: gate })
    }

    pub async fn migrate(&self) -> Result<Vec<i64>, DbError> {
        let status = self.schema_status().await?;
        self.begin_and_commit(CREATE_MIGRATIONS_TABLE).await?;

        for migration in MIGRATIONS
            .iter()
            .filter(|migration| status.pending.contains(&migration.version))
        {
            let tx = self.begin().await?;
            tx.execute_batch(migration.sql).await?;
            tx.execute(
                "INSERT INTO schema_migrations (version, name, checksum, build_revision) VALUES (?, ?, ?, ?)",
                (
                    migration.version,
                    migration.name,
                    checksum(migration.sql),
                    build::GIT_REVISION,
                ),
            )
            .await?;
            tx.commit().await?;
        }
        Ok(status.pending)
    }

    pub async fn schema_status(&self) -> Result<SchemaStatus, DbError> {
        schema_status(&*self.read().await?).await
    }

    /// The copy is attached to the writer rather than opened: a new connection is never closed
    /// (see [`never_close`]) and would keep the file open for the life of the daemon.
    pub async fn snapshot_into<T, E: From<DbError>>(
        &self,
        path: &Path,
        shape: impl AsyncFnOnce(&Connection) -> Result<T, E>,
    ) -> Result<T, E> {
        let target = path.to_string_lossy().into_owned();
        // VACUUM cannot run inside a transaction, so this takes the write gate by hand.
        let _gate = self.write_gate.lock().await;
        self.writer
            .execute("VACUUM INTO ?", [target.clone()])
            .await
            .map_err(DbError::from)?;
        self.writer
            .execute(&format!("ATTACH DATABASE ? AS {SNAPSHOT_SCHEMA}"), [target])
            .await
            .map_err(DbError::from)?;

        let shaped = self.shape_and_seal_snapshot(shape).await;
        let detached = self
            .writer
            .execute(&format!("DETACH DATABASE {SNAPSHOT_SCHEMA}"), ())
            .await;
        let value = shaped?;
        detached.map_err(DbError::from)?;
        Ok(value)
    }

    async fn shape_and_seal_snapshot<T, E: From<DbError>>(
        &self,
        shape: impl AsyncFnOnce(&Connection) -> Result<T, E>,
    ) -> Result<T, E> {
        self.writer
            .query(&format!("PRAGMA {SNAPSHOT_SCHEMA}.journal_mode = DELETE"), ())
            .await
            .map_err(DbError::from)?;
        let value = shape(&self.writer).await?;
        self.writer
            .execute(&format!("VACUUM {SNAPSHOT_SCHEMA}"), ())
            .await
            .map_err(DbError::from)?;
        Ok(value)
    }

    async fn begin_and_commit(&self, sql: &str) -> Result<(), DbError> {
        let tx = self.begin().await?;
        tx.execute(sql, ()).await?;
        tx.commit().await
    }
}

struct ReaderPool {
    idle: SyncMutex<Vec<Connection>>,
    available: Semaphore,
}

impl ReaderPool {
    fn new(connections: Vec<Connection>) -> ReaderPool {
        ReaderPool {
            available: Semaphore::new(connections.len()),
            idle: SyncMutex::new(connections),
        }
    }

    async fn borrow(&self) -> ReadConn<'_> {
        let permit = self
            .available
            .acquire()
            .await
            .expect("the reader semaphore is never closed");
        let conn = self
            .idle
            .lock()
            .expect("reader pool lock")
            .pop()
            .expect("a permit implies an idle reader");
        ReadConn {
            conn: Some(conn),
            pool: self,
            _permit: permit,
        }
    }
}

pub struct ReadConn<'db> {
    conn: Option<Connection>,
    pool: &'db ReaderPool,
    _permit: SemaphorePermit<'db>,
}

impl Deref for ReadConn<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("present until dropped")
    }
}

impl Drop for ReadConn<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.idle.lock().expect("reader pool lock").push(conn);
        }
    }
}

pub struct WriteTx<'db> {
    tx: Transaction,
    _gate: MutexGuard<'db, ()>,
}

impl WriteTx<'_> {
    pub async fn commit(self) -> Result<(), DbError> {
        self.tx.commit().await?;
        Ok(())
    }
}

impl Deref for WriteTx<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self.tx
    }
}

pub struct ReadOnlyDb {
    _database: Database,
    conn: Connection,
}

impl ReadOnlyDb {
    pub async fn open(path: &Path) -> Result<ReadOnlyDb, DbError> {
        let database = Builder::new_local(path)
            .flags(OpenFlags::SQLITE_OPEN_READ_ONLY)
            .build()
            .await?;
        let conn = never_close(database.connect()?);
        conn.busy_timeout(BUSY_TIMEOUT)?;
        conn.query("PRAGMA query_only = ON", ()).await?;
        Ok(ReadOnlyDb {
            _database: database,
            conn,
        })
    }

    pub async fn schema_status(&self) -> Result<SchemaStatus, DbError> {
        schema_status(&self.conn).await
    }

    pub async fn integrity_problems(&self) -> Result<Vec<String>, DbError> {
        let mut rows = self.conn.query("PRAGMA integrity_check", ()).await?;
        let mut problems = Vec::new();
        while let Some(row) = rows.next().await? {
            let line: String = row.get(0)?;
            if line != "ok" {
                problems.push(line);
            }
        }
        Ok(problems)
    }
}

impl Deref for ReadOnlyDb {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self.conn
    }
}

pub async fn open_file(path: &Path) -> Result<Connection, DbError> {
    let database = Builder::new_local(path).build().await?;
    connect(&database).await
}

async fn connect(database: &Database) -> Result<Connection, DbError> {
    let conn = never_close(database.connect()?);
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.execute("PRAGMA foreign_keys = ON", ()).await?;
    Ok(conn)
}

/// Workaround: libsql 0.9 closes the handle twice when the last reference to a connection
/// is dropped. If another thread has just opened a connection at the freed address, the
/// second close hits that one: its statements fail with `SQLITE_MISUSE`, or the process
/// aborts. Leaking one reference means the last one is never dropped.
///
/// Upstream: tursodatabase/libsql#2251, fixed by #2261 but unreleased as of 0.9.30
/// (release requested in #2307). Remove once liotek depends on a release with the fix.
fn never_close(conn: Connection) -> Connection {
    std::mem::forget(conn.clone());
    conn
}

async fn schema_status(conn: &Connection) -> Result<SchemaStatus, DbError> {
    let applied = applied_migrations(conn).await?;

    if let Some((found, _)) = applied
        .iter()
        .find(|(version, _)| *version > SUPPORTED_SCHEMA_VERSION)
    {
        return Err(DbError::SchemaTooNew {
            found: *found,
            supported: SUPPORTED_SCHEMA_VERSION,
        });
    }
    for migration in MIGRATIONS {
        let applied_checksum = applied.iter().find(|(version, _)| *version == migration.version);
        if applied_checksum.is_some_and(|(_, applied)| *applied != checksum(migration.sql)) {
            return Err(DbError::ChecksumMismatch {
                version: migration.version,
                name: migration.name.to_string(),
            });
        }
    }

    Ok(SchemaStatus {
        current: applied.iter().map(|(version, _)| *version).max().unwrap_or(0),
        pending: MIGRATIONS
            .iter()
            .map(|migration| migration.version)
            .filter(|version| !applied.iter().any(|(applied, _)| applied == version))
            .collect(),
    })
}

async fn applied_migrations(conn: &Connection) -> Result<Vec<(i64, String)>, DbError> {
    let mut exists = conn
        .query(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
            (),
        )
        .await?;
    if exists.next().await?.is_none() {
        return Ok(Vec::new());
    }

    let mut rows = conn
        .query(
            "SELECT version, checksum FROM schema_migrations ORDER BY version",
            (),
        )
        .await?;
    let mut applied = Vec::new();
    while let Some(row) = rows.next().await? {
        applied.push((row.get(0)?, row.get(1)?));
    }
    Ok(applied)
}

fn checksum(sql: &str) -> String {
    hex::encode(Sha256::digest(sql.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn open_in(dir: &tempfile::TempDir) -> Db {
        let config = DatabaseConfig {
            path: dir.path().join("test.db"),
        };
        Db::open(&config).await.unwrap()
    }

    #[tokio::test]
    async fn migrates_an_empty_database_then_has_nothing_to_do() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_in(&dir).await;

        assert_eq!(
            db.schema_status().await.unwrap(),
            SchemaStatus {
                current: 0,
                pending: vec![1, 2, 3]
            }
        );
        assert_eq!(db.migrate().await.unwrap(), vec![1, 2, 3]);
        assert_eq!(db.migrate().await.unwrap(), Vec::<i64>::new());
        assert_eq!(
            db.schema_status().await.unwrap().current,
            SUPPORTED_SCHEMA_VERSION
        );
    }

    #[tokio::test]
    async fn refuses_a_database_from_a_newer_build() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_in(&dir).await;
        db.migrate().await.unwrap();

        let tx = db.begin().await.unwrap();
        tx.execute(
            "INSERT INTO schema_migrations (version, name, checksum, build_revision) VALUES (99, 'x', 'x', 'x')",
            (),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert!(matches!(
            db.migrate().await,
            Err(DbError::SchemaTooNew { found: 99, .. })
        ));
    }

    #[tokio::test]
    async fn refuses_a_migration_that_was_edited_after_being_applied() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_in(&dir).await;
        db.migrate().await.unwrap();

        let tx = db.begin().await.unwrap();
        tx.execute(
            "UPDATE schema_migrations SET checksum = 'tampered' WHERE version = 2",
            (),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert!(matches!(
            db.schema_status().await,
            Err(DbError::ChecksumMismatch { version: 2, .. })
        ));
    }

    #[tokio::test]
    async fn an_uncommitted_write_is_rolled_back() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_in(&dir).await;
        db.migrate().await.unwrap();

        let tx = db.begin().await.unwrap();
        tx.execute("INSERT INTO authors (name) VALUES ('Ama Ata Aidoo')", ())
            .await
            .unwrap();
        drop(tx);

        let mut rows = db
            .read()
            .await
            .unwrap()
            .query("SELECT count(*) FROM authors", ())
            .await
            .unwrap();
        assert_eq!(rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(), 0);
    }

    #[tokio::test]
    async fn a_read_sees_commits_made_while_another_read_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_in(&dir).await;
        db.migrate().await.unwrap();
        insert_author(&db, "Ama Ata Aidoo").await;

        let first = db.read().await.unwrap();
        let mut open_rows = first.query("SELECT name FROM authors", ()).await.unwrap();
        open_rows.next().await.unwrap();

        insert_author(&db, "Ayi Kwei Armah").await;
        let second = db.read().await.unwrap();
        let mut rows = second.query("SELECT count(*) FROM authors", ()).await.unwrap();

        assert_eq!(rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(), 2);
    }

    async fn insert_author(db: &Db, name: &str) {
        let tx = db.begin().await.unwrap();
        tx.execute("INSERT INTO authors (name) VALUES (?)", [name])
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn read_only_handle_cannot_write() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_in(&dir).await;
        db.migrate().await.unwrap();

        let read_only = ReadOnlyDb::open(&dir.path().join("test.db")).await.unwrap();
        assert!(
            read_only
                .execute("INSERT INTO authors (name) VALUES ('x')", ())
                .await
                .is_err()
        );
        assert_eq!(
            read_only.schema_status().await.unwrap().pending,
            Vec::<i64>::new()
        );
    }
}
