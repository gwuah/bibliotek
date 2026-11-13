use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/dist"]
struct Frontend;

const INDEX: &str = "index.html";

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let (name, file) = match Frontend::get(path) {
        Some(file) => (path, file),
        None => match Frontend::get(INDEX) {
            Some(index) => (INDEX, index),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    "the frontend was not built into this binary",
                )
                    .into_response();
            }
        },
    };

    let content_type = mime_guess::from_path(name).first_or_octet_stream();
    ([(header::CONTENT_TYPE, content_type.as_ref())], file.data).into_response()
}
