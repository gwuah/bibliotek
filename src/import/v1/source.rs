use std::path::Path;

use libsql::{Connection, Row};

use crate::db::ReadOnlyDb;
use crate::error::{Error, Result};

const REQUIRED_COLUMNS: &[(&str, &[&str])] = &[
    ("_migrations", &["name", "applied_at"]),
    (
        "books",
        &[
            "id",
            "title",
            "url",
            "cover_url",
            "ratings",
            "description",
            "pages",
            "status",
            "created_at",
            "updated_at",
        ],
    ),
    ("authors", &["id", "name"]),
    ("book_authors", &["id", "book_id", "author_id"]),
    ("categories", &["id", "name", "parent_id"]),
    ("book_categories", &["id", "book_id", "category_id"]),
    ("tags", &["id", "name", "parent_id"]),
    ("book_tags", &["id", "book_id", "tag_id"]),
    (
        "resources",
        &[
            "id",
            "title",
            "type",
            "external_id",
            "config",
            "deleted_at",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "annotations",
        &[
            "id",
            "resource_id",
            "text",
            "color",
            "boundary",
            "external_id",
            "deleted_at",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "comments",
        &[
            "id",
            "annotation_id",
            "content",
            "external_id",
            "deleted_at",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "notes",
        &[
            "id",
            "resource_id",
            "content",
            "external_id",
            "deleted_at",
            "created_at",
            "updated_at",
        ],
    ),
    ("words", &["id"]),
];

pub const STALE_TABLES: &[&str] = &["reader_settings", "page_texts"];

pub struct V1 {
    pub migrations: Vec<AppliedMigration>,
    pub books: Vec<Book>,
    pub authors: Vec<Author>,
    pub book_authors: Vec<Link>,
    pub categories: Vec<Label>,
    pub book_categories: Vec<Link>,
    pub tags: Vec<Label>,
    pub book_tags: Vec<Link>,
    pub resources: Vec<Resource>,
    pub annotations: Vec<Annotation>,
    pub comments: Vec<Comment>,
    pub notes: Vec<Note>,
    pub words: u64,
    pub stale_tables: Vec<(&'static str, u64)>,
    pub research_config: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppliedMigration {
    pub name: String,
    pub applied_at: String,
}

pub struct Book {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub cover_url: Option<String>,
    pub ratings: Option<i64>,
    pub description: Option<String>,
    pub pages: Option<i64>,
    pub status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct Author {
    pub id: i64,
    pub name: String,
}

pub struct Link {
    pub book_id: Option<i64>,
    pub target_id: Option<i64>,
}

pub struct Label {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
}

pub struct Resource {
    pub id: i64,
    pub title: String,
    pub kind: String,
    pub external_id: Option<String>,
    pub config: Option<String>,
    pub deleted: bool,
    pub created_at: String,
}

pub struct Annotation {
    pub id: i64,
    pub resource_id: i64,
    pub text: String,
    pub color: Option<String>,
    pub boundary: Option<String>,
    pub external_id: Option<String>,
    pub deleted: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub struct Text {
    pub id: i64,
    pub parent_id: i64,
    pub content: String,
    pub external_id: Option<String>,
    pub deleted: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub type Comment = Text;
pub type Note = Text;

impl V1 {
    pub async fn read(path: &Path) -> Result<V1> {
        if !path.is_file() {
            return Err(Error::invalid(
                "snapshot_missing",
                format!("{} is not a file", path.display()),
            ));
        }
        let snapshot = ReadOnlyDb::open(path).await?;
        check_schema(&snapshot).await?;

        let read = snapshot.transaction().await?;
        let v1 = V1 {
            migrations: select(&read, "SELECT name, applied_at FROM _migrations ORDER BY id", |row| {
                Ok(AppliedMigration {
                    name: row.get(0)?,
                    applied_at: row.get(1)?,
                })
            })
            .await?,
            books: select(
                &read,
                "SELECT id, title, url, cover_url, ratings, description, pages, status, created_at, updated_at
                 FROM books ORDER BY id",
                |row| {
                    Ok(Book {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        url: row.get(2)?,
                        cover_url: row.get(3)?,
                        ratings: row.get(4)?,
                        description: row.get(5)?,
                        pages: row.get(6)?,
                        status: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                    })
                },
            )
            .await?,
            authors: select(&read, "SELECT id, name FROM authors ORDER BY id", |row| {
                Ok(Author {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })
            .await?,
            book_authors: links(&read, "SELECT book_id, author_id FROM book_authors ORDER BY id").await?,
            categories: labels(&read, "SELECT id, name, parent_id FROM categories ORDER BY id").await?,
            book_categories: links(&read, "SELECT book_id, category_id FROM book_categories ORDER BY id").await?,
            tags: labels(&read, "SELECT id, name, parent_id FROM tags ORDER BY id").await?,
            book_tags: links(&read, "SELECT book_id, tag_id FROM book_tags ORDER BY id").await?,
            resources: select(
                &read,
                "SELECT id, title, type, external_id, config, deleted_at IS NOT NULL, created_at
                 FROM resources ORDER BY id",
                |row| {
                    Ok(Resource {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        kind: row.get(2)?,
                        external_id: row.get(3)?,
                        config: row.get(4)?,
                        deleted: row.get::<i64>(5)? != 0,
                        created_at: row.get(6)?,
                    })
                },
            )
            .await?,
            annotations: select(
                &read,
                "SELECT id, resource_id, text, color, boundary, external_id, deleted_at IS NOT NULL,
                        created_at, updated_at
                 FROM annotations ORDER BY id",
                |row| {
                    Ok(Annotation {
                        id: row.get(0)?,
                        resource_id: row.get(1)?,
                        text: row.get(2)?,
                        color: row.get(3)?,
                        boundary: row.get(4)?,
                        external_id: row.get(5)?,
                        deleted: row.get::<i64>(6)? != 0,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                },
            )
            .await?,
            comments: texts(
                &read,
                "SELECT id, annotation_id, content, external_id, deleted_at IS NOT NULL, created_at, updated_at
                 FROM comments ORDER BY id",
            )
            .await?,
            notes: texts(
                &read,
                "SELECT id, resource_id, content, external_id, deleted_at IS NOT NULL, created_at, updated_at
                 FROM notes ORDER BY id",
            )
            .await?,
            words: count(&read, "SELECT count(*) FROM words").await?,
            stale_tables: stale_tables(&read).await?,
            research_config: research_config(&read).await?,
        };
        Ok(v1)
    }
}

async fn check_schema(conn: &Connection) -> Result<()> {
    for (table, required) in REQUIRED_COLUMNS {
        let columns = columns_of(conn, table).await?;
        if let Some(missing) = required
            .iter()
            .find(|column| !columns.iter().any(|c| c == **column))
        {
            return Err(Error::invalid(
                "v1_schema_incompatible",
                format!(
                    "this is not a Bibliotek database this importer understands: `{table}.{missing}` is missing"
                ),
            ));
        }
    }
    Ok(())
}

async fn columns_of(conn: &Connection, table: &str) -> Result<Vec<String>> {
    select(
        conn,
        &format!("SELECT name FROM pragma_table_info('{table}')"),
        |row| Ok(row.get(0)?),
    )
    .await
}

async fn stale_tables(conn: &Connection) -> Result<Vec<(&'static str, u64)>> {
    let mut present = Vec::new();
    for table in STALE_TABLES {
        if !columns_of(conn, table).await?.is_empty() {
            present.push((
                *table,
                count(conn, &format!("SELECT count(*) FROM {table}")).await?,
            ));
        }
    }
    Ok(present)
}

async fn research_config(conn: &Connection) -> Result<Option<String>> {
    if !columns_of(conn, "research_config")
        .await?
        .iter()
        .any(|c| c == "db_path")
    {
        return Ok(None);
    }
    let paths = select(conn, "SELECT db_path FROM research_config", |row| Ok(row.get(0)?)).await?;
    Ok(paths.into_iter().next())
}

async fn links(conn: &Connection, sql: &str) -> Result<Vec<Link>> {
    select(conn, sql, |row| {
        Ok(Link {
            book_id: row.get(0)?,
            target_id: row.get(1)?,
        })
    })
    .await
}

async fn labels(conn: &Connection, sql: &str) -> Result<Vec<Label>> {
    select(conn, sql, |row| {
        Ok(Label {
            id: row.get(0)?,
            name: row.get(1)?,
            parent_id: row.get(2)?,
        })
    })
    .await
}

async fn texts(conn: &Connection, sql: &str) -> Result<Vec<Text>> {
    select(conn, sql, |row| {
        Ok(Text {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            content: row.get(2)?,
            external_id: row.get(3)?,
            deleted: row.get::<i64>(4)? != 0,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })
    .await
}

async fn count(conn: &Connection, sql: &str) -> Result<u64> {
    let counts = select(conn, sql, |row| Ok(row.get::<i64>(0)?)).await?;
    Ok(counts.first().copied().unwrap_or(0).max(0) as u64)
}

async fn select<T>(conn: &Connection, sql: &str, map: impl Fn(&Row) -> Result<T>) -> Result<Vec<T>> {
    let mut rows = conn.query(sql, ()).await?;
    let mut mapped = Vec::new();
    while let Some(row) = rows.next().await? {
        mapped.push(map(&row)?);
    }
    Ok(mapped)
}
