use std::path::{Path, PathBuf};

use libsql::Connection;
use serde::Serialize;

use crate::db::{Db, SNAPSHOT_SCHEMA};
use crate::error::{Error, Result};

/// Children come before their parents.
const OPERATIONAL_TABLES: &[&str] = &["upload_parts", "upload_sessions", "import_runs"];

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ExportSummary {
    pub path: PathBuf,
    #[serde(flatten)]
    pub counts: LibraryCounts,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct LibraryCounts {
    pub documents: i64,
    pub highlights: i64,
    pub comments: i64,
    pub notes: i64,
}

impl LibraryCounts {
    pub async fn read(conn: &Connection, schema: &str) -> Result<LibraryCounts> {
        Ok(LibraryCounts {
            documents: count(conn, schema, "documents").await?,
            highlights: count(conn, schema, "highlights").await?,
            comments: count(conn, schema, "comments").await?,
            notes: count(conn, schema, "notes").await?,
        })
    }
}

pub async fn export(db: &Db, output: &Path) -> Result<ExportSummary> {
    if output.exists() {
        return Err(Error::conflict(
            "export_target_exists",
            format!("{} already exists; choose a new path", output.display()),
        ));
    }
    let counts = db
        .snapshot_into(output, async |snapshot| {
            for table in OPERATIONAL_TABLES {
                snapshot
                    .execute(&format!("DELETE FROM {SNAPSHOT_SCHEMA}.{table}"), ())
                    .await?;
            }
            LibraryCounts::read(snapshot, SNAPSHOT_SCHEMA).await
        })
        .await;

    match counts {
        Ok(counts) => Ok(ExportSummary {
            path: output.to_path_buf(),
            counts,
        }),
        Err(error) => {
            let _ = std::fs::remove_file(output);
            Err(error)
        }
    }
}

async fn count(conn: &Connection, schema: &str, table: &str) -> Result<i64> {
    let mut rows = conn
        .query(&format!("SELECT count(*) FROM {schema}.{table}"), ())
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| anyhow::anyhow!("count(*) returned no row"))?;
    Ok(row.get(0)?)
}
