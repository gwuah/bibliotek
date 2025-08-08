use crate::config::Config;
use anyhow::{Context, Result};
use libsql::{Builder, Connection};
use std::path::Path;

use std::env;

const SCHEMA_SQL: &str = include_str!("schema.sql");

pub struct Database {
    conn: Connection,
}

fn get_home_dir() -> Result<String> {
    Ok(env::var("HOME")?)
}

impl Database {
    pub async fn new(cfg: &Config) -> Result<Self> {
        let home = get_home_dir()?;
        let path = Path::new::<String>(&home).join(cfg.app.get_db());
        let conn = Builder::new_local(path).build().await?.connect()?;
        let _ = conn
            .query("SELECT 1", ())
            .await
            .context("failed to connect to databas")?;
        tracing::debug!("established connection to db!");

        conn.execute_batch(SCHEMA_SQL)
            .await
            .context("failed to execute schema.sql")?;
        tracing::info!("db migration complete");

        let instance = Database { conn };

        Ok(instance)
    }
}
