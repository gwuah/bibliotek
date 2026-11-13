use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use super::report::{MigrationReport, TableCounts};
use super::sections::{self, ChapterPlan};
use super::source::{Annotation, Resource, Text, V1};
use crate::domain::documents::source;
use crate::error::Result;
use crate::import::snapshot::{
    Location, Snapshot, SourceComment, SourceDocument, SourceHighlight, SourceNote, SourceTimes,
};
use crate::import::{light, research};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Research(String),
    Web(String),
    Local,
}

pub struct ResourcePlan<'a> {
    pub resource: &'a Resource,
    pub owner: Owner,
    pub chapters: Option<ChapterPlan>,
}

#[derive(Debug, Clone)]
pub struct Address {
    pub source: String,
    pub document: String,
    pub external_id: String,
}

pub struct HighlightRoute {
    pub annotation_id: i64,
    pub resource_id: i64,
    pub address: Address,
}

pub enum CommentRoute<'a> {
    Source {
        comment_id: i64,
        annotation_id: i64,
        external_id: String,
    },
    Local {
        comment: &'a Text,
    },
}

pub enum NoteRoute<'a> {
    Source {
        note_id: i64,
        resource_id: i64,
        address: Address,
    },
    Local {
        note: &'a Text,
    },
}

pub struct Plan<'a> {
    pub snapshots: Vec<Snapshot>,
    pub resources: Vec<ResourcePlan<'a>>,
    pub highlights: Vec<HighlightRoute>,
    pub comments: Vec<CommentRoute<'a>>,
    pub notes: Vec<NoteRoute<'a>>,
}

pub fn plan<'a>(v1: &'a V1, report: &mut MigrationReport) -> Result<Plan<'a>> {
    let mut planner = Planner {
        resources: v1
            .resources
            .iter()
            .map(|resource| (resource.id, resource))
            .collect(),
        annotations: v1
            .annotations
            .iter()
            .map(|annotation| (annotation.id, annotation))
            .collect(),
        highlights: BTreeMap::new(),
        research_notes: HashMap::new(),
        report,
    };

    let highlights = planner.classify_annotations(&v1.annotations);
    let comments = planner.classify_comments(&v1.comments);
    let notes = planner.classify_notes(&v1.notes);
    let resources = planner.classify_resources(&v1.resources, &highlights, &notes);
    let snapshots = planner.snapshots(&resources)?;

    Ok(Plan {
        snapshots,
        resources,
        highlights,
        comments,
        notes,
    })
}

struct Planner<'a, 'r> {
    resources: HashMap<i64, &'a Resource>,
    annotations: HashMap<i64, &'a Annotation>,
    highlights: BTreeMap<i64, (Address, SourceHighlight)>,
    research_notes: HashMap<i64, Vec<SourceNote>>,
    report: &'r mut MigrationReport,
}

impl<'a> Planner<'a, '_> {
    fn counts(&mut self, table: &'static str) -> &mut TableCounts {
        self.report.tables.entry(table).or_default()
    }

    fn fail(&mut self, table: &'static str, id: i64, why: impl std::fmt::Display) {
        self.counts(table).failed += 1;
        self.report
            .unexplained_losses
            .push(format!("{table} #{id}: {why}"));
    }

    fn owner(resource: &Resource) -> Owner {
        match resource
            .external_id
            .as_deref()
            .and_then(|id| id.strip_prefix("research:"))
        {
            Some(item_id) => Owner::Research(item_id.to_string()),
            None if resource.kind == "website" => Owner::Web(resource.title.trim().to_string()),
            None => Owner::Local,
        }
    }

    fn live_resource(&self, resource_id: i64) -> std::result::Result<Option<&'a Resource>, String> {
        match self.resources.get(&resource_id) {
            None => Err(format!("its resource #{resource_id} does not exist")),
            Some(resource) if resource.deleted => Ok(None),
            Some(resource) => Ok(Some(resource)),
        }
    }

    fn classify_annotations(&mut self, annotations: &'a [Annotation]) -> Vec<HighlightRoute> {
        let mut routes = Vec::new();
        for annotation in annotations {
            self.counts("annotations").read += 1;
            let resource = match self.live_resource(annotation.resource_id) {
                Err(why) => {
                    self.fail("annotations", annotation.id, why);
                    continue;
                }
                Ok(None) => {
                    self.counts("annotations").already_deleted += 1;
                    continue;
                }
                Ok(Some(resource)) => resource,
            };
            if annotation.deleted {
                self.counts("annotations").already_deleted += 1;
                continue;
            }
            match self.stage_highlight(annotation, resource) {
                Ok((address, highlight)) => {
                    if let Some(installation) = address.source.strip_prefix("light:") {
                        let installations = &mut self.report.light_installations;
                        *installations.entry(installation.to_string()).or_default() += 1;
                    }
                    self.highlights
                        .insert(annotation.id, (address.clone(), highlight));
                    routes.push(HighlightRoute {
                        annotation_id: annotation.id,
                        resource_id: resource.id,
                        address,
                    });
                }
                Err(why) => self.fail("annotations", annotation.id, why),
            }
        }
        routes
    }

    fn stage_highlight(
        &self,
        annotation: &Annotation,
        resource: &Resource,
    ) -> std::result::Result<(Address, SourceHighlight), String> {
        let external_id = annotation
            .external_id
            .as_deref()
            .ok_or("it has no external ID, and a hand-written highlight has no source to belong to")?;
        let boundary: Value = match &annotation.boundary {
            Some(boundary) => serde_json::from_str(boundary)
                .map_err(|error| format!("its boundary is not JSON: {error}"))?,
            None => Value::Null,
        };
        let v1_times = SourceTimes {
            created_at: Some(annotation.created_at.clone()),
            updated_at: Some(annotation.updated_at.clone()),
        };

        match (AnnotationSource::parse(external_id), Self::owner(resource)) {
            (Some(AnnotationSource::Research(id)), Owner::Research(item_id)) => {
                let position = boundary["position"].as_str();
                let location = research::location(position).map_err(|error| error.to_string())?;
                let address = Address {
                    source: source::RESEARCH.to_string(),
                    document: item_id,
                    external_id: id,
                };
                let highlight = highlight(&address, annotation, location, annotation.color.clone(), v1_times);
                Ok((address, highlight))
            }
            (
                Some(AnnotationSource::Light {
                    installation,
                    group_id,
                }),
                Owner::Web(url),
            ) => {
                light::validate_installation(&installation).map_err(|error| error.to_string())?;
                let chunks: Vec<String> = serde_json::from_value(boundary["chunks"].clone())
                    .map_err(|_| "its Light boundary has no text chunks".to_string())?;
                let page = boundary["url"].as_str().unwrap_or(&url).to_string();
                let times = SourceTimes {
                    created_at: boundary["date"]
                        .as_str()
                        .map(str::to_string)
                        .or(v1_times.created_at),
                    updated_at: v1_times.updated_at,
                };
                let address = Address {
                    source: source::light(&installation),
                    document: url,
                    external_id: group_id,
                };
                // Bibliotek stamped every Light highlight "yellow"; Light has no colours.
                let highlight = highlight(&address, annotation, light::location(&page, &chunks), None, times);
                Ok((address, highlight))
            }
            (None, _) => Err(format!("external ID `{external_id}` names no known source")),
            (Some(_), owner) => Err(format!(
                "external ID `{external_id}` does not belong to its resource ({owner:?})"
            )),
        }
    }

    fn classify_comments(&mut self, comments: &'a [Text]) -> Vec<CommentRoute<'a>> {
        let mut routes = Vec::new();
        for comment in comments {
            self.counts("comments").read += 1;
            let Some(annotation) = self.annotations.get(&comment.parent_id).copied() else {
                self.fail(
                    "comments",
                    comment.id,
                    format!("its annotation #{} does not exist", comment.parent_id),
                );
                continue;
            };
            let parent_deleted = annotation.deleted
                || self
                    .live_resource(annotation.resource_id)
                    .is_ok_and(|resource| resource.is_none());
            if comment.deleted || parent_deleted {
                self.counts("comments").already_deleted += 1;
                continue;
            }
            let Some((_, highlight)) = self.highlights.get_mut(&annotation.id) else {
                self.fail(
                    "comments",
                    comment.id,
                    format!("its annotation #{} could not be imported", annotation.id),
                );
                continue;
            };

            let Some(external_id) = comment.external_id.as_deref() else {
                routes.push(CommentRoute::Local { comment });
                continue;
            };
            let own_id = annotation.external_id.as_deref().and_then(|annotation_id| {
                match AnnotationSource::parse(annotation_id)? {
                    AnnotationSource::Research(_) => external_id.strip_prefix("research:"),
                    AnnotationSource::Light { .. } => external_id
                        .strip_prefix(annotation_id)
                        .and_then(|rest| rest.strip_prefix(':')),
                }
            });
            match own_id.filter(|id| !id.is_empty()) {
                Some(own_id) => {
                    highlight.comments.push(SourceComment {
                        external_id: own_id.to_string(),
                        body: comment.content.clone(),
                        times: times(comment),
                    });
                    routes.push(CommentRoute::Source {
                        comment_id: comment.id,
                        annotation_id: annotation.id,
                        external_id: own_id.to_string(),
                    });
                }
                None => self.fail(
                    "comments",
                    comment.id,
                    format!("external ID `{external_id}` does not belong to its highlight's source"),
                ),
            }
        }
        routes
    }

    fn classify_notes(&mut self, notes: &'a [Text]) -> Vec<NoteRoute<'a>> {
        let mut routes = Vec::new();
        for note in notes {
            self.counts("notes").read += 1;
            let resource = match self.live_resource(note.parent_id) {
                Err(why) => {
                    self.fail("notes", note.id, why);
                    continue;
                }
                Ok(None) => {
                    self.counts("notes").already_deleted += 1;
                    continue;
                }
                Ok(Some(resource)) => resource,
            };
            if note.deleted {
                self.counts("notes").already_deleted += 1;
                continue;
            }

            let Some(external_id) = note.external_id.as_deref() else {
                routes.push(NoteRoute::Local { note });
                continue;
            };
            match (external_id.strip_prefix("research:"), Self::owner(resource)) {
                (Some(own_id), Owner::Research(item_id)) if !own_id.is_empty() => {
                    self.research_notes
                        .entry(resource.id)
                        .or_default()
                        .push(SourceNote {
                            external_id: own_id.to_string(),
                            body: note.content.clone(),
                            times: times(note),
                        });
                    routes.push(NoteRoute::Source {
                        note_id: note.id,
                        resource_id: resource.id,
                        address: Address {
                            source: source::RESEARCH.to_string(),
                            document: item_id,
                            external_id: own_id.to_string(),
                        },
                    });
                }
                _ => self.fail(
                    "notes",
                    note.id,
                    format!("external ID `{external_id}` does not belong to its resource's source"),
                ),
            }
        }
        routes
    }

    fn classify_resources(
        &mut self,
        resources: &'a [Resource],
        highlights: &[HighlightRoute],
        notes: &[NoteRoute],
    ) -> Vec<ResourcePlan<'a>> {
        let mut plans = Vec::new();
        for resource in resources {
            self.counts("resources").read += 1;
            if resource.deleted {
                self.counts("resources").already_deleted += 1;
                continue;
            }
            let owner = Self::owner(resource);
            let chapters = match self.chapters(resource, highlights) {
                Ok(chapters) => chapters,
                Err(why) => {
                    self.report.unexplained_losses.push(format!(
                        "resources #{}: chapter config not imported: {why}",
                        resource.id
                    ));
                    None
                }
            };
            let holds_something = highlights.iter().any(|route| route.resource_id == resource.id)
                || notes.iter().any(|route| match route {
                    NoteRoute::Source { resource_id, .. } => *resource_id == resource.id,
                    NoteRoute::Local { note } => note.parent_id == resource.id,
                })
                || chapters.is_some();
            if matches!(owner, Owner::Research(_)) || holds_something {
                plans.push(ResourcePlan {
                    resource,
                    owner,
                    chapters,
                });
            } else {
                self.counts("resources").skipped += 1;
            }
        }
        plans
    }

    fn chapters(
        &self,
        resource: &Resource,
        highlights: &[HighlightRoute],
    ) -> std::result::Result<Option<ChapterPlan>, String> {
        let Some(config) = resource
            .config
            .as_deref()
            .filter(|config| !config.trim().is_empty())
        else {
            return Ok(None);
        };
        let last_page = highlights
            .iter()
            .filter(|route| route.resource_id == resource.id)
            .filter_map(|route| self.annotations.get(&route.annotation_id))
            .filter_map(|annotation| annotation.boundary.as_deref())
            .filter_map(|boundary| serde_json::from_str::<Value>(boundary).ok())
            .filter_map(|boundary| boundary["pageNumber"].as_i64())
            .max();
        let plan = sections::plan(config, last_page)?;
        Ok((plan.chapters_read > 0 || plan.page_offset != 0).then_some(plan))
    }

    fn snapshots(&mut self, resources: &[ResourcePlan]) -> Result<Vec<Snapshot>> {
        let mut research_documents: BTreeMap<String, SourceDocument> = BTreeMap::new();
        for plan in resources {
            if let Owner::Research(item_id) = &plan.owner {
                let document = research_documents
                    .entry(item_id.clone())
                    .or_insert_with(|| research::document(item_id.clone(), Some(&plan.resource.title)));
                document
                    .notes
                    .extend(self.research_notes.remove(&plan.resource.id).unwrap_or_default());
            }
        }

        let mut light_documents: BTreeMap<String, BTreeMap<String, SourceDocument>> = BTreeMap::new();
        for (address, highlight) in std::mem::take(&mut self.highlights).into_values() {
            let document = if address.source == source::RESEARCH {
                research_documents
                    .get_mut(&address.document)
                    .ok_or_else(|| anyhow::anyhow!("no Research document for item {}", address.document))?
            } else {
                match light_documents
                    .entry(address.source)
                    .or_default()
                    .entry(address.document)
                {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => {
                        let page = light::document(entry.key())?;
                        entry.insert(page)
                    }
                }
            };
            document.highlights.push(highlight);
        }

        let mut snapshots = vec![Snapshot::new(
            source::RESEARCH,
            research_documents.into_values().collect(),
        )?];
        for (source, documents) in light_documents {
            snapshots.push(Snapshot::new(source, documents.into_values().collect())?);
        }
        Ok(snapshots)
    }
}

enum AnnotationSource {
    /// `research:<annotation id>`.
    Research(String),
    /// `light_<installation>:<groupID>`.
    Light { installation: String, group_id: String },
}

impl AnnotationSource {
    fn parse(external_id: &str) -> Option<AnnotationSource> {
        if let Some(id) = external_id.strip_prefix("research:") {
            return (!id.is_empty()).then(|| AnnotationSource::Research(id.to_string()));
        }
        let (source, group_id) = external_id.rsplit_once(':')?;
        let installation = source.strip_prefix("light_")?;
        (!group_id.is_empty()).then(|| AnnotationSource::Light {
            installation: installation.to_string(),
            group_id: group_id.to_string(),
        })
    }
}

fn highlight(
    address: &Address,
    annotation: &Annotation,
    location: Location,
    color: Option<String>,
    times: SourceTimes,
) -> SourceHighlight {
    SourceHighlight {
        external_id: address.external_id.clone(),
        text: annotation.text.clone(),
        color,
        location,
        times,
        comments: Vec::new(),
    }
}

fn times(text: &Text) -> SourceTimes {
    SourceTimes {
        created_at: Some(text.created_at.clone()),
        updated_at: Some(text.updated_at.clone()),
    }
}
