use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::blob::BlobError;
use crate::error::Error;

pub struct Data<T>(pub T);

impl<T: Serialize> IntoResponse for Data<T> {
    fn into_response(self) -> Response {
        Json(json!({ "data": self.0 })).into_response()
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Error::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            Error::Invalid { code, .. } => (StatusCode::BAD_REQUEST, *code),
            Error::Conflict { code, .. } => (StatusCode::CONFLICT, *code),
            Error::Unauthorized => (StatusCode::UNAUTHORIZED, "admin_token_required"),
            Error::Blob(BlobError::ObjectNotFound) => (StatusCode::NOT_FOUND, "object_not_found"),
            Error::Blob(_) => (StatusCode::BAD_GATEWAY, "object_store_failed"),
            Error::Database(_) | Error::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            // The cause may name paths or SQL; it belongs in the log, not the response.
            tracing::error!(error = ?self, "request failed");
            "something went wrong on the server".to_string()
        } else {
            if status.is_server_error() {
                tracing::warn!(error = ?self, "request failed");
            }
            self.to_string()
        };

        (
            status,
            Json(json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}

pub struct Body<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Body<T> {
    type Rejection = Error;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(Body(value)),
            Err(rejection) => Err(Error::invalid("invalid_request_body", describe(rejection))),
        }
    }
}

fn describe(rejection: JsonRejection) -> String {
    rejection.body_text()
}

pub struct Query<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for Query<T> {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(value)) => Ok(Query(value)),
            Err(rejection) => Err(Error::invalid("invalid_query", rejection.body_text())),
        }
    }
}

pub struct Path<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for Path<T> {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(value)) => Ok(Path(value)),
            Err(rejection) => Err(Error::invalid("invalid_path", rejection.body_text())),
        }
    }
}
