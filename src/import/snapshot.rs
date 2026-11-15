//! An imported document's title is the source's proposal at creation; later imports
//! never change it, because by then the title belongs to the library.

use std::collections::{HashMap, HashSet};

use libsql::{Connection, params};
use serde::Serialize;
use serde_json::{Value, json};

use crate::domain::documents::{self, DocumentKind, NewDocument};
use crate::domain::returning_id;
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct Snapshot {
    source: String,
    documents: Vec<SourceDocument>,
    highlights_can_move: bool,
}

#[derive(Debug, Clone)]
pub struct SourceDocument {
    pub external_id: String,
    pub kind: DocumentKind,
    pub title: String,
    pub canonical_url: Option<String>,
    pub highlights: Vec<SourceHighlight>,
    pub notes: Vec<SourceNote>,
}

#[derive(Debug, Clone)]
pub struct SourceHighlight {
    pub external_id: String,
    pub text: String,
    pub color: Option<String>,
    pub location: Location,
    pub times: SourceTimes,
    pub comments: Vec<SourceComment>,
}

#[derive(Debug, Clone)]
pub struct SourceComment {
    pub external_id: String,
    pub body: String,
    pub times: SourceTimes,
}

#[derive(Debug, Clone)]
pub struct SourceNote {
    pub external_id: String,
    pub body: String,
    pub times: SourceTimes,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceTimes {
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    kind: &'static str,
    payload: Value,
}

impl Location {
    const VERSION: i64 = 1;

    pub fn new(kind: &'static str, payload: Value) -> Location {
        Location { kind, payload }
    }

    /// Object keys serialize sorted, so equal locations compare equal as text and a
    /// repeated import is recognised as unchanged.
    fn to_json(&self) -> String {
        json!({ "version": Location::VERSION, "kind": self.kind, "payload": self.payload }).to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptySnapshot {
    Refuse,
    Accept,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportStats {
    pub import_run_id: i64,
    pub source: String,
    pub documents: DocumentCounts,
    pub highlights: Counts,
    pub comments: Counts,
    pub notes: Counts,
    pub comments_rehomed_as_notes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub created: u64,
    pub updated: u64,
    pub unchanged: u64,
    pub deleted: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DocumentCounts {
    pub created: u64,
    pub linked: u64,
    pub unchanged: u64,
    pub unlinked: u64,
    pub deleted: u64,
}

impl Snapshot {
    pub fn new(source: impl Into<String>, documents: Vec<SourceDocument>) -> Result<Snapshot> {
        let source = source.into();
        if source.trim().is_empty() {
            return Err(Error::invalid(
                "snapshot_source_required",
                "a snapshot needs a source name",
            ));
        }
        check_unique("document", documents.iter().map(|document| &document.external_id))?;
        for document in &documents {
            if document.title.trim().is_empty() {
                return Err(Error::invalid(
                    "snapshot_title_required",
                    format!("document `{}` has no title", document.external_id),
                ));
            }
            check_unique(
                "highlight",
                document.highlights.iter().map(|highlight| &highlight.external_id),
            )?;
            check_unique("note", document.notes.iter().map(|note| &note.external_id))?;
            for highlight in &document.highlights {
                check_unique(
                    "comment",
                    highlight.comments.iter().map(|comment| &comment.external_id),
                )?;
            }
        }
        Ok(Snapshot {
            source,
            documents,
            highlights_can_move: false,
        })
    }

    // Only for a source whose highlight IDs are unique across all its documents.
    pub fn with_movable_highlights(self) -> Snapshot {
        Snapshot {
            highlights_can_move: true,
            ..self
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn documents(&self) -> &[SourceDocument] {
        &self.documents
    }

    fn record_count(&self) -> usize {
        self.documents
            .iter()
            .map(|document| document.highlights.len() + document.notes.len())
            .sum()
    }
}

fn check_unique<'a>(what: &str, ids: impl Iterator<Item = &'a String>) -> Result<()> {
    let mut seen = HashSet::new();
    for id in ids {
        if id.trim().is_empty() {
            return Err(Error::invalid(
                "snapshot_id_required",
                format!("a {what} has a blank external ID"),
            ));
        }
        if !seen.insert(id) {
            return Err(Error::invalid(
                "snapshot_duplicate_id",
                format!("{what} external ID `{id}` appears more than once under one parent"),
            ));
        }
    }
    Ok(())
}

pub async fn apply(conn: &Connection, snapshot: &Snapshot, empty: EmptySnapshot) -> Result<ImportStats> {
    if empty == EmptySnapshot::Refuse {
        refuse_unexpectedly_empty(conn, snapshot).await?;
    }

    let mut stats = ImportStats {
        source: snapshot.source.clone(),
        ..ImportStats::default()
    };
    let mut present = HashSet::new();
    let mut placed = Vec::with_capacity(snapshot.documents.len());
    for document in &snapshot.documents {
        let identity = upsert_identity(conn, &snapshot.source, document, &mut stats.documents).await?;
        present.insert(identity.id);
        placed.push((document, identity));
    }
    if snapshot.highlights_can_move {
        move_highlights(conn, &snapshot.source, &placed).await?;
    }
    for (document, identity) in &placed {
        reconcile_highlights(conn, identity, &document.highlights, &mut stats).await?;
        reconcile_notes(conn, identity, &document.notes, &mut stats.notes).await?;
    }
    remove_absent_identities(conn, &snapshot.source, &present, &mut stats).await?;

    stats.import_run_id = record_run(conn, &stats).await?;
    Ok(stats)
}

async fn refuse_unexpectedly_empty(conn: &Connection, snapshot: &Snapshot) -> Result<()> {
    let mut rows = conn
        .query(
            "SELECT
                 (SELECT count(*) FROM document_sources WHERE source = ?1),
                 (SELECT count(*) FROM highlights h JOIN document_sources s ON s.id = h.document_source_id
                  WHERE s.source = ?1)
               + (SELECT count(*) FROM notes n JOIN document_sources s ON s.id = n.document_source_id
                  WHERE s.source = ?1)",
            [snapshot.source.as_str()],
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| anyhow::anyhow!("count query returned no row"))?;
    let (owned_documents, owned_records): (i64, i64) = (row.get(0)?, row.get(1)?);

    let empties_documents = snapshot.documents.is_empty() && owned_documents > 0;
    let empties_records = snapshot.record_count() == 0 && owned_records > 0;
    if empties_documents || empties_records {
        return Err(Error::conflict(
            "empty_snapshot",
            format!(
                "the snapshot from `{}` would remove all {owned_documents} documents and {owned_records} \
                 highlights and notes it owns here; if the source really is empty, repeat with \
                 confirm_empty=true",
                snapshot.source
            ),
        ));
    }
    Ok(())
}

struct Identity {
    id: i64,
    document_id: i64,
}

async fn upsert_identity(
    conn: &Connection,
    source: &str,
    document: &SourceDocument,
    counts: &mut DocumentCounts,
) -> Result<Identity> {
    if let Some(identity) = find_identity(conn, source, &document.external_id).await? {
        conn.execute(
            "UPDATE document_sources SET last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
            [identity.id],
        )
        .await?;
        counts.unchanged += 1;
        return Ok(identity);
    }

    let existing = match &document.canonical_url {
        Some(url) => find_document_by_url(conn, url).await?,
        None => None,
    };
    let document_id = match existing {
        Some(document_id) => {
            counts.linked += 1;
            document_id
        }
        None => {
            counts.created += 1;
            let mut new = NewDocument::new(document.kind, &document.title);
            new.canonical_url = document.canonical_url.clone();
            documents::create(conn, &new).await?
        }
    };
    let id = documents::add_source(conn, document_id, source, &document.external_id).await?;
    Ok(Identity { id, document_id })
}

async fn find_identity(conn: &Connection, source: &str, external_id: &str) -> Result<Option<Identity>> {
    let mut rows = conn
        .query(
            "SELECT id, document_id FROM document_sources WHERE source = ? AND external_id = ?",
            [source, external_id],
        )
        .await?;
    Ok(match rows.next().await? {
        Some(row) => Some(Identity {
            id: row.get(0)?,
            document_id: row.get(1)?,
        }),
        None => None,
    })
}

async fn find_document_by_url(conn: &Connection, url: &str) -> Result<Option<i64>> {
    let mut rows = conn
        .query("SELECT id FROM documents WHERE canonical_url = ?", [url])
        .await?;
    Ok(match rows.next().await? {
        Some(row) => Some(row.get(0)?),
        None => None,
    })
}

async fn remove_absent_identities(
    conn: &Connection,
    source: &str,
    present: &HashSet<i64>,
    stats: &mut ImportStats,
) -> Result<()> {
    let mut rows = conn
        .query(
            "SELECT id, document_id FROM document_sources WHERE source = ?",
            [source],
        )
        .await?;
    let mut absent = Vec::new();
    while let Some(row) = rows.next().await? {
        let identity = Identity {
            id: row.get(0)?,
            document_id: row.get(1)?,
        };
        if !present.contains(&identity.id) {
            absent.push(identity);
        }
    }

    for identity in absent {
        for highlight_id in highlight_ids(conn, identity.id).await? {
            delete_highlight(conn, highlight_id, stats).await?;
        }
        stats.notes.deleted += conn
            .execute("DELETE FROM notes WHERE document_source_id = ?", [identity.id])
            .await?;
        conn.execute("DELETE FROM document_sources WHERE id = ?", [identity.id])
            .await?;

        if delete_document_if_unheld(conn, identity.document_id).await? {
            stats.documents.deleted += 1;
        } else {
            stats.documents.unlinked += 1;
        }
    }
    Ok(())
}

/// A local edit is recognised by `updated_at` having moved away from `created_at`:
/// imports never change either, so only the library can.
async fn delete_document_if_unheld(conn: &Connection, document_id: i64) -> Result<bool> {
    let deleted = conn
        .execute(
            "DELETE FROM documents
             WHERE id = ?1
               AND object_key IS NULL
               AND (description IS NULL OR trim(description) = '')
               AND page_offset = 0
               AND updated_at = created_at
               AND NOT EXISTS (SELECT 1 FROM document_sources WHERE document_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM notes WHERE document_id = ?1 AND document_source_id IS NULL)
               AND NOT EXISTS (SELECT 1 FROM document_authors WHERE document_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM document_labels WHERE document_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM sections WHERE document_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM upload_sessions WHERE document_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM document_aliases WHERE survivor_document_id = ?1)",
            [document_id],
        )
        .await?;
    Ok(deleted > 0)
}

struct StoredHighlight {
    id: i64,
    text: String,
    color: Option<String>,
    location_json: Option<String>,
    created_at: String,
    updated_at: String,
}

impl StoredHighlight {
    fn matches(&self, incoming: &SourceHighlight) -> bool {
        self.text == incoming.text
            && self.color == incoming.color
            && self.location_json.as_deref() == Some(incoming.location.to_json().as_str())
            && times_match(&self.created_at, &self.updated_at, &incoming.times)
    }
}

async fn reconcile_highlights(
    conn: &Connection,
    identity: &Identity,
    incoming: &[SourceHighlight],
    stats: &mut ImportStats,
) -> Result<()> {
    let mut stored = stored_highlights(conn, identity.id).await?;
    for highlight in incoming {
        let highlight_id = match stored.remove(&highlight.external_id) {
            None => {
                stats.highlights.created += 1;
                insert_highlight(conn, identity, highlight).await?
            }
            Some(row) if row.matches(highlight) => {
                stats.highlights.unchanged += 1;
                row.id
            }
            Some(row) => {
                stats.highlights.updated += 1;
                update_highlight(conn, row.id, highlight).await?;
                row.id
            }
        };
        reconcile_comments(conn, highlight_id, &highlight.comments, &mut stats.comments).await?;
    }
    for absent in stored.into_values() {
        delete_highlight(conn, absent.id, stats).await?;
    }
    Ok(())
}

async fn move_highlights(
    conn: &Connection,
    source: &str,
    placed: &[(&SourceDocument, Identity)],
) -> Result<()> {
    let destinations = sole_destinations(placed);
    for (external_id, owners) in highlight_owners(conn, source).await? {
        let [(highlight_id, owner)] = owners[..] else {
            continue;
        };
        let Some(destination) = destinations.get(external_id.as_str()) else {
            continue;
        };
        if owner != destination.id {
            conn.execute(
                "UPDATE highlights SET document_id = ?2, document_source_id = ?3 WHERE id = ?1",
                params![highlight_id, destination.document_id, destination.id],
            )
            .await?;
        }
    }
    Ok(())
}

fn sole_destinations<'a>(placed: &'a [(&'a SourceDocument, Identity)]) -> HashMap<&'a str, &'a Identity> {
    let mut destinations = HashMap::new();
    let mut repeated = HashSet::new();
    for (document, identity) in placed {
        for highlight in &document.highlights {
            if destinations
                .insert(highlight.external_id.as_str(), identity)
                .is_some()
            {
                repeated.insert(highlight.external_id.as_str());
            }
        }
    }
    destinations.retain(|external_id, _| !repeated.contains(external_id));
    destinations
}

async fn highlight_owners(conn: &Connection, source: &str) -> Result<HashMap<String, Vec<(i64, i64)>>> {
    let mut rows = conn
        .query(
            "SELECT h.external_id, h.id, h.document_source_id
             FROM highlights h JOIN document_sources s ON s.id = h.document_source_id
             WHERE s.source = ?",
            [source],
        )
        .await?;
    let mut owners: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    while let Some(row) = rows.next().await? {
        owners
            .entry(row.get(0)?)
            .or_default()
            .push((row.get(1)?, row.get(2)?));
    }
    Ok(owners)
}

async fn stored_highlights(conn: &Connection, identity_id: i64) -> Result<HashMap<String, StoredHighlight>> {
    let mut rows = conn
        .query(
            "SELECT id, external_id, text, color, location_json, created_at, updated_at
             FROM highlights WHERE document_source_id = ?",
            [identity_id],
        )
        .await?;
    let mut stored = HashMap::new();
    while let Some(row) = rows.next().await? {
        stored.insert(
            row.get(1)?,
            StoredHighlight {
                id: row.get(0)?,
                text: row.get(2)?,
                color: row.get(3)?,
                location_json: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            },
        );
    }
    Ok(stored)
}

async fn highlight_ids(conn: &Connection, identity_id: i64) -> Result<Vec<i64>> {
    let mut rows = conn
        .query(
            "SELECT id FROM highlights WHERE document_source_id = ?",
            [identity_id],
        )
        .await?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await? {
        ids.push(row.get(0)?);
    }
    Ok(ids)
}

async fn insert_highlight(
    conn: &Connection,
    identity: &Identity,
    highlight: &SourceHighlight,
) -> Result<i64> {
    returning_id(
        conn,
        "INSERT INTO highlights
             (document_id, document_source_id, external_id, text, color, location_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                 COALESCE(?7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                 COALESCE(?8, ?7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))
         RETURNING id",
        params![
            identity.document_id,
            identity.id,
            highlight.external_id.as_str(),
            highlight.text.as_str(),
            highlight.color.as_deref(),
            highlight.location.to_json(),
            highlight.times.created_at.as_deref(),
            highlight.times.updated_at.as_deref()
        ],
    )
    .await
}

async fn update_highlight(conn: &Connection, highlight_id: i64, highlight: &SourceHighlight) -> Result<()> {
    conn.execute(
        "UPDATE highlights
         SET text = ?2, color = ?3, location_json = ?4,
             created_at = COALESCE(?5, created_at),
             updated_at = COALESCE(?6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         WHERE id = ?1",
        params![
            highlight_id,
            highlight.text.as_str(),
            highlight.color.as_deref(),
            highlight.location.to_json(),
            highlight.times.created_at.as_deref(),
            highlight.times.updated_at.as_deref()
        ],
    )
    .await?;
    Ok(())
}

async fn delete_highlight(conn: &Connection, highlight_id: i64, stats: &mut ImportStats) -> Result<()> {
    stats.comments_rehomed_as_notes += conn
        .execute(
            "INSERT INTO notes (document_id, body, created_at, updated_at)
             SELECT h.document_id,
                    '> ' || replace(h.text, char(10), char(10) || '> ') || char(10) || char(10) || c.body,
                    c.created_at, c.updated_at
             FROM comments c JOIN highlights h ON h.id = c.highlight_id
             WHERE c.highlight_id = ? AND c.external_id IS NULL
             ORDER BY c.id",
            [highlight_id],
        )
        .await?;
    stats.comments.deleted += conn
        .execute("DELETE FROM comments WHERE highlight_id = ?", [highlight_id])
        .await?;
    conn.execute("DELETE FROM highlights WHERE id = ?", [highlight_id])
        .await?;
    stats.highlights.deleted += 1;
    Ok(())
}

struct StoredText {
    id: i64,
    body: String,
    created_at: String,
    updated_at: String,
}

impl StoredText {
    fn matches(&self, body: &str, times: &SourceTimes) -> bool {
        self.body == body && times_match(&self.created_at, &self.updated_at, times)
    }
}

async fn stored_texts(conn: &Connection, sql: &str, parent_id: i64) -> Result<HashMap<String, StoredText>> {
    let mut rows = conn.query(sql, [parent_id]).await?;
    let mut stored = HashMap::new();
    while let Some(row) = rows.next().await? {
        stored.insert(
            row.get(1)?,
            StoredText {
                id: row.get(0)?,
                body: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            },
        );
    }
    Ok(stored)
}

async fn reconcile_comments(
    conn: &Connection,
    highlight_id: i64,
    incoming: &[SourceComment],
    counts: &mut Counts,
) -> Result<()> {
    let mut stored = stored_texts(
        conn,
        "SELECT id, external_id, body, created_at, updated_at
         FROM comments WHERE highlight_id = ? AND external_id IS NOT NULL",
        highlight_id,
    )
    .await?;
    for comment in incoming {
        match stored.remove(&comment.external_id) {
            None => {
                counts.created += 1;
                conn.execute(
                    "INSERT INTO comments (highlight_id, external_id, body, created_at, updated_at)
                     VALUES (?1, ?2, ?3,
                             COALESCE(?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                             COALESCE(?5, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))",
                    params![
                        highlight_id,
                        comment.external_id.as_str(),
                        comment.body.as_str(),
                        comment.times.created_at.as_deref(),
                        comment.times.updated_at.as_deref()
                    ],
                )
                .await?;
            }
            Some(row) if row.matches(&comment.body, &comment.times) => counts.unchanged += 1,
            Some(row) => {
                counts.updated += 1;
                conn.execute(
                    "UPDATE comments
                     SET body = ?2,
                         created_at = COALESCE(?3, created_at),
                         updated_at = COALESCE(?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     WHERE id = ?1",
                    params![
                        row.id,
                        comment.body.as_str(),
                        comment.times.created_at.as_deref(),
                        comment.times.updated_at.as_deref()
                    ],
                )
                .await?;
            }
        }
    }
    for absent in stored.into_values() {
        counts.deleted += 1;
        conn.execute("DELETE FROM comments WHERE id = ?", [absent.id])
            .await?;
    }
    Ok(())
}

async fn reconcile_notes(
    conn: &Connection,
    identity: &Identity,
    incoming: &[SourceNote],
    counts: &mut Counts,
) -> Result<()> {
    let mut stored = stored_texts(
        conn,
        "SELECT id, external_id, body, created_at, updated_at FROM notes WHERE document_source_id = ?",
        identity.id,
    )
    .await?;
    for note in incoming {
        match stored.remove(&note.external_id) {
            None => {
                counts.created += 1;
                conn.execute(
                    "INSERT INTO notes (document_id, document_source_id, external_id, body, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4,
                             COALESCE(?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                             COALESCE(?6, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))",
                    params![
                        identity.document_id,
                        identity.id,
                        note.external_id.as_str(),
                        note.body.as_str(),
                        note.times.created_at.as_deref(),
                        note.times.updated_at.as_deref()
                    ],
                )
                .await?;
            }
            Some(row) if row.matches(&note.body, &note.times) => counts.unchanged += 1,
            Some(row) => {
                counts.updated += 1;
                conn.execute(
                    "UPDATE notes
                     SET body = ?2,
                         created_at = COALESCE(?3, created_at),
                         updated_at = COALESCE(?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     WHERE id = ?1",
                    params![
                        row.id,
                        note.body.as_str(),
                        note.times.created_at.as_deref(),
                        note.times.updated_at.as_deref()
                    ],
                )
                .await?;
            }
        }
    }
    for absent in stored.into_values() {
        counts.deleted += 1;
        conn.execute("DELETE FROM notes WHERE id = ?", [absent.id])
            .await?;
    }
    Ok(())
}

fn times_match(created_at: &str, updated_at: &str, incoming: &SourceTimes) -> bool {
    incoming
        .created_at
        .as_deref()
        .is_none_or(|time| time == created_at)
        && incoming
            .updated_at
            .as_deref()
            .is_none_or(|time| time == updated_at)
}

async fn record_run(conn: &Connection, stats: &ImportStats) -> Result<i64> {
    let mut stats_json = serde_json::to_value(stats).map_err(anyhow::Error::from)?;
    if let Some(fields) = stats_json.as_object_mut() {
        fields.remove("import_run_id");
    }
    returning_id(
        conn,
        "INSERT INTO import_runs (source, stats_json) VALUES (?, ?) RETURNING id",
        params![stats.source.as_str(), stats_json.to_string()],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(external_id: &str) -> SourceDocument {
        SourceDocument {
            external_id: external_id.to_string(),
            kind: DocumentKind::Web,
            title: external_id.to_string(),
            canonical_url: Some(external_id.to_string()),
            highlights: Vec::new(),
            notes: Vec::new(),
        }
    }

    #[test]
    fn a_repeated_document_id_is_refused() {
        let error = Snapshot::new("light:a", vec![document("https://x"), document("https://x")]).unwrap_err();
        assert!(matches!(
            error,
            Error::Invalid {
                code: "snapshot_duplicate_id",
                ..
            }
        ));
    }

    #[test]
    fn locations_serialize_deterministically() {
        let a = Location::new("pdf_region", json!({ "b": 1, "a": 2 }));
        let b = Location::new("pdf_region", json!({ "a": 2, "b": 1 }));
        assert_eq!(a.to_json(), b.to_json());
        assert_eq!(
            a.to_json(),
            r#"{"kind":"pdf_region","payload":{"a":2,"b":1},"version":1}"#
        );
    }
}
