use crate::config::Config;
use crate::error::ObjectStorageError;
use aws_sdk_s3::Client;
use aws_sdk_s3::types::{Bucket, CompletedMultipartUpload, CompletedPart};
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};

pub struct UploadSession {
    key: String,
    parts: Arc<Mutex<Vec<CompletedPart>>>,
}

pub struct ObjectStorage {
    pub client: Client,
    sessions: Arc<Mutex<HashMap<String, UploadSession>>>,
    bucket: String,
}

impl ObjectStorage {
    pub async fn new(cfg: &Config) -> Result<Self, ObjectStorageError> {
        let region = env::var("AWS_REGION")?;
        let endpoint_url = env::var("AWS_ENDPOINT_URL_S3")?;

        let config = aws_config::from_env()
            .region(aws_config::Region::new(region))
            .endpoint_url(endpoint_url)
            .load()
            .await;

        let client = Client::new(&config);

        let object_storage = Self {
            client,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            bucket: cfg.app.get_bucket().to_string(),
        };

        Ok(object_storage)
    }

    pub async fn list_buckets(&self) -> Result<Vec<Bucket>, ObjectStorageError> {
        let response = self
            .client
            .list_buckets()
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;
        Ok(response.buckets().to_vec())
    }

    pub async fn start_upload(&self, key: &str) -> Result<String, ObjectStorageError> {
        tracing::info!("starting upload for key: {}", key);
        let response = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| ObjectStorageError::LockError(e.to_string()))?;

        let upload_id = response
            .upload_id
            .ok_or(ObjectStorageError::UploadIdMissing)?;

        if sessions.contains_key(&upload_id) {
            return Err(ObjectStorageError::SessionAlreadyExists(upload_id));
        }

        sessions.insert(
            upload_id.clone(),
            UploadSession {
                key: key.to_string(),
                parts: Arc::new(Mutex::new(Vec::new())),
            },
        );

        Ok(upload_id)
    }

    pub async fn upload(
        &self,
        upload_id: &str,
        data: Vec<u8>,
    ) -> Result<String, ObjectStorageError> {
        let (session_key, session_parts) = {
            let sessions = self
                .sessions
                .lock()
                .map_err(|e| ObjectStorageError::LockError(e.to_string()))?;

            let session = sessions
                .get(upload_id)
                .ok_or(ObjectStorageError::SessionNotFound(upload_id.to_string()))?;

            (session.key.clone(), session.parts.clone())
        };

        let part_number = session_parts
            .lock()
            .map_err(|e| ObjectStorageError::LockError(e.to_string()))?
            .len()
            + 1;

        let response = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(&session_key)
            .upload_id(upload_id)
            .part_number(part_number as i32)
            .body(data.into())
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        let etag = response.e_tag.ok_or(ObjectStorageError::ETagMissing)?;

        let completed_part = CompletedPart::builder()
            .part_number(part_number as i32)
            .e_tag(&etag)
            .build();

        let mut parts = session_parts
            .lock()
            .map_err(|e| ObjectStorageError::LockError(e.to_string()))?;
        parts.push(completed_part);

        Ok(etag)
    }

    pub async fn complete_upload(&self, upload_id: &str) -> Result<String, ObjectStorageError> {
        let (key, locked_parts) = {
            let sessions = self
                .sessions
                .lock()
                .map_err(|e| ObjectStorageError::LockError(e.to_string()))?;

            let session = sessions
                .get(upload_id)
                .ok_or(ObjectStorageError::SessionNotFound(upload_id.to_string()))?;

            (session.key.clone(), session.parts.clone())
        };

        let parts = locked_parts
            .lock()
            .map_err(|e| ObjectStorageError::LockError(e.to_string()))?
            .as_slice()
            .to_vec();

        let completed_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(parts))
            .build();

        let response = self
            .client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(&key)
            .upload_id(upload_id)
            .multipart_upload(completed_upload)
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        let location = response.location.ok_or(ObjectStorageError::UploadFailed)?;

        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| ObjectStorageError::LockError(e.to_string()))?;

        sessions.remove(upload_id);
        Ok(location)
    }
}
