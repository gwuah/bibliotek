use axum::{Router, routing::post};

use super::handler;
use crate::handler::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/sync", post(handler::sync_highlights))
}
