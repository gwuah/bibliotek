use axum::{Json, response::IntoResponse};
use serde::{
    // Deserialize,
    Serialize,
};
use tracing::info;

#[derive(Debug, Serialize, Default)]
struct Response {
    status: String,
}

pub async fn healthcheck() -> impl IntoResponse {
    info!("got healthcheck request");
    Json(Response {
        status: "ok".to_string(),
    })
}
