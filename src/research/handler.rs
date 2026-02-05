use axum::{Json, extract::State, response::Response};
use libsql::{Builder, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use crate::commonplace::{
    Commonplace, CreateAnnotation, CreateComment, CreateNote, CreateResource, ResourceType, UpdateAnnotation,
    UpdateComment, UpdateNote, UpdateResource, compute_annotation_hash, compute_comment_hash, compute_note_hash,
    compute_resource_hash,
};
use crate::handler::AppState;
use crate::response::{bad_request, internal_error, success};
use crate::sync::{
    SyncResult, SyncStats, delete_orphans, handle_create_result, handle_create_result_unit, handle_update_result,
    handle_update_result_unit, is_unchanged, log_find_error,
};

#[derive(Debug, Deserialize)]
pub struct SetConfigRequest {
    pub db_path: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub db_path: Option<String>,
    pub last_sync_at: Option<String>,
}

#[derive(Debug, Serialize, Default)]
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

impl SyncResponse {
    fn apply_resources(&mut self, stats: &SyncStats) {
        self.resources_created = stats.created;
        self.resources_updated = stats.updated;
        self.resources_deleted = stats.deleted;
        self.resources_unchanged = stats.unchanged;
    }

    fn apply_annotations(&mut self, stats: &SyncStats) {
        self.annotations_created = stats.created;
        self.annotations_updated = stats.updated;
        self.annotations_deleted = stats.deleted;
        self.annotations_unchanged = stats.unchanged;
    }

    fn apply_comments(&mut self, stats: &SyncStats) {
        self.comments_created = stats.created;
        self.comments_updated = stats.updated;
        self.comments_deleted = stats.deleted;
        self.comments_unchanged = stats.unchanged;
    }

    fn apply_notes(&mut self, stats: &SyncStats) {
        self.notes_created = stats.created;
        self.notes_updated = stats.updated;
        self.notes_deleted = stats.deleted;
        self.notes_unchanged = stats.unchanged;
    }
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
    let query = r#"SELECT db_path, last_sync_at FROM research_config WHERE id = 1"#;
    let conn = state.db.connection();

    match conn.query(query, ()).await {
        Ok(mut rows) => match rows.next().await {
            Ok(Some(row)) => success(ConfigResponse {
                db_path: row.get(0).ok(),
                last_sync_at: row.get(1).ok(),
            }),
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

pub async fn set_config(State(state): State<AppState>, Json(payload): Json<SetConfigRequest>) -> Response {
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
    match conn.execute(query, libsql::params![payload.db_path.clone()]).await {
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
    let conn = state.db.connection();

    let db_path = match get_research_db_path(conn).await {
        Ok(path) => path,
        Err(response) => return response,
    };

    let research_conn = match open_research_db(&db_path).await {
        Ok(conn) => conn,
        Err(response) => return response,
    };

    let items = match fetch_research_items(&research_conn).await {
        Ok(items) => items,
        Err(e) => {
            tracing::error!("Failed to fetch items: {}", e);
            return internal_error("Failed to fetch items from Research database");
        }
    };

    let lib = Commonplace::new(conn);
    let stats = sync_all_entities(&lib, &research_conn, items).await;

    let _ = conn
        .execute(
            r#"
            UPDATE research_config 
            SET last_sync_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1
        "#,
            (),
        )
        .await;

    success(stats)
}

async fn get_research_db_path(conn: &libsql::Connection) -> Result<String, Response> {
    let query = r#"SELECT db_path FROM research_config WHERE id = 1"#;
    let not_configured = "Research database path not configured. Please set the path first.";

    let mut rows = conn.query(query, ()).await.map_err(|e| {
        tracing::error!("Failed to query config: {}", e);
        internal_error("Failed to query config")
    })?;

    let row = rows.next().await.map_err(|e| {
        tracing::error!("Failed to get config: {}", e);
        internal_error("Failed to get config")
    })?;

    let path: String = row
        .ok_or_else(|| bad_request(not_configured))?
        .get(0)
        .map_err(|_| bad_request(not_configured))?;

    if !Path::new(&path).exists() {
        return Err(bad_request("Research database file no longer exists at the configured path"));
    }

    Ok(path)
}

async fn open_research_db(db_path: &str) -> Result<Connection, Response> {
    let db = Builder::new_local(db_path)
        .flags(libsql::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .build()
        .await
        .map_err(|e| {
            tracing::error!("Failed to open Research database: {}", e);
            internal_error("Failed to open Research database")
        })?;

    db.connect().map_err(|e| {
        tracing::error!("Failed to connect to Research database: {}", e);
        internal_error("Failed to connect to Research database")
    })
}

#[derive(Default)]
struct SeenIds {
    resources: HashSet<String>,
    annotations: HashSet<String>,
    comments: HashSet<String>,
    notes: HashSet<String>,
}

async fn sync_all_entities(
    lib: &Commonplace<'_>,
    research_conn: &Connection,
    items: Vec<ResearchItem>,
) -> SyncResponse {
    let mut response = SyncResponse::default();
    let mut seen = SeenIds::default();

    let mut resource_stats = SyncStats::default();
    let mut annotation_stats = SyncStats::default();
    let mut comment_stats = SyncStats::default();
    let mut note_stats = SyncStats::default();

    for item in items {
        let resource_id = match sync_resource(lib, &item, &mut resource_stats, &mut seen.resources).await {
            Some(id) => id,
            None => continue,
        };

        sync_item_annotations(
            lib,
            research_conn,
            &item,
            resource_id,
            &mut annotation_stats,
            &mut comment_stats,
            &mut seen,
        )
        .await;
        sync_item_notes(lib, research_conn, &item, resource_id, &mut note_stats, &mut seen.notes).await;
    }

    soft_delete_orphans(lib, &seen, &mut resource_stats, &mut annotation_stats, &mut comment_stats, &mut note_stats)
        .await;

    response.apply_resources(&resource_stats);
    response.apply_annotations(&annotation_stats);
    response.apply_comments(&comment_stats);
    response.apply_notes(&note_stats);

    response
}

async fn sync_resource(
    lib: &Commonplace<'_>,
    item: &ResearchItem,
    stats: &mut SyncStats,
    seen: &mut HashSet<String>,
) -> Option<i32> {
    let external_id = format!("research:{}", item.id);
    let content_hash = compute_resource_hash(&item.title);
    seen.insert(external_id.clone());

    upsert_resource(lib, &external_id, &item.title, &content_hash)
        .await
        .record(stats)
}

async fn upsert_resource(lib: &Commonplace<'_>, external_id: &str, title: &str, content_hash: &str) -> SyncResult<i32> {
    let existing = match lib.find_resource_by_external_id(external_id).await {
        Ok(r) => r,
        Err(e) => {
            log_find_error("resource", external_id, e);
            return SyncResult::Error;
        }
    };

    let Some(resource) = existing else {
        return create_resource(lib, external_id, title, content_hash).await;
    };

    if is_unchanged(&resource, content_hash) {
        return SyncResult::Unchanged(resource.id);
    }

    update_resource(lib, external_id, resource.id, title, content_hash).await
}

async fn create_resource(lib: &Commonplace<'_>, external_id: &str, title: &str, content_hash: &str) -> SyncResult<i32> {
    let result = lib
        .create_resource(CreateResource {
            title: title.to_string(),
            resource_type: ResourceType::Pdf,
            external_id: Some(external_id.to_string()),
            content_hash: Some(content_hash.to_string()),
        })
        .await;

    handle_create_result(result, |r| r.id, "resource", external_id)
}

async fn update_resource(
    lib: &Commonplace<'_>,
    external_id: &str,
    id: i32,
    title: &str,
    content_hash: &str,
) -> SyncResult<i32> {
    let result = lib
        .update_resource(
            id,
            UpdateResource {
                title: Some(title.to_string()),
                resource_type: None,
                content_hash: Some(content_hash.to_string()),
                config: None,
            },
        )
        .await;

    handle_update_result(result, id, "resource", external_id)
}

async fn sync_item_annotations(
    lib: &Commonplace<'_>,
    research_conn: &Connection,
    item: &ResearchItem,
    resource_id: i32,
    annotation_stats: &mut SyncStats,
    comment_stats: &mut SyncStats,
    seen: &mut SeenIds,
) {
    let annotations = match fetch_research_annotations(research_conn, &item.id).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch annotations for {}: {}", item.id, e);
            return;
        }
    };

    for annotation in annotations {
        let annotation_id =
            match sync_annotation(lib, &annotation, resource_id, annotation_stats, &mut seen.annotations).await {
                Some(id) => id,
                None => continue,
            };

        sync_annotation_comments(lib, research_conn, &annotation, annotation_id, comment_stats, &mut seen.comments)
            .await;
    }
}

async fn sync_annotation(
    lib: &Commonplace<'_>,
    annotation: &ResearchAnnotation,
    resource_id: i32,
    stats: &mut SyncStats,
    seen: &mut HashSet<String>,
) -> Option<i32> {
    let external_id = format!("research:{}", annotation.id);
    let content_hash = compute_annotation_hash(&annotation.text, annotation.color.as_deref());
    seen.insert(external_id.clone());

    let boundary = serde_json::json!({
        "pageNumber": annotation.page_number,
        "position": annotation.position,
        "source": "research",
    });

    upsert_annotation(lib, &external_id, annotation, resource_id, &content_hash, boundary)
        .await
        .record(stats)
}

async fn upsert_annotation(
    lib: &Commonplace<'_>,
    external_id: &str,
    annotation: &ResearchAnnotation,
    resource_id: i32,
    content_hash: &str,
    boundary: serde_json::Value,
) -> SyncResult<i32> {
    let existing = match lib.find_annotation_by_external_id(external_id).await {
        Ok(a) => a,
        Err(e) => {
            log_find_error("annotation", external_id, e);
            return SyncResult::Error;
        }
    };

    let Some(ann) = existing else {
        return create_annotation(lib, external_id, annotation, resource_id, content_hash, boundary).await;
    };

    if is_unchanged(&ann, content_hash) {
        return SyncResult::Unchanged(ann.id);
    }

    update_annotation(lib, external_id, ann.id, annotation, content_hash, boundary).await
}

async fn create_annotation(
    lib: &Commonplace<'_>,
    external_id: &str,
    annotation: &ResearchAnnotation,
    resource_id: i32,
    content_hash: &str,
    boundary: serde_json::Value,
) -> SyncResult<i32> {
    let result = lib
        .create_annotation(CreateAnnotation {
            resource_id,
            text: annotation.text.clone(),
            color: annotation.color.clone(),
            boundary: Some(boundary),
            external_id: Some(external_id.to_string()),
            content_hash: Some(content_hash.to_string()),
        })
        .await;

    handle_create_result(result, |a| a.id, "annotation", external_id)
}

async fn update_annotation(
    lib: &Commonplace<'_>,
    external_id: &str,
    id: i32,
    annotation: &ResearchAnnotation,
    content_hash: &str,
    boundary: serde_json::Value,
) -> SyncResult<i32> {
    let result = lib
        .update_annotation(
            id,
            UpdateAnnotation {
                text: Some(annotation.text.clone()),
                color: annotation.color.clone(),
                boundary: Some(boundary),
                content_hash: Some(content_hash.to_string()),
            },
        )
        .await;

    handle_update_result(result, id, "annotation", external_id)
}

async fn sync_annotation_comments(
    lib: &Commonplace<'_>,
    research_conn: &Connection,
    annotation: &ResearchAnnotation,
    annotation_id: i32,
    stats: &mut SyncStats,
    seen: &mut HashSet<String>,
) {
    let comments = match fetch_research_comments(research_conn, &annotation.id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to fetch comments for {}: {}", annotation.id, e);
            return;
        }
    };

    for comment in comments {
        sync_comment(lib, &comment, annotation_id, stats, seen).await;
    }
}

async fn sync_comment(
    lib: &Commonplace<'_>,
    comment: &ResearchComment,
    annotation_id: i32,
    stats: &mut SyncStats,
    seen: &mut HashSet<String>,
) {
    let external_id = format!("research:{}", comment.id);
    let content_hash = compute_comment_hash(&comment.content);
    seen.insert(external_id.clone());

    upsert_comment(lib, &external_id, &comment.content, annotation_id, &content_hash)
        .await
        .record_unit(stats);
}

async fn upsert_comment(
    lib: &Commonplace<'_>,
    external_id: &str,
    content: &str,
    annotation_id: i32,
    content_hash: &str,
) -> SyncResult<()> {
    let existing = match lib.find_comment_by_external_id(external_id).await {
        Ok(c) => c,
        Err(e) => {
            log_find_error("comment", external_id, e);
            return SyncResult::Error;
        }
    };

    let Some(cmt) = existing else {
        return create_comment(lib, external_id, content, annotation_id, content_hash).await;
    };

    if is_unchanged(&cmt, content_hash) {
        return SyncResult::Unchanged(());
    }

    update_comment(lib, external_id, cmt.id, content, content_hash).await
}

async fn create_comment(
    lib: &Commonplace<'_>,
    external_id: &str,
    content: &str,
    annotation_id: i32,
    content_hash: &str,
) -> SyncResult<()> {
    let result = lib
        .create_comment(CreateComment {
            annotation_id,
            content: content.to_string(),
            external_id: Some(external_id.to_string()),
            content_hash: Some(content_hash.to_string()),
        })
        .await;

    handle_create_result_unit(result, "comment", external_id)
}

async fn update_comment(
    lib: &Commonplace<'_>,
    external_id: &str,
    id: i32,
    content: &str,
    content_hash: &str,
) -> SyncResult<()> {
    let result = lib
        .update_comment(
            id,
            UpdateComment {
                content: content.to_string(),
                content_hash: Some(content_hash.to_string()),
            },
        )
        .await;

    handle_update_result_unit(result, id, "comment", external_id)
}

async fn sync_item_notes(
    lib: &Commonplace<'_>,
    research_conn: &Connection,
    item: &ResearchItem,
    resource_id: i32,
    stats: &mut SyncStats,
    seen: &mut HashSet<String>,
) {
    let notes = match fetch_research_notes(research_conn, &item.id).await {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("Failed to fetch notes for {}: {}", item.id, e);
            return;
        }
    };

    for note in notes {
        sync_note(lib, &note, resource_id, stats, seen).await;
    }
}

async fn sync_note(
    lib: &Commonplace<'_>,
    note: &ResearchNote,
    resource_id: i32,
    stats: &mut SyncStats,
    seen: &mut HashSet<String>,
) {
    let external_id = format!("research:{}", note.id);
    let content_hash = compute_note_hash(&note.content);
    seen.insert(external_id.clone());

    upsert_note(lib, &external_id, &note.content, resource_id, &content_hash)
        .await
        .record_unit(stats);
}

async fn upsert_note(
    lib: &Commonplace<'_>,
    external_id: &str,
    content: &str,
    resource_id: i32,
    content_hash: &str,
) -> SyncResult<()> {
    let existing = match lib.find_note_by_external_id(external_id).await {
        Ok(n) => n,
        Err(e) => {
            log_find_error("note", external_id, e);
            return SyncResult::Error;
        }
    };

    let Some(n) = existing else {
        return create_note(lib, external_id, content, resource_id, content_hash).await;
    };

    if is_unchanged(&n, content_hash) {
        return SyncResult::Unchanged(());
    }

    update_note(lib, external_id, n.id, content, content_hash).await
}

async fn create_note(
    lib: &Commonplace<'_>,
    external_id: &str,
    content: &str,
    resource_id: i32,
    content_hash: &str,
) -> SyncResult<()> {
    let result = lib
        .create_note(CreateNote {
            resource_id,
            content: content.to_string(),
            external_id: Some(external_id.to_string()),
            content_hash: Some(content_hash.to_string()),
        })
        .await;

    handle_create_result_unit(result, "note", external_id)
}

async fn update_note(
    lib: &Commonplace<'_>,
    external_id: &str,
    id: i32,
    content: &str,
    content_hash: &str,
) -> SyncResult<()> {
    let result = lib
        .update_note(
            id,
            UpdateNote {
                content: content.to_string(),
                content_hash: Some(content_hash.to_string()),
            },
        )
        .await;

    handle_update_result_unit(result, id, "note", external_id)
}

async fn soft_delete_orphans(
    lib: &Commonplace<'_>,
    seen: &SeenIds,
    resource_stats: &mut SyncStats,
    annotation_stats: &mut SyncStats,
    comment_stats: &mut SyncStats,
    note_stats: &mut SyncStats,
) {
    delete_orphans(
        || lib.find_comments_by_source_prefix("research"),
        |id| lib.soft_delete_comment(id),
        &seen.comments,
        comment_stats,
        "comment",
    )
    .await;

    delete_orphans(
        || lib.find_annotations_by_source_prefix("research", None),
        |id| lib.soft_delete_annotation(id),
        &seen.annotations,
        annotation_stats,
        "annotation",
    )
    .await;

    delete_orphans(
        || lib.find_notes_by_source_prefix("research"),
        |id| lib.soft_delete_note(id),
        &seen.notes,
        note_stats,
        "note",
    )
    .await;

    delete_orphans(
        || lib.find_resources_by_source_prefix("research"),
        |id| lib.soft_delete_resource(id),
        &seen.resources,
        resource_stats,
        "resource",
    )
    .await;
}

async fn fetch_research_items(conn: &Connection) -> anyhow::Result<Vec<ResearchItem>> {
    let query_with_filter = r#"
        SELECT id, title
        FROM items 
        WHERE deleted_at IS NULL
    "#;

    let query_no_filter = r#"
        SELECT id, title
        FROM items
    "#;

    let mut rows = match conn.query(query_with_filter, ()).await {
        Ok(rows) => rows,
        Err(_) => conn.query(query_no_filter, ()).await?,
    };

    let mut items = Vec::new();

    while let Some(row) = rows.next().await? {
        items.push(ResearchItem {
            id: row.get(0)?,
            title: row.get::<Option<String>>(1)?.unwrap_or_default(),
        });
    }

    Ok(items)
}

async fn fetch_research_annotations(conn: &Connection, item_id: &str) -> anyhow::Result<Vec<ResearchAnnotation>> {
    let query = r#"
        SELECT 
            id,
            json_extract(content, '$.text') as text,
            color,
            json_extract(position, '$.boundingRect.pageNumber') as page_number,
            position
        FROM annotations 
        WHERE item_id = ?
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

async fn fetch_research_comments(conn: &Connection, annotation_id: &str) -> anyhow::Result<Vec<ResearchComment>> {
    let query = r#"
        SELECT id, content
        FROM comments 
        WHERE annotation_id = ?
    "#;

    let mut rows = conn.query(query, libsql::params![annotation_id]).await?;
    let mut comments = Vec::new();

    while let Some(row) = rows.next().await? {
        comments.push(ResearchComment {
            id: row.get(0)?,
            content: row.get::<Option<String>>(1)?.unwrap_or_default(),
        });
    }

    Ok(comments)
}

async fn fetch_research_notes(conn: &Connection, item_id: &str) -> anyhow::Result<Vec<ResearchNote>> {
    let query = r#"
        SELECT id, content
        FROM notes 
        WHERE item_id = ?
    "#;

    let mut rows = conn.query(query, libsql::params![item_id]).await?;
    let mut notes = Vec::new();

    while let Some(row) = rows.next().await? {
        notes.push(ResearchNote {
            id: row.get(0)?,
            content: row.get::<Option<String>>(1)?.unwrap_or_default(),
        });
    }

    Ok(notes)
}
