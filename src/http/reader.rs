use std::path::Path;

use axum::Router;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Serialize;

use super::AppState;
use super::envelope::{Data, Path as UrlPath};
use crate::domain::annotations;
use crate::error::Error;
use crate::reader::{self, Book};

pub fn read_routes() -> Router<AppState> {
    Router::new().route("/reader/books", get(books))
}

pub fn file_routes() -> Router<AppState> {
    Router::new().route("/reader/{*path}", get(file))
}

#[derive(Serialize)]
struct CatalogBook {
    #[serde(flatten)]
    book: Book,
    highlights: i64,
}

fn books_dir(state: &AppState) -> Result<&Path, Error> {
    state
        .config
        .reader_books_dir
        .as_deref()
        .ok_or(Error::NotFound("reader"))
}

async fn books(State(state): State<AppState>) -> Result<Data<Vec<CatalogBook>>, Error> {
    let books = reader::catalog(books_dir(&state)?).await?;
    let urls: Vec<String> = books.iter().map(|book| book.url.clone()).collect();
    let counts = annotations::highlight_counts_by_url_ending(&*state.db.read().await?, &urls).await?;
    Ok(Data(
        books
            .into_iter()
            .map(|book| CatalogBook {
                highlights: counts.get(&book.url).copied().unwrap_or(0),
                book,
            })
            .collect(),
    ))
}

async fn file(State(state): State<AppState>, UrlPath(path): UrlPath<String>) -> Result<Response, Error> {
    let file = reader::servable_file(books_dir(&state)?, &path)
        .await
        .ok_or(Error::NotFound("file"))?;
    let bytes = tokio::fs::read(&file)
        .await
        .map_err(|error| Error::Internal(error.into()))?;
    let content_type = mime_guess::from_path(&file).first_or_octet_stream();
    Ok((
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        bytes,
    )
        .into_response())
}
