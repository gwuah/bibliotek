//! Routes for the Light Extension Sync API

use axum::{Router, routing::post};

use super::handler;
use crate::handler::AppState;

/// Creates a Router with Light extension sync routes.
///
/// # Example
///
/// ```rust,ignore
/// use bibliotek::light;
/// use bibliotek::handler::AppState;
///
/// let app = Router::new()
///     .nest("/light", light::routes())
///     .with_state(app_state);
/// ```
pub fn routes() -> Router<AppState> {
    Router::new().route("/sync", post(handler::sync_highlights))
}

