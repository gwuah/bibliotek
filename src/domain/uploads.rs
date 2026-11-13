//! A write transaction is never held across an object-store call. Each step is recorded
//! before the next begins, so a retry can finish an interrupted one; `object_ready`
//! records the assembled object before the document exists, so a failed commit is
//! repaired rather than leaked.

use std::fmt;
use std::path::Path;

use bytes::Bytes;
use libsql::{Connection, params};
use serde::{Deserialize, Serialize};

use super::documents::{self, DocumentKind, NewDocument, source};
use crate::blob::{BlobError, BlobStore, Multipart, ObjectKey, StoredPart};
use crate::db::Db;
use crate::error::{Error, Result};

const MIB: u64 = 1024 * 1024;

pub const DEFAULT_PART_SIZE: u64 = 8 * MIB;

/// S3-compatible providers reject non-final parts smaller than this.
pub const MIN_PROVIDER_PART_SIZE: u64 = 5 * MIB;

const _: () = assert!(DEFAULT_PART_SIZE >= MIN_PROVIDER_PART_SIZE);

const MAX_PARTS: u64 = 10_000;

const CONTENT_TYPE: &str = "application/pdf";

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";

const EXPIRES_AT: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+7 days')";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadStatus {
    Created,
    Uploading,
    Completing,
    ObjectReady,
    Committed,
    Failed,
    Aborting,
    Aborted,
    Expired,
}

impl UploadStatus {
    const ALL: [UploadStatus; 9] = [
        UploadStatus::Created,
        UploadStatus::Uploading,
        UploadStatus::Completing,
        UploadStatus::ObjectReady,
        UploadStatus::Committed,
        UploadStatus::Failed,
        UploadStatus::Aborting,
        UploadStatus::Aborted,
        UploadStatus::Expired,
    ];

    const FINISHED: [UploadStatus; 3] = [
        UploadStatus::Committed,
        UploadStatus::Aborted,
        UploadStatus::Expired,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            UploadStatus::Created => "created",
            UploadStatus::Uploading => "uploading",
            UploadStatus::Completing => "completing",
            UploadStatus::ObjectReady => "object_ready",
            UploadStatus::Committed => "committed",
            UploadStatus::Failed => "failed",
            UploadStatus::Aborting => "aborting",
            UploadStatus::Aborted => "aborted",
            UploadStatus::Expired => "expired",
        }
    }

    fn parse(value: &str) -> Result<UploadStatus> {
        UploadStatus::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
            .ok_or_else(|| anyhow::anyhow!("upload session has unknown status `{value}`").into())
    }

    /// Until a session is finished, staying put is allowed: it is how a retry re-enters
    /// an interrupted step.
    fn allows(self, next: UploadStatus) -> bool {
        use UploadStatus::*;
        if self.is_finished() {
            return false;
        }
        self == next
            || matches!(
                (self, next),
                (Created, Uploading)
                    | (Created | Uploading | Failed, Completing)
                    | (Completing, ObjectReady | Failed)
                    | (ObjectReady, Committed)
                    | (Created | Uploading | Failed, Aborting)
                    | (Aborting, Aborted)
                    | (Created | Uploading | Completing | Failed | Aborting, Expired)
            )
    }

    fn is_finished(self) -> bool {
        UploadStatus::FINISHED.contains(&self)
    }

    fn accepts_parts(self) -> bool {
        matches!(self, UploadStatus::Created | UploadStatus::Uploading)
    }

    fn may_have_assembled(self) -> bool {
        matches!(self, UploadStatus::Completing | UploadStatus::Failed)
    }

    fn ensure_can_become(self, next: UploadStatus) -> Result<()> {
        if self.allows(next) {
            Ok(())
        } else {
            Err(self.refusal(&format!("become {next}")))
        }
    }

    fn refusal(self, attempted: &str) -> Error {
        let (code, advice) = match self {
            UploadStatus::Committed => ("upload_committed", "it is already in the library"),
            UploadStatus::Aborting | UploadStatus::Aborted | UploadStatus::Expired => {
                ("upload_not_resumable", "upload the file again")
            }
            UploadStatus::Completing | UploadStatus::ObjectReady => {
                ("upload_completing", "retry completing it")
            }
            UploadStatus::Failed => ("upload_failed", "retry completing it, or abort it"),
            UploadStatus::Created | UploadStatus::Uploading => {
                ("upload_in_progress", "finish uploading its parts")
            }
        };
        Error::conflict(
            code,
            format!("the upload is {self} and cannot {attempted}; {advice}"),
        )
    }
}

impl fmt::Display for UploadStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UploadMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub subjects: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewUpload {
    pub filename: String,
    pub size: u64,
    #[serde(default)]
    pub resume_key: Option<String>,
    #[serde(default)]
    pub metadata: UploadMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct Upload {
    pub id: String,
    pub status: UploadStatus,
    pub filename: String,
    pub file_size: u64,
    pub part_size: u64,
    pub total_parts: u32,
    pub uploaded_parts: Vec<u32>,
    pub resume_key: Option<String>,
    pub metadata: UploadMetadata,
    pub document_id: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize)]
pub struct GcReport {
    pub dry_run: bool,
    pub expired: Vec<GcEntry>,
    /// Never expired: retrying `complete` turns them into documents.
    pub needs_completion: Vec<GcEntry>,
    pub failed: Vec<GcFailure>,
}

#[derive(Debug, Serialize)]
pub struct GcEntry {
    pub id: String,
    pub status: UploadStatus,
    pub filename: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize)]
pub struct GcFailure {
    #[serde(flatten)]
    pub session: GcEntry,
    pub error: String,
}

struct Session {
    id: String,
    status: UploadStatus,
    provider_upload_id: String,
    object_key: ObjectKey,
    filename: String,
    file_size: u64,
    part_size: u64,
    resume_key: Option<String>,
    metadata: UploadMetadata,
    document_id: Option<i64>,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
    expires_at: String,
}

const SESSION_COLUMNS: &str = "id, status, provider_upload_id, object_key, original_filename, file_size, \
     part_size, client_resume_key, metadata_json, document_id, last_error, created_at, updated_at, expires_at";

impl Session {
    fn from_row(row: &libsql::Row) -> Result<Session> {
        let metadata_json: String = row.get(8)?;
        Ok(Session {
            id: row.get(0)?,
            status: UploadStatus::parse(&row.get::<String>(1)?)?,
            provider_upload_id: row.get(2)?,
            object_key: ObjectKey::parse(row.get::<String>(3)?)?,
            filename: row.get(4)?,
            file_size: row.get::<i64>(5)? as u64,
            part_size: row.get::<i64>(6)? as u64,
            resume_key: row.get(7)?,
            metadata: serde_json::from_str(&metadata_json)
                .map_err(|error| anyhow::anyhow!("upload metadata is not valid JSON: {error}"))?,
            document_id: row.get(9)?,
            last_error: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
            expires_at: row.get(13)?,
        })
    }

    fn multipart(&self) -> Multipart<'_> {
        Multipart {
            key: &self.object_key,
            upload_id: &self.provider_upload_id,
        }
    }

    fn total_parts(&self) -> u32 {
        total_parts(self.file_size, self.part_size) as u32 // at most MAX_PARTS, checked on creation
    }

    fn expected_part_size(&self, part_number: u32) -> u64 {
        let total = self.total_parts();
        if part_number < total {
            self.part_size
        } else {
            self.file_size - self.part_size * u64::from(total - 1)
        }
    }

    fn check_part(&self, part_number: u32, size: u64) -> Result<()> {
        let total = self.total_parts();
        if !(1..=total).contains(&part_number) {
            return Err(Error::invalid(
                "invalid_part_number",
                format!("part {part_number} is outside this upload's parts 1 to {total}"),
            ));
        }
        let expected = self.expected_part_size(part_number);
        if size != expected {
            return Err(Error::invalid(
                "part_size_mismatch",
                format!("part {part_number} must be {expected} bytes, but {size} were sent"),
            ));
        }
        Ok(())
    }

    fn parts_to_assemble(&self, stored: Vec<StoredPart>) -> Result<Vec<StoredPart>> {
        let total = self.total_parts();
        let missing: Vec<String> = (1..=total)
            .filter(|number| !stored.iter().any(|part| part.part_number == *number))
            .map(|number| number.to_string())
            .collect();
        if !missing.is_empty() {
            let message = match missing.as_slice() {
                [one] => format!("part {one} has not been uploaded"),
                many => format!("parts {} have not been uploaded", many.join(", ")),
            };
            return Err(Error::conflict("upload_incomplete", message));
        }

        let parts: Vec<StoredPart> = stored
            .into_iter()
            .filter(|part| part.part_number <= total)
            .collect();
        if let Some(part) = parts
            .iter()
            .find(|part| part.size != self.expected_part_size(part.part_number))
        {
            return Err(Error::conflict(
                "upload_incomplete",
                format!(
                    "part {} holds {} bytes but must hold {}; upload it again",
                    part.part_number,
                    part.size,
                    self.expected_part_size(part.part_number)
                ),
            ));
        }
        Ok(parts)
    }

    fn new_document(&self) -> NewDocument {
        let metadata = &self.metadata;
        let mut document = NewDocument::new(DocumentKind::Pdf, self.title());
        document.description = metadata
            .description
            .clone()
            .filter(|text| !text.trim().is_empty());
        document.object_key = Some(self.object_key.clone());
        document.original_filename = Some(self.filename.clone());
        document.authors = metadata.authors.clone();
        document.subjects = metadata.subjects.clone();
        document.tags = metadata.tags.clone();
        document
    }

    fn title(&self) -> String {
        let supplied = self.metadata.title.as_deref().map(str::trim);
        let stem = Path::new(&self.filename)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::trim);
        [supplied, stem]
            .into_iter()
            .flatten()
            .find(|title| !title.is_empty())
            .unwrap_or(&self.filename)
            .to_string()
    }
}

fn total_parts(file_size: u64, part_size: u64) -> u64 {
    file_size.div_ceil(part_size)
}

pub async fn create(db: &Db, blobs: &dyn BlobStore, new: &NewUpload, part_size: u64) -> Result<Upload> {
    let resume_key = validate_new_upload(new, part_size)?;
    if let Some(existing) = find_open_session(&*db.read().await?, resume_key, new.size).await? {
        return view(&*db.read().await?, &existing).await;
    }

    let key = ObjectKey::generate(blobs.namespace(), "pdf");
    let provider_upload_id = blobs.begin_multipart(&key, CONTENT_TYPE).await?;
    let upload = Multipart {
        key: &key,
        upload_id: &provider_upload_id,
    };

    let tx = db.begin().await?;
    if let Some(existing) = find_open_session(&tx, resume_key, new.size).await? {
        drop(tx);
        if let Err(error) = blobs.abort_multipart(upload).await {
            tracing::warn!(%error, key = %key, "could not abort a duplicate multipart upload");
        }
        return view(&*db.read().await?, &existing).await;
    }
    let id = insert_session(&tx, new, resume_key, &key, &provider_upload_id, part_size).await?;
    tx.commit().await?;

    current_view(db, &id).await
}

fn validate_new_upload(new: &NewUpload, part_size: u64) -> Result<Option<&str>> {
    if new.filename.trim().is_empty() {
        return Err(Error::invalid("filename_required", "an upload needs a filename"));
    }
    if new.size == 0 {
        return Err(Error::invalid(
            "invalid_file_size",
            "an empty file cannot be uploaded",
        ));
    }
    if total_parts(new.size, part_size) > MAX_PARTS {
        return Err(Error::invalid(
            "file_too_large",
            format!("a file of {} bytes needs more than {MAX_PARTS} parts", new.size),
        ));
    }
    Ok(new.resume_key.as_deref().filter(|key| !key.trim().is_empty()))
}

async fn find_open_session(
    conn: &Connection,
    resume_key: Option<&str>,
    size: u64,
) -> Result<Option<Session>> {
    let Some(resume_key) = resume_key else {
        return Ok(None);
    };
    let sql = format!(
        "SELECT {SESSION_COLUMNS} FROM upload_sessions
         WHERE client_resume_key = ? AND file_size = ? AND status NOT IN ({})
         ORDER BY created_at DESC LIMIT 1",
        status_list(&UploadStatus::FINISHED)
    );
    let mut rows = conn.query(&sql, params![resume_key, size as i64]).await?;
    match rows.next().await? {
        Some(row) => Ok(Some(Session::from_row(&row)?)),
        None => Ok(None),
    }
}

async fn insert_session(
    conn: &Connection,
    new: &NewUpload,
    resume_key: Option<&str>,
    key: &ObjectKey,
    provider_upload_id: &str,
    part_size: u64,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let metadata_json = serde_json::to_string(&new.metadata)
        .map_err(|error| anyhow::anyhow!("serialize metadata: {error}"))?;
    conn.execute(
        &format!(
            "INSERT INTO upload_sessions (id, status, provider_upload_id, object_key, original_filename,
                 file_size, part_size, client_resume_key, metadata_json, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, {EXPIRES_AT})"
        ),
        params![
            id.as_str(),
            UploadStatus::Created.as_str(),
            provider_upload_id,
            key.as_str(),
            new.filename.trim(),
            new.size as i64,
            part_size as i64,
            resume_key,
            metadata_json
        ],
    )
    .await?;
    Ok(id)
}

pub async fn list(db: &Db) -> Result<Vec<Upload>> {
    let conn = db.read().await?;
    let sql = format!(
        "SELECT {SESSION_COLUMNS} FROM upload_sessions WHERE status NOT IN ({})
         ORDER BY created_at DESC, id",
        status_list(&UploadStatus::FINISHED)
    );
    let mut rows = conn.query(&sql, ()).await?;
    let mut sessions = Vec::new();
    while let Some(row) = rows.next().await? {
        sessions.push(Session::from_row(&row)?);
    }

    let mut uploads = Vec::with_capacity(sessions.len());
    for session in &sessions {
        uploads.push(view(&conn, session).await?);
    }
    Ok(uploads)
}

pub async fn get(db: &Db, blobs: &dyn BlobStore, id: &str) -> Result<Upload> {
    let session = load(&*db.read().await?, id).await?;
    if session.status.accepts_parts() {
        let stored = blobs.list_parts(session.multipart()).await.map_err(store_error)?;
        reconcile_parts(db, &session.id, &stored).await?;
    }
    view(&*db.read().await?, &session).await
}

pub async fn put_part(
    db: &Db,
    blobs: &dyn BlobStore,
    id: &str,
    part_number: u32,
    bytes: Bytes,
) -> Result<Upload> {
    let session = load(&*db.read().await?, id).await?;
    if !session.status.accepts_parts() {
        return Err(session.status.refusal("accept parts"));
    }
    session.check_part(part_number, bytes.len() as u64)?;

    let stored = blobs
        .put_part(session.multipart(), part_number, bytes)
        .await
        .map_err(store_error)?;

    let tx = db.begin().await?;
    // Re-read under the write gate: parts upload concurrently, and the session may have
    // been aborted while the store was busy.
    let status = load(&tx, id).await?.status;
    if !status.accepts_parts() {
        return Err(status.refusal("accept parts"));
    }
    advance(&tx, id, status, UploadStatus::Uploading).await?;
    record_part(&tx, id, &stored).await?;
    tx.execute(
        &format!("UPDATE upload_sessions SET expires_at = {EXPIRES_AT} WHERE id = ?"),
        [id],
    )
    .await?;
    tx.commit().await?;

    current_view(db, id).await
}

pub async fn complete(db: &Db, blobs: &dyn BlobStore, id: &str) -> Result<Upload> {
    let session = load(&*db.read().await?, id).await?;
    match session.status {
        UploadStatus::Committed => {}
        UploadStatus::ObjectReady => commit_document(db, &session).await?,
        _ => {
            assemble_object(db, blobs, &session).await?;
            commit_document(db, &session).await?;
        }
    }
    current_view(db, id).await
}

async fn assemble_object(db: &Db, blobs: &dyn BlobStore, session: &Session) -> Result<()> {
    session.status.ensure_can_become(UploadStatus::Completing)?;

    // An earlier attempt may have assembled the object and then lost track of it, through
    // a crash or an ambiguous provider failure. The object itself is the evidence.
    if session.status.may_have_assembled() && blobs.head(&session.object_key).await?.is_some() {
        let tx = db.begin().await?;
        advance(&tx, &session.id, session.status, UploadStatus::Completing).await?;
        advance(
            &tx,
            &session.id,
            UploadStatus::Completing,
            UploadStatus::ObjectReady,
        )
        .await?;
        return Ok(tx.commit().await?);
    }

    let stored = blobs.list_parts(session.multipart()).await.map_err(store_error)?;
    reconcile_parts(db, &session.id, &stored).await?;
    let parts = session.parts_to_assemble(stored)?;

    let tx = db.begin().await?;
    advance(&tx, &session.id, session.status, UploadStatus::Completing).await?;
    tx.commit().await?;

    if let Err(error) = blobs.complete_multipart(session.multipart(), &parts).await {
        let error = store_error(error);
        let recorded = async {
            let tx = db.begin().await?;
            advance(&tx, &session.id, UploadStatus::Completing, UploadStatus::Failed).await?;
            set_last_error(&tx, &session.id, &error).await?;
            tx.commit().await?;
            Ok::<_, Error>(())
        };
        if let Err(record_error) = recorded.await {
            tracing::warn!(error = %record_error, session = %session.id, "could not record a failed completion");
        }
        return Err(error);
    }

    let tx = db.begin().await?;
    advance(
        &tx,
        &session.id,
        UploadStatus::Completing,
        UploadStatus::ObjectReady,
    )
    .await?;
    Ok(tx.commit().await?)
}

async fn commit_document(db: &Db, session: &Session) -> Result<()> {
    let committed = async {
        let tx = db.begin().await?;
        advance(
            &tx,
            &session.id,
            UploadStatus::ObjectReady,
            UploadStatus::Committed,
        )
        .await?;
        let document_id = documents::create(&tx, &session.new_document()).await?;
        documents::add_source(&tx, document_id, source::UPLOAD, &session.id).await?;
        tx.execute(
            "UPDATE upload_sessions SET document_id = ? WHERE id = ?",
            params![document_id, session.id.as_str()],
        )
        .await?;
        tx.commit().await?;
        Ok::<_, Error>(())
    };
    let Err(error) = committed.await else {
        return Ok(());
    };

    let recorded = async {
        let tx = db.begin().await?;
        let still_ready = load(&tx, &session.id).await?.status == UploadStatus::ObjectReady;
        if still_ready {
            set_last_error(&tx, &session.id, &error).await?;
        }
        tx.commit().await?;
        Ok::<_, Error>(())
    };
    if let Err(record_error) = recorded.await {
        tracing::warn!(error = %record_error, session = %session.id, "could not record a failed commit");
    }
    Err(error)
}

pub async fn abort(db: &Db, blobs: &dyn BlobStore, id: &str) -> Result<Upload> {
    let session = load(&*db.read().await?, id).await?;
    if matches!(session.status, UploadStatus::Aborted | UploadStatus::Expired) {
        return view(&*db.read().await?, &session).await;
    }

    let tx = db.begin().await?;
    advance(&tx, id, session.status, UploadStatus::Aborting).await?;
    tx.commit().await?;

    if let Err(error) = blobs.abort_multipart(session.multipart()).await {
        let error = Error::from(error);
        let tx = db.begin().await?;
        set_last_error(&tx, id, &error).await?;
        tx.commit().await?;
        return Err(error);
    }

    let tx = db.begin().await?;
    advance(&tx, id, UploadStatus::Aborting, UploadStatus::Aborted).await?;
    tx.commit().await?;

    current_view(db, id).await
}

pub async fn collect_garbage(db: &Db, blobs: &dyn BlobStore, dry_run: bool) -> Result<GcReport> {
    let mut report = GcReport {
        dry_run,
        expired: Vec::new(),
        needs_completion: Vec::new(),
        failed: Vec::new(),
    };
    for session in overdue_sessions(&*db.read().await?).await? {
        let entry = GcEntry {
            id: session.id.clone(),
            status: session.status,
            filename: session.filename.clone(),
            expires_at: session.expires_at.clone(),
        };
        match expire(db, blobs, &session, dry_run).await {
            Ok(Expiry::Expired) => report.expired.push(entry),
            Ok(Expiry::NeedsCompletion) => report.needs_completion.push(entry),
            Err(error) => report.failed.push(GcFailure {
                session: entry,
                error: error.to_string(),
            }),
        }
    }
    Ok(report)
}

enum Expiry {
    Expired,
    NeedsCompletion,
}

async fn expire(db: &Db, blobs: &dyn BlobStore, session: &Session, dry_run: bool) -> Result<Expiry> {
    let assembled = session.status == UploadStatus::ObjectReady
        || (session.status.may_have_assembled() && blobs.head(&session.object_key).await?.is_some());
    if assembled {
        return Ok(Expiry::NeedsCompletion);
    }
    session.status.ensure_can_become(UploadStatus::Expired)?;
    if dry_run {
        return Ok(Expiry::Expired);
    }

    blobs.abort_multipart(session.multipart()).await?;
    let tx = db.begin().await?;
    advance(&tx, &session.id, session.status, UploadStatus::Expired).await?;
    tx.commit().await?;
    Ok(Expiry::Expired)
}

async fn overdue_sessions(conn: &Connection) -> Result<Vec<Session>> {
    let sql = format!(
        "SELECT {SESSION_COLUMNS} FROM upload_sessions
         WHERE expires_at < {NOW} AND status NOT IN ({})
         ORDER BY expires_at, id",
        status_list(&UploadStatus::FINISHED)
    );
    let mut rows = conn.query(&sql, ()).await?;
    let mut sessions = Vec::new();
    while let Some(row) = rows.next().await? {
        sessions.push(Session::from_row(&row)?);
    }
    Ok(sessions)
}

async fn load(conn: &Connection, id: &str) -> Result<Session> {
    let mut rows = conn
        .query(
            &format!("SELECT {SESSION_COLUMNS} FROM upload_sessions WHERE id = ?"),
            [id],
        )
        .await?;
    match rows.next().await? {
        Some(row) => Session::from_row(&row),
        None => Err(Error::NotFound("upload")),
    }
}

async fn advance(conn: &Connection, id: &str, from: UploadStatus, to: UploadStatus) -> Result<()> {
    from.ensure_can_become(to)?;
    let changed = conn
        .execute(
            &format!(
                "UPDATE upload_sessions SET status = ?, last_error = NULL, updated_at = {NOW}
                 WHERE id = ? AND status = ?"
            ),
            params![to.as_str(), id, from.as_str()],
        )
        .await?;
    if changed == 0 {
        return Err(Error::conflict(
            "upload_changed",
            "the upload changed while this request ran; fetch it and retry",
        ));
    }
    Ok(())
}

async fn set_last_error(conn: &Connection, id: &str, error: &Error) -> Result<()> {
    conn.execute(
        &format!("UPDATE upload_sessions SET last_error = ?, updated_at = {NOW} WHERE id = ?"),
        params![error.to_string(), id],
    )
    .await?;
    Ok(())
}

async fn record_part(conn: &Connection, id: &str, part: &StoredPart) -> Result<()> {
    conn.execute(
        "INSERT INTO upload_parts (upload_session_id, part_number, size, etag) VALUES (?, ?, ?, ?)
         ON CONFLICT (upload_session_id, part_number) DO UPDATE SET size = excluded.size, etag = excluded.etag",
        params![id, part.part_number, part.size as i64, part.etag.as_str()],
    )
    .await?;
    Ok(())
}

async fn reconcile_parts(db: &Db, id: &str, stored: &[StoredPart]) -> Result<()> {
    if recorded_parts(&*db.read().await?, id).await? == stored {
        return Ok(());
    }
    let tx = db.begin().await?;
    tx.execute("DELETE FROM upload_parts WHERE upload_session_id = ?", [id])
        .await?;
    for part in stored {
        record_part(&tx, id, part).await?;
    }
    Ok(tx.commit().await?)
}

async fn recorded_parts(conn: &Connection, id: &str) -> Result<Vec<StoredPart>> {
    let mut rows = conn
        .query(
            "SELECT part_number, size, etag FROM upload_parts WHERE upload_session_id = ? ORDER BY part_number",
            [id],
        )
        .await?;
    let mut parts = Vec::new();
    while let Some(row) = rows.next().await? {
        parts.push(StoredPart {
            part_number: row.get(0)?,
            size: row.get::<i64>(1)? as u64,
            etag: row.get(2)?,
        });
    }
    Ok(parts)
}

async fn current_view(db: &Db, id: &str) -> Result<Upload> {
    let conn = db.read().await?;
    view(&conn, &load(&conn, id).await?).await
}

async fn view(conn: &Connection, session: &Session) -> Result<Upload> {
    let uploaded_parts = recorded_parts(conn, &session.id)
        .await?
        .into_iter()
        .map(|part| part.part_number)
        .collect();
    Ok(Upload {
        id: session.id.clone(),
        status: session.status,
        filename: session.filename.clone(),
        file_size: session.file_size,
        part_size: session.part_size,
        total_parts: session.total_parts(),
        uploaded_parts,
        resume_key: session.resume_key.clone(),
        metadata: session.metadata.clone(),
        document_id: session.document_id,
        last_error: session.last_error.clone(),
        created_at: session.created_at.clone(),
        updated_at: session.updated_at.clone(),
        expires_at: session.expires_at.clone(),
    })
}

fn store_error(error: BlobError) -> Error {
    match error {
        BlobError::UploadNotFound => Error::conflict(
            "upload_lost",
            "the object store no longer holds this upload; abort it and upload the file again",
        ),
        other => Error::Blob(other),
    }
}

fn status_list(statuses: &[UploadStatus]) -> String {
    statuses
        .iter()
        .map(|status| format!("'{}'", status.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(file_size: u64, part_size: u64) -> Session {
        Session {
            id: "s".into(),
            status: UploadStatus::Uploading,
            provider_upload_id: "u".into(),
            object_key: ObjectKey::parse("local/k.pdf").unwrap(),
            filename: "Our Sister Killjoy.pdf".into(),
            file_size,
            part_size,
            resume_key: None,
            metadata: UploadMetadata::default(),
            document_id: None,
            last_error: None,
            created_at: String::new(),
            updated_at: String::new(),
            expires_at: String::new(),
        }
    }

    fn part(part_number: u32, size: u64) -> StoredPart {
        StoredPart {
            part_number,
            size,
            etag: format!("etag-{part_number}"),
        }
    }

    #[test]
    fn statuses_round_trip_through_their_names() {
        for status in UploadStatus::ALL {
            assert_eq!(UploadStatus::parse(status.as_str()).unwrap(), status);
        }
    }

    #[test]
    fn finished_sessions_go_nowhere() {
        for finished in UploadStatus::FINISHED {
            for next in UploadStatus::ALL {
                assert!(!finished.allows(next), "{finished} -> {next}");
            }
        }
    }

    #[test]
    fn the_lifecycle_has_only_its_documented_steps() {
        use UploadStatus::*;
        assert!(Created.allows(Uploading));
        assert!(Failed.allows(Completing), "a failed completion is retryable");
        assert!(
            Completing.allows(Completing),
            "an interrupted completion is re-entered"
        );
        assert!(
            !Uploading.allows(ObjectReady),
            "the object is never ready without completing"
        );
        assert!(!Uploading.allows(Committed));
        assert!(
            !ObjectReady.allows(Expired),
            "an assembled object is never garbage"
        );
        assert!(!ObjectReady.allows(Aborting));
        assert!(!Aborting.allows(Uploading));
    }

    #[test]
    fn the_last_part_holds_the_remainder() {
        let uneven = session(10, 4);
        assert_eq!(uneven.total_parts(), 3);
        assert_eq!(uneven.expected_part_size(1), 4);
        assert_eq!(uneven.expected_part_size(3), 2);
        assert_eq!(session(8, 4).expected_part_size(2), 4);
    }

    #[test]
    fn assembly_names_every_missing_part() {
        let error = session(10, 4).parts_to_assemble(vec![part(2, 4)]).unwrap_err();
        assert!(matches!(
            error,
            Error::Conflict {
                code: "upload_incomplete",
                ..
            }
        ));
        assert_eq!(error.to_string(), "parts 1, 3 have not been uploaded");

        let one = session(10, 4)
            .parts_to_assemble(vec![part(1, 4), part(3, 2)])
            .unwrap_err();
        assert_eq!(one.to_string(), "part 2 has not been uploaded");
    }

    #[test]
    fn titles_fall_back_to_the_filename_stem() {
        let mut session = session(1, 1);
        assert_eq!(session.title(), "Our Sister Killjoy");
        session.metadata.title = Some("  ".into());
        assert_eq!(session.title(), "Our Sister Killjoy");
        session.metadata.title = Some("Killjoy".into());
        assert_eq!(session.title(), "Killjoy");
    }
}
