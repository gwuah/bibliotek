use crate::config::Config;
use crate::handler::HandlerParams;
use anyhow::Result;
use libsql::{Builder, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

use std::env;

// const SCHEMA_SQL: &str = include_str!("schema.sql");

const MIGRATIONS: &[(&str, &str)] = &[
    ("001_schema.sql", include_str!("migrations/001_schema.sql")),
    (
        "002_categories.sql",
        include_str!("migrations/002_categories.sql"),
    ),
];

#[derive(Debug, Serialize, Deserialize)]
pub struct Book {
    pub id: i32,
    pub title: String,
    pub download_url: String,
    pub cover_url: String,
    pub ratings: Option<i32>,
    pub author: Option<Author>,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Author {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tag {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetadataAggregate {
    pub authors: Vec<AuthorAggregate>,
    pub tags: Vec<TagAggregate>,
    pub ratings: Vec<RatingAggregate>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthorAggregate {
    pub author: Author,
    pub book_count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagAggregate {
    pub tag: Tag,
    pub book_count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RatingAggregate {
    pub rating: i32,
    pub book_count: i32,
}

pub struct Database {
    conn: Connection,
}

fn get_home_dir() -> Result<String> {
    Ok(env::var("HOME")?)
}

impl Database {
    pub async fn new(cfg: &Config) -> Result<Self> {
        let path = Path::new(&get_home_dir()?).join(cfg.app.get_db());
        let conn = Builder::new_local(path).build().await?.connect()?;
        let _ = conn
            .query("SELECT 1", ())
            .await
            .map_err(|e| anyhow::anyhow!("failed to connect to database: {e}"))?;
        tracing::debug!("established connection to db!");

        for (filename, sql) in MIGRATIONS {
            tracing::info!("executing migration: {}", filename);
            conn.execute_batch(sql)
                .await
                .map_err(|e| anyhow::anyhow!("failed to execute migration {filename}: {e}"))?;
        }

        tracing::info!("db migrations complete");

        let instance = Database { conn };

        Ok(instance)
    }

    pub async fn get_books(&self, params: HandlerParams) -> Result<Vec<Book>> {
        let mut rows = if let Some(search) = &params.query {
            self.conn.query(
                "SELECT books.id, books.title, books.url, books.cover_url, books.ratings, authors.id, authors.name FROM books LEFT JOIN authors on books.author_id=authors.id WHERE books.title LIKE ? ORDER BY books.id LIMIT ? OFFSET ?",
                (format!("%{}%", search), params.limit as i32, params.offset as i32)
            ).await?
        } else {
            self.conn.query(
                "SELECT books.id, books.title, books.url, books.cover_url, books.ratings, authors.id, authors.name FROM books LEFT JOIN authors on books.author_id=authors.id ORDER BY books.id LIMIT ? OFFSET ?",
                (params.limit as i32, params.offset as i32)
            ).await?
        };
        let mut books: Vec<Book> = vec![];

        while let Some(row) = rows.next().await? {
            let book_id: i32 = row.get(0)?;
            let author = if let (Some(author_id), Some(author_name)) =
                (row.get::<Option<i32>>(5)?, row.get::<Option<String>>(6)?)
            {
                Some(Author {
                    id: author_id,
                    name: author_name,
                })
            } else {
                None
            };

            let tags = self.get_book_tags(book_id).await?;

            books.push(Book {
                id: book_id,
                title: row.get(1)?,
                download_url: row.get(2)?,
                cover_url: row.get(3)?,
                ratings: row.get(4)?,
                author,
                tags,
            });
        }

        Ok(books)
    }

    pub async fn get_book_tags(&self, book_id: i32) -> Result<Vec<Tag>> {
        let mut rows = self
            .conn
            .query(
                "SELECT tags.id, tags.name FROM tags INNER JOIN book_tags ON tags.id = book_tags.tag_id WHERE book_tags.book_id = ?",
                [book_id],
            )
            .await?;

        let mut tags: Vec<Tag> = vec![];
        while let Some(row) = rows.next().await? {
            tags.push(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            });
        }

        Ok(tags)
    }

    pub async fn get_metadata_aggregates(&self) -> Result<MetadataAggregate> {
        let authors = self.get_author_aggregates().await?;
        let tags = self.get_tag_aggregates().await?;
        let ratings = self.get_rating_aggregates().await?;

        Ok(MetadataAggregate {
            authors,
            tags,
            ratings,
        })
    }

    pub async fn get_author_aggregates(&self) -> Result<Vec<AuthorAggregate>> {
        let mut rows = self
            .conn
            .query(
                "SELECT authors.id, authors.name,
                 COUNT(books.id) as book_count FROM authors LEFT JOIN books ON authors.id = books.author_id GROUP BY authors.id, authors.name HAVING book_count > 0 ORDER BY book_count DESC",
                (),
            )
            .await?;

        let mut aggregates: Vec<AuthorAggregate> = vec![];
        while let Some(row) = rows.next().await? {
            aggregates.push(AuthorAggregate {
                author: Author {
                    id: row.get(0)?,
                    name: row.get(1)?,
                },
                book_count: row.get(2)?,
            });
        }

        Ok(aggregates)
    }

    pub async fn get_tag_aggregates(&self) -> Result<Vec<TagAggregate>> {
        let mut rows = self
            .conn
            .query(
                "SELECT tags.id, tags.name, COUNT(book_tags.book_id) as book_count FROM tags LEFT JOIN book_tags ON tags.id = book_tags.tag_id GROUP BY tags.id, tags.name HAVING book_count > 0 ORDER BY book_count DESC",
                (),
            )
            .await?;

        let mut aggregates: Vec<TagAggregate> = vec![];
        while let Some(row) = rows.next().await? {
            aggregates.push(TagAggregate {
                tag: Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                },
                book_count: row.get(2)?,
            });
        }

        Ok(aggregates)
    }

    pub async fn get_rating_aggregates(&self) -> Result<Vec<RatingAggregate>> {
        let mut rows = self
            .conn
            .query(
                "SELECT ratings, COUNT(*) as book_count FROM books WHERE ratings IS NOT NULL GROUP BY ratings ORDER BY ratings DESC",
                (),
            )
            .await?;

        let mut aggregates: Vec<RatingAggregate> = vec![];
        while let Some(row) = rows.next().await? {
            aggregates.push(RatingAggregate {
                rating: row.get(0)?,
                book_count: row.get(1)?,
            });
        }

        Ok(aggregates)
    }
}
