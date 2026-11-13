use axum::Router;
use axum::extract::rejection::BytesRejection;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::routing::{delete, get, post, put};
use bytes::Bytes;

use super::AppState;
use super::envelope::{Body, Data};
use crate::domain::uploads::{self, DEFAULT_PART_SIZE, NewUpload, Upload};
use crate::error::{Error, Result};

pub fn read_routes() -> Router<AppState> {
    Router::new()
        .route("/uploads", get(list))
        .route("/uploads/{id}", get(show))
}

pub fn write_routes() -> Router<AppState> {
    Router::new()
        .route("/uploads", post(create))
        .route("/uploads/{id}", delete(abort))
        .route(
            "/uploads/{id}/parts/{part}",
            put(put_part).layer(DefaultBodyLimit::max(DEFAULT_PART_SIZE as usize)),
        )
        .route("/uploads/{id}/complete", post(complete))
}

async fn list(State(state): State<AppState>) -> Result<Data<Vec<Upload>>> {
    Ok(Data(uploads::list(&state.db).await?))
}

async fn show(State(state): State<AppState>, Path(id): Path<String>) -> Result<Data<Upload>> {
    Ok(Data(uploads::get(&state.db, state.blobs.as_ref(), &id).await?))
}

async fn create(State(state): State<AppState>, Body(new): Body<NewUpload>) -> Result<Data<Upload>> {
    Ok(Data(
        uploads::create(&state.db, state.blobs.as_ref(), &new, DEFAULT_PART_SIZE).await?,
    ))
}

async fn put_part(
    State(state): State<AppState>,
    Path((id, part)): Path<(String, String)>,
    bytes: std::result::Result<Bytes, BytesRejection>,
) -> Result<Data<Upload>> {
    let part_number = part
        .parse()
        .map_err(|_| Error::invalid("invalid_part_number", format!("`{part}` is not a part number")))?;
    let bytes = bytes.map_err(|rejection| Error::invalid("invalid_part_body", rejection.body_text()))?;
    Ok(Data(
        uploads::put_part(&state.db, state.blobs.as_ref(), &id, part_number, bytes).await?,
    ))
}

async fn complete(State(state): State<AppState>, Path(id): Path<String>) -> Result<Data<Upload>> {
    Ok(Data(
        uploads::complete(&state.db, state.blobs.as_ref(), &id).await?,
    ))
}

async fn abort(State(state): State<AppState>, Path(id): Path<String>) -> Result<Data<Upload>> {
    Ok(Data(uploads::abort(&state.db, state.blobs.as_ref(), &id).await?))
}
