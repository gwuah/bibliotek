use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;

use super::AppState;
use super::envelope::{Data, Path};
use super::files;
use crate::backup::{self, Backup, BackupName, Trigger};
use crate::blob::BlobAccess;
use crate::error::{Error, Result};

const FILE_LINK_TTL: Duration = Duration::from_secs(15 * 60);

/// Admin-only, reads included: a backup holds every personal annotation in the library.
pub fn write_routes() -> Router<AppState> {
    Router::new()
        .route("/backups", get(list).post(create))
        .route("/backups/{name}/file", get(file))
}

async fn list(State(state): State<AppState>) -> Result<Data<Vec<Backup>>> {
    Ok(Data(backup::list(state.blobs.as_ref()).await?))
}

async fn create(State(state): State<AppState>) -> Result<Data<Backup>> {
    let name = BackupName::now(Trigger::Manual);
    Ok(Data(
        backup::create(&state.db, state.blobs.as_ref(), &state.config.temp_dir(), name).await?,
    ))
}

async fn file(State(state): State<AppState>, Path(name): Path<String>) -> Result<Response> {
    let name = BackupName::parse(&name)?;
    let key = name.object_key(state.blobs.namespace())?;
    if state.blobs.head(&key).await?.is_none() {
        return Err(Error::NotFound("backup"));
    }
    match state.blobs.access(&key, FILE_LINK_TTL).await? {
        BlobAccess::Redirect(url) => Ok(Redirect::temporary(&url).into_response()),
        BlobAccess::LocalFile(path) => {
            let filename = format!("liotek-{name}.sqlite");
            files::stream(&path, backup::CONTENT_TYPE, files::attachment(&filename)).await
        }
    }
}
