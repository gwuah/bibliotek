use std::collections::BTreeMap;

use serde::Serialize;

use super::source::AppliedMigration;
use crate::import::snapshot::ImportStats;

#[derive(Debug, Serialize)]
pub struct MigrationReport {
    pub succeeded: bool,
    pub dry_run: bool,
    pub committed: bool,
    pub snapshot: SnapshotIdentity,
    pub importer_build_revision: &'static str,
    pub tables: BTreeMap<&'static str, TableCounts>,
    pub id_mapping: IdMapping,
    pub merge_candidates: Vec<MergeCandidate>,
    pub relationships: Relationships,
    pub parents: ParentVerification,
    pub sections: SectionReport,
    pub object_references: ObjectReferences,
    pub suspicious_labels: Vec<SuspiciousLabel>,
    pub light_installations: BTreeMap<String, u64>,
    pub imports: Vec<ImportStats>,
    pub not_imported: Vec<Exclusion>,
    pub unexplained_losses: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SnapshotIdentity {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub migrations: Vec<AppliedMigration>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TableCounts {
    pub read: u64,
    pub written: u64,
    pub merged: u64,
    pub skipped: u64,
    pub already_deleted: u64,
    pub failed: u64,
}

impl TableCounts {
    pub fn accounted(&self) -> u64 {
        self.written + self.merged + self.skipped + self.already_deleted + self.failed
    }
}

#[derive(Debug, Default, Serialize)]
pub struct IdMapping {
    pub entries: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
pub struct MergeCandidate {
    pub title: String,
    pub book_id: i64,
    pub book_document_id: i64,
    pub resource_id: i64,
    pub resource_document_id: i64,
    pub decision: &'static str,
}

#[derive(Debug, Default, Serialize)]
pub struct Relationships {
    pub authors: Relationship,
    pub subjects: Relationship,
    pub tags: Relationship,
    pub subject_parents: Relationship,
    pub tag_parents: Relationship,
    pub case_merged_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Relationship {
    pub read: u64,
    pub written: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct ParentVerification {
    pub highlights_verified: u64,
    pub comments_verified: u64,
    pub notes_verified: u64,
    pub mismatches: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct SectionReport {
    pub documents_with_chapters: u64,
    pub chapters_read: u64,
    pub sections_written: u64,
    pub page_offsets_preserved: u64,
    pub problems: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ObjectReferences {
    pub parsed: u64,
    pub unparsed: Vec<UnparsedObject>,
    pub hosts: BTreeMap<String, u64>,
    pub existence_checked: bool,
}

#[derive(Debug, Serialize)]
pub struct UnparsedObject {
    pub book_id: i64,
    pub url: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct SuspiciousLabel {
    pub kind: &'static str,
    pub name: String,
    pub reason: &'static str,
    pub books: u64,
}

#[derive(Debug, Serialize)]
pub struct Exclusion {
    pub table: String,
    pub rows: u64,
    pub reason: String,
}
