use crate::config::Config;
use crate::handler::HandlerParams;
use anyhow::{Context, Result};
use libsql::{Builder, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

use std::env;

const SCHEMA_SQL: &str = include_str!("schema.sql");

#[derive(Debug, Serialize, Deserialize)]
pub struct Book {
    pub id: i32,
    pub title: String,
    pub download_url: String,
    pub cover_url: String,
    // pub author: Author,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Author {
    pub id: i32,
    pub name: String,
}

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

    pub async fn get_books(&self, params: HandlerParams) -> Result<Vec<Book>> {
        let mut rows = self
            .conn
            .query(
                "SELECT books.id, books.title, books.url, books.cover_url, authors.id, authors.name FROM books LEFT JOIN authors on books.author_id=authors.id ORDER BY books.id LIMIT ? OFFSET ?",
                (params.limit, params.offset),
            )
            .await?;

        let mut books: Vec<Book> = vec![];

        while let Some(row) = rows.next().await? {
            books.push(Book {
                id: row.get(0)?,
                title: row.get(1)?,
                download_url: row.get(2)?,
                cover_url: row.get(3)?, // author: row.get("author"),
            });
        }

        Ok(books)
    }
}
