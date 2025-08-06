use crate::config::Config;
use anyhow::Result;
use libsql::{Builder, Connection};
use std::path::Path;
use tracing::info;

use std::env;

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
        let db = Builder::new_local(path).build().await?;
        let conn = db.connect()?;
        match conn.query("SELECT 1", ()).await {
            Ok(_) => info!("established connection to db!"),
            Err(e) => return Err(anyhow::anyhow!("failed to connect to database: {}", e)),
        }

        Ok(Database { conn })
    }
}
