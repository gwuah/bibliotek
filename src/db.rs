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

    fn split_comma_separated_string(s: String) -> Vec<String> {
        s.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    // pub async fn pre_insert_book(&self, book: Book) -> Result<()> {
    //     let query = r#"
    //     INSERT INTO books (title, ratings, pages)
    //     VALUES (?, ?, ?);
    //     "#;

    //     self.conn
    //         .execute(query, (book.title, book.ratings, book.pages))
    //         .await?;

    //     Ok(())
    // }

    // pub async fn post_insert_book(&self, book: Book) -> Result<()> {
    //     let query = r#"
    //     UPDATE books SET url = ?, cover_url = ?, description = ?, author_ids = ?, tag_ids = ?, category_ids = ? WHERE id = ?;
    //     "#;

    //     self.conn
    //         .execute(query, (book.title, book.ratings, book.pages))
    //         .await?;

    //     Ok(())
    // }

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
            // println!("Col/mn {:?}", row.get_value(0));

            let book_id: i32 = row.get(0)?;
            let book_title: String = row.get(1)?;
            let book_url: String = row.get(2)?;
            let book_cover_url: String = row.get(3)?;
            let book_ratings: i32 = row.get(4)?;
            let book_description: String = row.get(5)?;
            let book_pages: i32 = row.get(6)?;
            let book_authors_ids: String = row.get::<String>(7)?;
            let book_tags_ids: String = row.get::<String>(8)?;
            let book_categories_ids: String = row.get::<String>(9)?;

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
}
