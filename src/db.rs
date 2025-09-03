use crate::config::Config;
use crate::handler::HandlerParams;
use crate::model::*;
use anyhow::Result;
use libsql::{Builder, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

use std::env;

// const SCHEMA_SQL: &str = include_str!("schema.sql");

const MIGRATIONS: &[(&str, &str)] = &[
    ("001_schema.sql", include_str!("migrations/001_schema.sql")),
    (
        "002_seed_categories.sql",
        include_str!("migrations/002_seed_categories.sql"),
    ),
    (
        "003_seed_db.sql",
        include_str!("migrations/003_seed_db.sql"),
    ),
];

#[derive(Debug, Serialize, Deserialize)]
pub struct MetadataAggregate {
    pub authors: Vec<AuthorAggregate>,
    pub tags: Vec<TagAggregate>,
    pub categories: Vec<CategoryAggregate>,
    pub ratings: Vec<RatingAggregate>,
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
        Ok(vec![])
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
        let query = r#"
WITH 
author_count AS (
    SELECT authors.id, authors.name, COUNT(book_authors.id) as count 
    FROM authors 
    LEFT JOIN book_authors ON authors.id = book_authors.author_id
    GROUP BY authors.id, authors.name 
    HAVING count > 0
),
category_count AS (
    SELECT categories.id, categories.name, COUNT(book_categories.id) as count 
    FROM categories 
    LEFT JOIN book_categories ON categories.id = book_categories.category_id
    GROUP BY categories.id, categories.name 
    HAVING count > 0
),
tag_count AS (
    SELECT tags.id, tags.name, COUNT(book_tags.id) as count 
    FROM tags 
    LEFT JOIN book_tags ON tags.id = book_tags.tag_id
    GROUP BY tags.id, tags.name 
    HAVING count > 0
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
            let id = row
                .get(1)
                .map_err(|e| anyhow::anyhow!("failed to get id: {e}"))?;
            let name = row
                .get::<String>(2)
                .map_err(|e| anyhow::anyhow!("failed to get name: {e}"))?;
            let count = row
                .get(3)
                .map_err(|e| anyhow::anyhow!("failed to get count: {e}"))?;

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
                    rating: name
                        .parse()
                        .map_err(|e| anyhow::anyhow!("failed to parse rating: {e}"))?,
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
}
