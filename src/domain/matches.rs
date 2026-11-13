use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use libsql::{Connection, params};
use serde::{Serialize, Serializer};

use super::annotations::{self, AnnotationCounts};
use super::documents::{self, DocumentKind, SourceIdentity};
use super::id_array;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PairId {
    lower: i64,
    higher: i64,
}

impl PairId {
    pub fn new(a: i64, b: i64) -> Result<PairId> {
        if a == b {
            return Err(Error::invalid(
                "same_document",
                "a document cannot be matched with itself",
            ));
        }
        Ok(PairId {
            lower: a.min(b),
            higher: a.max(b),
        })
    }

    pub fn parse(text: &str) -> Result<PairId> {
        let malformed = || {
            Error::invalid(
                "invalid_pair_id",
                format!("`{text}` is not a pair id like `3-7` (lower id first)"),
            )
        };
        let (lower, higher) = text.split_once('-').ok_or_else(malformed)?;
        let lower: i64 = lower.parse().map_err(|_| malformed())?;
        let higher: i64 = higher.parse().map_err(|_| malformed())?;
        if lower > higher {
            return Err(malformed());
        }
        PairId::new(lower, higher)
    }

    pub fn other(self, id: i64) -> Option<i64> {
        match id {
            id if id == self.lower => Some(self.higher),
            id if id == self.higher => Some(self.lower),
            _ => None,
        }
    }
}

impl fmt::Display for PairId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.lower, self.higher)
    }
}

impl Serialize for PairId {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

pub fn normalize_title(title: &str) -> String {
    let spaced: String = title
        .chars()
        .filter(|c| !matches!(c, '\'' | '\u{2019}'))
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Serialize)]
pub struct MatchCandidate {
    pub id: PairId,
    pub basis: String,
    pub evidence: Evidence,
    pub documents: [MatchedDocument; 2],
}

#[derive(Debug, Default, Serialize)]
pub struct Evidence {
    pub shared_authors: Vec<String>,
    pub same_filename: bool,
}

impl Evidence {
    fn strength(&self) -> usize {
        usize::from(!self.shared_authors.is_empty()) + usize::from(self.same_filename)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchedDocument {
    pub id: i64,
    pub kind: DocumentKind,
    pub title: String,
    pub authors: Vec<String>,
    pub sources: Vec<SourceIdentity>,
    pub has_file: bool,
    pub original_filename: Option<String>,
    pub highlight_count: i64,
    pub comment_count: i64,
    pub note_count: i64,
}

pub async fn candidates(conn: &Connection) -> Result<Vec<MatchCandidate>> {
    let titles = titles(conn).await?;
    let dismissed = dismissed(conn).await?;
    let pairs: Vec<(PairId, String)> = pairs_with_equal_titles(&titles)
        .into_iter()
        .filter(|(pair, basis)| dismissed.get(pair) != Some(basis))
        .collect();

    let ids: Vec<i64> = pairs
        .iter()
        .flat_map(|(pair, _)| [pair.lower, pair.higher])
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let documents = matched_documents(conn, &ids).await?;

    let mut candidates: Vec<MatchCandidate> = pairs
        .into_iter()
        .filter_map(|(id, basis)| {
            let lower = documents.get(&id.lower)?.clone();
            let higher = documents.get(&id.higher)?.clone();
            Some(MatchCandidate {
                id,
                basis,
                evidence: evidence(&lower, &higher),
                documents: [lower, higher],
            })
        })
        .collect();
    candidates.sort_by(|a, b| {
        (b.evidence.strength().cmp(&a.evidence.strength()))
            .then_with(|| a.basis.cmp(&b.basis))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(candidates)
}

fn pairs_with_equal_titles(titles: &[(i64, String)]) -> Vec<(PairId, String)> {
    let mut groups: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for (id, title) in titles {
        groups.entry(normalize_title(title)).or_default().push(*id);
    }
    let mut pairs = Vec::new();
    for (basis, ids) in groups.into_iter().filter(|(basis, _)| !basis.is_empty()) {
        for (index, &a) in ids.iter().enumerate() {
            for &b in &ids[index + 1..] {
                if let Ok(pair) = PairId::new(a, b) {
                    pairs.push((pair, basis.clone()));
                }
            }
        }
    }
    pairs
}

fn evidence(a: &MatchedDocument, b: &MatchedDocument) -> Evidence {
    let shared_authors = a
        .authors
        .iter()
        .filter(|name| b.authors.iter().any(|other| other.eq_ignore_ascii_case(name)))
        .cloned()
        .collect();
    let same_filename = match (&a.original_filename, &b.original_filename) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    };
    Evidence {
        shared_authors,
        same_filename,
    }
}

async fn titles(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut rows = conn.query("SELECT id, title FROM documents", ()).await?;
    let mut titles = Vec::new();
    while let Some(row) = rows.next().await? {
        titles.push((row.get(0)?, row.get(1)?));
    }
    Ok(titles)
}

async fn dismissed(conn: &Connection) -> Result<HashMap<PairId, String>> {
    let mut rows = conn
        .query(
            "SELECT left_document_id, right_document_id, basis FROM document_match_decisions
             WHERE decision = 'dismissed'",
            (),
        )
        .await?;
    let mut dismissed = HashMap::new();
    while let Some(row) = rows.next().await? {
        dismissed.insert(PairId::new(row.get(0)?, row.get(1)?)?, row.get(2)?);
    }
    Ok(dismissed)
}

async fn matched_documents(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, MatchedDocument>> {
    let mut authors = documents::authors_of(conn, ids).await?;
    let mut sources = documents::sources_of(conn, ids).await?;
    let counts = annotations::counts_of(conn, ids).await?;

    let mut rows = conn
        .query(
            "SELECT id, kind, title, original_filename, object_key IS NOT NULL FROM documents
             WHERE id IN (SELECT value FROM json_each(?))",
            [id_array(ids)],
        )
        .await?;
    let mut documents = HashMap::new();
    while let Some(row) = rows.next().await? {
        let id: i64 = row.get(0)?;
        let AnnotationCounts {
            highlights,
            comments,
            notes,
        } = counts.get(&id).copied().unwrap_or_default();
        documents.insert(
            id,
            MatchedDocument {
                id,
                kind: DocumentKind::parse(&row.get::<String>(1)?)?,
                title: row.get(2)?,
                original_filename: row.get(3)?,
                has_file: row.get(4)?,
                authors: authors.remove(&id).unwrap_or_default(),
                sources: sources.remove(&id).unwrap_or_default(),
                highlight_count: highlights,
                comment_count: comments,
                note_count: notes,
            },
        );
    }
    Ok(documents)
}

#[derive(Debug, Serialize)]
pub struct Dismissal {
    pub id: PairId,
    pub basis: String,
}

pub async fn dismiss(conn: &Connection, pair: PairId) -> Result<Dismissal> {
    let lower = normalize_title(&title_of(conn, pair.lower).await?);
    let higher = normalize_title(&title_of(conn, pair.higher).await?);
    if lower != higher {
        return Err(Error::conflict(
            "not_a_candidate",
            format!("documents {pair} no longer share a title, so there is nothing to dismiss"),
        ));
    }

    conn.execute(
        "INSERT INTO document_match_decisions (left_document_id, right_document_id, decision, basis)
         VALUES (?, ?, 'dismissed', ?)
         ON CONFLICT (left_document_id, right_document_id) DO UPDATE SET
             basis = excluded.basis,
             decided_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![pair.lower, pair.higher, lower.as_str()],
    )
    .await?;
    Ok(Dismissal {
        id: pair,
        basis: lower,
    })
}

async fn title_of(conn: &Connection, document_id: i64) -> Result<String> {
    let mut rows = conn
        .query("SELECT title FROM documents WHERE id = ?", [document_id])
        .await?;
    match rows.next().await? {
        Some(row) => Ok(row.get(0)?),
        None => Err(Error::NotFound("document")),
    }
}

#[derive(Debug, Serialize)]
pub struct Merged {
    pub survivor_id: i64,
    pub retired_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MergeFields {
    description: Option<String>,
    canonical_url: Option<String>,
    object_key: Option<String>,
    original_filename: Option<String>,
    page_offset: i64,
}

impl MergeFields {
    fn combine(survivor: MergeFields, retired: MergeFields, adopted_sections: bool) -> MergeFields {
        let (object_key, original_filename) = match survivor.object_key {
            Some(key) => (Some(key), survivor.original_filename),
            None => (retired.object_key, retired.original_filename),
        };
        MergeFields {
            description: survivor.description.or(retired.description),
            canonical_url: survivor.canonical_url.or(retired.canonical_url),
            object_key,
            original_filename,
            page_offset: if adopted_sections {
                retired.page_offset
            } else {
                survivor.page_offset
            },
        }
    }
}

pub async fn merge(conn: &Connection, pair: PairId, survivor_id: i64) -> Result<Merged> {
    let retired_id = pair.other(survivor_id).ok_or_else(|| {
        Error::invalid(
            "survivor_not_in_pair",
            format!("document {survivor_id} is not one of the pair {pair}"),
        )
    })?;
    let survivor = merge_fields(conn, survivor_id).await?;
    let retired = merge_fields(conn, retired_id).await?;
    if survivor.object_key.is_some() && retired.object_key.is_some() {
        return Err(Error::conflict(
            "both_have_files",
            format!(
                "documents {survivor_id} and {retired_id} each have a stored file; \
                 remove one before merging"
            ),
        ));
    }

    move_annotations_and_sources(conn, retired_id, survivor_id).await?;
    let adopted_sections = adopt_sections_if_none(conn, retired_id, survivor_id).await?;
    merge_authors(conn, retired_id, survivor_id).await?;
    conn.execute(
        "INSERT OR IGNORE INTO document_labels (document_id, label_id)
         SELECT ?, label_id FROM document_labels WHERE document_id = ?",
        params![survivor_id, retired_id],
    )
    .await?;
    record_alias(conn, retired_id, survivor_id).await?;

    ensure_nothing_left_behind(conn, retired_id).await?;
    // Deleted before the survivor takes its fields: `object_key` and `canonical_url` are unique.
    conn.execute("DELETE FROM documents WHERE id = ?", [retired_id])
        .await?;
    let fields = MergeFields::combine(survivor, retired, adopted_sections);
    conn.execute(
        "UPDATE documents SET
             description = ?, canonical_url = ?, object_key = ?, original_filename = ?, page_offset = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE id = ?",
        params![
            fields.description,
            fields.canonical_url,
            fields.object_key,
            fields.original_filename,
            fields.page_offset,
            survivor_id
        ],
    )
    .await?;

    Ok(Merged {
        survivor_id,
        retired_id,
    })
}

async fn merge_fields(conn: &Connection, document_id: i64) -> Result<MergeFields> {
    let mut rows = conn
        .query(
            "SELECT description, canonical_url, object_key, original_filename, page_offset
             FROM documents WHERE id = ?",
            [document_id],
        )
        .await?;
    let row = rows.next().await?.ok_or(Error::NotFound("document"))?;
    Ok(MergeFields {
        description: row.get(0)?,
        canonical_url: row.get(1)?,
        object_key: row.get(2)?,
        original_filename: row.get(3)?,
        page_offset: row.get(4)?,
    })
}

async fn move_annotations_and_sources(conn: &Connection, from: i64, to: i64) -> Result<()> {
    for statement in [
        "UPDATE document_sources SET document_id = ? WHERE document_id = ?",
        "UPDATE highlights SET document_id = ? WHERE document_id = ?",
        "UPDATE notes SET document_id = ? WHERE document_id = ?",
        "UPDATE upload_sessions SET document_id = ? WHERE document_id = ?",
    ] {
        conn.execute(statement, params![to, from]).await?;
    }
    Ok(())
}

async fn adopt_sections_if_none(conn: &Connection, from: i64, to: i64) -> Result<bool> {
    let mut rows = conn
        .query("SELECT 1 FROM sections WHERE document_id = ? LIMIT 1", [to])
        .await?;
    if rows.next().await?.is_some() {
        return Ok(false);
    }
    let moved = conn
        .execute(
            "UPDATE sections SET document_id = ? WHERE document_id = ?",
            params![to, from],
        )
        .await?;
    Ok(moved > 0)
}

async fn merge_authors(conn: &Connection, from: i64, to: i64) -> Result<()> {
    let mut authors = documents::authors_of(conn, &[to, from]).await?;
    let mut merged = authors.remove(&to).unwrap_or_default();
    merged.extend(authors.remove(&from).unwrap_or_default());
    documents::replace_authors(conn, to, &merged).await
}

async fn record_alias(conn: &Connection, retired: i64, survivor: i64) -> Result<()> {
    conn.execute(
        "UPDATE document_aliases SET survivor_document_id = ? WHERE survivor_document_id = ?",
        params![survivor, retired],
    )
    .await?;
    conn.execute(
        "INSERT INTO document_aliases (alias_document_id, survivor_document_id) VALUES (?, ?)",
        params![retired, survivor],
    )
    .await?;
    Ok(())
}

/// Anything still attached to the retired document would be destroyed by
/// `ON DELETE CASCADE`. Sections are exempt: dropping them when the survivor has its
/// own is intended.
async fn ensure_nothing_left_behind(conn: &Connection, retired: i64) -> Result<()> {
    let mut rows = conn
        .query(
            "SELECT (SELECT count(*) FROM document_sources WHERE document_id = :id)
                  + (SELECT count(*) FROM highlights WHERE document_id = :id)
                  + (SELECT count(*) FROM notes WHERE document_id = :id)
                  + (SELECT count(*) FROM document_aliases WHERE survivor_document_id = :id)",
            libsql::named_params! { ":id": retired },
        )
        .await?;
    let remaining: i64 = match rows.next().await? {
        Some(row) => row.get(0)?,
        None => 0,
    };
    if remaining > 0 {
        return Err(anyhow::anyhow!("merge left {remaining} rows attached to document {retired}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_normalize_by_case_punctuation_and_spacing() {
        assert_eq!(normalize_title("  Things   Fall Apart. "), "things fall apart");
        assert_eq!(normalize_title("Don't Look Back"), "dont look back");
        assert_eq!(normalize_title("Don\u{2019}t Look Back"), "dont look back");
        assert_eq!(normalize_title("Self-Help: A Guide"), "self help a guide");
        assert_eq!(normalize_title("Ngũgĩ — Decolonising"), "ngũgĩ decolonising");
        assert_eq!(normalize_title("?!"), "");
    }

    #[test]
    fn pair_ids_have_one_spelling() {
        assert_eq!(PairId::new(7, 3).unwrap().to_string(), "3-7");
        assert_eq!(PairId::parse("3-7").unwrap(), PairId::new(3, 7).unwrap());
        assert!(PairId::parse("7-3").is_err(), "the lower id comes first");
        assert!(PairId::parse("3").is_err());
        assert!(PairId::parse("a-b").is_err());
        assert!(matches!(
            PairId::parse("4-4"),
            Err(Error::Invalid {
                code: "same_document",
                ..
            })
        ));
        assert_eq!(PairId::new(3, 7).unwrap().other(3), Some(7));
        assert_eq!(PairId::new(3, 7).unwrap().other(5), None);
    }

    #[test]
    fn every_pair_in_a_title_group_is_proposed() {
        let titles = vec![
            (1, "Arrow of God".to_string()),
            (2, "arrow of god!".into()),
            (3, "Arrow  of God".into()),
            (4, "No Longer at Ease".into()),
            (5, "...".into()),
            (6, "--".into()),
        ];
        let pairs: Vec<String> = pairs_with_equal_titles(&titles)
            .into_iter()
            .map(|(pair, _)| pair.to_string())
            .collect();
        assert_eq!(
            pairs,
            vec!["1-2", "1-3", "2-3"],
            "titles without words never match"
        );
    }

    #[test]
    fn merged_fields_prefer_the_survivor_and_keep_a_file_with_its_name() {
        let survivor = MergeFields {
            description: None,
            canonical_url: Some("https://survivor".into()),
            object_key: None,
            original_filename: Some("orphan-name.pdf".into()),
            page_offset: 2,
        };
        let retired = MergeFields {
            description: Some("from retired".into()),
            canonical_url: Some("https://retired".into()),
            object_key: Some("local/book.pdf".into()),
            original_filename: Some("book.pdf".into()),
            page_offset: 9,
        };

        let kept_sections = MergeFields::combine(survivor.clone(), retired.clone(), false);
        assert_eq!(kept_sections.description.as_deref(), Some("from retired"));
        assert_eq!(kept_sections.canonical_url.as_deref(), Some("https://survivor"));
        assert_eq!(kept_sections.object_key.as_deref(), Some("local/book.pdf"));
        assert_eq!(kept_sections.original_filename.as_deref(), Some("book.pdf"));
        assert_eq!(kept_sections.page_offset, 2);

        assert_eq!(MergeFields::combine(survivor, retired, true).page_offset, 9);
    }
}
