use crate::config::Config;
use crate::error::ObjectStorageError;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::Client;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use serde::Serialize;

const DEFAULT_CHUNK_SIZE: i64 = 5 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct InitResponse {
    pub upload_id: String,
    pub key: String,
    pub chunk_size: i64,
    pub total_chunks: i64,
    pub completed_chunks: i64,
    pub is_resume: bool,
}

#[derive(Debug, Serialize)]
pub struct PendingUpload {
    pub upload_id: String,
    pub key: String,
    pub file_name: String,
    pub file_signature: String,
    pub completed_chunks: i64,
    pub bytes_uploaded: i64,
    pub created_at: String,
}

#[derive(Debug)]
pub struct UploadMetadata {
    pub signature: String,
    pub file_name: String,
}

pub struct ResumableUploadManager {
    pub client: Client,
    bucket: String,
    service: String,
}

impl ResumableUploadManager {
    pub async fn new(cfg: &Config) -> Result<Self, ObjectStorageError> {
        let region = cfg.storage.aws_region.clone();
        let endpoint_url = cfg.storage.aws_endpoint_url_s3.clone();
        let credentials = Credentials::new(
            &cfg.storage.aws_access_key_id,
            &cfg.storage.aws_secret_access_key,
            None,
            None,
            "config",
        );

        let config = aws_config::from_env()
            .region(aws_config::Region::new(region))
            .endpoint_url(endpoint_url)
            .credentials_provider(credentials)
            .load()
            .await;

        let client = Client::new(&config);

        Ok(Self {
            client,
            bucket: cfg.app.get_bucket().to_string(),
            service: cfg.storage.service.to_string(),
        })
    }

    fn build_key(signature: &str, file_name: &str) -> String {
        format!("{}_{}", signature, file_name)
    }

    fn parse_key(key: &str) -> Option<UploadMetadata> {
        let underscore_pos = key.find('_')?;
        if underscore_pos != 16 {
            return None;
        }
        Some(UploadMetadata {
            signature: key[..16].to_string(),
            file_name: key[17..].to_string(),
        })
    }

    async fn find_upload_by_signature(
        &self,
        signature: &str,
    ) -> Result<Option<(String, String)>, ObjectStorageError> {
        let prefix = format!("{}_", signature);

        let response = self
            .client
            .list_multipart_uploads()
            .bucket(&self.bucket)
            .prefix(&prefix)
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        if let Some(uploads) = response.uploads {
            if let Some(upload) = uploads.first() {
                let upload_id = upload.upload_id().unwrap_or_default().to_string();
                let key = upload.key().unwrap_or_default().to_string();
                if !upload_id.is_empty() && !key.is_empty() {
                    return Ok(Some((upload_id, key)));
                }
            }
        }

        Ok(None)
    }

    async fn get_parts_info(
        &self,
        upload_id: &str,
        key: &str,
    ) -> Result<(Vec<CompletedPart>, i64, i64, i64), ObjectStorageError> {
        let response = self
            .client
            .list_parts()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        let parts = response.parts();
        let completed_count = parts.len() as i64;

        let chunk_size = parts
            .first()
            .and_then(|p| p.size())
            .unwrap_or(DEFAULT_CHUNK_SIZE);

        let bytes_uploaded: i64 = parts.iter().filter_map(|p| p.size()).sum();

        let completed_parts: Vec<CompletedPart> = parts
            .iter()
            .map(|p| {
                CompletedPart::builder()
                    .part_number(p.part_number().unwrap_or(0))
                    .e_tag(p.e_tag().unwrap_or_default())
                    .build()
            })
            .collect();

        Ok((completed_parts, completed_count, bytes_uploaded, chunk_size))
    }

    pub async fn init_or_resume(
        &self,
        signature: &str,
        file_name: &str,
        file_size: i64,
    ) -> Result<InitResponse, ObjectStorageError> {
        if let Some((upload_id, key)) = self.find_upload_by_signature(signature).await? {
            let (_, completed_count, _, chunk_size) = self.get_parts_info(&upload_id, &key).await?;
            let total_chunks = (file_size + chunk_size - 1) / chunk_size;

            tracing::info!(
                "Resuming upload: signature={}, completed={}/{}",
                signature,
                completed_count,
                total_chunks
            );

            return Ok(InitResponse {
                upload_id,
                key,
                chunk_size,
                total_chunks,
                completed_chunks: completed_count,
                is_resume: true,
            });
        }

        let key = Self::build_key(signature, file_name);
        let chunk_size = DEFAULT_CHUNK_SIZE;
        let total_chunks = (file_size + chunk_size - 1) / chunk_size;

        let response = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        let upload_id = response
            .upload_id
            .ok_or(ObjectStorageError::UploadIdMissing)?;

        tracing::info!(
            "Created new upload: signature={}, upload_id={}, total_chunks={}",
            signature,
            upload_id,
            total_chunks
        );

        Ok(InitResponse {
            upload_id,
            key,
            chunk_size,
            total_chunks,
            completed_chunks: 0,
            is_resume: false,
        })
    }

    pub async fn upload_part(
        &self,
        upload_id: &str,
        key: &str,
        data: Vec<u8>,
        part_number: i32,
    ) -> Result<String, ObjectStorageError> {
        let response = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(part_number)
            .body(data.into())
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        let etag = response.e_tag.ok_or(ObjectStorageError::ETagMissing)?;
        Ok(etag)
    }

    pub async fn complete(
        &self,
        upload_id: &str,
        key: &str,
    ) -> Result<String, ObjectStorageError> {
        let (parts, _, _, _) = self.get_parts_info(upload_id, key).await?;

        if parts.is_empty() {
            return Err(ObjectStorageError::SessionNotFound(
                "No parts uploaded".to_string(),
            ));
        }

        let completed_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(parts))
            .build();

        let response = self
            .client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(completed_upload)
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        let location = response.key.unwrap_or_else(|| key.to_string());
        let bucket = response.bucket.unwrap_or_else(|| self.bucket.clone());

        Ok(crate::get_s3_url(&self.service, &bucket, &location))
    }

    pub async fn abort(&self, upload_id: &str, key: &str) -> Result<(), ObjectStorageError> {
        self.client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        tracing::info!("Aborted upload: upload_id={}", upload_id);
        Ok(())
    }

    pub async fn list_pending(&self) -> Result<Vec<PendingUpload>, ObjectStorageError> {
        let response = self
            .client
            .list_multipart_uploads()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        let mut pending = Vec::new();

        if let Some(uploads) = response.uploads {
            for upload in uploads {
                let upload_id = match upload.upload_id() {
                    Some(id) if !id.is_empty() => id.to_string(),
                    _ => continue,
                };
                let key = match upload.key() {
                    Some(k) if !k.is_empty() => k.to_string(),
                    _ => continue,
                };

                let metadata = match Self::parse_key(&key) {
                    Some(m) => m,
                    None => continue,
                };

                let (_, completed_chunks, bytes_uploaded, _) =
                    match self.get_parts_info(&upload_id, &key).await {
                        Ok(info) => info,
                        Err(e) => {
                            tracing::warn!("Failed to get parts for upload {}: {}", upload_id, e);
                            (vec![], 0, 0, DEFAULT_CHUNK_SIZE)
                        }
                    };

                let created_at = upload
                    .initiated()
                    .map(|dt| dt.to_string())
                    .unwrap_or_default();

                pending.push(PendingUpload {
                    upload_id,
                    key,
                    file_name: metadata.file_name,
                    file_signature: metadata.signature,
                    completed_chunks,
                    bytes_uploaded,
                    created_at,
                });
            }
        }

        pending.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(pending)
    }

    pub async fn cleanup_expired(&self, max_age_hours: u64) -> Result<usize, ObjectStorageError> {
        let response = self
            .client
            .list_multipart_uploads()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        let mut count = 0;
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(max_age_hours as i64);

        if let Some(uploads) = response.uploads {
            for upload in uploads {
                let upload_id = match upload.upload_id() {
                    Some(id) => id,
                    None => continue,
                };
                let key = match upload.key() {
                    Some(k) => k,
                    None => continue,
                };

                if Self::parse_key(key).is_none() {
                    continue;
                }

                if let Some(initiated) = upload.initiated() {
                    let initiated_str = initiated.to_string();
                    if let Ok(initiated_dt) = chrono::DateTime::parse_from_rfc3339(&initiated_str) {
                        if initiated_dt.with_timezone(&chrono::Utc) < cutoff {
                            if let Err(e) = self.abort(upload_id, key).await {
                                tracing::warn!("Failed to abort expired upload {}: {}", upload_id, e);
                            } else {
                                count += 1;
                            }
                        }
                    }
                }
            }
        }

        if count > 0 {
            tracing::info!("Cleaned up {} expired uploads", count);
        }

        Ok(count)
    }

    pub fn get_file_url(&self, key: &str) -> String {
        crate::get_s3_url(&self.service, &self.bucket, key)
    }

    pub fn get_filename_from_key(key: &str) -> Option<String> {
        Self::parse_key(key).map(|m| m.file_name)
    }

    pub async fn download_file(&self, key: &str) -> Result<Vec<u8>, ObjectStorageError> {
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        let body = response
            .body
            .collect()
            .await
            .map_err(|e| ObjectStorageError::S3Error(Box::new(e)))?;

        Ok(body.to_vec())
    }
}
