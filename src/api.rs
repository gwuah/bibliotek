use crate::db::{Book, MetadataAggregate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    pub q: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub state: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct APIResponse {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub books: Vec<Book>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataAggregate>,
}

impl APIResponse {
    pub fn new_from_msg(msg: &str) -> Self {
        return APIResponse {
            status: msg.to_owned(),
            books: vec![],
            upload_id: None,
            metadata: None,
        };
    }

    pub fn to_json(self) -> impl serde::Serialize {
        return self;
    }
}
