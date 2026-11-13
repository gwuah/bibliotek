use std::collections::HashMap;

use libsql::{Connection, Value, params};
use serde::{Deserialize, Deserializer, Serialize};

use super::annotations::{self, Highlight, Note};
use super::{id_array, returning_id};
use crate::blob::ObjectKey;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentKind {
    Pdf,
    Web,
}

impl DocumentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentKind::Pdf => "pdf",
            DocumentKind::Web => "web",
        }
    }

    pub fn parse(value: &str) -> Result<DocumentKind> {
        match value {
            "pdf" => Ok(DocumentKind::Pdf),
            "web" => Ok(DocumentKind::Web),
            other => Err(Error::invalid(
                "invalid_document_kind",
                format!("unknown document kind `{other}`"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelKind {
    Subject,
    Tag,
}

impl LabelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LabelKind::Subject => "subject",
            LabelKind::Tag => "tag",
        }
    }
}

pub mod source {
    pub const UPLOAD: &str = "upload";
    pub const RESEARCH: &str = "research";

    pub fn light(installation: &str) -> String {
        format!("light:{installation}")
    }
}

#[derive(Debug)]
pub enum Lookup<T> {
    Found(T),
    Merged { survivor_id: i64 },
}

impl<T> Lookup<T> {
    /// Treats a retired ID as missing, so an edit addressed to it is refused rather than
    /// applied to the survivor.
    pub fn live(self) -> Result<T> {
        match self {
            Lookup::Found(found) => Ok(found),
            Lookup::Merged { .. } => Err(Error::NotFound("document")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewDocument {
    pub kind: DocumentKind,
    pub title: String,
    pub description: Option<String>,
    pub canonical_url: Option<String>,
    pub object_key: Option<ObjectKey>,
    pub original_filename: Option<String>,
    pub authors: Vec<String>,
    pub subjects: Vec<String>,
    pub tags: Vec<String>,
}

impl NewDocument {
    pub fn new(kind: DocumentKind, title: impl Into<String>) -> NewDocument {
        NewDocument {
            kind,
            title: title.into(),
            description: None,
            canonical_url: None,
            object_key: None,
            original_filename: None,
            authors: Vec::new(),
            subjects: Vec::new(),
            tags: Vec::new(),
        }
    }
}

pub async fn create(conn: &Connection, new: &NewDocument) -> Result<i64> {
    let title = required_title(&new.title)?;

    let document_id = returning_id(
        conn,
        "INSERT INTO documents (kind, title, description, canonical_url, object_key, original_filename)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        params![
            new.kind.as_str(),
            title,
            new.description.as_deref(),
            new.canonical_url.as_deref(),
            new.object_key.as_ref().map(ObjectKey::as_str),
            new.original_filename.as_deref()
        ],
    )
    .await?;

    replace_authors(conn, document_id, &new.authors).await?;
    replace_labels(conn, document_id, LabelKind::Subject, &new.subjects).await?;
    replace_labels(conn, document_id, LabelKind::Tag, &new.tags).await?;
    Ok(document_id)
}

pub async fn add_source(conn: &Connection, document_id: i64, source: &str, external_id: &str) -> Result<i64> {
    returning_id(
        conn,
        "INSERT INTO document_sources (document_id, source, external_id) VALUES (?, ?, ?) RETURNING id",
        params![document_id, source, external_id],
    )
    .await
}

pub async fn replace_authors(conn: &Connection, document_id: i64, names: &[String]) -> Result<()> {
    conn.execute(
        "DELETE FROM document_authors WHERE document_id = ?",
        [document_id],
    )
    .await?;
    for (position, name) in distinct_names(names).into_iter().enumerate() {
        let author_id = returning_id(
            conn,
            // The no-op update makes RETURNING yield the existing row's id on conflict.
            "INSERT INTO authors (name) VALUES (?)
             ON CONFLICT (name) DO UPDATE SET name = name RETURNING id",
            [name],
        )
        .await?;
        conn.execute(
            "INSERT INTO document_authors (document_id, author_id, position) VALUES (?, ?, ?)",
            params![document_id, author_id, position as i64],
        )
        .await?;
    }
    Ok(())
}

pub async fn replace_labels(
    conn: &Connection,
    document_id: i64,
    kind: LabelKind,
    names: &[String],
) -> Result<()> {
    conn.execute(
        "DELETE FROM document_labels
         WHERE document_id = ? AND label_id IN (SELECT id FROM labels WHERE kind = ?)",
        params![document_id, kind.as_str()],
    )
    .await?;
    for name in distinct_names(names) {
        let label_id = returning_id(
            conn,
            "INSERT INTO labels (kind, name) VALUES (?, ?)
             ON CONFLICT (kind, name) DO UPDATE SET name = name RETURNING id",
            params![kind.as_str(), name],
        )
        .await?;
        conn.execute(
            "INSERT INTO document_labels (document_id, label_id) VALUES (?, ?)",
            params![document_id, label_id],
        )
        .await?;
    }
    Ok(())
}

fn distinct_names(names: &[String]) -> Vec<&str> {
    let mut distinct: Vec<&str> = Vec::new();
    for name in names
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
    {
        if !distinct.iter().any(|seen| seen.eq_ignore_ascii_case(name)) {
            distinct.push(name);
        }
    }
    distinct
}

fn required_title(title: &str) -> Result<&str> {
    match title.trim() {
        "" => Err(Error::invalid("title_required", "a document needs a title")),
        title => Ok(title),
    }
}

/// PDF page = printed page + `page_offset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub title: String,
    pub start_page: i64,
    pub end_page: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sections(Vec<Section>);

impl Sections {
    pub fn new(sections: Vec<Section>) -> Result<Sections> {
        let mut checked: Vec<Section> = Vec::with_capacity(sections.len());
        for (index, section) in sections.into_iter().enumerate() {
            let number = index + 1;
            let section = Section {
                title: section.title.trim().to_string(),
                ..section
            };
            if section.title.is_empty() {
                return Err(Error::invalid(
                    "section_title_required",
                    format!("section {number} needs a title"),
                ));
            }
            if section.start_page < 1 {
                return Err(Error::invalid(
                    "section_page_out_of_range",
                    format!("section {number} starts before page 1"),
                ));
            }
            if section.start_page > section.end_page {
                return Err(Error::invalid(
                    "section_range_inverted",
                    format!(
                        "section {number} starts on page {} but ends on page {}",
                        section.start_page, section.end_page
                    ),
                ));
            }
            if let Some(previous) = checked.last() {
                if section.start_page < previous.start_page {
                    return Err(Error::invalid(
                        "sections_out_of_order",
                        format!("section {number} starts before section {index}"),
                    ));
                }
                if section.start_page <= previous.end_page {
                    return Err(Error::invalid(
                        "sections_overlap",
                        format!(
                            "section {number} starts on page {}, before section {index} ends on page {}",
                            section.start_page, previous.end_page
                        ),
                    ));
                }
            }
            checked.push(section);
        }
        Ok(Sections(checked))
    }
}

pub async fn replace_sections(conn: &Connection, document_id: i64, sections: &Sections) -> Result<()> {
    conn.execute("DELETE FROM sections WHERE document_id = ?", [document_id])
        .await?;
    for (position, section) in sections.0.iter().enumerate() {
        conn.execute(
            "INSERT INTO sections (document_id, title, start_page, end_page, position) VALUES (?, ?, ?, ?, ?)",
            params![
                document_id,
                section.title.as_str(),
                section.start_page,
                section.end_page,
                position as i64
            ],
        )
        .await?;
    }
    Ok(())
}

async fn sections_of(conn: &Connection, document_id: i64) -> Result<Vec<Section>> {
    let mut rows = conn
        .query(
            "SELECT title, start_page, end_page FROM sections WHERE document_id = ? ORDER BY position",
            [document_id],
        )
        .await?;
    let mut sections = Vec::new();
    while let Some(row) = rows.next().await? {
        sections.push(Section {
            title: row.get(0)?,
            start_page: row.get(1)?,
            end_page: row.get(2)?,
        });
    }
    Ok(sections)
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentPatch {
    pub title: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub description: Option<Option<String>>,
    pub authors: Option<Vec<String>>,
    pub subjects: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub page_offset: Option<i64>,
    pub sections: Option<Vec<Section>>,
}

fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

pub async fn update(conn: &Connection, document_id: i64, patch: &DocumentPatch) -> Result<()> {
    let title = patch.title.as_deref().map(required_title).transpose()?;
    let sections = patch.sections.clone().map(Sections::new).transpose()?;
    let description = patch.description.as_ref().map(|description| {
        description
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
    });

    let changed = conn
        .execute(
            "UPDATE documents SET
                 title       = coalesce(:title, title),
                 description = CASE WHEN :set_description THEN :description ELSE description END,
                 page_offset = coalesce(:page_offset, page_offset),
                 updated_at  = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = :id",
            libsql::named_params! {
                ":title": title,
                ":set_description": description.is_some(),
                ":description": description.flatten(),
                ":page_offset": patch.page_offset,
                ":id": document_id,
            },
        )
        .await?;
    if changed == 0 {
        return Err(Error::NotFound("document"));
    }

    if let Some(authors) = &patch.authors {
        replace_authors(conn, document_id, authors).await?;
    }
    if let Some(subjects) = &patch.subjects {
        replace_labels(conn, document_id, LabelKind::Subject, subjects).await?;
    }
    if let Some(tags) = &patch.tags {
        replace_labels(conn, document_id, LabelKind::Tag, tags).await?;
    }
    if let Some(sections) = &sections {
        replace_sections(conn, document_id, sections).await?;
    }
    Ok(())
}

pub const PAGE_SIZE: u32 = 50;

#[derive(Debug, Clone, Default)]
pub struct DocumentFilter {
    pub query: Option<String>,
    pub kind: Option<DocumentKind>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub tag: Option<String>,
}

impl DocumentFilter {
    fn params(&self) -> Vec<(String, Value)> {
        let given = |value: &Option<String>| -> Value {
            match value.as_deref().map(str::trim) {
                Some(text) if !text.is_empty() => text.to_string().into(),
                _ => Value::Null,
            }
        };
        vec![
            (":q".into(), given(&self.query)),
            (":kind".into(), self.kind.map(DocumentKind::as_str).into()),
            (":author".into(), given(&self.author)),
            (":subject".into(), given(&self.subject)),
            (":tag".into(), given(&self.tag)),
        ]
    }
}

#[derive(Debug, Serialize)]
pub struct DocumentSummary {
    pub id: i64,
    pub kind: DocumentKind,
    pub title: String,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub subjects: Vec<String>,
    pub tags: Vec<String>,
    pub has_file: bool,
    pub highlight_count: i64,
    pub note_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct DocumentPage {
    pub documents: Vec<DocumentSummary>,
    pub page: u32,
    pub page_size: u32,
    pub total: i64,
}

const MATCHING_DOCUMENTS: &str = "
    WITH RECURSIVE
    subject_tree (id) AS (
        SELECT id FROM labels WHERE kind = 'subject' AND name = :subject
        UNION
        SELECT child.id FROM labels child JOIN subject_tree parent ON child.parent_id = parent.id
    ),
    matching AS (
        SELECT d.id FROM documents d
        WHERE (:kind IS NULL OR d.kind = :kind)
          AND (:q IS NULL
               OR instr(lower(d.title), lower(:q)) > 0
               OR instr(lower(coalesce(d.description, '')), lower(:q)) > 0
               OR EXISTS (SELECT 1 FROM document_authors da JOIN authors a ON a.id = da.author_id
                          WHERE da.document_id = d.id AND instr(lower(a.name), lower(:q)) > 0))
          AND (:author IS NULL
               OR EXISTS (SELECT 1 FROM document_authors da JOIN authors a ON a.id = da.author_id
                          WHERE da.document_id = d.id AND a.name = :author))
          AND (:subject IS NULL
               OR EXISTS (SELECT 1 FROM document_labels dl
                          WHERE dl.document_id = d.id AND dl.label_id IN (SELECT id FROM subject_tree)))
          AND (:tag IS NULL
               OR EXISTS (SELECT 1 FROM document_labels dl JOIN labels l ON l.id = dl.label_id
                          WHERE dl.document_id = d.id AND l.kind = 'tag' AND l.name = :tag))
    )";

pub async fn list(conn: &Connection, filter: &DocumentFilter, page: u32) -> Result<DocumentPage> {
    if page == 0 {
        return Err(Error::invalid("invalid_page", "pages are numbered from 1"));
    }

    let mut rows = conn
        .query(
            &format!("{MATCHING_DOCUMENTS} SELECT count(*) FROM matching"),
            filter.params(),
        )
        .await?;
    let total: i64 = match rows.next().await? {
        Some(row) => row.get(0)?,
        None => 0,
    };

    let mut params = filter.params();
    params.push((":limit".into(), i64::from(PAGE_SIZE).into()));
    params.push((
        ":offset".into(),
        (i64::from(page - 1) * i64::from(PAGE_SIZE)).into(),
    ));
    let mut rows = conn
        .query(
            &format!(
                "{MATCHING_DOCUMENTS}
                 SELECT d.id, d.kind, d.title, d.description, d.object_key IS NOT NULL,
                        d.created_at, d.updated_at
                 FROM matching JOIN documents d ON d.id = matching.id
                 ORDER BY d.created_at DESC, d.id DESC
                 LIMIT :limit OFFSET :offset"
            ),
            params,
        )
        .await?;
    let mut documents = Vec::new();
    while let Some(row) = rows.next().await? {
        documents.push(DocumentSummary {
            id: row.get(0)?,
            kind: DocumentKind::parse(&row.get::<String>(1)?)?,
            title: row.get(2)?,
            description: row.get(3)?,
            has_file: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            authors: Vec::new(),
            subjects: Vec::new(),
            tags: Vec::new(),
            highlight_count: 0,
            note_count: 0,
        });
    }

    let ids: Vec<i64> = documents.iter().map(|document| document.id).collect();
    let mut authors = authors_of(conn, &ids).await?;
    let mut labels = labels_of(conn, &ids).await?;
    let counts = annotations::counts_of(conn, &ids).await?;
    for document in &mut documents {
        document.authors = authors.remove(&document.id).unwrap_or_default();
        let labels = labels.remove(&document.id).unwrap_or_default();
        document.subjects = labels.subjects;
        document.tags = labels.tags;
        let counts = counts.get(&document.id).copied().unwrap_or_default();
        document.highlight_count = counts.highlights;
        document.note_count = counts.notes;
    }

    Ok(DocumentPage {
        documents,
        page,
        page_size: PAGE_SIZE,
        total,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceIdentity {
    pub source: String,
    pub external_id: String,
    pub last_seen_at: String,
}

#[derive(Debug, Serialize)]
pub struct DocumentDetail {
    pub id: i64,
    pub kind: DocumentKind,
    pub title: String,
    pub description: Option<String>,
    pub canonical_url: Option<String>,
    pub original_filename: Option<String>,
    pub page_offset: i64,
    pub has_file: bool,
    pub created_at: String,
    pub updated_at: String,
    pub authors: Vec<String>,
    pub subjects: Vec<String>,
    pub tags: Vec<String>,
    pub sources: Vec<SourceIdentity>,
    pub sections: Vec<Section>,
    pub highlights: Vec<Highlight>,
    pub notes: Vec<Note>,
}

pub async fn detail(conn: &Connection, document_id: i64) -> Result<Lookup<DocumentDetail>> {
    let mut rows = conn
        .query(
            "SELECT kind, title, description, canonical_url, original_filename, page_offset,
                    object_key IS NOT NULL, created_at, updated_at
             FROM documents WHERE id = ?",
            [document_id],
        )
        .await?;
    let Some(row) = rows.next().await? else {
        return retired(conn, document_id).await;
    };

    let ids = [document_id];
    let labels = labels_of(conn, &ids)
        .await?
        .remove(&document_id)
        .unwrap_or_default();
    Ok(Lookup::Found(DocumentDetail {
        id: document_id,
        kind: DocumentKind::parse(&row.get::<String>(0)?)?,
        title: row.get(1)?,
        description: row.get(2)?,
        canonical_url: row.get(3)?,
        original_filename: row.get(4)?,
        page_offset: row.get(5)?,
        has_file: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        authors: authors_of(conn, &ids)
            .await?
            .remove(&document_id)
            .unwrap_or_default(),
        subjects: labels.subjects,
        tags: labels.tags,
        sources: sources_of(conn, &ids)
            .await?
            .remove(&document_id)
            .unwrap_or_default(),
        sections: sections_of(conn, document_id).await?,
        highlights: annotations::highlights_of(conn, document_id).await?,
        notes: annotations::notes_of(conn, document_id).await?,
    }))
}

#[derive(Debug)]
pub struct StoredFile {
    pub key: ObjectKey,
    pub original_filename: Option<String>,
}

pub async fn stored_file(conn: &Connection, document_id: i64) -> Result<Lookup<StoredFile>> {
    let mut rows = conn
        .query(
            "SELECT object_key, original_filename FROM documents WHERE id = ?",
            [document_id],
        )
        .await?;
    let Some(row) = rows.next().await? else {
        return retired(conn, document_id).await;
    };
    let key: Option<String> = row.get(0)?;
    let key = key.ok_or(Error::NotFound("file"))?;
    Ok(Lookup::Found(StoredFile {
        key: ObjectKey::parse(key)?,
        original_filename: row.get(1)?,
    }))
}

async fn retired<T>(conn: &Connection, document_id: i64) -> Result<Lookup<T>> {
    let mut rows = conn
        .query(
            "SELECT survivor_document_id FROM document_aliases WHERE alias_document_id = ?",
            [document_id],
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(Lookup::Merged {
            survivor_id: row.get(0)?,
        }),
        None => Err(Error::NotFound("document")),
    }
}

#[derive(Debug, Serialize)]
pub struct Facets {
    pub authors: Vec<Facet>,
    pub subjects: Vec<SubjectFacet>,
    pub tags: Vec<Facet>,
}

#[derive(Debug, Serialize)]
pub struct Facet {
    pub id: i64,
    pub name: String,
    pub document_count: i64,
}

#[derive(Debug, Serialize)]
pub struct SubjectFacet {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub parent: Option<String>,
    pub document_count: i64,
}

pub async fn facets(conn: &Connection) -> Result<Facets> {
    let mut rows = conn
        .query(
            "SELECT a.id, a.name, count(*) FROM authors a
             JOIN document_authors da ON da.author_id = a.id
             GROUP BY a.id ORDER BY a.name",
            (),
        )
        .await?;
    let mut authors = Vec::new();
    while let Some(row) = rows.next().await? {
        authors.push(Facet {
            id: row.get(0)?,
            name: row.get(1)?,
            document_count: row.get(2)?,
        });
    }

    let mut rows = conn
        .query(
            "SELECT l.id, l.name, count(*) FROM labels l
             JOIN document_labels dl ON dl.label_id = l.id
             WHERE l.kind = 'tag'
             GROUP BY l.id ORDER BY l.name",
            (),
        )
        .await?;
    let mut tags = Vec::new();
    while let Some(row) = rows.next().await? {
        tags.push(Facet {
            id: row.get(0)?,
            name: row.get(1)?,
            document_count: row.get(2)?,
        });
    }

    let mut rows = conn
        .query(
            // `UNION` rather than `UNION ALL` ends the recursion even if parents form a cycle.
            "WITH RECURSIVE descendant (subject_id, label_id) AS (
                 SELECT id, id FROM labels WHERE kind = 'subject'
                 UNION
                 SELECT descendant.subject_id, child.id
                 FROM labels child JOIN descendant ON child.parent_id = descendant.label_id
             )
             SELECT s.id, s.name, s.parent_id, parent.name, count(DISTINCT dl.document_id)
             FROM labels s
             LEFT JOIN labels parent ON parent.id = s.parent_id
             JOIN descendant ON descendant.subject_id = s.id
             LEFT JOIN document_labels dl ON dl.label_id = descendant.label_id
             WHERE s.kind = 'subject'
             GROUP BY s.id ORDER BY s.name",
            (),
        )
        .await?;
    let mut subjects = Vec::new();
    while let Some(row) = rows.next().await? {
        subjects.push(SubjectFacet {
            id: row.get(0)?,
            name: row.get(1)?,
            parent_id: row.get(2)?,
            parent: row.get(3)?,
            document_count: row.get(4)?,
        });
    }

    Ok(Facets {
        authors,
        subjects,
        tags,
    })
}

pub(crate) async fn authors_of(conn: &Connection, document_ids: &[i64]) -> Result<HashMap<i64, Vec<String>>> {
    let mut rows = conn
        .query(
            "SELECT da.document_id, a.name FROM document_authors da
             JOIN authors a ON a.id = da.author_id
             WHERE da.document_id IN (SELECT value FROM json_each(?))
             ORDER BY da.document_id, da.position",
            [id_array(document_ids)],
        )
        .await?;
    let mut authors: HashMap<i64, Vec<String>> = HashMap::new();
    while let Some(row) = rows.next().await? {
        authors.entry(row.get(0)?).or_default().push(row.get(1)?);
    }
    Ok(authors)
}

#[derive(Debug, Default)]
pub(crate) struct Labels {
    pub subjects: Vec<String>,
    pub tags: Vec<String>,
}

pub(crate) async fn labels_of(conn: &Connection, document_ids: &[i64]) -> Result<HashMap<i64, Labels>> {
    let mut rows = conn
        .query(
            "SELECT dl.document_id, l.kind, l.name FROM document_labels dl
             JOIN labels l ON l.id = dl.label_id
             WHERE dl.document_id IN (SELECT value FROM json_each(?))
             ORDER BY l.name",
            [id_array(document_ids)],
        )
        .await?;
    let mut labels: HashMap<i64, Labels> = HashMap::new();
    while let Some(row) = rows.next().await? {
        let document = labels.entry(row.get(0)?).or_default();
        let name: String = row.get(2)?;
        match row.get::<String>(1)?.as_str() {
            "subject" => document.subjects.push(name),
            _ => document.tags.push(name),
        }
    }
    Ok(labels)
}

pub(crate) async fn sources_of(
    conn: &Connection,
    document_ids: &[i64],
) -> Result<HashMap<i64, Vec<SourceIdentity>>> {
    let mut rows = conn
        .query(
            "SELECT document_id, source, external_id, last_seen_at FROM document_sources
             WHERE document_id IN (SELECT value FROM json_each(?))
             ORDER BY source, external_id",
            [id_array(document_ids)],
        )
        .await?;
    let mut sources: HashMap<i64, Vec<SourceIdentity>> = HashMap::new();
    while let Some(row) = rows.next().await? {
        sources.entry(row.get(0)?).or_default().push(SourceIdentity {
            source: row.get(1)?,
            external_id: row.get(2)?,
            last_seen_at: row.get(3)?,
        });
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_deduplicated_in_order() {
        let names = [
            "  Ama Ata Aidoo ".to_string(),
            "".into(),
            "ama ata aidoo".into(),
            "Ayi Kwei Armah".into(),
        ];
        assert_eq!(distinct_names(&names), vec!["Ama Ata Aidoo", "Ayi Kwei Armah"]);
    }

    fn section(title: &str, start_page: i64, end_page: i64) -> Section {
        Section {
            title: title.into(),
            start_page,
            end_page,
        }
    }

    fn rejection(sections: Vec<Section>) -> &'static str {
        match Sections::new(sections) {
            Err(Error::Invalid { code, .. }) => code,
            other => panic!("expected an invalid-sections error, got {other:?}"),
        }
    }

    #[test]
    fn sections_must_be_ordered_and_disjoint() {
        let valid = Sections::new(vec![section(" One ", 1, 10), section("Two", 11, 11)]).unwrap();
        assert_eq!(valid.0[0].title, "One", "titles are trimmed");

        assert_eq!(rejection(vec![section("  ", 1, 2)]), "section_title_required");
        assert_eq!(rejection(vec![section("A", 0, 2)]), "section_page_out_of_range");
        assert_eq!(rejection(vec![section("A", 5, 4)]), "section_range_inverted");
        assert_eq!(
            rejection(vec![section("A", 1, 10), section("B", 10, 20)]),
            "sections_overlap"
        );
        assert_eq!(
            rejection(vec![section("A", 20, 30), section("B", 1, 5)]),
            "sections_out_of_order"
        );
    }
}
