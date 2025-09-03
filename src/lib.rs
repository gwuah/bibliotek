use crate::api::APIResponse;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::error::Error;

pub mod api;
pub mod config;
pub mod db;
pub mod error;
pub mod handler;
pub mod model;
pub mod s3;

pub fn internal_error<E: std::error::Error>(err: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

pub fn server_error(body: APIResponse) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
}

pub fn bad_request(body: APIResponse) -> Response {
    (StatusCode::BAD_REQUEST, Json(body)).into_response()
}

fn good_response(body: APIResponse) -> Response {
    (StatusCode::OK, Json(body)).into_response()
}

pub fn unpack_error(err: &(dyn Error)) -> String {
    let mut parts = Vec::new();
    parts.push(err.to_string());
    let mut current = err.source();
    while let Some(source) = current {
        parts.push(source.to_string());
        current = source.source();
    }
    parts.join(": ")
}

pub fn get_s3_url(service: &str, bucket: &str, key: &str) -> String {
    match service {
        "t3" => format!("https://{}.t3.storage.dev/{}", bucket, key),
        "s3" => format!("https://{}.s3.amazonaws.com/{}", bucket, key),
        _ => format!("https://{}.storage.dev/{}", service, key),
    }
}
