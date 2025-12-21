use std::collections::HashSet;
use std::future::Future;

pub enum SyncResult<T> {
    Created(T),
    Updated(T),
    Unchanged(T),
    Error,
}

impl<T: Copy> SyncResult<T> {
    pub fn record(self, stats: &mut SyncStats) -> Option<T> {
        match self {
            SyncResult::Created(id) => {
                stats.created += 1;
                Some(id)
            }
            SyncResult::Updated(id) => {
                stats.updated += 1;
                Some(id)
            }
            SyncResult::Unchanged(id) => {
                stats.unchanged += 1;
                Some(id)
            }
            SyncResult::Error => None,
        }
    }
}

impl SyncResult<()> {
    pub fn record_unit(self, stats: &mut SyncStats) {
        match self {
            SyncResult::Created(()) => stats.created += 1,
            SyncResult::Updated(()) => stats.updated += 1,
            SyncResult::Unchanged(()) => stats.unchanged += 1,
            SyncResult::Error => {}
        }
    }
}

#[derive(Debug, Default)]
pub struct SyncStats {
    pub created: i32,
    pub updated: i32,
    pub deleted: i32,
    pub unchanged: i32,
}

pub trait Syncable {
    fn external_id(&self) -> Option<&str>;
    fn content_hash(&self) -> Option<&str>;
    fn id(&self) -> i32;
}

pub fn is_unchanged<T: Syncable>(existing: &T, new_hash: &str) -> bool {
    existing.content_hash() == Some(new_hash)
}

pub fn is_orphan(external_id: &Option<String>, seen: &HashSet<String>) -> bool {
    external_id.as_ref().map_or(false, |id| !seen.contains(id))
}

pub fn log_find_error(entity: &str, external_id: &str, e: impl std::fmt::Display) {
    tracing::error!("Failed to check {} {}: {}", entity, external_id, e);
}

pub fn log_update_error(entity: &str, external_id: &str, e: impl std::fmt::Display) {
    tracing::error!("Failed to update {} {}: {}", entity, external_id, e);
}

pub fn log_update_not_found(entity: &str, id: i32) {
    tracing::warn!("{} {} not found for update", entity, id);
}

pub fn log_create_error(entity: &str, external_id: &str, e: impl std::fmt::Display) {
    tracing::error!("Failed to create {} {}: {}", entity, external_id, e);
}

pub fn handle_update_result<T>(
    result: Result<Option<T>, impl std::fmt::Display>,
    id: i32,
    entity: &str,
    external_id: &str,
) -> SyncResult<i32> {
    match result {
        Ok(Some(_)) => SyncResult::Updated(id),
        Ok(None) => {
            log_update_not_found(entity, id);
            SyncResult::Error
        }
        Err(e) => {
            log_update_error(entity, external_id, e);
            SyncResult::Error
        }
    }
}

pub fn handle_update_result_unit<T>(
    result: Result<Option<T>, impl std::fmt::Display>,
    id: i32,
    entity: &str,
    external_id: &str,
) -> SyncResult<()> {
    match result {
        Ok(Some(_)) => SyncResult::Updated(()),
        Ok(None) => {
            log_update_not_found(entity, id);
            SyncResult::Error
        }
        Err(e) => {
            log_update_error(entity, external_id, e);
            SyncResult::Error
        }
    }
}

pub fn handle_create_result<T, F>(
    result: Result<T, impl std::fmt::Display>,
    extract_id: F,
    entity: &str,
    external_id: &str,
) -> SyncResult<i32>
where
    F: FnOnce(T) -> i32,
{
    match result {
        Ok(created) => SyncResult::Created(extract_id(created)),
        Err(e) => {
            log_create_error(entity, external_id, e);
            SyncResult::Error
        }
    }
}

pub fn handle_create_result_unit<T>(
    result: Result<T, impl std::fmt::Display>,
    entity: &str,
    external_id: &str,
) -> SyncResult<()> {
    match result {
        Ok(_) => SyncResult::Created(()),
        Err(e) => {
            log_create_error(entity, external_id, e);
            SyncResult::Error
        }
    }
}

pub async fn delete_orphans<T, I, F, Fut, D, DFut>(
    fetch_orphans: F,
    delete_fn: D,
    seen: &HashSet<String>,
    stats: &mut SyncStats,
    entity: &str,
) where
    T: Syncable,
    I: IntoIterator<Item = T>,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<I, anyhow::Error>>,
    D: Fn(i32) -> DFut,
    DFut: Future<Output = Result<bool, anyhow::Error>>,
{
    let items = match fetch_orphans().await {
        Ok(items) => items,
        Err(e) => {
            tracing::error!("Failed to find orphan {}s: {}", entity, e);
            return;
        }
    };

    for item in items {
        let ext_id = item.external_id().map(|s| s.to_string());
        if is_orphan(&ext_id, seen) {
            if delete_fn(item.id()).await.unwrap_or(false) {
                stats.deleted += 1;
            }
        }
    }
}
