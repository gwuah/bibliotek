use std::sync::Arc;

use axum::{
    Json,
    extract::{Multipart, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

use tracing::info;

use crate::db::Database;
use crate::{
    api::{APIResponse, QueryParams},
    s3::ObjectStorage,
};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub s3: Arc<ObjectStorage>,
}

const DEFAULT_PAGE: u32 = 1;
const DEFAULT_LIMIT: u32 = 50;

#[derive(Debug)]
pub struct HandlerParams {
    pub q: Option<String>,
    pub page: u32,
    pub limit: u32,
    pub offset: u32,
}

impl QueryParams {
    pub fn into_handler_params(self) -> HandlerParams {
        let page = self.page.unwrap_or(DEFAULT_PAGE).min(1);
        let limit = self
            .limit
            .unwrap_or(DEFAULT_LIMIT)
            .max(0)
            .min(DEFAULT_LIMIT);

        HandlerParams {
            q: self.q,
            page: page,
            limit: limit,
            offset: (page - 1) * limit,
        }
    }
}

pub async fn healthcheck() -> impl IntoResponse {
    info!("got healthcheck request");
    Json(APIResponse::new(Some("ok"), None))
}

pub async fn get_books(State(state): State<AppState>, Query(qp): Query<QueryParams>) -> Response {
    let hp = qp.into_handler_params();
    let db_call = state.db.get_books(hp).await;

    if let Err(e) = db_call {
        tracing::info!("failed to get books. db_error: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(APIResponse::new(Some("failed to get books"), None)),
        )
            .into_response();
    }

    tracing::info!("got books");
    let response = APIResponse::new(Some("got books"), db_call.ok());

    (StatusCode::OK, Json(response)).into_response()
}

pub async fn add_book(State(state): State<AppState>, Query(qp): Query<QueryParams>) -> Response {
    let hp = qp.into_handler_params();
    let db_call = state.db.get_books(hp).await;

    if let Err(e) = db_call {
        tracing::info!("failed to get books. db_error: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(APIResponse::new(Some("failed to get books"), None)),
        )
            .into_response();
    }

    tracing::info!("got books");
    let response = APIResponse::new(Some("got books"), db_call.ok());

    (StatusCode::OK, Json(response)).into_response()
}

pub async fn upload(State(_state): State<AppState>, mut multipart: Multipart) -> impl IntoResponse {
    let mut uploaded_files = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("unknown").to_string();
        let data = match field.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::error!("Failed to read field bytes: {}", e);
                continue;
            }
        };

        tracing::info!("Processing file: {} ({} bytes)", name, data.len());

        // For now, just log the file info
        uploaded_files.push(format!("{}: {} bytes", name, data.len()));
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "Files uploaded successfully",
            "files": uploaded_files
        })),
    )
}

pub async fn show_form() -> Html<&'static str> {
    Html(
        r#"
        <!doctype html>
        <html>
            <head></head>
            <body>
                <form action="/upload" method="post" enctype="multipart/form-data">
                    <label>
                        Upload file:
                        <input type="file" name="file" multiple>
                    </label>

                    <input type="submit" value="Upload files">
                </form>
            </body>
        </html>
        "#,
    )
}
#[cfg(test)]
mod tests {
    // use super::*;

    #[test]
    fn test_params_deserialize() {
        // let params = QueryParams {
        //     q: Some("test".to_string()),
        // };
        // assert_eq!(params.q, Some("test".to_string()));
    }
}
