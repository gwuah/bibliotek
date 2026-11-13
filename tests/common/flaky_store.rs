#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;

use liotek::blob::{
    BlobAccess, BlobError, BlobResult, BlobStore, ListedObject, Multipart, ObjectInfo, ObjectKey, StoredPart,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    BeginMultipart,
    PutPart,
    ListParts,
    CompleteMultipart,
    AbortMultipart,
    Head,
    List,
    Delete,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Before,
    /// The store does the work, then the caller is told it failed.
    After,
}

pub struct FlakyStore {
    inner: Arc<dyn BlobStore>,
    failing: Mutex<HashMap<Operation, Failure>>,
}

impl FlakyStore {
    pub fn wrap(inner: Arc<dyn BlobStore>) -> FlakyStore {
        FlakyStore {
            inner,
            failing: Mutex::new(HashMap::new()),
        }
    }

    pub fn fail(&self, operation: Operation, failure: Failure) {
        self.failing.lock().unwrap().insert(operation, failure);
    }

    pub fn heal(&self, operation: Operation) {
        self.failing.lock().unwrap().remove(&operation);
    }

    fn failure(&self, operation: Operation) -> Option<Failure> {
        self.failing.lock().unwrap().get(&operation).copied()
    }

    async fn run<T>(&self, operation: Operation, call: impl Future<Output = BlobResult<T>>) -> BlobResult<T> {
        match self.failure(operation) {
            None => call.await,
            Some(Failure::Before) => Err(injected(operation)),
            Some(Failure::After) => {
                call.await?;
                Err(injected(operation))
            }
        }
    }
}

fn injected(operation: Operation) -> BlobError {
    BlobError::Provider(anyhow::anyhow!("injected failure in {operation:?}"))
}

#[async_trait]
impl BlobStore for FlakyStore {
    fn describe(&self) -> String {
        format!("flaky {}", self.inner.describe())
    }

    fn namespace(&self) -> &str {
        self.inner.namespace()
    }

    async fn begin_multipart(&self, key: &ObjectKey, content_type: &str) -> BlobResult<String> {
        self.run(
            Operation::BeginMultipart,
            self.inner.begin_multipart(key, content_type),
        )
        .await
    }

    async fn put_part(
        &self,
        upload: Multipart<'_>,
        part_number: u32,
        bytes: Bytes,
    ) -> BlobResult<StoredPart> {
        self.run(
            Operation::PutPart,
            self.inner.put_part(upload, part_number, bytes),
        )
        .await
    }

    async fn list_parts(&self, upload: Multipart<'_>) -> BlobResult<Vec<StoredPart>> {
        self.run(Operation::ListParts, self.inner.list_parts(upload))
            .await
    }

    async fn complete_multipart(&self, upload: Multipart<'_>, parts: &[StoredPart]) -> BlobResult<()> {
        self.run(
            Operation::CompleteMultipart,
            self.inner.complete_multipart(upload, parts),
        )
        .await
    }

    async fn abort_multipart(&self, upload: Multipart<'_>) -> BlobResult<()> {
        self.run(Operation::AbortMultipart, self.inner.abort_multipart(upload))
            .await
    }

    async fn head(&self, key: &ObjectKey) -> BlobResult<Option<ObjectInfo>> {
        self.run(Operation::Head, self.inner.head(key)).await
    }

    async fn access(&self, key: &ObjectKey, ttl: Duration) -> BlobResult<BlobAccess> {
        self.inner.access(key, ttl).await
    }

    async fn list(&self, prefix: &str) -> BlobResult<Vec<ListedObject>> {
        self.run(Operation::List, self.inner.list(prefix)).await
    }

    async fn delete(&self, key: &ObjectKey) -> BlobResult<()> {
        self.run(Operation::Delete, self.inner.delete(key)).await
    }

    async fn download(&self, key: &ObjectKey, destination: &Path) -> BlobResult<u64> {
        self.run(Operation::Download, self.inner.download(key, destination))
            .await
    }
}
