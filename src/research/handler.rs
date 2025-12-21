use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use libsql::{Builder, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::commonplace::{
    Commonplace, CreateAnnotation, CreateComment, CreateNote, CreateResource, ResourceType,
    UpdateAnnotation, UpdateComment, UpdateNote, UpdateResource,
    compute_resource_hash, compute_annotation_hash, compute_comment_hash, compute_note_hash,
};
use crate::handler::AppState;
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
pub struct SetConfigRequest {
    pub db_path: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub db_path: Option<String>,
    pub last_sync_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub resources_created: i32,
    pub resources_updated: i32,
    pub resources_deleted: i32,
    pub resources_unchanged: i32,
    pub annotations_created: i32,
    pub annotations_updated: i32,
    pub annotations_deleted: i32,
    pub annotations_unchanged: i32,
    pub comments_created: i32,
    pub comments_updated: i32,
    pub comments_deleted: i32,
    pub comments_unchanged: i32,
    pub notes_created: i32,
    pub notes_updated: i32,
    pub notes_deleted: i32,
    pub notes_unchanged: i32,
}

#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    data: T,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

fn success<T: Serialize>(data: T) -> Response {
    (StatusCode::OK, Json(ApiResponse { data })).into_response()
}

fn bad_request(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

fn internal_error(msg: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

#[derive(Debug)]
struct ResearchItem {
    id: String,
    title: String,
}

#[derive(Debug)]
struct ResearchAnnotation {
    id: String,
    text: String,
    color: Option<String>,
    page_number: Option<i64>,
    position: Option<String>,
}

#[derive(Debug)]
struct ResearchComment {
    id: String,
    content: String,
}

#[derive(Debug)]
struct ResearchNote {
    id: String,
    content: String,
}

pub async fn get_config(State(state): State<AppState>) -> Response {
    let query = r#"
        SELECT db_path, last_sync_at FROM research_config WHERE id = 1
    "#;

    let conn = state.db.connection();
    match conn.query(query, ()).await {
        Ok(mut rows) => match rows.next().await {
            Ok(Some(row)) => {
                let db_path: Option<String> = row.get(0).ok();
                let last_sync_at: Option<String> = row.get(1).ok();
                success(ConfigResponse {
                    db_path,
                    last_sync_at,
                })
            }
            Ok(None) => success(ConfigResponse {
                db_path: None,
                last_sync_at: None,
            }),
            Err(e) => {
                tracing::error!("Failed to get config: {}", e);
                internal_error("Failed to get config")
            }
        },
        Err(e) => {
            tracing::error!("Failed to query config: {}", e);
            internal_error("Failed to query config")
        }
    }
}

pub async fn set_config(
    State(state): State<AppState>,
    Json(payload): Json<SetConfigRequest>,
) -> Response {
    if !Path::new(&payload.db_path).exists() {
        return bad_request("Database file does not exist at the specified path");
    }

    let query = r#"
        INSERT INTO research_config (id, db_path)
        VALUES (1, ?)
        ON CONFLICT(id) DO UPDATE SET 
            db_path = excluded.db_path,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    "#;

    let conn = state.db.connection();
    match conn
        .execute(query, libsql::params![payload.db_path.clone()])
        .await
    {
        Ok(_) => success(ConfigResponse {
            db_path: Some(payload.db_path),
            last_sync_at: None,
        }),
        Err(e) => {
            tracing::error!("Failed to set config: {}", e);
            internal_error("Failed to save configuration")
        }
    }
}

pub async fn sync(State(state): State<AppState>) -> Response {
    let query = r#"SELECT db_path FROM research_config WHERE id = 1"#;
    let conn = state.db.connection();

    let db_path: String = match conn.query(query, ()).await {
        Ok(mut rows) => match rows.next().await {
            Ok(Some(row)) => match row.get::<String>(0) {
                Ok(path) => path,
                Err(_) => {
                    return bad_request(
                        "Research database path not configured. Please set the path first.",
                    );
                }
            },
            Ok(None) => {
                return bad_request(
                    "Research database path not configured. Please set the path first.",
                );
            }
            Err(e) => {
                tracing::error!("Failed to get config: {}", e);
                return internal_error("Failed to get config");
            }
        },
        Err(e) => {
            tracing::error!("Failed to query config: {}", e);
            return internal_error("Failed to query config");
        }
    };

    if !Path::new(&db_path).exists() {
        return bad_request("Research database file no longer exists at the configured path");
    }

    let research_db = match Builder::new_local(&db_path)
        .flags(libsql::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .build()
        .await
    {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("Failed to open Research database: {}", e);
            return internal_error("Failed to open Research database");
        }
    };

    let research_conn = match research_db.connect() {
        Ok(conn) => conn,
        Err(e) => {
            tracing::error!("Failed to connect to Research database: {}", e);
            return internal_error("Failed to connect to Research database");
        }
    };

    let lib = Commonplace::new(conn);
    let mut stats = SyncResponse {
        resources_created: 0,
        resources_updated: 0,
        resources_deleted: 0,
        resources_unchanged: 0,
        annotations_created: 0,
        annotations_updated: 0,
        annotations_deleted: 0,
        annotations_unchanged: 0,
        comments_created: 0,
        comments_updated: 0,
        comments_deleted: 0,
        comments_unchanged: 0,
        notes_created: 0,
        notes_updated: 0,
        notes_deleted: 0,
        notes_unchanged: 0,
    };

    let mut seen_resources = HashSet::new();
    let mut seen_annotations = HashSet::new();
    let mut seen_comments = HashSet::new();
    let mut seen_notes = HashSet::new();

    let items = match fetch_research_items(&research_conn).await {
        Ok(items) => items,
        Err(e) => {
            tracing::error!("Failed to fetch items: {}", e);
            return internal_error("Failed to fetch items from Research database");
        }
    };

    // Phase 1: Upsert all entities
    for item in items {
        let external_id = format!("research:{}", item.id);
        let content_hash = compute_resource_hash(&item.title);
        seen_resources.insert(external_id.clone());

        let resource_id = match lib.find_resource_by_external_id(&external_id).await {
            Ok(Some(existing)) => {
                if existing.content_hash.as_deref() != Some(&content_hash) {
                    // Content changed, update it
                    match lib
                        .update_resource(
                            existing.id,
                            UpdateResource {
                                title: Some(item.title.clone()),
                                resource_type: None,
                                content_hash: Some(content_hash),
                            },
                        )
                        .await
                    {
                        Ok(Some(_)) => {
                            stats.resources_updated += 1;
                        }
                        Ok(None) => {
                            tracing::warn!("Resource {} not found for update", existing.id);
                            continue;
                        }
                        Err(e) => {
                            tracing::error!("Failed to update resource {}: {}", item.id, e);
                            continue;
                        }
                    }
                    existing.id
                } else {
                    stats.resources_unchanged += 1;
                    existing.id
                }
            }
            Ok(None) => {
                match lib
                    .create_resource(CreateResource {
                        title: item.title.clone(),
                        resource_type: ResourceType::Pdf,
                        external_id: Some(external_id.clone()),
                        content_hash: Some(content_hash),
                    })
                    .await
                {
                    Ok(resource) => {
                        stats.resources_created += 1;
                        resource.id
                    }
                    Err(e) => {
                        tracing::error!("Failed to create resource for {}: {}", item.id, e);
                        continue;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to check resource {}: {}", item.id, e);
                continue;
            }
        };

        let annotations = match fetch_research_annotations(&research_conn, &item.id).await {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("Failed to fetch annotations for {}: {}", item.id, e);
                continue;
            }
        };

        for annotation in annotations {
            let ann_external_id = format!("research:{}", annotation.id);
            let ann_hash = compute_annotation_hash(
                &annotation.text,
                annotation.color.as_deref(),
            );
            seen_annotations.insert(ann_external_id.clone());

            let annotation_id = match lib.find_annotation_by_external_id(&ann_external_id).await {
                Ok(Some(existing)) => {
                    if existing.content_hash.as_deref() != Some(&ann_hash) {
                        // Content changed, update it
                        let boundary = serde_json::json!({
                            "pageNumber": annotation.page_number,
                            "position": annotation.position,
                            "source": "research",
                        });

                        match lib
                            .update_annotation(
                                existing.id,
                                UpdateAnnotation {
                                    text: Some(annotation.text.clone()),
                                    color: annotation.color.clone(),
                                    boundary: Some(boundary),
                                    content_hash: Some(ann_hash),
                                },
                            )
                            .await
                        {
                            Ok(Some(_)) => {
                                stats.annotations_updated += 1;
                            }
                            Ok(None) => {
                                tracing::warn!("Annotation {} not found for update", existing.id);
                                continue;
                            }
                            Err(e) => {
                                tracing::error!("Failed to update annotation {}: {}", annotation.id, e);
                                continue;
                            }
                        }
                        existing.id
                    } else {
                        stats.annotations_unchanged += 1;
                        existing.id
                    }
                }
                Ok(None) => {
                    let boundary = serde_json::json!({
                        "pageNumber": annotation.page_number,
                        "position": annotation.position,
                        "source": "research",
                    });

                    match lib
                        .create_annotation(CreateAnnotation {
                            resource_id,
                            text: annotation.text.clone(),
                            color: annotation.color.clone(),
                            boundary: Some(boundary),
                            external_id: Some(ann_external_id.clone()),
                            content_hash: Some(ann_hash),
                        })
                        .await
                    {
                        Ok(created) => {
                            stats.annotations_created += 1;
                            created.id
                        }
                        Err(e) => {
                            tracing::error!("Failed to create annotation {}: {}", annotation.id, e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to check annotation {}: {}", annotation.id, e);
                    continue;
                }
            };

            let comments = match fetch_research_comments(&research_conn, &annotation.id).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to fetch comments for {}: {}", annotation.id, e);
                    continue;
                }
            };

            for comment in comments {
                let comment_external_id = format!("research:{}", comment.id);
                let comment_hash = compute_comment_hash(&comment.content);
                seen_comments.insert(comment_external_id.clone());

                match lib.find_comment_by_external_id(&comment_external_id).await {
                    Ok(Some(existing)) => {
                        if existing.content_hash.as_deref() != Some(&comment_hash) {
                            // Content changed, update it
                            match lib
                                .update_comment(
                                    existing.id,
                                    UpdateComment {
                                        content: comment.content.clone(),
                                        content_hash: Some(comment_hash),
                                    },
                                )
                                .await
                            {
                                Ok(Some(_)) => {
                                    stats.comments_updated += 1;
                                }
                                Ok(None) => {
                                    tracing::warn!("Comment {} not found for update", existing.id);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to update comment {}: {}", comment.id, e);
                                }
                            }
                        } else {
                            stats.comments_unchanged += 1;
                        }
                    }
                    Ok(None) => {
                        match lib
                            .create_comment(CreateComment {
                                annotation_id,
                                content: comment.content.clone(),
                                external_id: Some(comment_external_id),
                                content_hash: Some(comment_hash),
                            })
                            .await
                        {
                            Ok(_) => {
                                stats.comments_created += 1;
                            }
                            Err(e) => {
                                tracing::error!("Failed to create comment {}: {}", comment.id, e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to check comment {}: {}", comment.id, e);
                    }
                }
            }
        }

        let notes = match fetch_research_notes(&research_conn, &item.id).await {
            Ok(n) => n,
            Err(e) => {
                tracing::error!("Failed to fetch notes for {}: {}", item.id, e);
                continue;
            }
        };

        for note in notes {
            let note_external_id = format!("research:{}", note.id);
            let note_hash = compute_note_hash(&note.content);
            seen_notes.insert(note_external_id.clone());

            match lib.find_note_by_external_id(&note_external_id).await {
                Ok(Some(existing)) => {
                    if existing.content_hash.as_deref() != Some(&note_hash) {
                        // Content changed, update it
                        match lib
                            .update_note(
                                existing.id,
                                UpdateNote {
                                    content: note.content.clone(),
                                    content_hash: Some(note_hash),
                                },
                            )
                            .await
                        {
                            Ok(Some(_)) => {
                                stats.notes_updated += 1;
                            }
                            Ok(None) => {
                                tracing::warn!("Note {} not found for update", existing.id);
                            }
                            Err(e) => {
                                tracing::error!("Failed to update note {}: {}", note.id, e);
                            }
                        }
                    } else {
                        stats.notes_unchanged += 1;
                    }
                }
                Ok(None) => {
                    match lib
                        .create_note(CreateNote {
                            resource_id,
                            content: note.content.clone(),
                            external_id: Some(note_external_id),
                            content_hash: Some(note_hash),
                        })
                        .await
                    {
                        Ok(_) => {
                            stats.notes_created += 1;
                        }
                        Err(e) => {
                            tracing::error!("Failed to create note {}: {}", note.id, e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to check note {}: {}", note.id, e);
                }
            }
        }
    }

    // Phase 2: Soft delete orphans (order doesn't matter)
    match lib.find_comments_by_source_prefix("research").await {
        Ok(orphans) => {
            for orphan in orphans {
                if !seen_comments.contains(
                    orphan.external_id.as_ref().unwrap_or(&String::new()),
                ) {
                    match lib.soft_delete_comment(orphan.id).await {
                        Ok(true) => {
                            stats.comments_deleted += 1;
                        }
                        Ok(false) | Err(_) => {
                            tracing::warn!("Failed to soft delete comment {}", orphan.id);
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to find orphan comments: {}", e);
        }
    }

    match lib.find_annotations_by_source_prefix("research", None).await {
        Ok(orphans) => {
            for orphan in orphans {
                if !seen_annotations.contains(
                    orphan.external_id.as_ref().unwrap_or(&String::new()),
                ) {
                    match lib.soft_delete_annotation(orphan.id).await {
                        Ok(true) => {
                            stats.annotations_deleted += 1;
                        }
                        Ok(false) | Err(_) => {
                            tracing::warn!("Failed to soft delete annotation {}", orphan.id);
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to find orphan annotations: {}", e);
        }
    }

    match lib.find_notes_by_source_prefix("research").await {
        Ok(orphans) => {
            for orphan in orphans {
                if !seen_notes.contains(
                    orphan.external_id.as_ref().unwrap_or(&String::new()),
                ) {
                    match lib.soft_delete_note(orphan.id).await {
                        Ok(true) => {
                            stats.notes_deleted += 1;
                        }
                        Ok(false) | Err(_) => {
                            tracing::warn!("Failed to soft delete note {}", orphan.id);
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to find orphan notes: {}", e);
        }
    }

    match lib.find_resources_by_source_prefix("research").await {
        Ok(orphans) => {
            for orphan in orphans {
                if !seen_resources.contains(
                    orphan.external_id.as_ref().unwrap_or(&String::new()),
                ) {
                    match lib.soft_delete_resource(orphan.id).await {
                        Ok(true) => {
                            stats.resources_deleted += 1;
                        }
                        Ok(false) | Err(_) => {
                            tracing::warn!("Failed to soft delete resource {}", orphan.id);
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to find orphan resources: {}", e);
        }
    }

    let update_query = r#"
        UPDATE research_config 
        SET last_sync_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE id = 1
    "#;
    let _ = conn.execute(update_query, ()).await;

    success(stats)
}

async fn fetch_research_items(conn: &Connection) -> anyhow::Result<Vec<ResearchItem>> {
    let query = r#"
        SELECT id, title
        FROM items 
        WHERE deleted_at IS NULL
    "#;

    let mut rows = conn.query(query, ()).await?;
    let mut items = Vec::new();

    while let Some(row) = rows.next().await? {
        items.push(ResearchItem {
            id: row.get(0)?,
            title: row.get(1)?,
        });
    }

    Ok(items)
}

async fn fetch_research_annotations(
    conn: &Connection,
    item_id: &str,
) -> anyhow::Result<Vec<ResearchAnnotation>> {
    let query = r#"
        SELECT 
            id,
            json_extract(content, '$.text') as text,
            color,
            json_extract(position, '$.boundingRect.pageNumber') as page_number,
            position
        FROM annotations 
        WHERE item_id = ? AND deleted_at IS NULL
    "#;

    let mut rows = conn.query(query, libsql::params![item_id]).await?;
    let mut annotations = Vec::new();

    while let Some(row) = rows.next().await? {
        annotations.push(ResearchAnnotation {
            id: row.get(0)?,
            text: row.get::<Option<String>>(1)?.unwrap_or_default(),
            color: row.get(2)?,
            page_number: row.get(3)?,
            position: row.get(4)?,
        });
    }

    Ok(annotations)
}

async fn fetch_research_comments(
    conn: &Connection,
    annotation_id: &str,
) -> anyhow::Result<Vec<ResearchComment>> {
    let query = r#"
        SELECT id, content
        FROM comments 
        WHERE annotation_id = ? AND deleted_at IS NULL
    "#;

    let mut rows = conn.query(query, libsql::params![annotation_id]).await?;
    let mut comments = Vec::new();

    while let Some(row) = rows.next().await? {
        comments.push(ResearchComment {
            id: row.get(0)?,
            content: row.get(1)?,
        });
    }

    Ok(comments)
}

async fn fetch_research_notes(
    conn: &Connection,
    item_id: &str,
) -> anyhow::Result<Vec<ResearchNote>> {
    let query = r#"
        SELECT id, content
        FROM notes 
        WHERE item_id = ? AND deleted_at IS NULL
    "#;

    let mut rows = conn.query(query, libsql::params![item_id]).await?;
    let mut notes = Vec::new();

    while let Some(row) = rows.next().await? {
        notes.push(ResearchNote {
            id: row.get(0)?,
            content: row.get(1)?,
        });
    }

    Ok(notes)
}
