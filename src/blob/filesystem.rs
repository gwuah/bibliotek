use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::{
    BlobAccess, BlobError, BlobResult, BlobStore, ListedObject, Multipart, ObjectInfo, ObjectKey, StoredPart,
    ensure_listable,
};

const NAMESPACE: &str = "local/";

const KEY_FILE: &str = "key";

pub struct FilesystemStore {
    root: PathBuf,
}

impl FilesystemStore {
    pub async fn open(root: &Path) -> BlobResult<FilesystemStore> {
        let store = FilesystemStore {
            root: root.to_path_buf(),
        };
        fs::create_dir_all(store.root.join("objects"))
            .await
            .map_err(provider)?;
        fs::create_dir_all(store.root.join("multipart"))
            .await
            .map_err(provider)?;
        Ok(store)
    }

    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn object_path(&self, key: &ObjectKey) -> PathBuf {
        self.objects_dir().join(key.as_str())
    }

    fn key_at(&self, path: &Path) -> Option<ObjectKey> {
        let relative = path.strip_prefix(self.objects_dir()).ok()?;
        let segments: Option<Vec<&str>> = relative
            .components()
            .map(|part| part.as_os_str().to_str())
            .collect();
        ObjectKey::parse(segments?.join("/")).ok()
    }

    async fn upload_dir(&self, upload: Multipart<'_>) -> BlobResult<PathBuf> {
        upload.key.ensure_within(NAMESPACE)?;
        let dir = self.upload_dir_unchecked(upload.upload_id)?;
        match fs::read_to_string(dir.join(KEY_FILE)).await {
            Ok(key) if key == upload.key.as_str() => Ok(dir),
            Ok(_) => Err(BlobError::UploadNotFound),
            Err(error) if error.kind() == ErrorKind::NotFound => Err(BlobError::UploadNotFound),
            Err(error) => Err(provider(error)),
        }
    }

    fn upload_dir_unchecked(&self, upload_id: &str) -> BlobResult<PathBuf> {
        // Upload IDs are generated here as hex; anything else must not become a path.
        if upload_id.is_empty() || !upload_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(BlobError::UploadNotFound);
        }
        Ok(self.root.join("multipart").join(upload_id))
    }
}

#[async_trait]
impl BlobStore for FilesystemStore {
    fn describe(&self) -> String {
        format!("filesystem at {}", self.root.display())
    }

    fn namespace(&self) -> &str {
        NAMESPACE
    }

    async fn begin_multipart(&self, key: &ObjectKey, _content_type: &str) -> BlobResult<String> {
        key.ensure_within(NAMESPACE)?;
        let upload_id = uuid::Uuid::new_v4().simple().to_string();
        let dir = self.upload_dir_unchecked(&upload_id)?;
        fs::create_dir_all(&dir).await.map_err(provider)?;
        fs::write(dir.join(KEY_FILE), key.as_str())
            .await
            .map_err(provider)?;
        Ok(upload_id)
    }

    async fn put_part(
        &self,
        upload: Multipart<'_>,
        part_number: u32,
        bytes: Bytes,
    ) -> BlobResult<StoredPart> {
        let dir = self.upload_dir(upload).await?;
        write_atomically(&dir.join(part_number.to_string()), &bytes).await?;
        Ok(StoredPart {
            part_number,
            size: bytes.len() as u64,
            etag: etag(&bytes),
        })
    }

    async fn list_parts(&self, upload: Multipart<'_>) -> BlobResult<Vec<StoredPart>> {
        let dir = self.upload_dir(upload).await?;
        let mut entries = fs::read_dir(&dir).await.map_err(provider)?;
        let mut parts = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(provider)? {
            let Some(part_number) = entry.file_name().to_str().and_then(|name| name.parse().ok()) else {
                continue;
            };
            let bytes = fs::read(entry.path()).await.map_err(provider)?;
            parts.push(StoredPart {
                part_number,
                size: bytes.len() as u64,
                etag: etag(&bytes),
            });
        }
        parts.sort_by_key(|part| part.part_number);
        Ok(parts)
    }

    async fn complete_multipart(&self, upload: Multipart<'_>, parts: &[StoredPart]) -> BlobResult<()> {
        let dir = self.upload_dir(upload).await?;
        let assembled = dir.join("assembled");
        let mut output = fs::File::create(&assembled).await.map_err(provider)?;
        for part in parts {
            let bytes = match fs::read(dir.join(part.part_number.to_string())).await {
                Ok(bytes) if etag(&bytes) == part.etag => bytes,
                Ok(_) => return Err(invalid_part(part.part_number, "does not match its etag")),
                Err(_) => return Err(invalid_part(part.part_number, "was never uploaded")),
            };
            output.write_all(&bytes).await.map_err(provider)?;
        }
        output.sync_all().await.map_err(provider)?;

        let destination = self.object_path(upload.key);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).await.map_err(provider)?;
        }
        fs::rename(&assembled, &destination).await.map_err(provider)?;
        fs::remove_dir_all(&dir).await.map_err(provider)
    }

    async fn abort_multipart(&self, upload: Multipart<'_>) -> BlobResult<()> {
        match self.upload_dir(upload).await {
            Ok(dir) => fs::remove_dir_all(dir).await.map_err(provider),
            Err(BlobError::UploadNotFound) => Ok(()),
            Err(error) => Err(error),
        }
    }

    async fn head(&self, key: &ObjectKey) -> BlobResult<Option<ObjectInfo>> {
        match fs::metadata(self.object_path(key)).await {
            Ok(metadata) if metadata.is_file() => Ok(Some(ObjectInfo { size: metadata.len() })),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(provider(error)),
        }
    }

    async fn access(&self, key: &ObjectKey, _ttl: Duration) -> BlobResult<BlobAccess> {
        match self.head(key).await? {
            Some(_) => Ok(BlobAccess::LocalFile(self.object_path(key))),
            None => Err(BlobError::ObjectNotFound),
        }
    }

    async fn list(&self, prefix: &str) -> BlobResult<Vec<ListedObject>> {
        ensure_listable(prefix, NAMESPACE)?;
        let directory = &prefix[..prefix.rfind('/').map_or(0, |slash| slash + 1)];
        let mut pending = vec![self.objects_dir().join(directory)];
        let mut objects = Vec::new();
        while let Some(dir) = pending.pop() {
            let mut entries = match fs::read_dir(&dir).await {
                Ok(entries) => entries,
                Err(error) if error.kind() == ErrorKind::NotFound => continue,
                Err(error) => return Err(provider(error)),
            };
            while let Some(entry) = entries.next_entry().await.map_err(provider)? {
                let metadata = entry.metadata().await.map_err(provider)?;
                if metadata.is_dir() {
                    pending.push(entry.path());
                } else if let Some(key) = self.key_at(&entry.path())
                    && key.as_str().starts_with(prefix)
                {
                    objects.push(ListedObject {
                        key,
                        size: metadata.len(),
                    });
                }
            }
        }
        objects.sort_by(|a, b| a.key.as_str().cmp(b.key.as_str()));
        Ok(objects)
    }

    async fn delete(&self, key: &ObjectKey) -> BlobResult<()> {
        key.ensure_within(NAMESPACE)?;
        match fs::remove_file(self.object_path(key)).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(provider(error)),
        }
    }

    async fn download(&self, key: &ObjectKey, destination: &Path) -> BlobResult<u64> {
        match fs::copy(self.object_path(key), destination).await {
            Ok(size) => Ok(size),
            Err(error) if error.kind() == ErrorKind::NotFound => Err(BlobError::ObjectNotFound),
            Err(error) => Err(provider(error)),
        }
    }
}

async fn write_atomically(path: &Path, bytes: &[u8]) -> BlobResult<()> {
    let temporary = path.with_extension("writing");
    fs::write(&temporary, bytes).await.map_err(provider)?;
    fs::rename(&temporary, path).await.map_err(provider)
}

fn etag(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn provider(error: std::io::Error) -> BlobError {
    BlobError::Provider(error.into())
}

fn invalid_part(part_number: u32, problem: &'static str) -> BlobError {
    BlobError::InvalidPart { part_number, problem }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> (tempfile::TempDir, FilesystemStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemStore::open(dir.path()).await.unwrap();
        (dir, store)
    }

    #[tokio::test]
    async fn assembles_parts_in_order_into_one_object() {
        let (_dir, store) = store().await;
        let key = ObjectKey::generate(store.namespace(), "pdf");
        let upload_id = store.begin_multipart(&key, "application/pdf").await.unwrap();
        let upload = Multipart {
            key: &key,
            upload_id: &upload_id,
        };

        store
            .put_part(upload, 2, Bytes::from_static(b"world"))
            .await
            .unwrap();
        store
            .put_part(upload, 1, Bytes::from_static(b"jello "))
            .await
            .unwrap();
        store
            .put_part(upload, 1, Bytes::from_static(b"hello "))
            .await
            .unwrap();

        let parts = store.list_parts(upload).await.unwrap();
        assert_eq!(
            parts.iter().map(|part| part.part_number).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            store.head(&key).await.unwrap(),
            None,
            "no object before completion"
        );

        store.complete_multipart(upload, &parts).await.unwrap();

        assert_eq!(store.head(&key).await.unwrap(), Some(ObjectInfo { size: 11 }));
        let BlobAccess::LocalFile(path) = store.access(&key, Duration::from_secs(60)).await.unwrap() else {
            panic!("the filesystem store serves local files");
        };
        assert_eq!(fs::read(path).await.unwrap(), b"hello world");
        assert!(matches!(
            store.list_parts(upload).await,
            Err(BlobError::UploadNotFound)
        ));
    }

    #[tokio::test]
    async fn completion_fails_whole_when_a_part_is_missing() {
        let (_dir, store) = store().await;
        let key = ObjectKey::generate(store.namespace(), "pdf");
        let upload_id = store.begin_multipart(&key, "application/pdf").await.unwrap();
        let upload = Multipart {
            key: &key,
            upload_id: &upload_id,
        };
        let first = store
            .put_part(upload, 1, Bytes::from_static(b"only"))
            .await
            .unwrap();
        let phantom = StoredPart {
            part_number: 2,
            size: 4,
            etag: "none".into(),
        };

        assert!(store.complete_multipart(upload, &[first, phantom]).await.is_err());
        assert_eq!(store.head(&key).await.unwrap(), None);
        assert_eq!(
            store.list_parts(upload).await.unwrap().len(),
            1,
            "the upload is still resumable"
        );
    }

    #[tokio::test]
    async fn abort_is_idempotent() {
        let (_dir, store) = store().await;
        let key = ObjectKey::generate(store.namespace(), "pdf");
        let upload_id = store.begin_multipart(&key, "application/pdf").await.unwrap();
        let upload = Multipart {
            key: &key,
            upload_id: &upload_id,
        };

        store.abort_multipart(upload).await.unwrap();
        store.abort_multipart(upload).await.unwrap();
        assert!(matches!(
            store.put_part(upload, 1, Bytes::from_static(b"late")).await,
            Err(BlobError::UploadNotFound)
        ));
    }

    #[tokio::test]
    async fn refuses_to_create_outside_the_namespace() {
        let (_dir, store) = store().await;
        let foreign = ObjectKey::parse("someone-else/book.pdf").unwrap();

        assert!(matches!(
            store.begin_multipart(&foreign, "application/pdf").await,
            Err(BlobError::OutsideNamespace { .. })
        ));
        assert_eq!(store.head(&foreign).await.unwrap(), None);
    }

    async fn put_object(store: &FilesystemStore, key: &ObjectKey, bytes: &'static [u8]) {
        let upload_id = store
            .begin_multipart(key, "application/octet-stream")
            .await
            .unwrap();
        let upload = Multipart {
            key,
            upload_id: &upload_id,
        };
        let part = store
            .put_part(upload, 1, Bytes::from_static(bytes))
            .await
            .unwrap();
        store.complete_multipart(upload, &[part]).await.unwrap();
    }

    fn key(key: &str) -> ObjectKey {
        ObjectKey::parse(key).unwrap()
    }

    #[tokio::test]
    async fn lists_finished_objects_under_a_prefix_in_key_order() {
        let (dir, store) = store().await;
        put_object(&store, &key("local/backups/manual/b.sqlite"), b"bb").await;
        put_object(&store, &key("local/backups/manual/a.sqlite"), b"a").await;
        put_object(&store, &key("local/backups/scheduled/c.sqlite"), b"ccc").await;
        put_object(&store, &key("local/book.pdf"), b"pdf").await;
        fs::create_dir_all(dir.path().join("objects/elsewhere"))
            .await
            .unwrap();
        fs::write(dir.path().join("objects/elsewhere/x"), b"x")
            .await
            .unwrap();
        store
            .begin_multipart(&key("local/backups/manual/z.sqlite"), "application/octet-stream")
            .await
            .unwrap();

        let listed = store.list("local/backups/").await.unwrap();
        assert_eq!(
            listed,
            vec![
                ListedObject {
                    key: key("local/backups/manual/a.sqlite"),
                    size: 1
                },
                ListedObject {
                    key: key("local/backups/manual/b.sqlite"),
                    size: 2
                },
                ListedObject {
                    key: key("local/backups/scheduled/c.sqlite"),
                    size: 3
                },
            ]
        );

        assert_eq!(store.list("local/backups/man").await.unwrap().len(), 2);
        assert_eq!(store.list("local/nothing/").await.unwrap(), vec![]);
        assert_eq!(store.list("local/").await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn refuses_to_list_outside_the_namespace() {
        let (_dir, store) = store().await;

        for prefix in ["", "elsewhere/", "../"] {
            assert!(
                matches!(store.list(prefix).await, Err(BlobError::OutsideNamespace { .. })),
                "{prefix:?}"
            );
        }
        assert!(matches!(
            store.list("local/../../").await,
            Err(BlobError::InvalidKey(_))
        ));
    }

    #[tokio::test]
    async fn delete_is_idempotent_and_confined_to_the_namespace() {
        let (dir, store) = store().await;
        let ours = key("local/backups/manual/a.sqlite");
        put_object(&store, &ours, b"a").await;
        fs::create_dir_all(dir.path().join("objects/elsewhere"))
            .await
            .unwrap();
        fs::write(dir.path().join("objects/elsewhere/x"), b"x")
            .await
            .unwrap();

        store.delete(&ours).await.unwrap();
        store.delete(&ours).await.unwrap();
        assert_eq!(store.head(&ours).await.unwrap(), None);

        let foreign = key("elsewhere/x");
        assert!(matches!(
            store.delete(&foreign).await,
            Err(BlobError::OutsideNamespace { .. })
        ));
        assert!(
            store.head(&foreign).await.unwrap().is_some(),
            "the foreign object survives"
        );
    }

    #[tokio::test]
    async fn downloads_an_object_into_a_local_file() {
        let (dir, store) = store().await;
        let stored = key("local/backups/manual/a.sqlite");
        put_object(&store, &stored, b"library").await;
        let destination = dir.path().join("downloaded.sqlite");

        assert_eq!(store.download(&stored, &destination).await.unwrap(), 7);
        assert_eq!(fs::read(&destination).await.unwrap(), b"library");
        assert!(matches!(
            store
                .download(&key("local/missing"), &dir.path().join("missing"))
                .await,
            Err(BlobError::ObjectNotFound)
        ));
    }
}
