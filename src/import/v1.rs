mod object_url;
mod plan;
mod report;
mod sections;
mod source;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use libsql::{Connection, params};
use sha2::{Digest, Sha256};

use self::object_url::ObjectReference;
use self::plan::{Address, CommentRoute, NoteRoute, Owner, Plan};
use self::sections::ChapterPlan;
use self::source::{Label, Link, V1};
use crate::build;
use crate::db::Db;
use crate::domain::documents::{self, DocumentKind, LabelKind, NewDocument};
use crate::domain::returning_id;
use crate::error::{Error, Result};
use crate::import::snapshot::{self, EmptySnapshot};

pub use self::report::*;

pub const SOURCE: &str = "bibliotek-v1";

pub async fn run(db: &Db, snapshot_path: &Path, dry_run: bool) -> Result<MigrationReport> {
    let (size_bytes, sha256) = fingerprint(snapshot_path)?;
    let v1 = V1::read(snapshot_path).await?;
    let mut report = MigrationReport {
        succeeded: false,
        dry_run,
        committed: false,
        snapshot: SnapshotIdentity {
            path: snapshot_path.display().to_string(),
            size_bytes,
            sha256,
            migrations: v1.migrations.clone(),
        },
        importer_build_revision: build::GIT_REVISION,
        tables: BTreeMap::new(),
        id_mapping: IdMapping::default(),
        merge_candidates: Vec::new(),
        relationships: Relationships::default(),
        parents: ParentVerification::default(),
        sections: SectionReport::default(),
        object_references: ObjectReferences::default(),
        suspicious_labels: Vec::new(),
        light_installations: BTreeMap::new(),
        imports: Vec::new(),
        not_imported: Vec::new(),
        unexplained_losses: Vec::new(),
    };
    let plan = plan::plan(&v1, &mut report)?;

    let tx = db.begin().await?;
    refuse_non_empty_target(&tx).await?;
    Importer {
        conn: &tx,
        v1: &v1,
        plan: &plan,
        report: &mut report,
        mapping: Vec::new(),
    }
    .import()
    .await?;

    report.succeeded = report.unexplained_losses.is_empty();
    if report.succeeded && !dry_run {
        tx.commit().await?;
        report.committed = true;
    }
    Ok(report)
}

/// Refuses a database with a live WAL beside it: that is a copy of a running database,
/// not a consistent snapshot.
fn fingerprint(path: &Path) -> Result<(u64, String)> {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    if std::fs::metadata(&wal).is_ok_and(|wal| wal.len() > 0) {
        return Err(Error::invalid(
            "snapshot_not_consistent",
            "the snapshot has a write-ahead log beside it; take a consistent copy with \
             `sqlite3 bibliotek.db \".backup snapshot.db\"` or `VACUUM INTO`",
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| {
        Error::invalid(
            "snapshot_missing",
            format!("cannot read {}: {error}", path.display()),
        )
    })?;
    Ok((bytes.len() as u64, hex::encode(Sha256::digest(&bytes))))
}

async fn refuse_non_empty_target(conn: &Connection) -> Result<()> {
    let mut rows = conn.query("SELECT count(*) FROM documents", ()).await?;
    let documents: i64 = match rows.next().await? {
        Some(row) => row.get(0)?,
        None => 0,
    };
    if documents > 0 {
        return Err(Error::conflict(
            "target_not_empty",
            format!("the library already has {documents} documents; import-v1 only runs into an empty one"),
        ));
    }
    Ok(())
}

struct Importer<'a> {
    conn: &'a Connection,
    v1: &'a V1,
    plan: &'a Plan<'a>,
    report: &'a mut MigrationReport,
    mapping: Vec<String>,
}

impl Importer<'_> {
    async fn import(mut self) -> Result<()> {
        self.write_labels(LabelKind::Subject, "categories", &self.v1.categories)
            .await?;
        self.write_labels(LabelKind::Tag, "tags", &self.v1.tags).await?;
        self.write_authors().await?;
        let books = self.write_books().await?;
        self.count_relationships().await?;

        for snapshot in &self.plan.snapshots {
            let stats = snapshot::apply(self.conn, snapshot, EmptySnapshot::Refuse).await?;
            self.report.imports.push(stats);
        }
        let resources = self.write_resources().await?;
        let highlights = self.verify_highlights(&resources).await?;
        self.write_comments(&highlights).await?;
        self.write_notes(&resources).await?;

        self.find_merge_candidates(&books, &resources);
        self.report_exclusions();
        self.check_totals().await?;
        self.check_balances();
        self.report.id_mapping = digest(std::mem::take(&mut self.mapping));
        self.record_run().await
    }

    fn counts(&mut self, table: &'static str) -> &mut TableCounts {
        self.report.tables.entry(table).or_default()
    }

    fn map(&mut self, from: &str, from_id: i64, to: &str, to_id: i64) {
        self.mapping.push(format!("{from}:{from_id}={to}:{to_id}"));
    }

    fn lose(&mut self, what: String) {
        self.report.unexplained_losses.push(what);
    }

    async fn write_labels(&mut self, kind: LabelKind, table: &'static str, labels: &[Label]) -> Result<()> {
        let mut ids = HashMap::new();
        let mut written = HashSet::new();
        for label in labels {
            self.counts(table).read += 1;
            let id = returning_id(
                self.conn,
                "INSERT INTO labels (kind, name) VALUES (?, ?)
                 ON CONFLICT (kind, name) DO UPDATE SET name = name RETURNING id",
                params![kind.as_str(), label.name.trim()],
            )
            .await?;
            if written.insert(id) {
                self.counts(table).written += 1;
            } else {
                self.counts(table).merged += 1;
                self.report
                    .relationships
                    .case_merged_names
                    .push(format!("{table}: {}", label.name));
            }
            ids.insert(label.id, id);
        }

        for label in labels {
            let Some(parent) = label.parent_id else {
                continue;
            };
            let relationship = match kind {
                LabelKind::Subject => &mut self.report.relationships.subject_parents,
                LabelKind::Tag => &mut self.report.relationships.tag_parents,
            };
            relationship.read += 1;
            match (ids.get(&label.id), ids.get(&parent)) {
                (Some(&id), Some(&parent_id)) if id != parent_id => {
                    relationship.written += self
                        .conn
                        .execute(
                            "UPDATE labels SET parent_id = ? WHERE id = ? AND parent_id IS NULL",
                            params![parent_id, id],
                        )
                        .await?;
                }
                _ => self.lose(format!(
                    "{table} #{}: its parent #{parent} could not be linked",
                    label.id
                )),
            }
        }
        Ok(())
    }

    async fn write_authors(&mut self) -> Result<()> {
        for author in &self.v1.authors {
            self.counts("authors").read += 1;
            if author.name.trim().is_empty() {
                self.counts("authors").failed += 1;
                self.lose(format!("authors #{}: the name is blank", author.id));
                continue;
            }
            let inserted = self
                .conn
                .execute(
                    "INSERT INTO authors (name) VALUES (?) ON CONFLICT (name) DO NOTHING",
                    [author.name.trim()],
                )
                .await?;
            if inserted > 0 {
                self.counts("authors").written += 1;
            } else {
                self.counts("authors").merged += 1;
                self.report
                    .relationships
                    .case_merged_names
                    .push(format!("authors: {}", author.name));
            }
        }
        Ok(())
    }

    async fn write_books(&mut self) -> Result<HashMap<i64, i64>> {
        let books: HashSet<i64> = self.v1.books.iter().map(|book| book.id).collect();
        let authors = names(
            self.v1
                .authors
                .iter()
                .map(|author| (author.id, author.name.as_str())),
        );
        let categories = names(
            self.v1
                .categories
                .iter()
                .map(|label| (label.id, label.name.as_str())),
        );
        let tags = names(self.v1.tags.iter().map(|label| (label.id, label.name.as_str())));
        let authors = self.group_links("book_authors", &self.v1.book_authors, &books, &authors);
        let subjects = self.group_links("book_categories", &self.v1.book_categories, &books, &categories);
        let tags = self.group_links("book_tags", &self.v1.book_tags, &books, &tags);

        let mut documents = HashMap::new();
        let mut object_keys = HashSet::new();
        let mut buckets = HashSet::new();
        for book in &self.v1.books {
            self.counts("books").read += 1;
            for (field, populated) in [
                ("cover_url", book.cover_url.is_some()),
                ("ratings", book.ratings.is_some()),
                ("pages", book.pages.is_some()),
            ] {
                if populated {
                    self.lose(format!(
                        "books #{}: `{field}` is set, and liotek does not keep it",
                        book.id
                    ));
                }
            }

            let mut new = NewDocument::new(DocumentKind::Pdf, &book.title);
            new.description = book
                .description
                .clone()
                .filter(|description| !description.trim().is_empty());
            new.authors = authors.get(&book.id).cloned().unwrap_or_default();
            new.subjects = subjects.get(&book.id).cloned().unwrap_or_default();
            new.tags = tags.get(&book.id).cloned().unwrap_or_default();
            match ObjectReference::parse(&book.url) {
                Ok(object) if !object_keys.insert(object.key.clone()) => {
                    self.lose(format!(
                        "books #{}: object key `{}` is shared with another book",
                        book.id, object.key
                    ));
                }
                Ok(object) => {
                    self.report.object_references.parsed += 1;
                    *self
                        .report
                        .object_references
                        .hosts
                        .entry(object.host.clone())
                        .or_default() += 1;
                    buckets.extend(object.bucket.clone());
                    new.original_filename = Some(object.original_filename());
                    new.object_key = Some(object.key);
                }
                Err(reason) => {
                    self.lose(format!("books #{}: object URL not parsed: {reason}", book.id));
                    self.report.object_references.unparsed.push(UnparsedObject {
                        book_id: book.id,
                        url: book.url.clone(),
                        reason,
                    });
                }
            }

            let document_id = documents::create(self.conn, &new).await?;
            self.conn
                .execute(
                    "UPDATE documents SET created_at = ?, updated_at = ? WHERE id = ?",
                    params![book.created_at.as_str(), book.updated_at.as_str(), document_id],
                )
                .await?;
            documents::add_source(self.conn, document_id, SOURCE, &book.id.to_string()).await?;
            self.counts("books").written += 1;
            self.map("book", book.id, "document", document_id);
            documents.insert(book.id, document_id);
        }

        if buckets.len() > 1 {
            self.lose(format!(
                "book objects live in {} buckets ({:?}), but a library reads from one",
                buckets.len(),
                buckets
            ));
        }
        Ok(documents)
    }

    fn group_links(
        &mut self,
        table: &'static str,
        links: &[Link],
        books: &HashSet<i64>,
        targets: &HashMap<i64, String>,
    ) -> HashMap<i64, Vec<String>> {
        let mut grouped: HashMap<i64, Vec<String>> = HashMap::new();
        for link in links {
            self.counts(table).read += 1;
            match (
                link.book_id.filter(|book| books.contains(book)),
                link.target_id.and_then(|id| targets.get(&id)),
            ) {
                (Some(book), Some(name)) => grouped.entry(book).or_default().push(name.clone()),
                _ => {
                    self.counts(table).failed += 1;
                    self.lose(format!(
                        "{table}: link {:?} -> {:?} points at a row that does not exist",
                        link.book_id, link.target_id
                    ));
                }
            }
        }
        grouped
    }

    async fn count_relationships(&mut self) -> Result<()> {
        const LABEL_LINKS: &str =
            "SELECT count(*) FROM document_labels dl JOIN labels l ON l.id = dl.label_id WHERE l.kind = ?";
        self.report.relationships.authors = self
            .relationship("book_authors", "SELECT count(*) FROM document_authors", ())
            .await?;
        self.report.relationships.subjects = self
            .relationship("book_categories", LABEL_LINKS, [LabelKind::Subject.as_str()])
            .await?;
        self.report.relationships.tags = self
            .relationship("book_tags", LABEL_LINKS, [LabelKind::Tag.as_str()])
            .await?;
        Ok(())
    }

    async fn relationship(
        &mut self,
        table: &'static str,
        written_sql: &str,
        params: impl libsql::params::IntoParams,
    ) -> Result<Relationship> {
        let written = count(self.conn, written_sql, params).await?;
        let counts = self.counts(table);
        counts.written = written;
        counts.merged = (counts.read - counts.failed).saturating_sub(written);
        Ok(Relationship {
            read: counts.read,
            written,
        })
    }

    async fn write_resources(&mut self) -> Result<HashMap<i64, i64>> {
        let mut documents = HashMap::new();
        let mut claimed = HashSet::new();
        for planned in &self.plan.resources {
            let resource = planned.resource;
            let document_id = match &planned.owner {
                Owner::Research(item_id) => {
                    identity_document(self.conn, documents::source::RESEARCH, item_id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("Research item {item_id} was not imported"))?
                }
                Owner::Web(url) => match document_by_url(self.conn, url).await? {
                    Some(document_id) => document_id,
                    None => {
                        let mut new = NewDocument::new(DocumentKind::Web, url);
                        new.canonical_url = Some(url.clone());
                        documents::create(self.conn, &new).await?
                    }
                },
                Owner::Local => {
                    documents::create(self.conn, &NewDocument::new(DocumentKind::Pdf, &resource.title))
                        .await?
                }
            };

            if claimed.insert(document_id) {
                self.counts("resources").written += 1;
                self.conn
                    .execute(
                        "UPDATE documents SET created_at = ?1, updated_at = ?1 WHERE id = ?2",
                        params![resource.created_at.as_str(), document_id],
                    )
                    .await?;
                if let Some(chapters) = &planned.chapters {
                    self.write_sections(document_id, resource.id, chapters).await?;
                }
            } else {
                self.counts("resources").merged += 1;
                if planned.chapters.is_some() {
                    self.lose(format!(
                        "resources #{}: shares a document with another resource, so its chapters were not written",
                        resource.id
                    ));
                }
            }
            self.map("resource", resource.id, "document", document_id);
            documents.insert(resource.id, document_id);
        }
        Ok(documents)
    }

    async fn write_sections(
        &mut self,
        document_id: i64,
        resource_id: i64,
        chapters: &ChapterPlan,
    ) -> Result<()> {
        let report = &mut self.report.sections;
        report.documents_with_chapters += 1;
        report.chapters_read += chapters.chapters_read as u64;
        for problem in &chapters.problems {
            let problem = format!("resources #{resource_id}: {problem}");
            report.problems.push(problem.clone());
            self.report.unexplained_losses.push(problem);
        }
        for (position, section) in chapters.sections.iter().enumerate() {
            self.conn
                .execute(
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
            self.report.sections.sections_written += 1;
        }
        if chapters.page_offset != 0 {
            self.conn
                .execute(
                    "UPDATE documents SET page_offset = ? WHERE id = ?",
                    params![chapters.page_offset, document_id],
                )
                .await?;
            self.report.sections.page_offsets_preserved += 1;
        }
        Ok(())
    }

    async fn verify_highlights(&mut self, resources: &HashMap<i64, i64>) -> Result<HashMap<i64, i64>> {
        let mut highlights = HashMap::new();
        for route in &self.plan.highlights {
            let expected = resources.get(&route.resource_id).copied();
            match find_highlight(self.conn, &route.address).await? {
                Some((highlight_id, document_id)) if Some(document_id) == expected => {
                    self.counts("annotations").written += 1;
                    self.report.parents.highlights_verified += 1;
                    self.map("annotation", route.annotation_id, "highlight", highlight_id);
                    highlights.insert(route.annotation_id, highlight_id);
                }
                found => self.mismatch(format!(
                    "annotations #{}: expected on document {expected:?}, found {found:?}",
                    route.annotation_id
                )),
            }
        }
        Ok(highlights)
    }

    async fn write_comments(&mut self, highlights: &HashMap<i64, i64>) -> Result<()> {
        for route in &self.plan.comments {
            match route {
                CommentRoute::Source {
                    comment_id,
                    annotation_id,
                    external_id,
                } => {
                    let found = match highlights.get(annotation_id) {
                        Some(&highlight_id) => find_comment(self.conn, highlight_id, external_id).await?,
                        None => None,
                    };
                    match found {
                        Some(new_id) => {
                            self.counts("comments").written += 1;
                            self.report.parents.comments_verified += 1;
                            self.map("comment", *comment_id, "comment", new_id);
                        }
                        None => self.mismatch(format!(
                            "comments #{comment_id}: not found under annotation #{annotation_id}'s highlight"
                        )),
                    }
                }
                CommentRoute::Local { comment } => {
                    let Some(&highlight_id) = highlights.get(&comment.parent_id) else {
                        self.mismatch(format!(
                            "comments #{}: its highlight was not imported",
                            comment.id
                        ));
                        continue;
                    };
                    let new_id = returning_id(
                        self.conn,
                        "INSERT INTO comments (highlight_id, body, created_at, updated_at)
                         VALUES (?, ?, ?, ?) RETURNING id",
                        params![
                            highlight_id,
                            comment.content.as_str(),
                            comment.created_at.as_str(),
                            comment.updated_at.as_str()
                        ],
                    )
                    .await?;
                    self.counts("comments").written += 1;
                    self.report.parents.comments_verified += 1;
                    self.map("comment", comment.id, "comment", new_id);
                }
            }
        }
        Ok(())
    }

    async fn write_notes(&mut self, resources: &HashMap<i64, i64>) -> Result<()> {
        for route in &self.plan.notes {
            match route {
                NoteRoute::Source {
                    note_id,
                    resource_id,
                    address,
                } => {
                    let expected = resources.get(resource_id).copied();
                    match find_note(self.conn, address).await? {
                        Some((new_id, document_id)) if Some(document_id) == expected => {
                            self.counts("notes").written += 1;
                            self.report.parents.notes_verified += 1;
                            self.map("note", *note_id, "note", new_id);
                        }
                        found => self.mismatch(format!(
                            "notes #{note_id}: expected on document {expected:?}, found {found:?}"
                        )),
                    }
                }
                NoteRoute::Local { note } => {
                    let Some(&document_id) = resources.get(&note.parent_id) else {
                        self.mismatch(format!("notes #{}: its resource has no document", note.id));
                        continue;
                    };
                    let new_id = returning_id(
                        self.conn,
                        "INSERT INTO notes (document_id, body, created_at, updated_at) VALUES (?, ?, ?, ?) RETURNING id",
                        params![
                            document_id,
                            note.content.as_str(),
                            note.created_at.as_str(),
                            note.updated_at.as_str()
                        ],
                    )
                    .await?;
                    self.counts("notes").written += 1;
                    self.report.parents.notes_verified += 1;
                    self.map("note", note.id, "note", new_id);
                }
            }
        }
        Ok(())
    }

    fn mismatch(&mut self, what: String) {
        self.report.parents.mismatches.push(what.clone());
        self.lose(what);
    }

    fn find_merge_candidates(&mut self, books: &HashMap<i64, i64>, resources: &HashMap<i64, i64>) {
        let normalize = |title: &str| title.trim().to_lowercase();
        for book in &self.v1.books {
            for planned in &self.plan.resources {
                let resource = planned.resource;
                if !matches!(planned.owner, Owner::Research(_))
                    || normalize(&resource.title) != normalize(&book.title)
                {
                    continue;
                }
                if let (Some(&book_document_id), Some(&resource_document_id)) =
                    (books.get(&book.id), resources.get(&resource.id))
                {
                    self.report.merge_candidates.push(MergeCandidate {
                        title: book.title.clone(),
                        book_id: book.id,
                        book_document_id,
                        resource_id: resource.id,
                        resource_document_id,
                        decision: "kept apart; review in the duplicate workflow",
                    });
                }
            }
        }
    }

    fn report_exclusions(&mut self) {
        let v1 = self.v1;
        let exclude = |report: &mut MigrationReport, table: &'static str, rows: u64, reason: String| {
            let counts = report.tables.entry(table).or_default();
            counts.read += rows;
            counts.skipped += rows;
            report.not_imported.push(Exclusion {
                table: table.to_string(),
                rows,
                reason,
            });
        };
        exclude(
            self.report,
            "words",
            v1.words,
            "the vocabulary feature is retired; any rows remain in the retained snapshot".to_string(),
        );
        for (table, rows) in &v1.stale_tables {
            exclude(
                self.report,
                table,
                *rows,
                "left by a deleted reader experiment; ignored, never copied or modified".to_string(),
            );
        }
        if let Some(path) = &v1.research_config {
            exclude(
                self.report,
                "research_config",
                1,
                format!(
                    "runtime configuration: set `research: {{ database_path: {path} }}` in liotek's config file"
                ),
            );
        }

        let skipped = self
            .report
            .tables
            .get("resources")
            .map_or(0, |counts| counts.skipped);
        if skipped > 0 {
            self.report.not_imported.push(Exclusion {
                table: "resources".to_string(),
                rows: skipped,
                reason: "live web or PDF resources holding no highlights, notes, or chapters".to_string(),
            });
        }
        let light_colors = self
            .plan
            .highlights
            .iter()
            .filter(|route| route.address.source.starts_with("light:"))
            .count() as u64;
        if light_colors > 0 {
            self.report.not_imported.push(Exclusion {
                table: "annotations.color".to_string(),
                rows: light_colors,
                reason: "Bibliotek stamped every Light highlight \"yellow\"; Light has no colours, so none is kept"
                    .to_string(),
            });
        }
        let mut statuses: BTreeMap<&str, u64> = BTreeMap::new();
        for book in &v1.books {
            *statuses
                .entry(book.status.as_deref().unwrap_or("NULL"))
                .or_default() += 1;
        }
        for (status, rows) in statuses {
            self.report.not_imported.push(Exclusion {
                table: format!("books.status = {status}"),
                rows,
                reason: "upload state, which liotek tracks in upload sessions instead".to_string(),
            });
        }
        self.report.suspicious_labels = suspicious_labels(v1);
    }

    async fn check_totals(&mut self) -> Result<()> {
        let held = [
            (
                "books",
                count(
                    self.conn,
                    "SELECT count(*) FROM document_sources WHERE source = ?",
                    [SOURCE],
                )
                .await?,
            ),
            (
                "annotations",
                count(self.conn, "SELECT count(*) FROM highlights", ()).await?,
            ),
            (
                "comments",
                count(self.conn, "SELECT count(*) FROM comments", ()).await?,
            ),
            ("notes", count(self.conn, "SELECT count(*) FROM notes", ()).await?),
        ];
        for (table, held) in held {
            let written = self.counts(table).written;
            if held != written {
                self.lose(format!(
                    "{table}: {written} rows written but the library holds {held}"
                ));
            }
        }
        Ok(())
    }

    fn check_balances(&mut self) {
        let unbalanced: Vec<String> = self
            .report
            .tables
            .iter()
            .filter(|(_, counts)| counts.read != counts.accounted())
            .map(|(table, counts)| {
                format!(
                    "{table}: read {} but accounted for {}",
                    counts.read,
                    counts.accounted()
                )
            })
            .collect();
        self.report.unexplained_losses.extend(unbalanced);
    }

    async fn record_run(&mut self) -> Result<()> {
        let stats = serde_json::to_string(&self.report.tables).map_err(anyhow::Error::from)?;
        self.conn
            .execute(
                "INSERT INTO import_runs (source, stats_json) VALUES (?, ?)",
                params![SOURCE, stats],
            )
            .await?;
        Ok(())
    }
}

fn names<'a>(rows: impl Iterator<Item = (i64, &'a str)>) -> HashMap<i64, String> {
    rows.map(|(id, name)| (id, name.to_string())).collect()
}

fn digest(mut lines: Vec<String>) -> IdMapping {
    lines.sort();
    let mut hasher = Sha256::new();
    for line in &lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    IdMapping {
        entries: lines.len() as u64,
        sha256: hex::encode(hasher.finalize()),
    }
}

fn suspicious_labels(v1: &V1) -> Vec<SuspiciousLabel> {
    let uses =
        |links: &[Link], id: i64| links.iter().filter(|link| link.target_id == Some(id)).count() as u64;
    let categories = v1
        .categories
        .iter()
        .map(|label| ("subject", label, uses(&v1.book_categories, label.id)));
    let tags = v1
        .tags
        .iter()
        .map(|label| ("tag", label, uses(&v1.book_tags, label.id)));
    categories
        .chain(tags)
        .filter_map(|(kind, label, books)| {
            suspicion(&label.name).map(|reason| SuspiciousLabel {
                kind,
                name: label.name.clone(),
                reason,
                books,
            })
        })
        .collect()
}

fn suspicion(name: &str) -> Option<&'static str> {
    const PLACEHOLDERS: &[&str] = &["undefined", "null", "none", "unknown", "untitled", "n/a"];
    const PRODUCER_WORDS: &[&str] = &[
        "print",
        "dpi",
        "pdf",
        "acrobat",
        "distiller",
        "ghostscript",
        "quartz",
        "latex",
        "pdftex",
        "microsoft",
        "adobe",
    ];
    let lower = name.trim().to_lowercase();
    if lower.is_empty() {
        Some("blank")
    } else if PLACEHOLDERS.contains(&lower.as_str()) {
        Some("placeholder value")
    } else if PRODUCER_WORDS.iter().any(|word| lower.contains(word)) {
        Some("looks like printer or PDF-producer metadata")
    } else if lower
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_ascii_punctuation() || c == ' ')
    {
        Some("only digits or punctuation")
    } else if lower.len() > 60 || lower.contains([';', '=']) {
        Some("long or structured, like embedded metadata")
    } else {
        None
    }
}

async fn count(conn: &Connection, sql: &str, params: impl libsql::params::IntoParams) -> Result<u64> {
    let mut rows = conn.query(sql, params).await?;
    let count: i64 = match rows.next().await? {
        Some(row) => row.get(0)?,
        None => 0,
    };
    Ok(count.max(0) as u64)
}

async fn identity_document(conn: &Connection, source: &str, external_id: &str) -> Result<Option<i64>> {
    let mut rows = conn
        .query(
            "SELECT document_id FROM document_sources WHERE source = ? AND external_id = ?",
            [source, external_id],
        )
        .await?;
    Ok(match rows.next().await? {
        Some(row) => Some(row.get(0)?),
        None => None,
    })
}

async fn document_by_url(conn: &Connection, url: &str) -> Result<Option<i64>> {
    let mut rows = conn
        .query("SELECT id FROM documents WHERE canonical_url = ?", [url])
        .await?;
    Ok(match rows.next().await? {
        Some(row) => Some(row.get(0)?),
        None => None,
    })
}

async fn find_highlight(conn: &Connection, address: &Address) -> Result<Option<(i64, i64)>> {
    let mut rows = conn
        .query(
            "SELECT h.id, h.document_id FROM highlights h JOIN document_sources s ON s.id = h.document_source_id
             WHERE s.source = ? AND s.external_id = ? AND h.external_id = ?",
            [
                address.source.as_str(),
                address.document.as_str(),
                address.external_id.as_str(),
            ],
        )
        .await?;
    Ok(match rows.next().await? {
        Some(row) => Some((row.get(0)?, row.get(1)?)),
        None => None,
    })
}

async fn find_comment(conn: &Connection, highlight_id: i64, external_id: &str) -> Result<Option<i64>> {
    let mut rows = conn
        .query(
            "SELECT id FROM comments WHERE highlight_id = ? AND external_id = ?",
            params![highlight_id, external_id],
        )
        .await?;
    Ok(match rows.next().await? {
        Some(row) => Some(row.get(0)?),
        None => None,
    })
}

async fn find_note(conn: &Connection, address: &Address) -> Result<Option<(i64, i64)>> {
    let mut rows = conn
        .query(
            "SELECT n.id, n.document_id FROM notes n JOIN document_sources s ON s.id = n.document_source_id
             WHERE s.source = ? AND s.external_id = ? AND n.external_id = ?",
            [
                address.source.as_str(),
                address.document.as_str(),
                address.external_id.as_str(),
            ],
        )
        .await?;
    Ok(match rows.next().await? {
        Some(row) => Some((row.get(0)?, row.get(1)?)),
        None => None,
    })
}
