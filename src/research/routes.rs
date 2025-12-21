use axum::{
    Router,
    routing::{get, post},
};

use super::handler;
use crate::handler::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/config", get(handler::get_config))
        .route("/config", post(handler::set_config))
        .route("/sync", post(handler::sync))
}
