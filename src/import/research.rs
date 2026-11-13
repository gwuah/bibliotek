//! Research's SQLite file is opened read-only and read in one read transaction, so a
//! concurrent write cannot produce a half-old, half-new snapshot. A missing table or
//! column is an incompatible source, never a reason to import less.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use libsql::{Connection, Row, Value};
use serde_json::{Value as Json, json};

use super::snapshot::{
    Location, Snapshot, SourceComment, SourceDocument, SourceHighlight, SourceNote, SourceTimes,
};
use crate::db::ReadOnlyDb;
use crate::domain::documents::{DocumentKind, source};
use crate::error::{Error, Result};

pub const LOCATION_KIND: &str = "pdf_region";

const REQUIRED_COLUMNS: &[(&str, &[&str])] = &[
    ("items", &["id", "title"]),
    ("annotations", &["id", "item_id", "content", "color", "position"]),
    ("comments", &["id", "annotation_id", "content"]),
    ("notes", &["id", "item_id", "content"]),
];

pub fn location(position: Option<&str>) -> Result<Location> {
    let position = match position {
        None => Json::Null,
        Some(text) => serde_json::from_str(text).map_err(|error| {
            Error::invalid(
                "research_malformed",
                format!("a highlight position is not JSON: {error}"),
            )
        })?,
    };
    let page_number = position
        .pointer("/boundingRect/pageNumber")
        .cloned()
        .unwrap_or(Json::Null);
    Ok(Location::new(
        LOCATION_KIND,
        json!({ "page_number": page_number, "position": position }),
    ))
}

/// Area highlights have no text, which is stored as empty text.
pub fn highlight_text(content: Option<&str>) -> Result<String> {
    let Some(content) = content else {
        return Ok(String::new());
    };
    let content: Json = serde_json::from_str(content).map_err(|error| {
        Error::invalid(
            "research_malformed",
            format!("a highlight's content is not JSON: {error}"),
        )
    })?;
    Ok(content["text"].as_str().unwrap_or_default().to_string())
}

pub fn document(item_id: String, title: Option<&str>) -> SourceDocument {
    SourceDocument {
        external_id: item_id,
        kind: DocumentKind::Pdf,
        title: title
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or("Untitled Research item")
            .to_string(),
        canonical_url: None,
        highlights: Vec::new(),
        notes: Vec::new(),
    }
}

pub async fn read(path: &Path) -> Result<Snapshot> {
    let research = open(path).await?;
    let schema = Schema::inspect(&research).await?;
    let read = research.transaction().await?;
    let rows = Rows::read(&read, &schema).await?;
    drop(read);

    Snapshot::new(source::RESEARCH, rows.assemble()?)
}

async fn open(path: &Path) -> Result<ReadOnlyDb> {
    if !path.is_file() {
        return Err(Error::conflict(
            "research_database_missing",
            format!("the Research database {} does not exist", path.display()),
        ));
    }
    let unreadable = |error: &dyn std::fmt::Display| {
        Error::conflict(
            "research_database_unreadable",
            format!("cannot read the Research database: {error}"),
        )
    };
    let research = ReadOnlyDb::open(path).await.map_err(|error| unreadable(&error))?;
    research
        .query("SELECT count(*) FROM sqlite_master", ())
        .await
        .map_err(|error| unreadable(&error))?;
    Ok(research)
}

struct Schema {
    soft_deletes: HashSet<&'static str>,
}

impl Schema {
    async fn inspect(conn: &Connection) -> Result<Schema> {
        let mut soft_deletes = HashSet::new();
        for (table, required) in REQUIRED_COLUMNS {
            let columns = columns_of(conn, table).await?;
            if columns.is_empty() {
                return Err(incompatible(format!("it has no `{table}` table")));
            }
            if let Some(missing) = required.iter().find(|column| !columns.contains(**column)) {
                return Err(incompatible(format!("`{table}` has no `{missing}` column")));
            }
            if columns.contains("deleted_at") {
                soft_deletes.insert(*table);
            }
        }
        Ok(Schema { soft_deletes })
    }

    fn deleted(&self, table: &str) -> &'static str {
        if self.soft_deletes.contains(table) {
            "deleted_at IS NOT NULL"
        } else {
            "0"
        }
    }
}

async fn columns_of(conn: &Connection, table: &str) -> Result<HashSet<String>> {
    let mut rows = conn
        .query("SELECT name FROM pragma_table_info(?)", [table])
        .await?;
    let mut columns = HashSet::new();
    while let Some(row) = rows.next().await? {
        columns.insert(row.get(0)?);
    }
    Ok(columns)
}

fn incompatible(problem: String) -> Error {
    Error::conflict(
        "research_schema_incompatible",
        format!("the Research database is not in a shape liotek can read: {problem}"),
    )
}

struct ItemRow {
    id: String,
    title: Option<String>,
    deleted: bool,
}

struct AnnotationRow {
    id: String,
    item_id: String,
    content: Option<String>,
    color: Option<String>,
    position: Option<String>,
    deleted: bool,
}

struct TextRow {
    id: String,
    parent_id: String,
    content: Option<String>,
    deleted: bool,
}

struct Rows {
    items: Vec<ItemRow>,
    annotations: Vec<AnnotationRow>,
    comments: Vec<TextRow>,
    notes: Vec<TextRow>,
}

impl Rows {
    async fn read(conn: &Connection, schema: &Schema) -> Result<Rows> {
        let sql = format!("SELECT id, title, {} FROM items", schema.deleted("items"));
        let items = select(conn, &sql, |row| {
            Ok(ItemRow {
                id: id(row, 0)?,
                title: text(row, 1)?,
                deleted: flag(row, 2)?,
            })
        })
        .await?;

        let sql = format!(
            "SELECT id, item_id, content, color, position, {} FROM annotations",
            schema.deleted("annotations")
        );
        let annotations = select(conn, &sql, |row| {
            Ok(AnnotationRow {
                id: id(row, 0)?,
                item_id: id(row, 1)?,
                content: text(row, 2)?,
                color: text(row, 3)?,
                position: text(row, 4)?,
                deleted: flag(row, 5)?,
            })
        })
        .await?;

        let sql = format!(
            "SELECT id, annotation_id, content, {} FROM comments",
            schema.deleted("comments")
        );
        let comments = select(conn, &sql, text_row).await?;

        let sql = format!(
            "SELECT id, item_id, content, {} FROM notes",
            schema.deleted("notes")
        );
        let notes = select(conn, &sql, text_row).await?;

        Ok(Rows {
            items,
            annotations,
            comments,
            notes,
        })
    }

    fn assemble(self) -> Result<Vec<SourceDocument>> {
        let mut documents: Vec<SourceDocument> = Vec::new();
        let mut items: HashMap<String, Option<usize>> = HashMap::new();
        for item in self.items {
            if item.deleted {
                items.insert(item.id, None);
                continue;
            }
            items.insert(item.id.clone(), Some(documents.len()));
            documents.push(document(item.id, item.title.as_deref()));
        }

        let mut annotations: HashMap<String, Option<(usize, usize)>> = HashMap::new();
        for annotation in self.annotations {
            let parent = parent(&items, &annotation.item_id, "annotation", &annotation.id)?;
            let Some(document) = parent.filter(|_| !annotation.deleted) else {
                annotations.insert(annotation.id, None);
                continue;
            };
            let highlights = &mut documents[document].highlights;
            annotations.insert(annotation.id.clone(), Some((document, highlights.len())));
            highlights.push(SourceHighlight {
                text: highlight_text(annotation.content.as_deref())?,
                location: location(annotation.position.as_deref())?,
                external_id: annotation.id,
                color: annotation.color,
                times: SourceTimes::default(),
                comments: Vec::new(),
            });
        }

        for comment in self.comments {
            let parent = parent(&annotations, &comment.parent_id, "comment", &comment.id)?;
            if let Some((document, highlight)) = parent.filter(|_| !comment.deleted) {
                documents[document].highlights[highlight]
                    .comments
                    .push(SourceComment {
                        external_id: comment.id,
                        body: comment.content.unwrap_or_default(),
                        times: SourceTimes::default(),
                    });
            }
        }

        for note in self.notes {
            let parent = parent(&items, &note.parent_id, "note", &note.id)?;
            if let Some(document) = parent.filter(|_| !note.deleted) {
                documents[document].notes.push(SourceNote {
                    external_id: note.id,
                    body: note.content.unwrap_or_default(),
                    times: SourceTimes::default(),
                });
            }
        }
        Ok(documents)
    }
}

fn parent<T: Copy>(
    parents: &HashMap<String, Option<T>>,
    parent_id: &str,
    what: &str,
    id: &str,
) -> Result<Option<T>> {
    parents.get(parent_id).copied().ok_or_else(|| {
        Error::conflict(
            "research_missing_parent",
            format!("Research {what} `{id}` belongs to `{parent_id}`, which does not exist"),
        )
    })
}

async fn select<T>(conn: &Connection, sql: &str, map: impl Fn(&Row) -> Result<T>) -> Result<Vec<T>> {
    let mut rows = conn.query(sql, ()).await?;
    let mut mapped = Vec::new();
    while let Some(row) = rows.next().await? {
        mapped.push(map(&row)?);
    }
    Ok(mapped)
}

fn text_row(row: &Row) -> Result<TextRow> {
    Ok(TextRow {
        id: id(row, 0)?,
        parent_id: id(row, 1)?,
        content: text(row, 2)?,
        deleted: flag(row, 3)?,
    })
}

fn id(row: &Row, index: i32) -> Result<String> {
    match row.get_value(index)? {
        Value::Text(id) if !id.is_empty() => Ok(id),
        Value::Integer(id) => Ok(id.to_string()),
        _ => Err(incompatible(format!(
            "column {index} holds a blank or non-text ID"
        ))),
    }
}

fn text(row: &Row, index: i32) -> Result<Option<String>> {
    match row.get_value(index)? {
        Value::Text(text) => Ok(Some(text)),
        Value::Null => Ok(None),
        _ => Err(incompatible(format!("column {index} holds a non-text value"))),
    }
}

fn flag(row: &Row, index: i32) -> Result<bool> {
    Ok(row.get::<i64>(index)? != 0)
}
