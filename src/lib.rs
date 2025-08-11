use axum::http::StatusCode;

pub mod api;
pub mod config;
pub mod db;
pub mod handler;

pub fn internal_error<E: std::error::Error>(err: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
