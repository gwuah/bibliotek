mod auth;
pub mod backups;
pub mod documents;
pub mod envelope;
mod files;
pub mod imports;
pub mod reader;
pub mod uploads;

use std::sync::Arc;

use axum::http::{HeaderValue, Method, header};
use axum::routing::get;
use axum::{Router, middleware};
use serde::Serialize;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::blob::BlobStore;
use crate::config::Config;
use crate::db::{Db, SUPPORTED_SCHEMA_VERSION};
use crate::error::Error;
use crate::{assets, build};
use envelope::Data;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Arc<Db>,
    pub blobs: Arc<dyn BlobStore>,
}

pub fn router(state: AppState) -> Router {
    let reads = Router::new()
        .route("/health", get(health))
        .merge(documents::read_routes())
        .merge(uploads::read_routes())
        .merge(reader::read_routes());

    let writes = Router::new()
        .merge(documents::write_routes())
        .merge(uploads::write_routes())
        .merge(imports::write_routes())
        .merge(backups::write_routes())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_admin));

    let api = reads
        .merge(writes)
        .fallback(async || Error::NotFound("route"))
        .layer(cors(&state.config.extension_origins));

    Router::new()
        .nest("/api", api)
        .merge(reader::file_routes())
        .fallback(assets::serve)
        .with_state(state)
}

fn cors(extension_origins: &[String]) -> CorsLayer {
    let origins: Vec<HeaderValue> = extension_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}

#[derive(Serialize)]
struct Health {
    version: &'static str,
    revision: &'static str,
    schema_version: i64,
    environment: String,
}

async fn health(axum::extract::State(state): axum::extract::State<AppState>) -> Data<Health> {
    Data(Health {
        version: build::VERSION,
        revision: build::GIT_REVISION,
        schema_version: SUPPORTED_SCHEMA_VERSION,
        environment: state.config.environment.to_string(),
    })
}
