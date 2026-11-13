use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::routing::{post, put};
use serde::Deserialize;

use super::AppState;
use super::envelope::{Body, Data};
use crate::error::{Error, Result};
use crate::import::light::{self, LightPayload};
use crate::import::research;
use crate::import::snapshot::{self, EmptySnapshot, ImportStats, Snapshot};

const LIGHT_BODY_LIMIT: usize = 64 * 1024 * 1024;

pub fn write_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/imports/light/{installation}",
            put(import_light).layer(DefaultBodyLimit::max(LIGHT_BODY_LIMIT)),
        )
        .route("/imports/research", post(import_research))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportOptions {
    #[serde(default)]
    confirm_empty: bool,
}

impl ImportOptions {
    fn from_query(query: std::result::Result<Query<ImportOptions>, QueryRejection>) -> Result<ImportOptions> {
        query
            .map(|Query(options)| options)
            .map_err(|rejection| Error::invalid("invalid_query", rejection.body_text()))
    }

    fn empty_snapshot(&self) -> EmptySnapshot {
        if self.confirm_empty {
            EmptySnapshot::Accept
        } else {
            EmptySnapshot::Refuse
        }
    }
}

async fn import_light(
    State(state): State<AppState>,
    Path(installation): Path<String>,
    query: std::result::Result<Query<ImportOptions>, QueryRejection>,
    Body(payload): Body<LightPayload>,
) -> Result<Data<ImportStats>> {
    let options = ImportOptions::from_query(query)?;
    let snapshot = light::snapshot(&installation, payload)?;
    apply(&state, &snapshot, options.empty_snapshot()).await
}

async fn import_research(
    State(state): State<AppState>,
    query: std::result::Result<Query<ImportOptions>, QueryRejection>,
) -> Result<Data<ImportStats>> {
    let options = ImportOptions::from_query(query)?;
    let path = state.config.research_database.as_deref().ok_or_else(|| {
        Error::conflict(
            "research_not_configured",
            "no Research database is configured; set research_database in the config file",
        )
    })?;
    let snapshot = research::read(path).await?;
    apply(&state, &snapshot, options.empty_snapshot()).await
}

async fn apply(state: &AppState, snapshot: &Snapshot, empty: EmptySnapshot) -> Result<Data<ImportStats>> {
    let tx = state.db.begin().await?;
    let stats = snapshot::apply(&tx, snapshot, empty).await?;
    tx.commit().await?;
    Ok(Data(stats))
}
