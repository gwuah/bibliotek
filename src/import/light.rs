//! A payload is always the installation's complete state. It admits no other top-level
//! field (in particular no `scope`), so a partial sync cannot be mistaken for a
//! complete one and delete what it left out.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::json;

use super::snapshot::{Location, Snapshot, SourceComment, SourceDocument, SourceHighlight, SourceTimes};
use crate::domain::documents::{DocumentKind, source};
use crate::error::{Error, Result};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightPayload {
    pub highlights: BTreeMap<String, Vec<LightHighlight>>,
}

#[derive(Debug, Deserialize)]
pub struct LightHighlight {
    #[serde(rename = "groupID")]
    pub group_id: i64,
    pub repr: String,
    pub chunks: Vec<String>,
    pub url: String,
    pub date: String,
    #[serde(default)]
    pub comments: Vec<LightComment>,
}

#[derive(Debug, Deserialize)]
pub struct LightComment {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
}

pub const LOCATION_KIND: &str = "web_text";

pub fn location(url: &str, chunks: &[String]) -> Location {
    Location::new(LOCATION_KIND, json!({ "url": url, "chunks": chunks }))
}

pub fn validate_installation(installation: &str) -> Result<&str> {
    let well_formed = !installation.is_empty()
        && installation.len() <= 128
        && installation
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if well_formed {
        Ok(installation)
    } else {
        Err(Error::invalid(
            "invalid_light_installation",
            "a Light installation ID is 1-128 letters, digits, `-` or `_`",
        ))
    }
}

pub fn snapshot(installation: &str, payload: LightPayload) -> Result<Snapshot> {
    let installation = validate_installation(installation)?;
    let documents = payload
        .highlights
        .into_iter()
        .map(|(url, highlights)| page(url, highlights))
        .collect::<Result<Vec<_>>>()?;
    Snapshot::new(source::light(installation), documents)
}

pub fn document(url: &str) -> Result<SourceDocument> {
    let url = url.trim();
    if url.is_empty() {
        return Err(Error::invalid("invalid_light_snapshot", "a page URL is blank"));
    }
    Ok(SourceDocument {
        external_id: url.to_string(),
        kind: DocumentKind::Web,
        title: url.to_string(),
        canonical_url: Some(url.to_string()),
        highlights: Vec::new(),
        notes: Vec::new(),
    })
}

fn page(url: String, highlights: Vec<LightHighlight>) -> Result<SourceDocument> {
    Ok(SourceDocument {
        highlights: highlights.into_iter().map(highlight).collect::<Result<_>>()?,
        ..document(&url)?
    })
}

fn highlight(highlight: LightHighlight) -> Result<SourceHighlight> {
    Ok(SourceHighlight {
        external_id: highlight.group_id.to_string(),
        location: location(&highlight.url, &highlight.chunks),
        text: highlight.repr,
        color: None,
        times: SourceTimes {
            created_at: Some(highlight.date),
            updated_at: None,
        },
        comments: highlight
            .comments
            .into_iter()
            .map(comment)
            .collect::<Result<_>>()?,
    })
}

fn comment(comment: LightComment) -> Result<SourceComment> {
    if comment.text.trim().is_empty() {
        return Err(Error::invalid(
            "invalid_light_snapshot",
            format!("comment `{}` has no text", comment.id),
        ));
    }
    Ok(SourceComment {
        external_id: comment.id,
        body: comment.text.trim().to_string(),
        times: SourceTimes {
            created_at: comment.created,
            updated_at: comment.updated,
        },
    })
}
