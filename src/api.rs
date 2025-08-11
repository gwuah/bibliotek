use crate::db::Book;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    pub q: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Default)]
pub struct APIResponse {
    status: String,
    // #[serde(skip_serializing_if = "Vec::is_empty")]
    books: Vec<Book>,
}

impl APIResponse {
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
        };
    }

    pub fn to_json(self) -> impl serde::Serialize {
        return self;
    }
}
