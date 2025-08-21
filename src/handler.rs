use std::sync::Arc;

use axum::{
    Json,
    extract::{Multipart, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use std::fs;

use tracing::info;

use crate::{
    api::{APIResponse, QueryParams},
    s3::ObjectStorage,
};
use crate::{db::Database, error::HandlerError};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub s3: Arc<ObjectStorage>,
}

#[derive(Debug)]
pub struct Form {
    pub file_name: Option<String>,
    pub upload_id: Option<String>,
    pub chunk: Option<axum::body::Bytes>,
}

const DEFAULT_PAGE: u32 = 1;
const DEFAULT_LIMIT: u32 = 50;

#[derive(Debug)]
pub struct HandlerParams {
    pub q: Option<String>,
    pub page: u32,
    pub limit: u32,
    pub offset: u32,
    pub state: Option<String>,
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
            state: self.state,
        }
    }
}

pub async fn healthcheck() -> impl IntoResponse {
    info!("got healthcheck request");
    crate::good_response(APIResponse {
        books: vec![],
        status: "ok".to_owned(),
        upload_id: None,
    })
}

pub async fn get_books(State(state): State<AppState>, Query(qp): Query<QueryParams>) -> Response {
    let hp = qp.into_handler_params();
    let db_call = state.db.get_books(hp).await;

    if let Err(e) = db_call {
        tracing::info!("failed to get books. db_error: {}", e);
        return crate::bad_request(APIResponse::new_from_msg("failed to get books"));
    }

    tracing::info!("got books");
    crate::good_response(APIResponse {
        books: db_call.ok().unwrap_or_default(),
        status: "ok".to_owned(),
        upload_id: None,
    })
}

pub async fn add_book(State(state): State<AppState>, Query(qp): Query<QueryParams>) -> Response {
    let hp = qp.into_handler_params();
    let db_call = state.db.get_books(hp).await;

    if let Err(e) = db_call {
        tracing::info!("failed to add book. db_error: {}", e);
        return crate::bad_request(APIResponse::new_from_msg("failed to add book"));
    }

    tracing::info!("added book");
    crate::good_response(APIResponse {
        books: vec![],
        status: "ok".to_owned(),
        upload_id: None,
    })
}

async fn extract_form(multipart: &mut Multipart) -> Result<Form, HandlerError> {
    let mut form = Form {
        file_name: None,
        upload_id: None,
        chunk: None,
    };

    let map_err = |e: axum::extract::multipart::MultipartError| -> HandlerError {
        HandlerError::ValidationError(e.to_string())
    };

    while let Ok(Some(field)) = multipart.next_field().await {
        let form_field_name = field.name().unwrap_or("unknown");
        match form_field_name {
            "file_name" => form.file_name = Some(field.text().await.map_err(map_err)?),
            "upload_id" => form.upload_id = Some(field.text().await.map_err(map_err)?),
            "chunk" => form.chunk = Some(field.bytes().await.map_err(map_err)?),
            _ => {
                tracing::error!("unknown form field: {}", form_field_name);
                continue;
            }
        }
    }

    Ok(form)
}

async fn handle_init_upload(
    state: &AppState,
    multipart: &mut Multipart,
) -> Result<String, HandlerError> {
    let form = extract_form(multipart).await?;
    let filename = form.file_name.ok_or(HandlerError::ValidationError(
        "file_name is required".to_string(),
    ))?;
    let response = state.s3.start_upload(filename.as_str()).await?;
    Ok(response)
}

async fn handle_continue_upload(
    state: &AppState,
    multipart: &mut Multipart,
) -> Result<String, HandlerError> {
    let form = extract_form(multipart).await?;
    let upload_id = form.upload_id.ok_or(HandlerError::ValidationError(
        "upload_id is required".to_string(),
    ))?;
    let chunk = form.chunk.ok_or(HandlerError::ValidationError(
        "chunk is required".to_string(),
    ))?;
    let response = state.s3.upload(&upload_id, chunk.to_vec()).await?;
    Ok(response)
}

async fn handle_complete_upload(
    state: &AppState,
    multipart: &mut Multipart,
) -> Result<String, HandlerError> {
    let form = extract_form(multipart).await?;
    let upload_id = form.upload_id.ok_or(HandlerError::ValidationError(
        "upload_id is required".to_string(),
    ))?;
    let response = state.s3.complete_upload(&upload_id).await?;
    println!("response: {:?}", response);
    // let location = response.unwrap();
    Ok("location".to_string())
}

pub async fn upload(
    State(state): State<AppState>,
    Query(qp): Query<QueryParams>,
    mut multipart: Multipart,
) -> axum::response::Response {
    let upload_state = match qp.state {
        Some(state) => state,
        None => {
            tracing::error!("state is required");
            return crate::bad_request(APIResponse::new_from_msg("state is required"));
        }
    };

    if upload_state == "init" {
        let upload_id = match handle_init_upload(&state, &mut multipart).await {
            Ok(upload_id) => upload_id,
            Err(e) => {
                tracing::error!("failed to initialize upload: {}", e);
                return crate::server_error(APIResponse::new_from_msg(
                    "failed to initialize upload",
                ));
            }
        };

        return crate::good_response(APIResponse {
            books: vec![],
            status: "upload initialized".to_owned(),
            upload_id: Some(upload_id),
        });
    }

    if upload_state == "continue" {
        let upload_id = match handle_continue_upload(&state, &mut multipart).await {
            Ok(upload_id) => upload_id,
            Err(e) => {
                tracing::error!("failed to continue upload: {}", e);
                return crate::server_error(APIResponse::new_from_msg("failed to continue upload"));
            }
        };

        return crate::good_response(APIResponse {
            books: vec![],
            status: "upload progressed".to_owned(),
            upload_id: Some(upload_id),
        });
    }

    if upload_state == "complete" {
        let object_url = match handle_complete_upload(&state, &mut multipart).await {
            Ok(object_url) => object_url,
            Err(e) => {
                tracing::error!("failed to complete upload: {}", e);
                return crate::server_error(APIResponse::new_from_msg("failed to complete upload"));
            }
        };

        return crate::good_response(APIResponse {
            books: vec![],
            status: "upload completed".to_owned(),
            upload_id: Some(object_url),
        });
    }

    // match upload_state.as_str() {
    //     "init" => {
    //         let init = handle_init(&state, upload_state).await.map_err(|e| {
    //             tracing::error!("failed to initialize upload with s3: {}", e);
    //             return bad_request("failed to init storage");
    //         });
    //         let init = init?;
    //     }
    //     _ => {
    //         tracing::error!("invalid upload state");
    //         return bad_request("invalid upload state");
    //     }
    // }

    // let mut uploaded_files = Vec::new();

    // while let Ok(Some(field)) = multipart.next_field().await {
    //     let name = field.name().unwrap_or("unknown").to_string();
    //     println!("name: {}", name);
    //     let data = match field.bytes().await {
    //         Ok(bytes) => bytes,
    //         Err(e) => {
    //             tracing::error!("Failed to read field bytes: {}", e);
    //             continue;
    //         }
    //     };

    //     tracing::info!("Processing file: {} ({} bytes)", name, data.len());

    //     // For now, just log the file info
    //     uploaded_files.push(format!("{}: {} bytes", name, data.len()));
    // }

    // Json(APIResponse::new(
    //     Some("Files uploaded successfully"),
    //     Some(vec![Book::new(name, data.len())]),
    // ))
    (
        StatusCode::OK,
        Json(APIResponse::new_from_msg("Files uploaded successfully")),
    )
        .into_response()
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

pub async fn serve_index() -> impl IntoResponse {
    match fs::read_to_string("web/index.html") {
        Ok(content) => Html(content).into_response(),
        Err(e) => {
            tracing::error!("Failed to read web/index.html: {}", e);
            (StatusCode::NOT_FOUND, Html("<h1>404 - File not found</h1>")).into_response()
        }
    }
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
