use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use std::fs;
use std::sync::Arc;

use tracing::info;

use crate::{
    api::{APIResponse, CreateEntityRequest, EntityResponse, PendingUploadsResponse, QueryParams, UpdateBookRequest, UploadInitResponse},
    pdf_extract::{infer_category_from_metadata, parse_keywords},
    resumable::ResumableUploadManager,
};
use crate::{db::Database, error::HandlerError};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub resumable: Arc<ResumableUploadManager>,
}

#[derive(Debug)]
pub struct Form {
    pub file_name: String,
    pub file_size: i64,
    pub file_signature: String,
    pub upload_id: String,
    pub key: String,
    pub part_number: i32,
    pub chunk: axum::body::Bytes,
    // Client-extracted PDF metadata
    pub pdf_title: Option<String>,
    pub pdf_author: Option<String>,
    pub pdf_subject: Option<String>,
    pub pdf_keywords: Option<String>,
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
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT).max(1).min(DEFAULT_LIMIT);

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


async fn extract_form(multipart: &mut Multipart) -> Result<Form, HandlerError> {
    let mut form = Form {
        file_name: String::new(),
        file_size: 0,
        file_signature: String::new(),
        upload_id: String::new(),
        key: String::new(),
        part_number: 0,
        chunk: axum::body::Bytes::new(),
        pdf_title: None,
        pdf_author: None,
        pdf_subject: None,
        pdf_keywords: None,
    };

    while let Ok(Some(field)) = multipart.next_field().await {
        let form_field_name = field.name().unwrap_or("unknown");
        match form_field_name {
            "file_name" => form.file_name = crate::safe_parse_str("file_name", field).await?,
            "file_size" => {
                let size_str = crate::safe_parse_str("file_size", field).await?;
                form.file_size = size_str.parse().unwrap_or(0);
            }
            "file_signature" => form.file_signature = crate::safe_parse_str("file_signature", field).await?,
            "upload_id" => form.upload_id = crate::safe_parse_str("upload_id", field).await?,
            "key" => form.key = crate::safe_parse_str("key", field).await?,
            "chunk" => form.chunk = crate::safe_parse_bytes("chunk", field).await?,
            "part_number" => form.part_number = crate::safe_parse_num("part_number", field).await?,
            "pdf_title" => {
                let val = crate::safe_parse_str("pdf_title", field).await?;
                if !val.is_empty() { form.pdf_title = Some(val); }
            }
            "pdf_author" => {
                let val = crate::safe_parse_str("pdf_author", field).await?;
                if !val.is_empty() { form.pdf_author = Some(val); }
            }
            "pdf_subject" => {
                let val = crate::safe_parse_str("pdf_subject", field).await?;
                if !val.is_empty() { form.pdf_subject = Some(val); }
            }
            "pdf_keywords" => {
                let val = crate::safe_parse_str("pdf_keywords", field).await?;
                if !val.is_empty() { form.pdf_keywords = Some(val); }
            }
            _ => {
                tracing::warn!("unknown form field: {}", form_field_name);
                continue;
            }
        }
    }

    Ok(form)
}

async fn handle_init_upload(state: &AppState, multipart: &mut Multipart) -> Result<UploadInitResponse, HandlerError> {
    let form = extract_form(multipart).await?;

    if form.file_signature.is_empty() {
        return Err(HandlerError::ValidationError("file_signature is required".to_string()));
    }
    if form.file_size <= 0 {
        return Err(HandlerError::ValidationError("file_size must be positive".to_string()));
    }
    if form.file_name.is_empty() {
        return Err(HandlerError::ValidationError("file_name is required".to_string()));
    }

    let init_response = state
        .resumable
        .init_or_resume(&form.file_signature, &form.file_name, form.file_size)
        .await?;

    Ok(UploadInitResponse {
        upload_id: init_response.upload_id,
        status: if init_response.is_resume { "resuming" } else { "ok" }.to_string(),
        chunk_size: init_response.chunk_size,
        total_chunks: init_response.total_chunks,
        completed_chunks: init_response.completed_chunks,
        key: Some(init_response.key),
    })
}

async fn handle_continue_upload(state: &AppState, multipart: &mut Multipart) -> Result<String, HandlerError> {
    let form = extract_form(multipart).await?;

    if form.upload_id.is_empty() {
        return Err(HandlerError::ValidationError("upload_id is required".to_string()));
    }
    if form.key.is_empty() {
        return Err(HandlerError::ValidationError("key is required".to_string()));
    }
    if form.part_number <= 0 {
        return Err(HandlerError::ValidationError("part_number must be positive".to_string()));
    }

    let etag = state
        .resumable
        .upload_part(&form.upload_id, &form.key, form.chunk.to_vec(), form.part_number)
        .await?;

    Ok(etag)
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
        let init_response = match handle_init_upload(&state, &mut multipart).await {
            Ok(response) => response,
            Err(e) => {
                tracing::error!("failed to initialize upload: {}", e);
                return crate::server_error(APIResponse::new_from_msg(&format!("failed to initialize upload: {}", e)));
            }
        };

        return (StatusCode::OK, Json(init_response)).into_response();
    }

    if upload_state == "continue" {
        let _etag = match handle_continue_upload(&state, &mut multipart).await {
            Ok(etag) => etag,
            Err(e) => {
                tracing::error!("failed to continue upload: {}", e);
                return crate::server_error(APIResponse::new_from_msg(&format!("failed to continue upload: {}", e)));
            }
        };

        return crate::good_response(APIResponse {
            books: vec![],
            status: "ok".to_owned(),
            upload_id: None,
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

        if form.upload_id.is_empty() || form.key.is_empty() {
            return crate::bad_request(APIResponse::new_from_msg("upload_id and key are required"));
        }

        // Get filename from key
        let file_name = ResumableUploadManager::get_filename_from_key(&form.key)
            .unwrap_or_else(|| "unknown.pdf".to_string());

        // Complete the S3 upload
        let object_url = match state.resumable.complete(&form.upload_id, &form.key).await {
            Ok(url) => url,
            Err(e) => {
                tracing::error!("failed to complete upload: {}", e);
                return crate::server_error(APIResponse::new_from_msg(&format!("failed to complete upload: {}", e)));
            }
        };

        // Use client-provided metadata (extracted via pdf.js in browser)
        let title_from_filename = || {
            let without_ext = if let Some(dot_pos) = file_name.rfind('.') {
                &file_name[..dot_pos]
            } else {
                &file_name
            };
            without_ext.replace('_', " ").replace('-', " ").trim().to_string()
        };

        let title = match &form.pdf_title {
            Some(t) if !t.trim().is_empty() => t.clone(),
            _ => title_from_filename(),
        };

        let author_names: Vec<String> = if let Some(author) = &form.pdf_author {
            author
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![]
        };

        let tag_names = if let Some(keywords) = &form.pdf_keywords {
            parse_keywords(keywords)
        } else {
            vec![]
        };

        let mut category_names = vec![];
        if let Some(category) =
            infer_category_from_metadata(form.pdf_subject.as_deref(), form.pdf_keywords.as_deref())
        {
            category_names.push(category);
        }

        tracing::info!(
            "Using client-provided metadata: title={:?}, author={:?}",
            form.pdf_title,
            form.pdf_author
        );

        let mut created_book = None;
        match state
            .db
            .create_book(
                &title,
                &object_url,
                None,
                form.pdf_subject.as_deref(),
                None,
                None,
                &author_names,
                &tag_names,
                &category_names,
                "complete",
            )
            .await
        {
            Ok(book_id) => {
                tracing::info!("Created book with ID: {}", book_id);
                if let Ok(Some(book)) = state.db.get_book_by_id(book_id).await {
                    created_book = Some(book);
                }
            }
            Err(e) => {
                tracing::error!("Failed to create book record: {}", e);
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

    (StatusCode::OK, Json(APIResponse::new_from_msg("Files uploaded successfully"))).into_response()
}

pub async fn get_pending_uploads(State(state): State<AppState>) -> Response {
    match state.resumable.list_pending().await {
        Ok(uploads) => {
            (StatusCode::OK, Json(PendingUploadsResponse { uploads })).into_response()
        }
        Err(e) => {
            tracing::error!("failed to list pending uploads: {}", e);
            crate::server_error(APIResponse::new_from_msg(&format!("failed to list pending uploads: {}", e)))
        }
    }
}

pub async fn abort_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Response {
    let form = match extract_form(&mut multipart).await {
        Ok(form) => form,
        Err(e) => {
            tracing::error!("failed to extract form: {}", e);
            return crate::bad_request(APIResponse::new_from_msg("failed to extract form"));
        }
    };

    if form.upload_id.is_empty() || form.key.is_empty() {
        return crate::bad_request(APIResponse::new_from_msg("upload_id and key are required"));
    }

    match state.resumable.abort(&form.upload_id, &form.key).await {
        Ok(()) => {
            crate::good_response(APIResponse::new_from_msg("upload aborted"))
        }
        Err(e) => {
            tracing::error!("failed to abort upload: {}", e);
            crate::server_error(APIResponse::new_from_msg(&format!("failed to abort upload: {}", e)))
        }
    }
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

pub async fn update_book(
    State(state): State<AppState>,
    Path(book_id): Path<i32>,
    Json(payload): Json<UpdateBookRequest>,
) -> Response {
    match state
        .db
        .update_book(book_id, &payload.title, &payload.author_ids, &payload.tag_ids, &payload.category_ids)
        .await
    {
        Ok(_) => match state.db.get_book_by_id(book_id).await {
            Ok(Some(book)) => (
                StatusCode::OK,
                Json(APIResponse {
                    books: vec![book],
                    status: "ok".to_owned(),
                    upload_id: None,
                    metadata: None,
                }),
            )
                .into_response(),
            _ => crate::good_response(APIResponse::new_from_msg("book updated")),
        },
        Err(e) => {
            tracing::error!("failed to update book: {}", e);
            crate::bad_request(APIResponse::new_from_msg("failed to update book"))
        }
    }
}

pub async fn create_author(State(state): State<AppState>, Json(payload): Json<CreateEntityRequest>) -> Response {
    match state.db.create_author(&payload.name).await {
        Ok(author) => (StatusCode::CREATED, Json(EntityResponse { entity: author })).into_response(),
        Err(e) => {
            tracing::error!("failed to create author: {}", e);
            crate::bad_request(APIResponse::new_from_msg("failed to create author"))
        }
    }
}

pub async fn create_tag(State(state): State<AppState>, Json(payload): Json<CreateEntityRequest>) -> Response {
    match state.db.create_tag(&payload.name).await {
        Ok(tag) => (StatusCode::CREATED, Json(EntityResponse { entity: tag })).into_response(),
        Err(e) => {
            tracing::error!("failed to create tag: {}", e);
            crate::bad_request(APIResponse::new_from_msg("failed to create tag"))
        }
    }
}

pub async fn create_category(State(state): State<AppState>, Json(payload): Json<CreateEntityRequest>) -> Response {
    match state.db.create_category(&payload.name).await {
        Ok(category) => (StatusCode::CREATED, Json(EntityResponse { entity: category })).into_response(),
        Err(e) => {
            tracing::error!("failed to create category: {}", e);
            crate::bad_request(APIResponse::new_from_msg("failed to create category"))
        }
    }
}

#[derive(serde::Deserialize)]
pub struct DownloadQuery {
    pub key: String,
}

#[derive(serde::Serialize)]
pub struct DownloadResponse {
    pub url: String,
}

pub async fn get_download_url(
    State(state): State<AppState>,
    Query(query): Query<DownloadQuery>,
) -> Response {
    // Generate presigned URL valid for 1 hour
    match state.resumable.get_presigned_url(&query.key, 3600).await {
        Ok(url) => (StatusCode::OK, Json(DownloadResponse { url })).into_response(),
        Err(e) => {
            tracing::error!("failed to generate download url: {}", e);
            crate::server_error(APIResponse::new_from_msg(&format!(
                "failed to generate download url: {}",
                e
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_params_deserialize() {}
}
