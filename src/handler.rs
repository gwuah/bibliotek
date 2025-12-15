use axum::{
    Json,
    extract::{Multipart, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use std::fs;
use std::sync::Arc;

use tracing::info;

use crate::{
    api::{APIResponse, QueryParams},
    pdf_extract::{extract_metadata_from_bytes, infer_category_from_metadata, parse_keywords},
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
    pub file_name: String,
    pub upload_id: String,
    pub part_number: i32,
    pub chunk: axum::body::Bytes,
}

const DEFAULT_PAGE: u32 = 1;
const DEFAULT_LIMIT: u32 = 50;

#[derive(Debug)]
pub struct HandlerParams {
    pub query: Option<String>,
    pub page: u32,
    pub limit: u32,
    pub offset: u32,
    pub state: Option<String>,
}

impl QueryParams {
    pub fn into_handler_params(self) -> HandlerParams {
        let page = self.page.unwrap_or(DEFAULT_PAGE).max(1);
        let limit = self
            .limit
            .unwrap_or(DEFAULT_LIMIT)
            .max(1)
            .min(DEFAULT_LIMIT);

        HandlerParams {
            query: self.q,
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
        metadata: None,
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
        metadata: None,
    })
}

pub async fn get_metadata(State(state): State<AppState>) -> Response {
    let db_call = state.db.get_metadata_aggregates().await;

    if let Err(e) = db_call {
        tracing::info!("failed to get metadata. db_error: {}", e);
        return crate::bad_request(APIResponse::new_from_msg("failed to get metadata"));
    }

    tracing::info!("got metadata aggregates");
    crate::good_response(APIResponse {
        books: vec![],
        status: "ok".to_owned(),
        upload_id: None,
        metadata: Some(db_call.ok().unwrap()),
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
        metadata: None,
    })
}

async fn extract_form(multipart: &mut Multipart) -> Result<Form, HandlerError> {
    let mut form = Form {
        file_name: String::new(),
        upload_id: String::new(),
        part_number: 0,
        chunk: axum::body::Bytes::new(),
    };

    while let Ok(Some(field)) = multipart.next_field().await {
        let form_field_name = field.name().unwrap_or("unknown");
        match form_field_name {
            "file_name" => form.file_name = crate::safe_parse_str("file_name", field).await?,
            "upload_id" => form.upload_id = crate::safe_parse_str("upload_id", field).await?,
            "chunk" => form.chunk = crate::safe_parse_bytes("chunk", field).await?,
            "part_number" => form.part_number = crate::safe_parse_num("part_number", field).await?,
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
    let response = state.s3.start_upload(form.file_name.as_str()).await?;
    Ok(response)
}

async fn handle_continue_upload(
    state: &AppState,
    multipart: &mut Multipart,
) -> Result<String, HandlerError> {
    let form = extract_form(multipart).await?;

    let response = state
        .s3
        .upload(&form.upload_id, form.chunk.to_vec(), form.part_number)
        .await?;
    Ok(response)
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
            metadata: None,
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
            metadata: None,
        });
    }

    if upload_state == "complete" {
        let form = match extract_form(&mut multipart).await {
            Ok(form) => form,
            Err(e) => {
                tracing::error!("failed to extract form: {}", e);
                return crate::bad_request(APIResponse::new_from_msg("failed to extract form"));
            }
        };

        let chunks = match state.s3.get_upload_chunks(&form.upload_id).await {
            Ok(chunks) => chunks,
            Err(e) => {
                tracing::warn!("failed to get chunks for metadata extraction: {}", e);
                vec![]
            }
        };

        let object_url = match state.s3.complete_upload(&form.upload_id).await {
            Ok(object_url) => object_url,
            Err(e) => {
                tracing::error!("failed to complete upload: {}", e);
                return crate::server_error(APIResponse::new_from_msg("failed to complete upload"));
            }
        };

        let mut created_book = None;
        if !chunks.is_empty() {
            match extract_metadata_from_bytes(&chunks).await {
                Ok(pdf_metadata) => {
                    tracing::info!(
                        "Extracted PDF metadata: title={:?}, author={:?}",
                        pdf_metadata.title,
                        pdf_metadata.author
                    );

                    let author_names: Vec<String> = if let Some(author) = &pdf_metadata.author {
                        author
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        vec![]
                    };

                    let tag_names = if let Some(keywords) = &pdf_metadata.keywords {
                        parse_keywords(keywords)
                    } else {
                        vec![]
                    };

                    let mut category_names = vec![];
                    if let Some(category) = infer_category_from_metadata(
                        pdf_metadata.subject.as_deref(),
                        pdf_metadata.keywords.as_deref(),
                    ) {
                        category_names.push(category);
                    }

                    let title = pdf_metadata.title.unwrap_or_else(|| {
                        let name = &form.file_name;
                        let without_ext = if let Some(dot_pos) = name.rfind('.') {
                            &name[..dot_pos]
                        } else {
                            name
                        };
                        without_ext
                            .replace('_', " ")
                            .replace('-', " ")
                            .trim()
                            .to_string()
                    });

                    match state
                        .db
                        .create_book(
                            &title,
                            &object_url,
                            None,
                            pdf_metadata.subject.as_deref(),
                            None,
                            None,
                            &author_names,
                            &tag_names,
                            &category_names,
                        )
                        .await
                    {
                        Ok(book_id) => {
                            tracing::info!("Created book with ID: {}", book_id);
                            match state.db.get_book_by_id(book_id).await {
                                Ok(Some(book)) => {
                                    created_book = Some(book);
                                }
                                Ok(None) => {
                                    tracing::warn!("Book {} was created but not found", book_id);
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to fetch created book: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to create book record: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to extract PDF metadata: {}", e);
                }
            }
        }

        let mut response = APIResponse {
            books: vec![],
            status: "upload completed".to_owned(),
            upload_id: Some(object_url),
            metadata: None,
        };

        if let Some(book) = created_book {
            response.books.push(book);
            response.status = "upload completed and book created".to_owned();
        }

        return crate::good_response(response);
    }

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
    #[test]
    fn test_params_deserialize() {}
}
