//! HTTP Handlers for Light Extension Sync

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

// ============================================================================
// Request/Response Types
// ============================================================================

/// A single highlight from the Light extension
#[derive(Debug, Clone, Deserialize)]
pub struct LightHighlight {
    /// Text chunks that make up the highlight
    pub chunks: Vec<String>,
    /// ISO 8601 timestamp when the highlight was created
    pub date: String,
    /// Unique identifier for this highlight (timestamp-based)
    #[serde(rename = "groupID")]
    pub group_id: i64,
    /// The full highlighted text representation
    pub repr: String,
    /// The URL where the highlight was made
    pub url: String,
}

/// Request body for syncing highlights from Light extension
#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    /// Map of URL -> list of highlights on that page
    pub highlights: HashMap<String, Vec<LightHighlight>>,
}

/// Response containing sync statistics
#[derive(Debug, Serialize)]
pub struct SyncResponse {
    /// Number of new resources (URLs) created
    pub resources_created: i32,
    /// Number of new annotations created
    pub annotations_created: i32,
    /// Number of annotations skipped (already existed)
    pub annotations_skipped: i32,
}

#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    data: T,
}

fn success<T: Serialize>(data: T) -> Response {
    (StatusCode::OK, Json(ApiResponse { data })).into_response()
}

// ============================================================================
// Handlers
// ============================================================================

/// Sync highlights from Light extension to Commonplace
///
/// This handler:
/// 1. Creates a Resource for each unique URL (if it doesn't exist)
/// 2. Creates an Annotation for each highlight (if it doesn't exist by groupID)
/// 3. Returns statistics about what was created/skipped
///
/// The sync is idempotent - running it multiple times with the same data
/// will not create duplicate entries.
pub async fn sync_highlights(
    State(state): State<AppState>,
    Json(payload): Json<SyncRequest>,
) -> Response {
    let lib = Commonplace::new(state.db.connection());

    let mut resources_created = 0;
    let mut annotations_created = 0;
    let mut annotations_skipped = 0;

    for (url, highlights) in payload.highlights {
        // Find or create resource for this URL
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

        // Create annotations for each highlight
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

/// Find an existing resource by URL or create a new one
/// Returns (resource_id, was_created)
async fn find_or_create_resource(
    lib: &Commonplace<'_>,
    url: &str,
) -> anyhow::Result<(i32, bool)> {
    // Check if resource already exists
    if let Some(resource) = lib.find_resource_by_title(url).await? {
        return Ok((resource.id, false));
    }

    // Create new resource
    let resource = lib
        .create_resource(CreateResource {
            title: url.to_string(),
            resource_type: ResourceType::Website,
        })
        .await?;

    Ok((resource.id, true))
}

/// Create an annotation if one with the same groupID doesn't already exist
/// Returns true if a new annotation was created, false if it already existed
async fn create_annotation_if_not_exists(
    lib: &Commonplace<'_>,
    resource_id: i32,
    highlight: &LightHighlight,
) -> anyhow::Result<bool> {
    // Check if annotation with this groupID already exists
    if annotation_exists_by_group_id(lib, resource_id, highlight.group_id).await? {
        return Ok(false);
    }

    // Create the boundary JSON with all Light metadata
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
    })
    .await?;

    Ok(true)
}

/// Check if an annotation with the given groupID exists for the resource
async fn annotation_exists_by_group_id(
    lib: &Commonplace<'_>,
    resource_id: i32,
    group_id: i64,
) -> anyhow::Result<bool> {
    // Get all annotations for this resource and check their boundaries
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

