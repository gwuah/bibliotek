//! HTTP Handlers for the Commonplace API

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use super::{
    Commonplace, CreateAnnotation, CreateComment, CreateNote, CreateResource, CreateWord,
    UpdateAnnotation, UpdateComment, UpdateNote, UpdateResource, UpdateWord,
};
use crate::handler::AppState;

// ============================================================================
// Query Parameters
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
}

// ============================================================================
// Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct CommonplaceApiResponse<T> {
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

fn success<T: Serialize>(data: T) -> Response {
    (StatusCode::OK, Json(CommonplaceApiResponse { data })).into_response()
}

fn created<T: Serialize>(data: T) -> Response {
    (StatusCode::CREATED, Json(CommonplaceApiResponse { data })).into_response()
}

fn not_found(msg: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

fn bad_request(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

fn internal_error(msg: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

// ============================================================================
// Resource Handlers
// ============================================================================

pub async fn create_resource(
    State(state): State<AppState>,
    Json(payload): Json<CreateResource>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.create_resource(payload).await {
        Ok(resource) => created(resource),
        Err(e) => {
            tracing::error!("Failed to create resource: {}", e);
            internal_error("Failed to create resource")
        }
    }
}

pub async fn get_resource(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.get_resource(id).await {
        Ok(Some(resource)) => success(resource),
        Ok(None) => not_found("Resource not found"),
        Err(e) => {
            tracing::error!("Failed to get resource: {}", e);
            internal_error("Failed to get resource")
        }
    }
}

pub async fn get_resource_full(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.get_resource_full(id).await {
        Ok(Some(resource)) => success(resource),
        Ok(None) => not_found("Resource not found"),
        Err(e) => {
            tracing::error!("Failed to get resource: {}", e);
            internal_error("Failed to get resource")
        }
    }
}

pub async fn list_resources(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    match lib.list_resources(limit, offset).await {
        Ok(resources) => success(resources),
        Err(e) => {
            tracing::error!("Failed to list resources: {}", e);
            internal_error("Failed to list resources")
        }
    }
}

pub async fn update_resource(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateResource>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.update_resource(id, payload).await {
        Ok(Some(resource)) => success(resource),
        Ok(None) => not_found("Resource not found"),
        Err(e) => {
            tracing::error!("Failed to update resource: {}", e);
            internal_error("Failed to update resource")
        }
    }
}

pub async fn delete_resource(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.delete_resource(id).await {
        Ok(true) => (StatusCode::NO_CONTENT, ()).into_response(),
        Ok(false) => not_found("Resource not found"),
        Err(e) => {
            tracing::error!("Failed to delete resource: {}", e);
            internal_error("Failed to delete resource")
        }
    }
}

// ============================================================================
// Annotation Handlers
// ============================================================================

pub async fn create_annotation(
    State(state): State<AppState>,
    Json(payload): Json<CreateAnnotation>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.create_annotation(payload).await {
        Ok(annotation) => created(annotation),
        Err(e) => {
            tracing::error!("Failed to create annotation: {}", e);
            internal_error("Failed to create annotation")
        }
    }
}

pub async fn get_annotation(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.get_annotation(id).await {
        Ok(Some(annotation)) => success(annotation),
        Ok(None) => not_found("Annotation not found"),
        Err(e) => {
            tracing::error!("Failed to get annotation: {}", e);
            internal_error("Failed to get annotation")
        }
    }
}

pub async fn list_annotations_by_resource(
    State(state): State<AppState>,
    Path(resource_id): Path<i32>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.list_annotations_by_resource(resource_id).await {
        Ok(annotations) => success(annotations),
        Err(e) => {
            tracing::error!("Failed to list annotations: {}", e);
            internal_error("Failed to list annotations")
        }
    }
}

pub async fn update_annotation(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateAnnotation>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.update_annotation(id, payload).await {
        Ok(Some(annotation)) => success(annotation),
        Ok(None) => not_found("Annotation not found"),
        Err(e) => {
            tracing::error!("Failed to update annotation: {}", e);
            internal_error("Failed to update annotation")
        }
    }
}

pub async fn delete_annotation(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.delete_annotation(id).await {
        Ok(true) => (StatusCode::NO_CONTENT, ()).into_response(),
        Ok(false) => not_found("Annotation not found"),
        Err(e) => {
            tracing::error!("Failed to delete annotation: {}", e);
            internal_error("Failed to delete annotation")
        }
    }
}

// ============================================================================
// Comment Handlers
// ============================================================================

pub async fn create_comment(
    State(state): State<AppState>,
    Json(payload): Json<CreateComment>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.create_comment(payload).await {
        Ok(comment) => created(comment),
        Err(e) => {
            tracing::error!("Failed to create comment: {}", e);
            internal_error("Failed to create comment")
        }
    }
}

pub async fn get_comment(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.get_comment(id).await {
        Ok(Some(comment)) => success(comment),
        Ok(None) => not_found("Comment not found"),
        Err(e) => {
            tracing::error!("Failed to get comment: {}", e);
            internal_error("Failed to get comment")
        }
    }
}

pub async fn list_comments_by_annotation(
    State(state): State<AppState>,
    Path(annotation_id): Path<i32>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.list_comments_by_annotation(annotation_id).await {
        Ok(comments) => success(comments),
        Err(e) => {
            tracing::error!("Failed to list comments: {}", e);
            internal_error("Failed to list comments")
        }
    }
}

pub async fn update_comment(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateComment>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.update_comment(id, payload).await {
        Ok(Some(comment)) => success(comment),
        Ok(None) => not_found("Comment not found"),
        Err(e) => {
            tracing::error!("Failed to update comment: {}", e);
            internal_error("Failed to update comment")
        }
    }
}

pub async fn delete_comment(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.delete_comment(id).await {
        Ok(true) => (StatusCode::NO_CONTENT, ()).into_response(),
        Ok(false) => not_found("Comment not found"),
        Err(e) => {
            tracing::error!("Failed to delete comment: {}", e);
            internal_error("Failed to delete comment")
        }
    }
}

// ============================================================================
// Note Handlers
// ============================================================================

pub async fn create_note(
    State(state): State<AppState>,
    Json(payload): Json<CreateNote>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.create_note(payload).await {
        Ok(note) => created(note),
        Err(e) => {
            tracing::error!("Failed to create note: {}", e);
            internal_error("Failed to create note")
        }
    }
}

pub async fn get_note(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.get_note(id).await {
        Ok(Some(note)) => success(note),
        Ok(None) => not_found("Note not found"),
        Err(e) => {
            tracing::error!("Failed to get note: {}", e);
            internal_error("Failed to get note")
        }
    }
}

pub async fn list_notes_by_resource(
    State(state): State<AppState>,
    Path(resource_id): Path<i32>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.list_notes_by_resource(resource_id).await {
        Ok(notes) => success(notes),
        Err(e) => {
            tracing::error!("Failed to list notes: {}", e);
            internal_error("Failed to list notes")
        }
    }
}

pub async fn update_note(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateNote>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.update_note(id, payload).await {
        Ok(Some(note)) => success(note),
        Ok(None) => not_found("Note not found"),
        Err(e) => {
            tracing::error!("Failed to update note: {}", e);
            internal_error("Failed to update note")
        }
    }
}

pub async fn delete_note(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.delete_note(id).await {
        Ok(true) => (StatusCode::NO_CONTENT, ()).into_response(),
        Ok(false) => not_found("Note not found"),
        Err(e) => {
            tracing::error!("Failed to delete note: {}", e);
            internal_error("Failed to delete note")
        }
    }
}

// ============================================================================
// Word Handlers
// ============================================================================

pub async fn create_word(
    State(state): State<AppState>,
    Json(payload): Json<CreateWord>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.create_word(payload).await {
        Ok(word) => created(word),
        Err(e) => {
            tracing::error!("Failed to create word: {}", e);
            internal_error("Failed to create word")
        }
    }
}

pub async fn get_word(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.get_word(id).await {
        Ok(Some(word)) => success(word),
        Ok(None) => not_found("Word not found"),
        Err(e) => {
            tracing::error!("Failed to get word: {}", e);
            internal_error("Failed to get word")
        }
    }
}

pub async fn list_words_by_resource(
    State(state): State<AppState>,
    Path(resource_id): Path<i32>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.list_words_by_resource(resource_id).await {
        Ok(words) => success(words),
        Err(e) => {
            tracing::error!("Failed to list words: {}", e);
            internal_error("Failed to list words")
        }
    }
}

pub async fn search_words(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    let query = match params.q {
        Some(q) if !q.is_empty() => q,
        _ => return bad_request("Query parameter 'q' is required"),
    };

    match lib.search_words(&query).await {
        Ok(words) => success(words),
        Err(e) => {
            tracing::error!("Failed to search words: {}", e);
            internal_error("Failed to search words")
        }
    }
}

pub async fn update_word(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateWord>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.update_word(id, payload).await {
        Ok(Some(word)) => success(word),
        Ok(None) => not_found("Word not found"),
        Err(e) => {
            tracing::error!("Failed to update word: {}", e);
            internal_error("Failed to update word")
        }
    }
}

pub async fn delete_word(State(state): State<AppState>, Path(id): Path<i32>) -> Response {
    let lib = Commonplace::new(state.db.connection());

    match lib.delete_word(id).await {
        Ok(true) => (StatusCode::NO_CONTENT, ()).into_response(),
        Ok(false) => not_found("Word not found"),
        Err(e) => {
            tracing::error!("Failed to delete word: {}", e);
            internal_error("Failed to delete word")
        }
    }
}
