use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::commonplace::{
    Commonplace, CreateAnnotation, CreateResource, ResourceType,
    compute_annotation_hash, UpdateAnnotation,
};
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
    pub source: String,
    #[serde(default)]
    pub scope: Option<String>,
    pub highlights: HashMap<String, Vec<LightHighlight>>,
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub resources_created: i32,
    pub annotations_created: i32,
    pub annotations_updated: i32,
    pub annotations_deleted: i32,
    pub annotations_unchanged: i32,
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
    let mut annotations_updated = 0;
    let mut annotations_deleted = 0;
    let mut annotations_unchanged = 0;

    let mut seen_external_ids = HashSet::new();

    // Phase 1: Upsert all highlights
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
            let external_id = format!("{}:{}", payload.source, highlight.group_id);
            let content_hash = compute_annotation_hash(&highlight.repr, Some("yellow"));
            seen_external_ids.insert(external_id.clone());

            match lib.find_annotation_by_external_id(&external_id).await {
                Ok(Some(existing)) => {
                    if existing.content_hash.as_deref() != Some(&content_hash) {
                        // Content changed, update it
                        let boundary = serde_json::json!({
                            "groupID": highlight.group_id,
                            "date": highlight.date,
                            "chunks": highlight.chunks,
                            "url": highlight.url,
                        });

                        match lib
                            .update_annotation(
                                existing.id,
                                UpdateAnnotation {
                                    text: Some(highlight.repr.clone()),
                                    color: Some("yellow".to_string()),
                                    boundary: Some(boundary),
                                    content_hash: Some(content_hash),
                                },
                            )
                            .await
                        {
                            Ok(Some(_)) => {
                                annotations_updated += 1;
                            }
                            Ok(None) => {
                                tracing::warn!("Annotation {} not found for update", existing.id);
                            }
                            Err(e) => {
                                tracing::error!("Failed to update annotation {}: {}", external_id, e);
                            }
                        }
                    } else {
                        annotations_unchanged += 1;
                    }
                }
                Ok(None) => {
                    // New annotation, create it
                    let boundary = serde_json::json!({
                        "groupID": highlight.group_id,
                        "date": highlight.date,
                        "chunks": highlight.chunks,
                        "url": highlight.url,
                    });

                    match lib
                        .create_annotation(CreateAnnotation {
                            resource_id,
                            text: highlight.repr.clone(),
                            color: Some("yellow".to_string()),
                            boundary: Some(boundary),
                            external_id: Some(external_id),
                            content_hash: Some(content_hash),
                        })
                        .await
                    {
                        Ok(_) => {
                            annotations_created += 1;
                        }
                        Err(e) => {
                            tracing::error!("Failed to create annotation: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to check annotation {}: {}", external_id, e);
                }
            }
        }
    }

    // Phase 2: Soft delete orphans
    let orphan_query_resource_id = if let Some(scope_url) = &payload.scope {
        // Partial sync: only check annotations for the scoped resource
        match lib.find_resource_by_title(scope_url).await {
            Ok(Some(resource)) => Some(resource.id),
            Ok(None) => {
                tracing::warn!("Scope resource {} not found, skipping orphan detection", scope_url);
                None
            }
            Err(e) => {
                tracing::error!("Failed to find scope resource {}: {}", scope_url, e);
                None
            }
        }
    } else {
        None
    };

    match lib
        .find_annotations_by_source_prefix(&payload.source, orphan_query_resource_id)
        .await
    {
        Ok(orphans) => {
            for orphan in orphans {
                if !seen_external_ids.contains(
                    orphan.external_id.as_ref().unwrap_or(&String::new()),
                ) {
                    match lib.soft_delete_annotation(orphan.id).await {
                        Ok(true) => {
                            annotations_deleted += 1;
                        }
                        Ok(false) => {
                            tracing::warn!("Failed to soft delete annotation {}", orphan.id);
                        }
                        Err(e) => {
                            tracing::error!("Error soft deleting annotation {}: {}", orphan.id, e);
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to find orphan annotations: {}", e);
        }
    }

    success(SyncResponse {
        resources_created,
        annotations_created,
        annotations_updated,
        annotations_deleted,
        annotations_unchanged,
    })
}

async fn find_or_create_resource(
    lib: &Commonplace<'_>,
    url: &str,
) -> anyhow::Result<(i32, bool)> {
    if let Some(resource) = lib.find_resource_by_title(url).await? {
        return Ok((resource.id, false));
    }

    use crate::commonplace::compute_resource_hash;
    let content_hash = compute_resource_hash(url);

    let resource = lib
        .create_resource(CreateResource {
            title: url.to_string(),
            resource_type: ResourceType::Website,
            external_id: None,
            content_hash: Some(content_hash),
        })
        .await?;

    Ok((resource.id, true))
}
