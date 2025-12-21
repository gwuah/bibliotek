use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::commonplace::{Commonplace, CreateAnnotation, CreateResource, ResourceType};
use crate::handler::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct LightHighlight {
    pub chunks: Vec<String>,
    pub date: String,
    #[serde(rename = "groupID")]
    pub group_id: i64,
    pub repr: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    pub highlights: HashMap<String, Vec<LightHighlight>>,
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub resources_created: i32,
    pub annotations_created: i32,
    pub annotations_skipped: i32,
}

#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    data: T,
}

fn success<T: Serialize>(data: T) -> Response {
    (StatusCode::OK, Json(ApiResponse { data })).into_response()
}

pub async fn sync_highlights(
    State(state): State<AppState>,
    Json(payload): Json<SyncRequest>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    let mut resources_created = 0;
    let mut annotations_created = 0;
    let mut annotations_skipped = 0;

    for (url, highlights) in payload.highlights {
        let resource_id = match find_or_create_resource(&lib, &url).await {
            Ok((id, created)) => {
                if created {
                    resources_created += 1;
                }
                id
            }
            Err(e) => {
                tracing::error!("Failed to find/create resource for {}: {}", url, e);
                continue;
            }
        };

        for highlight in highlights {
            match create_annotation_if_not_exists(&lib, resource_id, &highlight).await {
                Ok(created) => {
                    if created {
                        annotations_created += 1;
                    } else {
                        annotations_skipped += 1;
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to create annotation for groupID {}: {}",
                        highlight.group_id,
                        e
                    );
                }
            }
        }
    }

    success(SyncResponse {
        resources_created,
        annotations_created,
        annotations_skipped,
    })
}

async fn find_or_create_resource(
    lib: &Commonplace<'_>,
    url: &str,
) -> anyhow::Result<(i32, bool)> {
    if let Some(resource) = lib.find_resource_by_title(url).await? {
        return Ok((resource.id, false));
    }

    let resource = lib
        .create_resource(CreateResource {
            title: url.to_string(),
            resource_type: ResourceType::Website,
            external_id: None,
        })
        .await?;

    Ok((resource.id, true))
}

async fn create_annotation_if_not_exists(
    lib: &Commonplace<'_>,
    resource_id: i32,
    highlight: &LightHighlight,
) -> anyhow::Result<bool> {
    if annotation_exists_by_group_id(lib, resource_id, highlight.group_id).await? {
        return Ok(false);
    }

    let boundary = serde_json::json!({
        "groupID": highlight.group_id,
        "date": highlight.date,
        "chunks": highlight.chunks,
        "url": highlight.url,
    });

    lib.create_annotation(CreateAnnotation {
        resource_id,
        text: highlight.repr.clone(),
        color: Some("yellow".to_string()),
        boundary: Some(boundary),
        external_id: Some(highlight.group_id.to_string()),
    })
    .await?;

    Ok(true)
}

async fn annotation_exists_by_group_id(
    lib: &Commonplace<'_>,
    resource_id: i32,
    group_id: i64,
) -> anyhow::Result<bool> {
    let annotations = lib.list_annotations_by_resource(resource_id).await?;

    for annotation in annotations {
        if let Some(boundary) = &annotation.boundary {
            if let Some(stored_group_id) = boundary.get("groupID") {
                if stored_group_id.as_i64() == Some(group_id) {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}
