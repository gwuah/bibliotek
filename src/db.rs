use crate::config::Config;
use crate::handler::HandlerParams;
use crate::model::*;
use anyhow::Result;
use libsql::{Builder, Connection, Database as LibsqlDatabase};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::Path;
use std::time::Duration;
use tokio::sync::Mutex;

fn get_home_dir() -> Result<String> {
    Ok(env::var("HOME")?)
}

const SYSTEM_MIGRATIONS: &[(&str, &str)] =
    &[("system/000_migrations_table.sql", include_str!("migrations/system/000_migrations_table.sql"))];

const MIGRATIONS: &[(&str, &str)] = &[
    ("001_schema.sql", include_str!("migrations/001_schema.sql")),
    ("002_seed_categories.sql", include_str!("migrations/002_seed_categories.sql")),
    ("003_add_book_status.sql", include_str!("migrations/003_add_book_status.sql")),
];

#[derive(Debug, Serialize, Deserialize)]
pub struct MetadataAggregate {
    pub authors: Vec<AuthorAggregate>,
    pub tags: Vec<TagAggregate>,
    pub categories: Vec<CategoryAggregate>,
    pub ratings: Vec<RatingAggregate>,
}

pub struct Database {
    db: LibsqlDatabase,
    conn: Connection,
    tx_lock: Mutex<()>,
    turso_url: Option<String>,
    turso_auth_token: Option<String>,
}

impl Database {
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn is_replica(turso_url: &Option<String>, turso_auth_token: &Option<String>) -> bool {
        turso_url.is_some() && turso_auth_token.is_some()
    }

    pub async fn sync(&self) -> Result<()> {
        if Self::is_replica(&self.turso_url, &self.turso_auth_token) {
            self.db
                .sync()
                .await
                .map_err(|e| anyhow::anyhow!("sync failed: {}", e))?;
        }
        Ok(())
    }

    async fn is_migration_applied(conn: &Connection, name: &str) -> Result<bool> {
        let query = "SELECT 1 FROM _migrations WHERE name = ?";
        match conn.query(query, libsql::params![name]).await {
            Ok(mut rows) => Ok(rows.next().await?.is_some()),
            Err(e) => {
                if e.to_string().contains("no such table") {
                    Ok(false)
                } else {
                    Err(e.into())
                }
            }
        }
    }

    async fn record_migration(conn: &Connection, name: &str) -> Result<()> {
        let query = r#"
            INSERT INTO _migrations (name, applied_at)
            VALUES (?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        "#;
        match conn.execute(query, libsql::params![name]).await {
            Ok(_) => Ok(()),
            Err(e) => {
                if e.to_string().contains("no such table") {
                    Ok(())
                } else {
                    Err(e.into())
                }
            }
        }
    }

    async fn run_migration(conn: &Connection, name: &str, sql: &str) -> Result<()> {
        if Self::is_migration_applied(conn, name).await? {
            tracing::debug!("migration {} already applied, skipping", name);
            return Ok(());
        }

        tracing::info!("applying migration: {}", name);
        conn.execute_batch(sql)
            .await
            .map_err(|e| anyhow::anyhow!("failed to execute migration {name}: {e}"))?;

        Self::record_migration(conn, name).await?;
        Ok(())
    }

    pub async fn new(cfg: &Config) -> Result<Self> {
        let base_dir = env::var("MONO_DATA_DIR").unwrap_or_else(|_| get_home_dir().unwrap());
        let path = Path::new(&base_dir).join(cfg.app.get_db());
        let turso_url = cfg.app.turso_url.clone();
        let turso_auth_token = cfg.app.turso_auth_token.clone();

        let db = match (&turso_url, &turso_auth_token) {
            (Some(url), Some(token)) => {
                tracing::info!("[db] running in synced database mode (offline writes)");
                let sync_interval = Duration::from_secs(cfg.app.sync_interval_seconds);
                Builder::new_synced_database(&path, url.clone(), token.clone())
                    .sync_interval(sync_interval)
                    .build()
                    .await?
            }
            _ => Builder::new_local(&path).build().await?,
        };

        let conn = db.connect()?;
        conn.query("SELECT 1", ()).await?;

        for (filename, sql) in SYSTEM_MIGRATIONS {
            Self::run_migration(&conn, filename, sql).await?;
        }

        for (filename, sql) in MIGRATIONS {
            Self::run_migration(&conn, filename, sql).await?;
        }

        for (filename, sql) in crate::commonplace::migrations() {
            Self::run_migration(&conn, filename, sql).await?;
        }

        for (filename, sql) in crate::research::migrations() {
            Self::run_migration(&conn, filename, sql).await?;
        }

        Ok(Database {
            db,
            conn,
            tx_lock: Mutex::new(()),
            turso_url,
            turso_auth_token,
        })
    }

    fn split_comma_separated_string(s: String) -> Vec<String> {
        s.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub async fn get_books(&self, params: HandlerParams) -> Result<Vec<Book>> {
        let last_n_books = r#"
SELECT
    books.id as book_id,
    books.title,
    books.url,
    books.cover_url,
    books.ratings,
    books.description,
    books.pages,
    GROUP_CONCAT(DISTINCT CAST(authors.id AS TEXT)) as author_ids,
    GROUP_CONCAT(DISTINCT CAST(tags.id AS TEXT)) as tag_ids,
    GROUP_CONCAT(DISTINCT CAST(categories.id AS TEXT)) as category_ids
FROM books
LEFT JOIN book_authors ON book_authors.book_id = books.id
LEFT JOIN authors ON authors.id = book_authors.author_id
LEFT JOIN book_tags ON book_tags.book_id = books.id
LEFT JOIN tags ON tags.id = book_tags.tag_id
LEFT JOIN book_categories ON book_categories.book_id = books.id
LEFT JOIN categories ON categories.id = book_categories.category_id
GROUP BY books.id, books.title, books.url, books.cover_url, books.ratings
ORDER BY book_id
LIMIT ? OFFSET ?
"#;

        let search_books = r#"
SELECT
    books.id as book_id,
    books.title,
    books.url,
    books.cover_url,
    books.ratings,
    books.description,
    books.pages,
    GROUP_CONCAT(DISTINCT CAST(authors.id AS TEXT)) as author_ids,
    GROUP_CONCAT(DISTINCT CAST(tags.id AS TEXT)) as tag_ids,
    GROUP_CONCAT(DISTINCT CAST(categories.id AS TEXT)) as category_ids
FROM books
LEFT JOIN book_authors ON book_authors.book_id = books.id
LEFT JOIN authors ON authors.id = book_authors.author_id
LEFT JOIN book_tags ON book_tags.book_id = books.id
LEFT JOIN tags ON tags.id = book_tags.tag_id
LEFT JOIN book_categories ON book_categories.book_id = books.id
LEFT JOIN categories ON categories.id = book_categories.category_id
GROUP BY books.id, books.title, books.url, books.cover_url, books.ratings
WHERE books.title LIKE ? OR authors.name LIKE ? OR tags.name LIKE ? OR categories.name LIKE ?
ORDER BY book_id
LIMIT ? OFFSET ?
"#;

        let mut rows = if let Some(search) = &params.query {
            self.conn
                .query(
                    search_books,
                    (
                        format!("%{}%", search),
                        format!("%{}%", search),
                        format!("%{}%", search),
                        format!("%{}%", search),
                        params.limit as i32,
                        params.offset as i32,
                    ),
                )
                .await?
        } else {
            self.conn
                .query(last_n_books, (params.limit as i32, params.offset as i32))
                .await?
        };
        let mut books: Vec<Book> = vec![];

        while let Some(row) = rows.next().await? {
            let book_id: i32 = row.get(0)?;
            let book_title: String = row.get(1)?;
            let book_url: String = row.get(2)?;
            let book_cover_url: String = row.get::<Option<String>>(3)?.unwrap_or_default();
            let book_ratings: i32 = row.get::<Option<i32>>(4)?.unwrap_or(0);
            let book_description: String = row.get::<Option<String>>(5)?.unwrap_or_default();
            let book_pages: i32 = row.get::<Option<i32>>(6)?.unwrap_or(0);
            let book_authors_ids: String = row.get::<Option<String>>(7)?.unwrap_or_default();
            let book_tags_ids: String = row.get::<Option<String>>(8)?.unwrap_or_default();
            let book_categories_ids: String = row.get::<Option<String>>(9)?.unwrap_or_default();

            let book_authors = Self::split_comma_separated_string(book_authors_ids);
            let book_tags = Self::split_comma_separated_string(book_tags_ids);
            let book_categories = Self::split_comma_separated_string(book_categories_ids);

            books.push(Book {
                id: book_id,
                title: book_title,
                download_url: book_url,
                cover_url: book_cover_url,
                ratings: book_ratings,
                description: book_description,
                pages: book_pages,
                author_ids: book_authors,
                tag_ids: book_tags,
                category_ids: book_categories,
            });
        }

        Ok(books)
    }

    pub async fn get_metadata_aggregates(&self) -> Result<MetadataAggregate> {
        let query = r#"
WITH
author_count AS (
    SELECT authors.id, authors.name, COUNT(book_authors.id) as count
    FROM authors
    LEFT JOIN book_authors ON authors.id = book_authors.author_id
    GROUP BY authors.id, authors.name
),
category_count AS (
    SELECT categories.id, categories.name, COUNT(book_categories.id) as count
    FROM categories
    LEFT JOIN book_categories ON categories.id = book_categories.category_id
    GROUP BY categories.id, categories.name
),
tag_count AS (
    SELECT tags.id, tags.name, COUNT(book_tags.id) as count
    FROM tags
    LEFT JOIN book_tags ON tags.id = book_tags.tag_id
    GROUP BY tags.id, tags.name
),
ratings_count AS (
SELECT
    ROW_NUMBER() OVER (ORDER BY ratings) as id,
    cast(ratings as TEXT) as name,
    COUNT(*) as count
FROM books
WHERE ratings IS NOT NULL
GROUP BY ratings
ORDER BY ratings DESC
)
SELECT 'author' as type, id, name, count FROM author_count
UNION ALL
SELECT 'category' as type, id, name, count FROM category_count
UNION ALL
SELECT 'tag' as type, id, name, count FROM tag_count
UNION ALL
SELECT 'ratings' as type, id, name, count FROM ratings_count
ORDER BY type, count DESC;
        "#;

        let mut category_aggregates: Vec<CategoryAggregate> = vec![];
        let mut author_aggregates: Vec<AuthorAggregate> = vec![];
        let mut tag_aggregates: Vec<TagAggregate> = vec![];
        let mut ratings_aggregates: Vec<RatingAggregate> = vec![];

        let mut rows = self.conn.query(query, ()).await?;

        while let Some(row) = rows.next().await? {
            let aggregate_type = row
                .get::<String>(0)
                .map_err(|e| anyhow::anyhow!("failed to get type: {e}"))?;
            let id = row.get(1).map_err(|e| anyhow::anyhow!("failed to get id: {e}"))?;
            let name = row
                .get::<String>(2)
                .map_err(|e| anyhow::anyhow!("failed to get name: {e}"))?;
            let count = row.get(3).map_err(|e| anyhow::anyhow!("failed to get count: {e}"))?;

            match aggregate_type.as_str() {
                "author" => author_aggregates.push(AuthorAggregate {
                    author: Author { id, name },
                    count,
                }),
                "category" => category_aggregates.push(CategoryAggregate {
                    category: Category { id, name },
                    count,
                }),
                "tag" => tag_aggregates.push(TagAggregate {
                    tag: Tag { id, name },
                    count,
                }),
                "ratings" => ratings_aggregates.push(RatingAggregate {
                    rating: Rating { id, name: name },
                    count,
                }),
                _ => {
                    tracing::error!("invalid type: ->{}", aggregate_type);
                    continue;
                }
            }
        }

        Ok(MetadataAggregate {
            authors: author_aggregates,
            categories: category_aggregates,
            tags: tag_aggregates,
            ratings: ratings_aggregates,
        })
    }

    pub async fn get_book_by_id(&self, book_id: i32) -> Result<Option<Book>> {
        let query = r#"
SELECT
    books.id as book_id,
    books.title,
    books.url,
    books.cover_url,
    books.ratings,
    books.description,
    books.pages,
    GROUP_CONCAT(DISTINCT CAST(authors.id AS TEXT)) as author_ids,
    GROUP_CONCAT(DISTINCT CAST(tags.id AS TEXT)) as tag_ids,
    GROUP_CONCAT(DISTINCT CAST(categories.id AS TEXT)) as category_ids
FROM books
LEFT JOIN book_authors ON book_authors.book_id = books.id
LEFT JOIN authors ON authors.id = book_authors.author_id
LEFT JOIN book_tags ON book_tags.book_id = books.id
LEFT JOIN tags ON tags.id = book_tags.tag_id
LEFT JOIN book_categories ON book_categories.book_id = books.id
LEFT JOIN categories ON categories.id = book_categories.category_id
WHERE books.id = ?
GROUP BY books.id, books.title, books.url, books.cover_url, books.ratings
"#;

        let mut rows = self.conn.query(query, libsql::params![book_id]).await?;

        if let Some(row) = rows.next().await? {
            let book_id: i32 = row.get(0)?;
            let book_title: String = row.get(1)?;
            let book_url: String = row.get(2)?;
            let book_cover_url: String = row.get::<Option<String>>(3)?.unwrap_or_default();
            let book_ratings: i32 = row.get::<Option<i32>>(4)?.unwrap_or(0);
            let book_description: String = row.get::<Option<String>>(5)?.unwrap_or_default();
            let book_pages: i32 = row.get::<Option<i32>>(6)?.unwrap_or(0);
            let book_authors_ids: String = row.get::<Option<String>>(7)?.unwrap_or_default();
            let book_tags_ids: String = row.get::<Option<String>>(8)?.unwrap_or_default();
            let book_categories_ids: String = row.get::<Option<String>>(9)?.unwrap_or_default();

            let book_authors = Self::split_comma_separated_string(book_authors_ids);
            let book_tags = Self::split_comma_separated_string(book_tags_ids);
            let book_categories = Self::split_comma_separated_string(book_categories_ids);

            Ok(Some(Book {
                id: book_id,
                title: book_title,
                download_url: book_url,
                cover_url: book_cover_url,
                ratings: book_ratings,
                description: book_description,
                pages: book_pages,
                author_ids: book_authors,
                tag_ids: book_tags,
                category_ids: book_categories,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_or_create_author(&self, name: &str) -> Result<i32> {
        let insert_query = "INSERT OR IGNORE INTO authors (name) VALUES (?)";
        self.conn.execute(insert_query, libsql::params![name]).await?;

        let select_query = "SELECT id FROM authors WHERE name = ? LIMIT 1";
        let mut rows = self.conn.query(select_query, libsql::params![name]).await?;

        if let Some(row) = rows.next().await? {
            Ok(row.get(0)?)
        } else {
            anyhow::bail!("Failed to get or create author: {}", name)
        }
    }

    pub async fn get_or_create_tag(&self, name: &str) -> Result<i32> {
        let insert_query = "INSERT OR IGNORE INTO tags (name) VALUES (?)";
        self.conn.execute(insert_query, libsql::params![name]).await?;

        let select_query = "SELECT id FROM tags WHERE name = ? LIMIT 1";
        let mut rows = self.conn.query(select_query, libsql::params![name]).await?;

        if let Some(row) = rows.next().await? {
            Ok(row.get(0)?)
        } else {
            anyhow::bail!("Failed to get or create tag: {}", name)
        }
    }

    pub async fn get_or_create_category(&self, name: &str) -> Result<i32> {
        let insert_query = "INSERT OR IGNORE INTO categories (name) VALUES (?)";
        self.conn.execute(insert_query, libsql::params![name]).await?;

        let select_query = "SELECT id FROM categories WHERE name = ? LIMIT 1";
        let mut rows = self.conn.query(select_query, libsql::params![name]).await?;

        if let Some(row) = rows.next().await? {
            Ok(row.get(0)?)
        } else {
            anyhow::bail!("Failed to get or create category: {}", name)
        }
    }

    pub async fn create_book(
        &self,
        title: &str,
        url: &str,
        cover_url: Option<&str>,
        description: Option<&str>,
        pages: Option<i32>,
        ratings: Option<i32>,
        author_names: &[String],
        tag_names: &[String],
        category_names: &[String],
        status: &str,
    ) -> Result<i32> {
        let _guard = self.tx_lock.lock().await;

        self.conn.execute("BEGIN TRANSACTION", ()).await?;

        let result = self
            .create_book_internal(
                title,
                url,
                cover_url,
                description,
                pages,
                ratings,
                author_names,
                tag_names,
                category_names,
                status,
            )
            .await;

        match result {
            Ok(book_id) => {
                self.conn.execute("COMMIT", ()).await?;
                Ok(book_id)
            }
            Err(e) => {
                let _ = self.conn.execute("ROLLBACK", ()).await;
                Err(e)
            }
        }
    }

    async fn create_book_internal(
        &self,
        title: &str,
        url: &str,
        cover_url: Option<&str>,
        description: Option<&str>,
        pages: Option<i32>,
        ratings: Option<i32>,
        author_names: &[String],
        tag_names: &[String],
        category_names: &[String],
        status: &str,
    ) -> Result<i32> {
        let insert_book = r#"
            INSERT INTO books (title, url, cover_url, description, pages, ratings, status)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            RETURNING id
        "#;

        let mut rows = self
            .conn
            .query(insert_book, libsql::params![title, url, cover_url, description, pages, ratings, status])
            .await?;

        let book_id: i32 = if let Some(row) = rows.next().await? {
            row.get(0)?
        } else {
            anyhow::bail!("Failed to create book")
        };

        for author_name in author_names {
            let author_id = self.get_or_create_author(author_name).await?;
            let link_query = "INSERT OR IGNORE INTO book_authors (book_id, author_id) VALUES (?, ?)";
            self.conn
                .execute(link_query, libsql::params![book_id, author_id])
                .await?;
        }

        for tag_name in tag_names {
            let tag_id = self.get_or_create_tag(tag_name).await?;
            let link_query = "INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?, ?)";
            self.conn.execute(link_query, libsql::params![book_id, tag_id]).await?;
        }

        for category_name in category_names {
            let category_id = self.get_or_create_category(category_name).await?;
            let link_query = "INSERT OR IGNORE INTO book_categories (book_id, category_id) VALUES (?, ?)";
            self.conn
                .execute(link_query, libsql::params![book_id, category_id])
                .await?;
        }

        Ok(book_id)
    }

    pub async fn update_book(
        &self,
        book_id: i32,
        title: &str,
        author_ids: &[i32],
        tag_ids: &[i32],
        category_ids: &[i32],
    ) -> Result<()> {
        let _guard = self.tx_lock.lock().await;

        self.conn.execute("BEGIN TRANSACTION", ()).await?;

        let result = self
            .update_book_internal(book_id, title, author_ids, tag_ids, category_ids)
            .await;

        match result {
            Ok(_) => {
                self.conn.execute("COMMIT", ()).await?;
                Ok(())
            }
            Err(e) => {
                let _ = self.conn.execute("ROLLBACK", ()).await;
                Err(e)
            }
        }
    }

    async fn update_book_internal(
        &self,
        book_id: i32,
        title: &str,
        author_ids: &[i32],
        tag_ids: &[i32],
        category_ids: &[i32],
    ) -> Result<()> {
        self.conn
            .execute(
                "UPDATE books SET title = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
                libsql::params![title, book_id],
            )
            .await?;

        self.conn
            .execute("DELETE FROM book_authors WHERE book_id = ?", libsql::params![book_id])
            .await?;
        for author_id in author_ids {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO book_authors (book_id, author_id) VALUES (?, ?)",
                    libsql::params![book_id, *author_id],
                )
                .await?;
        }

        self.conn
            .execute("DELETE FROM book_tags WHERE book_id = ?", libsql::params![book_id])
            .await?;
        for tag_id in tag_ids {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?, ?)",
                    libsql::params![book_id, *tag_id],
                )
                .await?;
        }

        self.conn
            .execute("DELETE FROM book_categories WHERE book_id = ?", libsql::params![book_id])
            .await?;
        for category_id in category_ids {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO book_categories (book_id, category_id) VALUES (?, ?)",
                    libsql::params![book_id, *category_id],
                )
                .await?;
        }

        Ok(())
    }

    pub async fn create_author(&self, name: &str) -> Result<Author> {
        self.conn
            .execute("INSERT INTO authors (name) VALUES (?)", libsql::params![name])
            .await?;
        let mut rows = self
            .conn
            .query("SELECT id, name FROM authors WHERE name = ?", libsql::params![name])
            .await?;
        if let Some(row) = rows.next().await? {
            Ok(Author {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        } else {
            anyhow::bail!("Failed to create author")
        }
    }

    pub async fn create_tag(&self, name: &str) -> Result<Tag> {
        self.conn
            .execute("INSERT INTO tags (name) VALUES (?)", libsql::params![name])
            .await?;
        let mut rows = self
            .conn
            .query("SELECT id, name FROM tags WHERE name = ?", libsql::params![name])
            .await?;
        if let Some(row) = rows.next().await? {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        } else {
            anyhow::bail!("Failed to create tag")
        }
    }

    pub async fn create_category(&self, name: &str) -> Result<Category> {
        self.conn
            .execute("INSERT INTO categories (name) VALUES (?)", libsql::params![name])
            .await?;
        let mut rows = self
            .conn
            .query("SELECT id, name FROM categories WHERE name = ?", libsql::params![name])
            .await?;
        if let Some(row) = rows.next().await? {
            Ok(Category {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        } else {
            anyhow::bail!("Failed to create category")
        }
    }

    pub async fn update_book_status(&self, book_id: i32, status: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE books SET status = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
                libsql::params![status, book_id],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_book(&self, book_id: i32) -> Result<()> {
        let _guard = self.tx_lock.lock().await;

        self.conn.execute("BEGIN TRANSACTION", ()).await?;

        let result = async {
            self.conn
                .execute("DELETE FROM book_authors WHERE book_id = ?", libsql::params![book_id])
                .await?;
            self.conn
                .execute("DELETE FROM book_tags WHERE book_id = ?", libsql::params![book_id])
                .await?;
            self.conn
                .execute("DELETE FROM book_categories WHERE book_id = ?", libsql::params![book_id])
                .await?;
            self.conn
                .execute("DELETE FROM books WHERE id = ?", libsql::params![book_id])
                .await?;
            Ok::<(), anyhow::Error>(())
        }
        .await;

        match result {
            Ok(_) => {
                self.conn.execute("COMMIT", ()).await?;
                Ok(())
            }
            Err(e) => {
                let _ = self.conn.execute("ROLLBACK", ()).await;
                Err(e)
            }
        }
    }
}
