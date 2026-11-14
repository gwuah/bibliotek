use std::collections::HashMap;

use anyhow::Context;
use libsql::Connection;
use serde::Serialize;

use super::id_array;
use crate::error::Result;

#[derive(Debug, Serialize)]
pub struct Highlight {
    pub id: i64,
    pub source: String,
    pub text: String,
    pub color: Option<String>,
    pub location: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Serialize)]
pub struct Comment {
    pub id: i64,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct Note {
    pub id: i64,
    pub source: Option<String>,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct AnnotationCounts {
    pub highlights: i64,
    pub comments: i64,
    pub notes: i64,
}

pub async fn highlights_of(conn: &Connection, document_id: i64) -> Result<Vec<Highlight>> {
    let mut comments = comments_of(conn, document_id).await?;

    let mut rows = conn
        .query(
            "SELECT h.id, s.source, h.text, h.color, h.location_json, h.created_at, h.updated_at
             FROM highlights h JOIN document_sources s ON s.id = h.document_source_id
             WHERE h.document_id = ?
             ORDER BY h.created_at, h.id",
            [document_id],
        )
        .await?;
    let mut highlights = Vec::new();
    while let Some(row) = rows.next().await? {
        let id: i64 = row.get(0)?;
        let location: Option<String> = row.get(4)?;
        highlights.push(Highlight {
            id,
            source: row.get(1)?,
            text: row.get(2)?,
            color: row.get(3)?,
            location: location
                .map(|json| serde_json::from_str(&json))
                .transpose()
                .with_context(|| format!("highlight {id} has malformed location_json"))?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            comments: comments.remove(&id).unwrap_or_default(),
        });
    }
    Ok(highlights)
}

async fn comments_of(conn: &Connection, document_id: i64) -> Result<HashMap<i64, Vec<Comment>>> {
    let mut rows = conn
        .query(
            "SELECT c.highlight_id, c.id, c.body, c.created_at, c.updated_at
             FROM comments c JOIN highlights h ON h.id = c.highlight_id
             WHERE h.document_id = ?
             ORDER BY c.created_at, c.id",
            [document_id],
        )
        .await?;
    let mut comments: HashMap<i64, Vec<Comment>> = HashMap::new();
    while let Some(row) = rows.next().await? {
        comments.entry(row.get(0)?).or_default().push(Comment {
            id: row.get(1)?,
            body: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        });
    }
    Ok(comments)
}

pub async fn notes_of(conn: &Connection, document_id: i64) -> Result<Vec<Note>> {
    let mut rows = conn
        .query(
            "SELECT n.id, s.source, n.body, n.created_at, n.updated_at
             FROM notes n LEFT JOIN document_sources s ON s.id = n.document_source_id
             WHERE n.document_id = ?
             ORDER BY n.created_at, n.id",
            [document_id],
        )
        .await?;
    let mut notes = Vec::new();
    while let Some(row) = rows.next().await? {
        notes.push(Note {
            id: row.get(0)?,
            source: row.get(1)?,
            body: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        });
    }
    Ok(notes)
}

pub async fn highlight_counts_by_url_ending(
    conn: &Connection,
    endings: &[String],
) -> Result<HashMap<String, i64>> {
    let mut rows = conn
        .query(
            "SELECT e.value,
                    (SELECT count(*) FROM highlights h JOIN documents d ON d.id = h.document_id
                     WHERE substr(d.canonical_url, -length(e.value)) = e.value)
             FROM json_each(?) e",
            [serde_json::Value::from(endings).to_string()],
        )
        .await?;
    let mut counts = HashMap::new();
    while let Some(row) = rows.next().await? {
        counts.insert(row.get(0)?, row.get(1)?);
    }
    Ok(counts)
}

pub async fn counts_of(conn: &Connection, document_ids: &[i64]) -> Result<HashMap<i64, AnnotationCounts>> {
    let mut rows = conn
        .query(
            "SELECT d.value,
                    (SELECT count(*) FROM highlights WHERE document_id = d.value),
                    (SELECT count(*) FROM comments c JOIN highlights h ON h.id = c.highlight_id
                     WHERE h.document_id = d.value),
                    (SELECT count(*) FROM notes WHERE document_id = d.value)
             FROM json_each(?) d",
            [id_array(document_ids)],
        )
        .await?;
    let mut counts = HashMap::new();
    while let Some(row) = rows.next().await? {
        counts.insert(
            row.get(0)?,
            AnnotationCounts {
                highlights: row.get(1)?,
                comments: row.get(2)?,
                notes: row.get(3)?,
            },
        );
    }
    Ok(counts)
}
