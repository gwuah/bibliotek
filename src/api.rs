use crate::db::Book;
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
    pub status: String,
    // #[serde(skip_serializing_if = "Vec::is_empty")]
    pub books: Vec<Book>,
    pub upload_id: Option<String>,
}

impl APIResponse {
    pub fn new_from_msg(msg: &str) -> Self {
        return APIResponse {
            status: msg.to_owned(),
            books: vec![],
            upload_id: None,
        };
    }

    pub fn new_from_msg_and_upload_id(msg: &str, upload_id: &str) -> Self {
        return APIResponse {
            status: msg.to_owned(),
            books: vec![],
            upload_id: None,
        };
    }

    pub fn new(msg: Option<&str>, books: Option<Vec<Book>>) -> Self {
        let books = match books {
            Some(books) => books,
            None => vec![],
        };

        let msg = match msg {
            Some(msg) => msg,
            None => "",
        };

        return APIResponse {
            status: msg.to_owned(),
            books: books,
            upload_id: None,
        };
    }

    pub fn to_json(self) -> impl serde::Serialize {
        return self;
    }
}
