use anyhow::Result;
use libsql::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    Website,
    Pdf,
}

impl ResourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceType::Website => "website",
            ResourceType::Pdf => "pdf",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "website" => Some(ResourceType::Website),
            "pdf" => Some(ResourceType::Pdf),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub id: i32,
    pub title: String,
    #[serde(rename = "type")]
    pub resource_type: ResourceType,
    pub external_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: i32,
    pub resource_id: i32,
    pub text: String,
    pub color: Option<String>,
    pub boundary: Option<JsonValue>,
    pub external_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: i32,
    pub annotation_id: i32,
    pub content: String,
    pub external_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: i32,
    pub resource_id: i32,
    pub content: String,
    pub external_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub id: i32,
    pub resource_id: i32,
    pub name: String,
    pub meaning: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResource {
    pub title: String,
    #[serde(rename = "type")]
    pub resource_type: ResourceType,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResource {
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub resource_type: Option<ResourceType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAnnotation {
    pub resource_id: i32,
    pub text: String,
    pub color: Option<String>,
    pub boundary: Option<JsonValue>,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAnnotation {
    pub text: Option<String>,
    pub color: Option<String>,
    pub boundary: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateComment {
    pub annotation_id: i32,
    pub content: String,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateComment {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNote {
    pub resource_id: i32,
    pub content: String,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNote {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWord {
    pub resource_id: i32,
    pub name: String,
    pub meaning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWord {
    pub name: Option<String>,
    pub meaning: Option<String>,
}

pub struct Commonplace<'a> {
    conn: &'a Connection,
}

impl<'a> Commonplace<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub async fn create_resource(&self, input: CreateResource) -> Result<Resource> {
        let query = r#"
            INSERT INTO resources (title, type, external_id)
            VALUES (?, ?, ?)
            RETURNING id, title, type, external_id, created_at, updated_at
        "#;

        let mut rows = self
            .conn
            .query(
                query,
                libsql::params![input.title, input.resource_type.as_str(), input.external_id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(self.row_to_resource(&row)?)
        } else {
            anyhow::bail!("Failed to create resource")
        }
    }

    pub async fn get_resource(&self, id: i32) -> Result<Option<Resource>> {
        let query = r#"
            SELECT id, title, type, external_id, created_at, updated_at
            FROM resources WHERE id = ?
        "#;

        let mut rows = self.conn.query(query, libsql::params![id]).await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_resource(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn find_resource_by_title(&self, title: &str) -> Result<Option<Resource>> {
        let query = r#"
            SELECT id, title, type, external_id, created_at, updated_at
            FROM resources WHERE title = ?
        "#;

        let mut rows = self.conn.query(query, libsql::params![title]).await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_resource(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn find_resource_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<Resource>> {
        let query = r#"
            SELECT id, title, type, external_id, created_at, updated_at
            FROM resources WHERE external_id = ?
        "#;

        let mut rows = self.conn.query(query, libsql::params![external_id]).await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_resource(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn list_resources(
        &self,
        limit: i32,
        offset: i32,
        resource_type: Option<&str>,
    ) -> Result<Vec<Resource>> {
        let mut resources = Vec::new();

        if let Some(rtype) = resource_type {
            let query = r#"
                SELECT id, title, type, external_id, created_at, updated_at
                FROM resources
                WHERE type = ?
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
            "#;

            let mut rows = self
                .conn
                .query(query, libsql::params![rtype, limit, offset])
                .await?;

            while let Some(row) = rows.next().await? {
                resources.push(self.row_to_resource(&row)?);
            }
        } else {
            let query = r#"
                SELECT id, title, type, external_id, created_at, updated_at
                FROM resources
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
            "#;

            let mut rows = self
                .conn
                .query(query, libsql::params![limit, offset])
                .await?;

            while let Some(row) = rows.next().await? {
                resources.push(self.row_to_resource(&row)?);
            }
        }

        Ok(resources)
    }

    pub async fn update_resource(
        &self,
        id: i32,
        input: UpdateResource,
    ) -> Result<Option<Resource>> {
        if self.get_resource(id).await?.is_none() {
            return Ok(None);
        }

        let mut updates = Vec::new();
        let mut params: Vec<libsql::Value> = Vec::new();

        if let Some(title) = &input.title {
            updates.push("title = ?");
            params.push(title.clone().into());
        }
        if let Some(resource_type) = &input.resource_type {
            updates.push("type = ?");
            params.push(resource_type.as_str().into());
        }

        if updates.is_empty() {
            return self.get_resource(id).await;
        }

        updates.push("updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')");
        params.push(id.into());

        let query = format!("UPDATE resources SET {} WHERE id = ?", updates.join(", "));

        self.conn.execute(&query, params).await?;
        self.get_resource(id).await
    }

    pub async fn delete_resource(&self, id: i32) -> Result<bool> {
        let result = self
            .conn
            .execute("DELETE FROM resources WHERE id = ?", libsql::params![id])
            .await?;
        Ok(result > 0)
    }

    fn row_to_resource(&self, row: &libsql::Row) -> Result<Resource> {
        let type_str: String = row.get(2)?;
        let resource_type = ResourceType::from_str(&type_str)
            .ok_or_else(|| anyhow::anyhow!("Invalid resource type: {}", type_str))?;

        Ok(Resource {
            id: row.get(0)?,
            title: row.get(1)?,
            resource_type,
            external_id: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    }

    pub async fn create_annotation(&self, input: CreateAnnotation) -> Result<Annotation> {
        let boundary_json = input
            .boundary
            .as_ref()
            .map(|b| serde_json::to_string(b))
            .transpose()?;

        let query = r#"
            INSERT INTO annotations (resource_id, text, color, boundary, external_id)
            VALUES (?, ?, ?, ?, ?)
            RETURNING id, resource_id, text, color, boundary, external_id, created_at, updated_at
        "#;

        let mut rows = self
            .conn
            .query(
                query,
                libsql::params![
                    input.resource_id,
                    input.text,
                    input.color,
                    boundary_json,
                    input.external_id
                ],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(self.row_to_annotation(&row)?)
        } else {
            anyhow::bail!("Failed to create annotation")
        }
    }

    pub async fn get_annotation(&self, id: i32) -> Result<Option<Annotation>> {
        let query = r#"
            SELECT id, resource_id, text, color, boundary, external_id, created_at, updated_at
            FROM annotations WHERE id = ?
        "#;

        let mut rows = self.conn.query(query, libsql::params![id]).await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_annotation(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn find_annotation_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<Annotation>> {
        let query = r#"
            SELECT id, resource_id, text, color, boundary, external_id, created_at, updated_at
            FROM annotations WHERE external_id = ?
        "#;

        let mut rows = self.conn.query(query, libsql::params![external_id]).await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_annotation(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn list_annotations_by_resource(&self, resource_id: i32) -> Result<Vec<Annotation>> {
        let query = r#"
            SELECT id, resource_id, text, color, boundary, external_id, created_at, updated_at
            FROM annotations
            WHERE resource_id = ?
            ORDER BY created_at ASC
        "#;

        let mut rows = self.conn.query(query, libsql::params![resource_id]).await?;
        let mut annotations = Vec::new();

        while let Some(row) = rows.next().await? {
            annotations.push(self.row_to_annotation(&row)?);
        }

        Ok(annotations)
    }

    pub async fn update_annotation(
        &self,
        id: i32,
        input: UpdateAnnotation,
    ) -> Result<Option<Annotation>> {
        if self.get_annotation(id).await?.is_none() {
            return Ok(None);
        }

        let mut updates = Vec::new();
        let mut params: Vec<libsql::Value> = Vec::new();

        if let Some(text) = &input.text {
            updates.push("text = ?");
            params.push(text.clone().into());
        }
        if let Some(color) = &input.color {
            updates.push("color = ?");
            params.push(color.clone().into());
        }
        if let Some(boundary) = &input.boundary {
            updates.push("boundary = ?");
            let json_str = serde_json::to_string(boundary)?;
            params.push(json_str.into());
        }

        if updates.is_empty() {
            return self.get_annotation(id).await;
        }

        updates.push("updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')");
        params.push(id.into());

        let query = format!("UPDATE annotations SET {} WHERE id = ?", updates.join(", "));

        self.conn.execute(&query, params).await?;
        self.get_annotation(id).await
    }

    pub async fn delete_annotation(&self, id: i32) -> Result<bool> {
        let result = self
            .conn
            .execute("DELETE FROM annotations WHERE id = ?", libsql::params![id])
            .await?;
        Ok(result > 0)
    }

    fn row_to_annotation(&self, row: &libsql::Row) -> Result<Annotation> {
        let boundary_str: Option<String> = row.get(4)?;
        let boundary = boundary_str.map(|s| serde_json::from_str(&s)).transpose()?;

        Ok(Annotation {
            id: row.get(0)?,
            resource_id: row.get(1)?,
            text: row.get(2)?,
            color: row.get(3)?,
            boundary,
            external_id: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    }

    pub async fn create_comment(&self, input: CreateComment) -> Result<Comment> {
        let query = r#"
            INSERT INTO comments (annotation_id, content, external_id)
            VALUES (?, ?, ?)
            RETURNING id, annotation_id, content, external_id, created_at, updated_at
        "#;

        let mut rows = self
            .conn
            .query(
                query,
                libsql::params![input.annotation_id, input.content, input.external_id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(self.row_to_comment(&row)?)
        } else {
            anyhow::bail!("Failed to create comment")
        }
    }

    pub async fn get_comment(&self, id: i32) -> Result<Option<Comment>> {
        let query = r#"
            SELECT id, annotation_id, content, external_id, created_at, updated_at
            FROM comments WHERE id = ?
        "#;

        let mut rows = self.conn.query(query, libsql::params![id]).await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_comment(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn find_comment_by_external_id(&self, external_id: &str) -> Result<Option<Comment>> {
        let query = r#"
            SELECT id, annotation_id, content, external_id, created_at, updated_at
            FROM comments WHERE external_id = ?
        "#;

        let mut rows = self.conn.query(query, libsql::params![external_id]).await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_comment(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn list_comments_by_annotation(&self, annotation_id: i32) -> Result<Vec<Comment>> {
        let query = r#"
            SELECT id, annotation_id, content, external_id, created_at, updated_at
            FROM comments
            WHERE annotation_id = ?
            ORDER BY created_at ASC
        "#;

        let mut rows = self
            .conn
            .query(query, libsql::params![annotation_id])
            .await?;
        let mut comments = Vec::new();

        while let Some(row) = rows.next().await? {
            comments.push(self.row_to_comment(&row)?);
        }

        Ok(comments)
    }

    pub async fn update_comment(&self, id: i32, input: UpdateComment) -> Result<Option<Comment>> {
        if self.get_comment(id).await?.is_none() {
            return Ok(None);
        }

        let query = r#"
            UPDATE comments 
            SET content = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?
        "#;

        self.conn
            .execute(query, libsql::params![input.content, id])
            .await?;
        self.get_comment(id).await
    }

    pub async fn delete_comment(&self, id: i32) -> Result<bool> {
        let result = self
            .conn
            .execute("DELETE FROM comments WHERE id = ?", libsql::params![id])
            .await?;
        Ok(result > 0)
    }

    fn row_to_comment(&self, row: &libsql::Row) -> Result<Comment> {
        Ok(Comment {
            id: row.get(0)?,
            annotation_id: row.get(1)?,
            content: row.get(2)?,
            external_id: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    }

    pub async fn create_note(&self, input: CreateNote) -> Result<Note> {
        let query = r#"
            INSERT INTO notes (resource_id, content, external_id)
            VALUES (?, ?, ?)
            RETURNING id, resource_id, content, external_id, created_at, updated_at
        "#;

        let mut rows = self
            .conn
            .query(
                query,
                libsql::params![input.resource_id, input.content, input.external_id],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(self.row_to_note(&row)?)
        } else {
            anyhow::bail!("Failed to create note")
        }
    }

    pub async fn get_note(&self, id: i32) -> Result<Option<Note>> {
        let query = r#"
            SELECT id, resource_id, content, external_id, created_at, updated_at
            FROM notes WHERE id = ?
        "#;

        let mut rows = self.conn.query(query, libsql::params![id]).await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_note(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn find_note_by_external_id(&self, external_id: &str) -> Result<Option<Note>> {
        let query = r#"
            SELECT id, resource_id, content, external_id, created_at, updated_at
            FROM notes WHERE external_id = ?
        "#;

        let mut rows = self.conn.query(query, libsql::params![external_id]).await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_note(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn list_notes_by_resource(&self, resource_id: i32) -> Result<Vec<Note>> {
        let query = r#"
            SELECT id, resource_id, content, external_id, created_at, updated_at
            FROM notes
            WHERE resource_id = ?
            ORDER BY created_at DESC
        "#;

        let mut rows = self.conn.query(query, libsql::params![resource_id]).await?;
        let mut notes = Vec::new();

        while let Some(row) = rows.next().await? {
            notes.push(self.row_to_note(&row)?);
        }

        Ok(notes)
    }

    pub async fn update_note(&self, id: i32, input: UpdateNote) -> Result<Option<Note>> {
        if self.get_note(id).await?.is_none() {
            return Ok(None);
        }

        let query = r#"
            UPDATE notes 
            SET content = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?
        "#;

        self.conn
            .execute(query, libsql::params![input.content, id])
            .await?;
        self.get_note(id).await
    }

    pub async fn delete_note(&self, id: i32) -> Result<bool> {
        let result = self
            .conn
            .execute("DELETE FROM notes WHERE id = ?", libsql::params![id])
            .await?;
        Ok(result > 0)
    }

    fn row_to_note(&self, row: &libsql::Row) -> Result<Note> {
        Ok(Note {
            id: row.get(0)?,
            resource_id: row.get(1)?,
            content: row.get(2)?,
            external_id: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    }

    pub async fn create_word(&self, input: CreateWord) -> Result<Word> {
        let query = r#"
            INSERT INTO words (resource_id, name, meaning)
            VALUES (?, ?, ?)
            RETURNING id, resource_id, name, meaning, created_at, updated_at
        "#;

        let mut rows = self
            .conn
            .query(
                query,
                libsql::params![input.resource_id, input.name, input.meaning],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(self.row_to_word(&row)?)
        } else {
            anyhow::bail!("Failed to create word")
        }
    }

    pub async fn get_word(&self, id: i32) -> Result<Option<Word>> {
        let query = r#"
            SELECT id, resource_id, name, meaning, created_at, updated_at
            FROM words WHERE id = ?
        "#;

        let mut rows = self.conn.query(query, libsql::params![id]).await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(self.row_to_word(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn list_words_by_resource(&self, resource_id: i32) -> Result<Vec<Word>> {
        let query = r#"
            SELECT id, resource_id, name, meaning, created_at, updated_at
            FROM words
            WHERE resource_id = ?
            ORDER BY name ASC
        "#;

        let mut rows = self.conn.query(query, libsql::params![resource_id]).await?;
        let mut words = Vec::new();

        while let Some(row) = rows.next().await? {
            words.push(self.row_to_word(&row)?);
        }

        Ok(words)
    }

    pub async fn search_words(&self, query_str: &str) -> Result<Vec<Word>> {
        let query = r#"
            SELECT id, resource_id, name, meaning, created_at, updated_at
            FROM words
            WHERE name LIKE ? OR meaning LIKE ?
            ORDER BY name ASC
        "#;

        let pattern = format!("%{}%", query_str);
        let mut rows = self
            .conn
            .query(query, libsql::params![pattern.clone(), pattern])
            .await?;
        let mut words = Vec::new();

        while let Some(row) = rows.next().await? {
            words.push(self.row_to_word(&row)?);
        }

        Ok(words)
    }

    pub async fn update_word(&self, id: i32, input: UpdateWord) -> Result<Option<Word>> {
        if self.get_word(id).await?.is_none() {
            return Ok(None);
        }

        let mut updates = Vec::new();
        let mut params: Vec<libsql::Value> = Vec::new();

        if let Some(name) = &input.name {
            updates.push("name = ?");
            params.push(name.clone().into());
        }
        if let Some(meaning) = &input.meaning {
            updates.push("meaning = ?");
            params.push(meaning.clone().into());
        }

        if updates.is_empty() {
            return self.get_word(id).await;
        }

        updates.push("updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')");
        params.push(id.into());

        let query = format!("UPDATE words SET {} WHERE id = ?", updates.join(", "));

        self.conn.execute(&query, params).await?;
        self.get_word(id).await
    }

    pub async fn delete_word(&self, id: i32) -> Result<bool> {
        let result = self
            .conn
            .execute("DELETE FROM words WHERE id = ?", libsql::params![id])
            .await?;
        Ok(result > 0)
    }

    fn row_to_word(&self, row: &libsql::Row) -> Result<Word> {
        Ok(Word {
            id: row.get(0)?,
            resource_id: row.get(1)?,
            name: row.get(2)?,
            meaning: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    }

    pub async fn get_resource_full(&self, id: i32) -> Result<Option<ResourceFull>> {
        let resource = match self.get_resource(id).await? {
            Some(r) => r,
            None => return Ok(None),
        };

        let annotations = self.list_annotations_by_resource(id).await?;
        let notes = self.list_notes_by_resource(id).await?;
        let words = self.list_words_by_resource(id).await?;

        let mut annotations_with_comments = Vec::new();
        for annotation in annotations {
            let comments = self.list_comments_by_annotation(annotation.id).await?;
            annotations_with_comments.push(AnnotationWithComments {
                annotation,
                comments,
            });
        }

        Ok(Some(ResourceFull {
            resource,
            annotations: annotations_with_comments,
            notes,
            words,
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationWithComments {
    #[serde(flatten)]
    pub annotation: Annotation,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceFull {
    #[serde(flatten)]
    pub resource: Resource,
    pub annotations: Vec<AnnotationWithComments>,
    pub notes: Vec<Note>,
    pub words: Vec<Word>,
}
