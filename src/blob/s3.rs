//! Nothing is read from the environment or `~/.aws`, so the store this process talks to is
//! exactly the one its configuration names.

use std::error::Error as StdError;
use std::fmt::Debug;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart, Object, Part};
use bytes::Bytes;
use tokio::io::AsyncWriteExt;

use super::{
    BlobAccess, BlobError, BlobResult, BlobStore, ListedObject, Multipart, ObjectInfo, ObjectKey, StoredPart,
    ensure_listable,
};
use crate::config::S3Config;

const NO_SUCH_UPLOAD: &str = "NoSuchUpload";

pub struct S3Store {
    client: Client,
    bucket: String,
    endpoint: String,
    namespace: String,
}

impl S3Store {
    pub async fn connect(config: &S3Config) -> S3Store {
        let credentials = Credentials::new(
            config.access_key_id.expose(),
            config.secret_access_key.expose(),
            None,
            None,
            "liotek-config",
        );
        let sdk_config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(config.region.clone()))
            .endpoint_url(&config.endpoint)
            .credentials_provider(credentials)
            .build();

        S3Store {
            client: Client::from_conf(sdk_config),
            bucket: config.bucket.clone(),
            endpoint: config.endpoint.clone(),
            namespace: config.namespace.clone(),
        }
    }
}

#[async_trait]
impl BlobStore for S3Store {
    fn describe(&self) -> String {
        format!(
            "s3 bucket `{}` at {} (namespace `{}`)",
            self.bucket, self.endpoint, self.namespace
        )
    }

    fn namespace(&self) -> &str {
        &self.namespace
    }

    async fn begin_multipart(&self, key: &ObjectKey, content_type: &str) -> BlobResult<String> {
        key.ensure_within(&self.namespace)?;
        let response = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key.as_str())
            .content_type(content_type)
            .send()
            .await
            .map_err(classify)?;
        response
            .upload_id
            .ok_or_else(|| missing("an upload ID from CreateMultipartUpload"))
    }

    async fn put_part(
        &self,
        upload: Multipart<'_>,
        part_number: u32,
        bytes: Bytes,
    ) -> BlobResult<StoredPart> {
        upload.key.ensure_within(&self.namespace)?;
        let size = bytes.len() as u64;
        let response = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(upload.key.as_str())
            .upload_id(upload.upload_id)
            .part_number(provider_part_number(part_number)?)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .map_err(classify)?;
        Ok(StoredPart {
            part_number,
            size,
            etag: response.e_tag.ok_or_else(|| missing("an ETag from UploadPart"))?,
        })
    }

    async fn list_parts(&self, upload: Multipart<'_>) -> BlobResult<Vec<StoredPart>> {
        upload.key.ensure_within(&self.namespace)?;
        let listed: Vec<Part> = self
            .client
            .list_parts()
            .bucket(&self.bucket)
            .key(upload.key.as_str())
            .upload_id(upload.upload_id)
            .into_paginator()
            .items()
            .send()
            .collect::<Result<_, _>>()
            .await
            .map_err(classify)?;

        let mut parts = listed
            .into_iter()
            .map(stored_part)
            .collect::<BlobResult<Vec<_>>>()?;
        parts.sort_by_key(|part| part.part_number);
        Ok(parts)
    }

    async fn complete_multipart(&self, upload: Multipart<'_>, parts: &[StoredPart]) -> BlobResult<()> {
        upload.key.ensure_within(&self.namespace)?;
        let completed = parts
            .iter()
            .map(|part| {
                Ok(CompletedPart::builder()
                    .part_number(provider_part_number(part.part_number)?)
                    .e_tag(&part.etag)
                    .build())
            })
            .collect::<BlobResult<Vec<_>>>()?;

        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(upload.key.as_str())
            .upload_id(upload.upload_id)
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .set_parts(Some(completed))
                    .build(),
            )
            .send()
            .await
            .map_err(classify)?;
        Ok(())
    }

    async fn abort_multipart(&self, upload: Multipart<'_>) -> BlobResult<()> {
        upload.key.ensure_within(&self.namespace)?;
        let result = self
            .client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(upload.key.as_str())
            .upload_id(upload.upload_id)
            .send()
            .await;
        match result.map_err(classify) {
            Ok(_) | Err(BlobError::UploadNotFound) => Ok(()),
            Err(error) => Err(error),
        }
    }

    async fn head(&self, key: &ObjectKey) -> BlobResult<Option<ObjectInfo>> {
        let result = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await;
        match result {
            Ok(object) => Ok(Some(ObjectInfo {
                size: object.content_length.unwrap_or(0).max(0) as u64,
            })),
            Err(error) if error.as_service_error().is_some_and(|error| error.is_not_found()) => Ok(None),
            Err(error) => Err(classify(error)),
        }
    }

    async fn access(&self, key: &ObjectKey, ttl: Duration) -> BlobResult<BlobAccess> {
        let presigning =
            PresigningConfig::expires_in(ttl).map_err(|error| BlobError::Provider(error.into()))?;
        let request = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .presigned(presigning)
            .await
            .map_err(classify)?;
        Ok(BlobAccess::Redirect(request.uri().to_string()))
    }

    async fn list(&self, prefix: &str) -> BlobResult<Vec<ListedObject>> {
        ensure_listable(prefix, &self.namespace)?;
        let pages: Vec<_> = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .into_paginator()
            .send()
            .collect::<Result<_, _>>()
            .await
            .map_err(classify)?;

        let mut objects: Vec<ListedObject> = pages
            .into_iter()
            .flat_map(|page| page.contents.unwrap_or_default())
            .filter_map(listed_object)
            .collect();
        objects.sort_by(|a, b| a.key.as_str().cmp(b.key.as_str()));
        Ok(objects)
    }

    async fn delete(&self, key: &ObjectKey) -> BlobResult<()> {
        key.ensure_within(&self.namespace)?;
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await
            .map_err(classify)?;
        Ok(())
    }

    async fn download(&self, key: &ObjectKey, destination: &Path) -> BlobResult<u64> {
        let result = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key.as_str())
            .send()
            .await;
        let mut body = match result {
            Ok(object) => object.body,
            Err(error)
                if error
                    .as_service_error()
                    .is_some_and(|error| error.is_no_such_key()) =>
            {
                return Err(BlobError::ObjectNotFound);
            }
            Err(error) => return Err(classify(error)),
        };

        let mut file = tokio::fs::File::create(destination).await.map_err(local)?;
        let mut size = 0;
        while let Some(chunk) = body
            .try_next()
            .await
            .map_err(|error| BlobError::Provider(error.into()))?
        {
            file.write_all(&chunk).await.map_err(local)?;
            size += chunk.len() as u64;
        }
        file.sync_all().await.map_err(local)?;
        Ok(size)
    }
}

fn classify<E, R>(error: SdkError<E, R>) -> BlobError
where
    E: ProvideErrorMetadata + StdError + Send + Sync + 'static,
    R: Debug + Send + Sync + 'static,
{
    if error.code() == Some(NO_SUCH_UPLOAD) {
        BlobError::UploadNotFound
    } else {
        BlobError::Provider(error.into())
    }
}

fn listed_object(object: Object) -> Option<ListedObject> {
    Some(ListedObject {
        key: ObjectKey::parse(object.key?).ok()?,
        size: object.size.unwrap_or(0).max(0) as u64,
    })
}

fn stored_part(part: Part) -> BlobResult<StoredPart> {
    let part_number = part
        .part_number
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| missing("a part number from ListParts"))?;
    Ok(StoredPart {
        part_number,
        size: part.size.unwrap_or(0).max(0) as u64,
        etag: part.e_tag.ok_or_else(|| missing("an ETag from ListParts"))?,
    })
}

fn provider_part_number(part_number: u32) -> BlobResult<i32> {
    i32::try_from(part_number).map_err(|_| BlobError::InvalidPart {
        part_number,
        problem: "is beyond the provider's part-number range",
    })
}

fn local(error: std::io::Error) -> BlobError {
    BlobError::Provider(anyhow::Error::from(error).context("cannot write the downloaded object"))
}

fn missing(what: &str) -> BlobError {
    BlobError::Provider(anyhow::anyhow!("the object store did not return {what}"))
}
