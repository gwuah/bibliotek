pub mod filesystem;
pub mod s3;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;

use crate::config::BlobStoreConfig;

pub type BlobResult<T> = Result<T, BlobError>;

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("`{0}` is not a valid object key")]
    InvalidKey(String),
    #[error("object key `{key}` is outside this store's namespace `{namespace}`")]
    OutsideNamespace { key: String, namespace: String },
    #[error("the multipart upload no longer exists in the object store")]
    UploadNotFound,
    #[error("object not found")]
    ObjectNotFound,
    #[error("part {part_number} {problem}")]
    InvalidPart { part_number: u32, problem: &'static str },
    #[error("object store failure: {0}")]
    Provider(#[source] anyhow::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectKey(String);

impl ObjectKey {
    pub fn parse(key: impl Into<String>) -> BlobResult<ObjectKey> {
        let key = key.into();
        let well_formed = !key.is_empty()
            && !key.chars().any(char::is_control)
            && key
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
        if well_formed {
            Ok(ObjectKey(key))
        } else {
            Err(BlobError::InvalidKey(key))
        }
    }

    pub fn within(namespace: &str, name: &str) -> BlobResult<ObjectKey> {
        ObjectKey::parse(format!("{namespace}{name}"))
    }

    pub fn generate(namespace: &str, extension: &str) -> ObjectKey {
        ObjectKey(format!(
            "{namespace}{}.{extension}",
            uuid::Uuid::new_v4().simple()
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn ensure_within(&self, namespace: &str) -> BlobResult<()> {
        if self.0.starts_with(namespace) {
            Ok(())
        } else {
            Err(BlobError::OutsideNamespace {
                key: self.0.clone(),
                namespace: namespace.to_string(),
            })
        }
    }
}

fn ensure_listable(prefix: &str, namespace: &str) -> BlobResult<()> {
    if !prefix.starts_with(namespace) {
        return Err(BlobError::OutsideNamespace {
            key: prefix.to_string(),
            namespace: namespace.to_string(),
        });
    }
    ObjectKey::parse(format!("{prefix}x")).map_err(|_| BlobError::InvalidKey(prefix.to_string()))?;
    Ok(())
}

impl fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Multipart<'a> {
    pub key: &'a ObjectKey,
    pub upload_id: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPart {
    pub part_number: u32,
    pub size: u64,
    pub etag: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedObject {
    pub key: ObjectKey,
    pub size: u64,
}

#[derive(Debug)]
pub enum BlobAccess {
    Redirect(String),
    LocalFile(PathBuf),
}

#[async_trait]
pub trait BlobStore: Send + Sync {
    fn describe(&self) -> String;

    /// Reads of one object may address any key, but anything that creates, lists, completes,
    /// aborts, or deletes must stay inside this prefix, so a misconfigured instance cannot
    /// disturb or enumerate objects it does not own.
    fn namespace(&self) -> &str;

    async fn begin_multipart(&self, key: &ObjectKey, content_type: &str) -> BlobResult<String>;

    async fn put_part(&self, upload: Multipart<'_>, part_number: u32, bytes: Bytes)
    -> BlobResult<StoredPart>;

    async fn list_parts(&self, upload: Multipart<'_>) -> BlobResult<Vec<StoredPart>>;

    async fn complete_multipart(&self, upload: Multipart<'_>, parts: &[StoredPart]) -> BlobResult<()>;

    async fn abort_multipart(&self, upload: Multipart<'_>) -> BlobResult<()>;

    async fn head(&self, key: &ObjectKey) -> BlobResult<Option<ObjectInfo>>;

    async fn access(&self, key: &ObjectKey, ttl: Duration) -> BlobResult<BlobAccess>;

    async fn list(&self, prefix: &str) -> BlobResult<Vec<ListedObject>>;

    async fn delete(&self, key: &ObjectKey) -> BlobResult<()>;

    async fn download(&self, key: &ObjectKey, destination: &Path) -> BlobResult<u64>;
}

pub async fn open(config: &BlobStoreConfig) -> BlobResult<Arc<dyn BlobStore>> {
    Ok(match config {
        BlobStoreConfig::Filesystem { root } => Arc::new(filesystem::FilesystemStore::open(root).await?),
        BlobStoreConfig::S3(s3) => Arc::new(s3::S3Store::connect(s3).await),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_keys_cannot_escape_or_be_empty() {
        for bad in [
            "",
            "/absolute",
            "a//b",
            "../up",
            "a/../b",
            "a/./b",
            "trailing/",
            "new\nline",
        ] {
            assert!(ObjectKey::parse(bad).is_err(), "{bad:?} should be rejected");
        }
        assert!(ObjectKey::parse("0a1b_My Book.pdf").is_ok());
        assert!(ObjectKey::parse("liotek/preview/abc.pdf").is_ok());
    }

    #[test]
    fn named_keys_are_placed_inside_the_namespace() {
        let key = ObjectKey::within("liotek/preview/", "backups/manual/x.sqlite").unwrap();
        assert_eq!(key.as_str(), "liotek/preview/backups/manual/x.sqlite");
        assert_eq!(ObjectKey::within("", "a.pdf").unwrap().as_str(), "a.pdf");
        assert!(ObjectKey::within("liotek/preview/", "../escape").is_err());
    }

    #[test]
    fn listable_prefixes_stay_inside_the_namespace_and_cannot_escape() {
        assert!(ensure_listable("local/backups/", "local/").is_ok());
        assert!(ensure_listable("local/back", "local/").is_ok());
        assert!(ensure_listable("local/", "local/").is_ok());
        assert!(ensure_listable("", "").is_ok());
        assert!(matches!(
            ensure_listable("other/backups/", "local/"),
            Err(BlobError::OutsideNamespace { .. })
        ));
        assert!(matches!(
            ensure_listable("", "local/"),
            Err(BlobError::OutsideNamespace { .. })
        ));
        for escaping in ["local/../", "local/../../etc/", "local//", "local/./"] {
            assert!(
                matches!(ensure_listable(escaping, "local/"), Err(BlobError::InvalidKey(_))),
                "{escaping:?} should be rejected"
            );
        }
    }

    #[test]
    fn generated_keys_stay_inside_the_namespace() {
        let key = ObjectKey::generate("liotek/preview/", "pdf");
        assert!(key.ensure_within("liotek/preview/").is_ok());
        assert!(key.ensure_within("liotek/production/").is_err());
        assert!(key.as_str().ends_with(".pdf"));
    }
}
