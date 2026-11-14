use std::cmp::Reverse;
use std::fmt;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Context;
use bytes::Bytes;
use serde::{Serialize, Serializer};
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, PrimitiveDateTime};
use tokio::io::AsyncReadExt;

use crate::blob::{BlobError, BlobStore, Multipart, ObjectKey};
use crate::config::{BackupSchedule, Config};
use crate::db::{Db, ReadOnlyDb};
use crate::domain::uploads::MIN_PROVIDER_PART_SIZE;
use crate::error::{Error, Result};
use crate::export::{self, LibraryCounts};
use crate::health;

const PREFIX: &str = "backups/";

pub const CONTENT_TYPE: &str = "application/vnd.sqlite3";

const PART_SIZE: u64 = 8 * 1024 * 1024;

const _: () = assert!(PART_SIZE >= MIN_PROVIDER_PART_SIZE);

const TIMESTAMP: &[BorrowedFormatItem<'_>] =
    format_description!("[year][month][day]T[hour][minute][second]Z");

/// Polled on the wall clock: one long sleep would use the monotonic clock, which stops
/// while a laptop sleeps.
const SCHEDULE_TICK: Duration = Duration::from_secs(60);

const STALE_STAGED_FILE: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Trigger {
    Manual,
    Scheduled,
}

impl Trigger {
    const ALL: [Trigger; 2] = [Trigger::Manual, Trigger::Scheduled];

    fn as_str(self) -> &'static str {
        match self {
            Trigger::Manual => "manual",
            Trigger::Scheduled => "scheduled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupName {
    pub trigger: Trigger,
    pub created_at: OffsetDateTime,
}

impl BackupName {
    pub fn now(trigger: Trigger) -> BackupName {
        BackupName::at(trigger, OffsetDateTime::now_utc())
    }

    pub fn at(trigger: Trigger, time: OffsetDateTime) -> BackupName {
        BackupName {
            trigger,
            created_at: time
                .to_offset(time::UtcOffset::UTC)
                .replace_nanosecond(0)
                .expect("zero is always a valid nanosecond"),
        }
    }

    pub fn parse(name: &str) -> Result<BackupName> {
        let invalid = || {
            Error::invalid(
                "invalid_backup_name",
                format!("`{name}` is not a backup name such as manual-20261113T020304Z"),
            )
        };
        let (trigger, timestamp) = name.split_once('-').ok_or_else(invalid)?;
        let trigger = Trigger::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == trigger)
            .ok_or_else(invalid)?;
        let created_at = PrimitiveDateTime::parse(timestamp, TIMESTAMP)
            .map_err(|_| invalid())?
            .assume_utc();
        let parsed = BackupName { trigger, created_at };
        if parsed.to_string() != name {
            return Err(invalid());
        }
        Ok(parsed)
    }

    pub fn object_key(&self, namespace: &str) -> Result<ObjectKey> {
        Ok(ObjectKey::within(
            namespace,
            &format!("{PREFIX}{}/{}.sqlite", self.trigger.as_str(), self.timestamp()),
        )?)
    }

    fn from_object_key(namespace: &str, key: &ObjectKey) -> Option<BackupName> {
        let path = key.as_str().strip_prefix(namespace)?.strip_prefix(PREFIX)?;
        let (trigger, file) = path.split_once('/')?;
        let timestamp = file.strip_suffix(".sqlite")?;
        BackupName::parse(&format!("{trigger}-{timestamp}")).ok()
    }

    fn timestamp(&self) -> String {
        let time = self.created_at;
        format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            time.year(),
            u8::from(time.month()),
            time.day(),
            time.hour(),
            time.minute(),
            time.second()
        )
    }
}

impl fmt::Display for BackupName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.trigger.as_str(), self.timestamp())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    pub name: BackupName,
    pub size: u64,
}

impl Serialize for Backup {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct View {
            name: String,
            trigger: Trigger,
            size: u64,
            #[serde(with = "time::serde::rfc3339")]
            created_at: OffsetDateTime,
        }
        View {
            name: self.name.to_string(),
            trigger: self.name.trigger,
            size: self.size,
            created_at: self.name.created_at,
        }
        .serialize(serializer)
    }
}

pub async fn create(db: &Db, blobs: &dyn BlobStore, temp_dir: &Path, name: BackupName) -> Result<Backup> {
    let key = name.object_key(blobs.namespace())?;
    // Two backups in the same second share a key; the later upload replaces the earlier
    // one whole, which loses nothing.
    if blobs.head(&key).await?.is_some() {
        return Err(Error::conflict(
            "backup_exists",
            format!("backup {name} already exists; try again in a second"),
        ));
    }

    let staged = StagedFile::new(temp_dir, "backup")?;
    export::export(db, staged.path()).await?;
    let size = upload(blobs, &key, staged.path()).await?;
    Ok(Backup { name, size })
}

pub async fn list(blobs: &dyn BlobStore) -> Result<Vec<Backup>> {
    let namespace = blobs.namespace();
    let mut backups: Vec<Backup> = blobs
        .list(&format!("{namespace}{PREFIX}"))
        .await?
        .into_iter()
        .filter_map(|object| {
            Some(Backup {
                name: BackupName::from_object_key(namespace, &object.key)?,
                size: object.size,
            })
        })
        .collect();
    backups.sort_by_key(|backup| Reverse((backup.name.created_at, backup.name.trigger)));
    Ok(backups)
}

pub async fn prune(blobs: &dyn BlobStore, keep: NonZeroUsize) -> Result<Vec<Backup>> {
    let expired: Vec<Backup> = list(blobs)
        .await?
        .into_iter()
        .filter(|backup| backup.name.trigger == Trigger::Scheduled)
        .skip(keep.get())
        .collect();
    for backup in &expired {
        blobs.delete(&backup.name.object_key(blobs.namespace())?).await?;
    }
    Ok(expired)
}

pub async fn download(blobs: &dyn BlobStore, name: &BackupName, destination: &Path) -> Result<u64> {
    match blobs
        .download(&name.object_key(blobs.namespace())?, destination)
        .await
    {
        Ok(size) => Ok(size),
        Err(BlobError::ObjectNotFound) => Err(Error::NotFound("backup")),
        Err(error) => Err(error.into()),
    }
}

async fn upload(blobs: &dyn BlobStore, key: &ObjectKey, path: &Path) -> Result<u64> {
    let upload_id = blobs.begin_multipart(key, CONTENT_TYPE).await?;
    let upload = Multipart {
        key,
        upload_id: &upload_id,
    };
    let sent = send_parts(blobs, upload, path).await;
    if sent.is_err()
        && let Err(error) = blobs.abort_multipart(upload).await
    {
        tracing::warn!(%key, %error, "could not abort the failed backup upload");
    }
    sent
}

async fn send_parts(blobs: &dyn BlobStore, upload: Multipart<'_>, path: &Path) -> Result<u64> {
    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("cannot read {}", path.display()))?;
    let mut parts = Vec::new();
    loop {
        let mut chunk = Vec::new();
        (&mut file)
            .take(PART_SIZE)
            .read_to_end(&mut chunk)
            .await
            .with_context(|| format!("cannot read {}", path.display()))?;
        if chunk.is_empty() {
            break;
        }
        let part_number = u32::try_from(parts.len() + 1).context("the backup has too many parts")?;
        parts.push(blobs.put_part(upload, part_number, Bytes::from(chunk)).await?);
    }
    blobs.complete_multipart(upload, &parts).await?;
    Ok(parts.iter().map(|part| part.size).sum())
}

#[derive(Debug)]
pub struct ScheduledRun {
    pub backup: Backup,
    pub pruned: Vec<Backup>,
}

pub fn is_due(schedule: &BackupSchedule, last_attempt: Option<OffsetDateTime>, now: OffsetDateTime) -> bool {
    last_attempt.is_none_or(|last| now - last >= schedule.every)
}

/// Prunes only once the new backup is in the store, so a failing schedule never eats
/// into the backups it already has.
pub async fn run_scheduled(
    db: &Db,
    blobs: &dyn BlobStore,
    temp_dir: &Path,
    keep: NonZeroUsize,
) -> Result<ScheduledRun> {
    let backup = create(db, blobs, temp_dir, BackupName::now(Trigger::Scheduled)).await?;
    let pruned = prune(blobs, keep).await.map_err(|error| {
        anyhow::anyhow!(
            "backup {} was made, but pruning older ones failed: {error}",
            backup.name
        )
    })?;
    Ok(ScheduledRun { backup, pruned })
}

pub async fn run_schedule(
    db: Arc<Db>,
    blobs: Arc<dyn BlobStore>,
    temp_dir: PathBuf,
    schedule: BackupSchedule,
    shutdown: impl Future<Output = ()>,
) {
    tokio::pin!(shutdown);
    let mut last_attempt = match newest_scheduled(blobs.as_ref()).await {
        Ok(newest) => newest,
        Err(error) => {
            tracing::warn!(%error, "cannot list backups; the first scheduled backup waits one interval");
            Some(OffsetDateTime::now_utc())
        }
    };

    loop {
        let now = OffsetDateTime::now_utc();
        if is_due(&schedule, last_attempt, now) {
            last_attempt = Some(now);
            match run_scheduled(&db, blobs.as_ref(), &temp_dir, schedule.keep).await {
                Ok(run) => tracing::info!(
                    backup = %run.backup.name,
                    size = run.backup.size,
                    pruned = ?run.pruned.iter().map(|backup| backup.name.to_string()).collect::<Vec<_>>(),
                    "made a scheduled backup"
                ),
                Err(error) => {
                    tracing::warn!(%error, "scheduled backup failed; retrying in one interval")
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(SCHEDULE_TICK) => {}
            _ = &mut shutdown => return,
        }
    }
}

async fn newest_scheduled(blobs: &dyn BlobStore) -> Result<Option<OffsetDateTime>> {
    Ok(list(blobs)
        .await?
        .into_iter()
        .find(|backup| backup.name.trigger == Trigger::Scheduled)
        .map(|backup| backup.name.created_at))
}

pub fn sweep_temp_dir(temp_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(temp_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| {
                SystemTime::now()
                    .duration_since(modified)
                    .is_ok_and(|age| age > STALE_STAGED_FILE)
            });
        if stale && entry.path().is_file() {
            match std::fs::remove_file(entry.path()) {
                Ok(()) => tracing::info!(path = %entry.path().display(), "removed a stale staged file"),
                Err(error) => {
                    tracing::warn!(path = %entry.path().display(), %error, "cannot remove a stale staged file")
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreSource {
    Backup(BackupName),
    File(PathBuf),
}

impl RestoreSource {
    pub fn parse(value: &str) -> RestoreSource {
        match BackupName::parse(value) {
            Ok(name) => RestoreSource::Backup(name),
            Err(_) => RestoreSource::File(PathBuf::from(value)),
        }
    }
}

impl fmt::Display for RestoreSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestoreSource::Backup(name) => write!(f, "backup {name}"),
            RestoreSource::File(path) => write!(f, "{}", path.display()),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Restored {
    pub database: PathBuf,
    pub from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_aside: Option<PathBuf>,
    #[serde(flatten)]
    pub counts: LibraryCounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Existing {
    Refuse,
    SetAside,
}

pub async fn restore(config: &Config, blobs: &dyn BlobStore, source: &RestoreSource) -> Result<Restored> {
    restore_with(config, blobs, source, Existing::Refuse).await
}

/// The current database is moved, never deleted, and only once the copy replacing it has
/// been verified, so a bad backup cannot displace a working database.
pub async fn restore_setting_aside(
    config: &Config,
    blobs: &dyn BlobStore,
    source: &RestoreSource,
) -> Result<Restored> {
    restore_with(config, blobs, source, Existing::SetAside).await
}

async fn restore_with(
    config: &Config,
    blobs: &dyn BlobStore,
    source: &RestoreSource,
    existing: Existing,
) -> Result<Restored> {
    let database = &config.database.path;
    if existing == Existing::Refuse {
        ensure_absent(database)?;
    }
    ensure_nothing_listens(config)?;

    let staged = StagedFile::new(&config.temp_dir(), "restore")?;
    match source {
        RestoreSource::Backup(name) => {
            download(blobs, name, staged.path()).await?;
        }
        RestoreSource::File(path) => copy_local(path, staged.path())?,
    }
    let counts = verify(staged.path()).await?;
    let set_aside = match existing {
        Existing::Refuse => None,
        Existing::SetAside => set_aside(database, &config.data_dir)?,
    };
    move_into_place(staged.path(), database)?;

    Ok(Restored {
        database: database.clone(),
        from: source.to_string(),
        set_aside,
        counts,
    })
}

fn set_aside(database: &Path, data_dir: &Path) -> Result<Option<PathBuf>> {
    let present: Vec<PathBuf> = sqlite_files(database)
        .into_iter()
        .filter(|path| path.exists())
        .collect();
    if present.is_empty() {
        return Ok(None);
    }
    let stamp = OffsetDateTime::now_utc()
        .format(TIMESTAMP)
        .context("cannot format the set-aside timestamp")?;
    let destination = data_dir.join("set-aside").join(stamp);
    std::fs::create_dir_all(&destination)
        .with_context(|| format!("cannot create {}", destination.display()))?;
    for path in present {
        let name = path.file_name().expect("database files have names");
        std::fs::rename(&path, destination.join(name))
            .with_context(|| format!("cannot move {} aside", path.display()))?;
    }
    Ok(Some(destination))
}

/// Refuses if the database, or a write-ahead log that SQLite would replay into whatever
/// file is put there, already exists.
fn ensure_absent(database: &Path) -> Result<()> {
    match sqlite_files(database).into_iter().find(|path| path.exists()) {
        Some(existing) => Err(already_exists(&existing)),
        None => Ok(()),
    }
}

fn already_exists(path: &Path) -> Error {
    Error::conflict(
        "database_exists",
        format!(
            "{} already exists, and restore never replaces a database; move it aside \
             (with any -wal and -shm files beside it) and run restore again",
            path.display()
        ),
    )
}

fn ensure_nothing_listens(config: &Config) -> Result<()> {
    if !health::address_in_use(config.bind) {
        return Ok(());
    }
    Err(Error::conflict(
        "daemon_running",
        format!(
            "something is listening on {}; stop liotek before restoring",
            config.bind
        ),
    ))
}

fn copy_local(source: &Path, staged: &Path) -> Result<()> {
    let wal = sibling(source, "-wal");
    if wal.exists() {
        return Err(Error::invalid(
            "restore_source_in_use",
            format!(
                "{} exists, so {} is a live database whose latest writes are not in the file \
                 itself; restore from an export or a backup instead",
                wal.display(),
                source.display()
            ),
        ));
    }
    match std::fs::copy(source, staged) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(Error::invalid(
            "restore_source_missing",
            format!(
                "{} is neither a backup name nor an existing file",
                source.display()
            ),
        )),
        Err(error) => Err(anyhow::Error::from(error)
            .context(format!("cannot copy {}", source.display()))
            .into()),
    }
}

async fn verify(staged: &Path) -> Result<LibraryCounts> {
    let corrupt = |problem: String| {
        Error::invalid(
            "restore_source_corrupt",
            format!("the file to restore is damaged: {problem}"),
        )
    };
    let db = ReadOnlyDb::open(staged)
        .await
        .map_err(|error| corrupt(error.to_string()))?;
    let problems = db
        .integrity_problems()
        .await
        .map_err(|error| corrupt(error.to_string()))?;
    if !problems.is_empty() {
        return Err(corrupt(problems.join("; ")));
    }

    let status = db.schema_status().await?;
    if status.current == 0 {
        return Err(Error::invalid(
            "restore_source_not_liotek",
            "the file to restore is a SQLite database, but not a liotek one",
        ));
    }
    LibraryCounts::read(&db, "main").await
}

/// Linking fails rather than replaces if a database appeared there since [`ensure_absent`].
fn move_into_place(staged: &Path, database: &Path) -> Result<()> {
    if let Some(parent) = database.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    std::fs::File::open(staged)
        .and_then(|file| file.sync_all())
        .with_context(|| format!("cannot flush {}", staged.display()))?;
    match std::fs::hard_link(staged, database) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(already_exists(database)),
        Err(error) => Err(anyhow::Error::from(error)
            .context(format!(
                "cannot move the restored database to {}; it must be on the same filesystem as {}",
                database.display(),
                staged.display()
            ))
            .into()),
    }
}

fn sqlite_files(path: &Path) -> [PathBuf; 4] {
    [
        path.to_path_buf(),
        sibling(path, "-journal"),
        sibling(path, "-wal"),
        sibling(path, "-shm"),
    ]
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

struct StagedFile {
    path: PathBuf,
}

impl StagedFile {
    fn new(temp_dir: &Path, purpose: &str) -> Result<StagedFile> {
        std::fs::create_dir_all(temp_dir).with_context(|| format!("cannot create {}", temp_dir.display()))?;
        Ok(StagedFile {
            path: temp_dir.join(format!("{purpose}-{}.sqlite", uuid::Uuid::new_v4().simple())),
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        for path in sqlite_files(&self.path) {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => tracing::warn!(path = %path.display(), %error, "cannot remove a staged file"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    fn daily() -> BackupSchedule {
        BackupSchedule {
            every: Duration::from_secs(24 * 60 * 60),
            keep: NonZeroUsize::new(3).unwrap(),
        }
    }

    #[test]
    fn names_round_trip_through_text_and_object_keys() {
        let name = BackupName::at(Trigger::Scheduled, datetime!(2026-11-13 02:03:04.567 UTC));

        assert_eq!(name.to_string(), "scheduled-20261113T020304Z");
        assert_eq!(BackupName::parse("scheduled-20261113T020304Z").unwrap(), name);
        let key = name.object_key("liotek/preview/").unwrap();
        assert_eq!(
            key.as_str(),
            "liotek/preview/backups/scheduled/20261113T020304Z.sqlite"
        );
        assert_eq!(BackupName::from_object_key("liotek/preview/", &key), Some(name));
        assert_eq!(BackupName::from_object_key("other/", &key), None);
    }

    #[test]
    fn names_are_parsed_strictly() {
        for bad in [
            "",
            "manual",
            "manual-",
            "weekly-20261113T020304Z",
            "manual-20261113T020304",
            "manual-20261113T020304Z.sqlite",
            "manual-20261313T020304Z",
            "manual-20261113T020304Z/../x",
            "../manual-20261113T020304Z",
            "manual/20261113T020304Z",
            "manual-+20261113T020304Z",
            " manual-20261113T020304Z",
        ] {
            assert!(BackupName::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn a_restore_source_is_a_backup_name_or_else_a_path() {
        assert!(matches!(
            RestoreSource::parse("manual-20261113T020304Z"),
            RestoreSource::Backup(_)
        ));
        assert_eq!(
            RestoreSource::parse("exports/library.sqlite"),
            RestoreSource::File(PathBuf::from("exports/library.sqlite"))
        );
    }

    #[test]
    fn a_backup_is_due_when_there_is_none_or_the_last_is_an_interval_old() {
        let schedule = daily();
        let now = datetime!(2026-11-13 12:00 UTC);

        assert!(is_due(&schedule, None, now), "no backup yet");
        assert!(
            !is_due(&schedule, Some(datetime!(2026-11-13 02:00 UTC)), now),
            "ten hours old"
        );
        assert!(
            is_due(&schedule, Some(datetime!(2026-11-12 12:00 UTC)), now),
            "exactly one interval old"
        );
        assert!(
            is_due(&schedule, Some(datetime!(2026-11-01 00:00 UTC)), now),
            "long overdue"
        );
    }
}
