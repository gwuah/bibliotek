use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use serde::Deserialize;

use super::AppState;
use super::envelope::{Body, Data, Path, Query};
use super::files;
use crate::blob::BlobAccess;
use crate::domain::documents::{
    self, DocumentDetail, DocumentFilter, DocumentKind, DocumentPage, DocumentPatch, Facets, Lookup,
};
use crate::domain::matches::{self, Dismissal, MatchCandidate, Merged, PairId};
use crate::error::Error;

const FILE_LINK_TTL: Duration = Duration::from_secs(15 * 60);

pub fn read_routes() -> Router<AppState> {
    Router::new()
        .route("/documents", get(list))
        .route("/documents/{id}", get(detail))
        .route("/documents/{id}/file", get(file))
        .route("/facets", get(facets))
        .route("/document-matches", get(candidates))
}

pub fn write_routes() -> Router<AppState> {
    Router::new()
        .route("/documents/{id}", axum::routing::patch(update))
        .route("/document-matches/{id}/merge", post(merge))
        .route("/document-matches/{id}/dismiss", post(dismiss))
}

#[derive(Deserialize)]
struct ListParams {
    q: Option<String>,
    kind: Option<DocumentKind>,
    author: Option<String>,
    subject: Option<String>,
    tag: Option<String>,
    page: Option<u32>,
}

async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Data<DocumentPage>, Error> {
    let filter = DocumentFilter {
        query: params.q,
        kind: params.kind,
        author: params.author,
        subject: params.subject,
        tag: params.tag,
    };
    let conn = state.db.read().await?;
    Ok(Data(
        documents::list(&conn, &filter, params.page.unwrap_or(1)).await?,
    ))
}

async fn detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Response, Error> {
    let conn = state.db.read().await?;
    Ok(match documents::detail(&conn, id).await? {
        Lookup::Found(document) => Data(document).into_response(),
        Lookup::Merged { survivor_id } => to_survivor(survivor_id, ""),
    })
}

async fn file(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Response, Error> {
    let stored = documents::stored_file(&*state.db.read().await?, id).await?;
    let stored = match stored {
        Lookup::Found(stored) => stored,
        Lookup::Merged { survivor_id } => return Ok(to_survivor(survivor_id, "/file")),
    };
    match state.blobs.access(&stored.key, FILE_LINK_TTL).await? {
        BlobAccess::Redirect(url) => Ok(Redirect::temporary(&url).into_response()),
        BlobAccess::LocalFile(path) => {
            let filename = stored.original_filename.as_deref().unwrap_or("document.pdf");
            files::stream(&path, "application/pdf", files::inline(filename)).await
        }
    }
}

async fn facets(State(state): State<AppState>) -> Result<Data<Facets>, Error> {
    let conn = state.db.read().await?;
    Ok(Data(documents::facets(&conn).await?))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Body(patch): Body<DocumentPatch>,
) -> Result<Data<DocumentDetail>, Error> {
    let tx = state.db.begin().await?;
    documents::update(&tx, id, &patch).await?;
    let document = documents::detail(&tx, id).await?.live()?;
    tx.commit().await?;
    Ok(Data(document))
}

async fn candidates(State(state): State<AppState>) -> Result<Data<Vec<MatchCandidate>>, Error> {
    let conn = state.db.read().await?;
    Ok(Data(matches::candidates(&conn).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MergeRequest {
    survivor_id: i64,
}

async fn merge(
    State(state): State<AppState>,
    Path(pair): Path<String>,
    Body(request): Body<MergeRequest>,
) -> Result<Data<Merged>, Error> {
    let pair = PairId::parse(&pair)?;
    let tx = state.db.begin().await?;
    let merged = matches::merge(&tx, pair, request.survivor_id).await?;
    tx.commit().await?;
    Ok(Data(merged))
}

async fn dismiss(State(state): State<AppState>, Path(pair): Path<String>) -> Result<Data<Dismissal>, Error> {
    let pair = PairId::parse(&pair)?;
    let tx = state.db.begin().await?;
    let dismissal = matches::dismiss(&tx, pair).await?;
    tx.commit().await?;
    Ok(Data(dismissal))
}

fn to_survivor(survivor_id: i64, suffix: &str) -> Response {
    Redirect::permanent(&format!("/api/documents/{survivor_id}{suffix}")).into_response()
}
