use std::fmt;

use time::OffsetDateTime;

use crate::backup::{self, Backup, Trigger};
use crate::blob::{self, ObjectKey};
use crate::build;
use crate::config::{BackupSchedule, BlobStoreConfig, Config};
use crate::db::ReadOnlyDb;
use crate::health;
use crate::reader;

#[derive(Debug, PartialEq, Eq)]
enum Finding {
    Ok(String),
    Warning(String),
    Problem(String),
}

pub struct Report {
    checks: Vec<(&'static str, Finding)>,
}

impl Report {
    pub fn is_healthy(&self) -> bool {
        !self
            .checks
            .iter()
            .any(|(_, finding)| matches!(finding, Finding::Problem(_)))
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, finding) in &self.checks {
            let (mark, detail) = match finding {
                Finding::Ok(detail) => ("ok", detail),
                Finding::Warning(detail) => ("warn", detail),
                Finding::Problem(detail) => ("FAIL", detail),
            };
            writeln!(f, "{mark:>4}  {name:<14} {detail}")?;
        }
        Ok(())
    }
}

pub async fn examine(config: &Config, check_objects: bool) -> Report {
    let mut checks = vec![
        ("build", Finding::Ok(build::identity())),
        (
            "config",
            Finding::Ok(format!(
                "{} ({})",
                config.config_path.display(),
                config.environment
            )),
        ),
        ("data directory", data_directory(config)),
        ("database", database(config).await),
        ("listen address", listen_address(config)),
        ("object store", object_store(config)),
        ("backups", backup_schedule(config)),
        ("research source", research_source(config).await),
        ("reader books", reader_books(config).await),
    ];
    if check_objects {
        checks.push(("objects", object_references(config).await));
        checks.push(("newest backup", newest_backup(config).await));
    }
    Report { checks }
}

fn data_directory(config: &Config) -> Finding {
    let path = config.data_dir.display();
    if config.data_dir.is_dir() {
        Finding::Ok(path.to_string())
    } else {
        Finding::Warning(format!("{path} does not exist yet; `serve` will create it"))
    }
}

async fn database(config: &Config) -> Finding {
    let path = &config.database.path;
    if !path.exists() {
        return Finding::Warning(format!(
            "{} does not exist yet; `serve` will create it",
            path.display()
        ));
    }
    let status = match ReadOnlyDb::open(path).await {
        Ok(db) => db.schema_status().await,
        Err(error) => Err(error),
    };
    match status {
        Ok(status) if status.pending.is_empty() => {
            Finding::Ok(format!("{} at schema {}", path.display(), status.current))
        }
        Ok(status) => Finding::Ok(format!(
            "{} at schema {}; `serve` will apply {:?}",
            path.display(),
            status.current,
            status.pending
        )),
        Err(error) => Finding::Problem(format!("{}: {error}", path.display())),
    }
}

fn listen_address(config: &Config) -> Finding {
    if health::address_in_use(config.bind) {
        Finding::Warning(format!("{} is in use (a running liotek?)", config.bind))
    } else {
        Finding::Ok(format!("{} is free", config.bind))
    }
}

fn object_store(config: &Config) -> Finding {
    Finding::Ok(match &config.blob_store {
        BlobStoreConfig::Filesystem { root } => format!("filesystem at {}", root.display()),
        BlobStoreConfig::S3(s3) => {
            format!(
                "s3 bucket {} at {}, namespace `{}`",
                s3.bucket, s3.endpoint, s3.namespace
            )
        }
    })
}

fn backup_schedule(config: &Config) -> Finding {
    Finding::Ok(match &config.backups {
        Some(schedule) => schedule.to_string(),
        None => "on request only; no schedule configured".to_string(),
    })
}

async fn research_source(config: &Config) -> Finding {
    let Some(path) = &config.research_database else {
        return Finding::Ok("not configured".to_string());
    };
    match ReadOnlyDb::open(path).await {
        Ok(_) => Finding::Ok(format!("{} is readable", path.display())),
        Err(error) => Finding::Problem(format!("{}: {error}", path.display())),
    }
}

async fn reader_books(config: &Config) -> Finding {
    let Some(path) = &config.reader_books_dir else {
        return Finding::Ok("not configured".to_string());
    };
    match reader::catalog(path).await {
        Ok(books) => Finding::Ok(format!("{} built books in {}", books.len(), path.display())),
        Err(error) => Finding::Problem(format!("{error:#}")),
    }
}

async fn object_references(config: &Config) -> Finding {
    match missing_objects(config).await {
        Ok((checked, missing)) if missing.is_empty() => {
            Finding::Ok(format!("all {checked} referenced objects exist"))
        }
        Ok((checked, missing)) => Finding::Problem(format!(
            "{} of {checked} referenced objects are missing: {}",
            missing.len(),
            missing.join(", ")
        )),
        Err(error) => Finding::Problem(format!("could not verify objects: {error:#}")),
    }
}

async fn newest_backup(config: &Config) -> Finding {
    let backups = match blob::open(&config.blob_store).await {
        Ok(blobs) => backup::list(blobs.as_ref()).await,
        Err(error) => Err(error.into()),
    };
    match backups {
        Ok(backups) => backup_freshness(&backups, config.backups.as_ref(), OffsetDateTime::now_utc()),
        Err(error) => Finding::Problem(format!("could not list backups: {error}")),
    }
}

fn backup_freshness(backups: &[Backup], schedule: Option<&BackupSchedule>, now: OffsetDateTime) -> Finding {
    let Some(newest) = backups.first() else {
        return Finding::Warning(match schedule {
            Some(_) => "none yet; `serve` makes the first scheduled backup when it starts".to_string(),
            None => "none; make one with `liotek backup`".to_string(),
        });
    };
    let summary = format!(
        "{}, {} old, {:.1} MiB",
        newest.name,
        describe_age(now - newest.name.created_at),
        newest.size as f64 / (1024.0 * 1024.0)
    );
    let Some(schedule) = schedule else {
        return Finding::Ok(summary);
    };

    let newest_scheduled = backups
        .iter()
        .find(|backup| backup.name.trigger == Trigger::Scheduled);
    match newest_scheduled {
        Some(scheduled) if now - scheduled.name.created_at <= 2 * schedule.every => Finding::Ok(summary),
        Some(scheduled) => Finding::Warning(format!(
            "{summary}; the newest scheduled backup, {}, is {} old, more than twice the interval",
            scheduled.name,
            describe_age(now - scheduled.name.created_at)
        )),
        None => Finding::Warning(format!(
            "{summary}; a schedule is configured but there is no scheduled backup"
        )),
    }
}

fn describe_age(age: time::Duration) -> String {
    match age.whole_minutes() {
        minutes if minutes < 60 => format!("{} min", minutes.max(0)),
        minutes if minutes < 48 * 60 => format!("{} h", minutes / 60),
        minutes => format!("{} days", minutes / (24 * 60)),
    }
}

async fn missing_objects(config: &Config) -> anyhow::Result<(usize, Vec<String>)> {
    let db = ReadOnlyDb::open(&config.database.path).await?;
    let blobs = blob::open(&config.blob_store).await?;

    let mut rows = db
        .query(
            "SELECT object_key FROM documents WHERE object_key IS NOT NULL ORDER BY id",
            (),
        )
        .await?;
    let (mut checked, mut missing) = (0, Vec::new());
    while let Some(row) = rows.next().await? {
        let key = ObjectKey::parse(row.get::<String>(0)?)?;
        checked += 1;
        if blobs.head(&key).await?.is_none() {
            missing.push(key.to_string());
        }
    }
    Ok((checked, missing))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use time::macros::datetime;

    use super::*;
    use crate::backup::BackupName;

    const NOW: OffsetDateTime = datetime!(2026-11-13 12:00 UTC);

    fn backup(trigger: Trigger, created_at: OffsetDateTime) -> Backup {
        Backup {
            name: BackupName::at(trigger, created_at),
            size: 3 * 1024 * 1024,
        }
    }

    fn daily() -> BackupSchedule {
        BackupSchedule {
            every: Duration::from_secs(24 * 60 * 60),
            keep: NonZeroUsize::new(30).unwrap(),
        }
    }

    #[test]
    fn reports_the_newest_backup_and_its_age() {
        let backups = [
            backup(Trigger::Manual, datetime!(2026-11-13 09:00 UTC)),
            backup(Trigger::Scheduled, datetime!(2026-11-12 02:00 UTC)),
        ];

        assert_eq!(
            backup_freshness(&backups, Some(&daily()), NOW),
            Finding::Ok("manual-20261113T090000Z, 3 h old, 3.0 MiB".to_string())
        );
    }

    #[test]
    fn warns_when_the_schedule_has_fallen_two_intervals_behind() {
        let recent_manual_only = [
            backup(Trigger::Manual, datetime!(2026-11-13 09:00 UTC)),
            backup(Trigger::Scheduled, datetime!(2026-11-10 02:00 UTC)),
        ];
        assert!(matches!(
            backup_freshness(&recent_manual_only, Some(&daily()), NOW),
            Finding::Warning(detail) if detail.contains("scheduled-20261110T020000Z, is 3 days old")
        ));

        assert!(matches!(
            backup_freshness(&recent_manual_only, None, NOW),
            Finding::Ok(_)
        ));

        let no_scheduled = [backup(Trigger::Manual, datetime!(2026-11-13 09:00 UTC))];
        assert!(matches!(
            backup_freshness(&no_scheduled, Some(&daily()), NOW),
            Finding::Warning(_)
        ));
        assert!(matches!(backup_freshness(&[], None, NOW), Finding::Warning(_)));
    }
}
