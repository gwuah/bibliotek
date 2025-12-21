use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::commonplace::{
    Commonplace, CreateAnnotation, CreateResource, ResourceType, UpdateAnnotation,
    compute_annotation_hash, compute_resource_hash,
};
use crate::handler::AppState;
use crate::sync::{
    SyncResult, Syncable, handle_create_result_unit, handle_update_result_unit,
    is_orphan, is_unchanged, log_find_error,
};

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

#[derive(Debug, Serialize, Default)]
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
    let mut stats = SyncResponse::default();
    let mut seen_external_ids = HashSet::new();

    for (url, highlights) in &payload.highlights {
        let resource_id = match find_or_create_resource(&lib, url, &mut stats).await {
            Some(id) => id,
            None => continue,
        };

        for highlight in highlights {
            sync_highlight(
                &lib,
                &payload.source,
                resource_id,
                highlight,
                &mut stats,
                &mut seen_external_ids,
            )
            .await;
        }
    }

    soft_delete_orphan_annotations(&lib, &payload, &seen_external_ids, &mut stats).await;

    success(stats)
}

async fn find_or_create_resource(
    lib: &Commonplace<'_>,
    url: &str,
    stats: &mut SyncResponse,
) -> Option<i32> {
    match lib.find_resource_by_title(url).await {
        Ok(Some(resource)) => return Some(resource.id),
        Ok(None) => {}
        Err(e) => {
            tracing::error!("Failed to find resource for {}: {}", url, e);
            return None;
        }
    }

    let content_hash = compute_resource_hash(url);
    match lib
        .create_resource(CreateResource {
            title: url.to_string(),
            resource_type: ResourceType::Website,
            external_id: None,
            content_hash: Some(content_hash),
        })
        .await
    {
        Ok(resource) => {
            stats.resources_created += 1;
            Some(resource.id)
        }
        Err(e) => {
            tracing::error!("Failed to create resource for {}: {}", url, e);
            None
        }
    }
}

async fn sync_highlight(
    lib: &Commonplace<'_>,
    source: &str,
    resource_id: i32,
    highlight: &LightHighlight,
    stats: &mut SyncResponse,
    seen: &mut HashSet<String>,
) {
    let external_id = format!("{}:{}", source, highlight.group_id);
    let content_hash = compute_annotation_hash(&highlight.repr, Some("yellow"));
    seen.insert(external_id.clone());

    match upsert_highlight(lib, &external_id, resource_id, highlight, &content_hash).await {
        SyncResult::Created(()) => stats.annotations_created += 1,
        SyncResult::Updated(()) => stats.annotations_updated += 1,
        SyncResult::Unchanged(()) => stats.annotations_unchanged += 1,
        SyncResult::Error => {}
    }
}

async fn upsert_highlight(
    lib: &Commonplace<'_>,
    external_id: &str,
    resource_id: i32,
    highlight: &LightHighlight,
    content_hash: &str,
) -> SyncResult<()> {
    let existing = match lib.find_annotation_by_external_id(external_id).await {
        Ok(a) => a,
        Err(e) => {
            log_find_error("annotation", external_id, e);
            return SyncResult::Error;
        }
    };

    let boundary = serde_json::json!({
        "groupID": highlight.group_id,
        "date": highlight.date,
        "chunks": highlight.chunks,
        "url": highlight.url,
    });

    let Some(ann) = existing else {
        return create_highlight(lib, external_id, resource_id, highlight, content_hash, boundary).await;
    };

    if is_unchanged(&ann, content_hash) {
        return SyncResult::Unchanged(());
    }

    update_highlight(lib, external_id, ann.id, highlight, content_hash, boundary).await
}

async fn create_highlight(
    lib: &Commonplace<'_>,
    external_id: &str,
    resource_id: i32,
    highlight: &LightHighlight,
    content_hash: &str,
    boundary: serde_json::Value,
) -> SyncResult<()> {
    let result = lib
        .create_annotation(CreateAnnotation {
            resource_id,
            text: highlight.repr.clone(),
            color: Some("yellow".to_string()),
            boundary: Some(boundary),
            external_id: Some(external_id.to_string()),
            content_hash: Some(content_hash.to_string()),
        })
        .await;

    handle_create_result_unit(result, "annotation", external_id)
}

async fn update_highlight(
    lib: &Commonplace<'_>,
    external_id: &str,
    id: i32,
    highlight: &LightHighlight,
    content_hash: &str,
    boundary: serde_json::Value,
) -> SyncResult<()> {
    let result = lib
        .update_annotation(
            id,
            UpdateAnnotation {
                text: Some(highlight.repr.clone()),
                color: Some("yellow".to_string()),
                boundary: Some(boundary),
                content_hash: Some(content_hash.to_string()),
            },
        )
        .await;

    handle_update_result_unit(result, id, "annotation", external_id)
}

async fn soft_delete_orphan_annotations(
    lib: &Commonplace<'_>,
    payload: &SyncRequest,
    seen: &HashSet<String>,
    stats: &mut SyncResponse,
) {
    let scope_resource_id = match &payload.scope {
        Some(scope_url) => match lib.find_resource_by_title(scope_url).await {
            Ok(Some(resource)) => Some(resource.id),
            Ok(None) => {
                tracing::warn!(
                    "Scope resource {} not found, skipping orphan detection",
                    scope_url
                );
                return;
            }
            Err(e) => {
                tracing::error!("Failed to find scope resource {}: {}", scope_url, e);
                return;
            }
        },
        None => None,
    };

    let orphans = match lib
        .find_annotations_by_source_prefix(&payload.source, scope_resource_id)
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::error!("Failed to find orphan annotations: {}", e);
            return;
        }
    };

    for orphan in orphans {
        let ext_id = orphan.external_id().map(|s| s.to_string());
        if is_orphan(&ext_id, seen) {
            if lib.soft_delete_annotation(orphan.id()).await.unwrap_or(false) {
                stats.annotations_deleted += 1;
            }
        }
    }
}
