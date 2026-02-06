use crate::db::*;
use crate::model::*;
use crate::resumable::PendingUpload;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    pub q: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UploadInitRequest {
    pub file_name: String,
    pub file_size: i64,
    pub file_signature: String,
}

#[derive(Debug, Serialize)]
pub struct UploadInitResponse {
    pub upload_id: String,
    pub status: String,
    pub chunk_size: i64,
    pub total_chunks: i64,
    pub completed_chunks: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PendingUploadsResponse {
    pub uploads: Vec<PendingUpload>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBookRequest {
    pub title: String,
    pub author_ids: Vec<i32>,
    pub tag_ids: Vec<i32>,
    pub category_ids: Vec<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEntityRequest {
    pub name: String,
}

#[derive(Debug, Serialize, Default)]
pub struct APIResponse {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub books: Vec<Book>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_books: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataAggregate>,
}

#[derive(Debug, Serialize)]
pub struct EntityResponse<T> {
    pub entity: T,
}

impl APIResponse {
    pub fn new_from_msg(msg: &str) -> Self {
        return APIResponse {
            status: msg.to_owned(),
            ..Default::default()
        };
    }

    pub fn to_json(self) -> impl serde::Serialize {
        return self;
    }
}
