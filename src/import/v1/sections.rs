//! Bibliotek stored chapters as JSON in `resources.config`, in one of two shapes:
//!
//! * current: `{"chapters": {"<n>": [title, start_page]}}`. Pages are PDF pages; a
//!   chapter ends where the next begins and the last one runs to the end.
//! * earlier: `{"chapters": {"<n>": [title, start, end]}, "page_offset": k}`. Pages are
//!   printed pages, and PDF page = printed page + `k`.
//!
//! Bibliotek recorded no page count, so the last chapter of the current shape ends at
//! the last page any of the document's highlights is on, or at its own start page.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedSection {
    pub title: String,
    pub start_page: i64,
    pub end_page: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChapterPlan {
    pub page_offset: i64,
    pub chapters_read: usize,
    pub sections: Vec<PlannedSection>,
    pub problems: Vec<String>,
}

pub fn plan(config: &str, last_highlighted_page: Option<i64>) -> Result<ChapterPlan, String> {
    let config: Value =
        serde_json::from_str(config).map_err(|error| format!("config is not JSON: {error}"))?;
    let page_offset = match config.get("page_offset") {
        None | Some(Value::Null) => 0,
        Some(offset) => offset
            .as_i64()
            .ok_or_else(|| format!("page_offset `{offset}` is not an integer"))?,
    };
    let chapters = match config.get("chapters") {
        None | Some(Value::Null) => return Ok(ChapterPlan::default()),
        Some(Value::Object(chapters)) => chapters,
        Some(other) => return Err(format!("chapters is `{other}`, not an object")),
    };

    let mut plan = ChapterPlan {
        page_offset,
        chapters_read: chapters.len(),
        ..ChapterPlan::default()
    };
    let mut parsed = Vec::new();
    for (key, chapter) in chapters {
        match parse_chapter(chapter) {
            Some(chapter) => parsed.push((key.parse::<i64>().unwrap_or(i64::MAX), chapter)),
            None => plan
                .problems
                .push(format!("chapter `{key}` is not [title, start(, end)]: {chapter}")),
        }
    }
    parsed.sort_by_key(|(key, chapter)| (chapter.start, *key));

    for (index, (key, chapter)) in parsed.iter().enumerate() {
        let (start_page, end_page) = match chapter.end {
            Some(end) => (chapter.start + page_offset, end + page_offset),
            None => {
                let end = match parsed.get(index + 1) {
                    Some((_, next)) => next.start - 1,
                    None => last_highlighted_page.unwrap_or(chapter.start).max(chapter.start),
                };
                (chapter.start, end)
            }
        };
        if start_page < 1 || end_page < start_page {
            plan.problems.push(format!(
                "chapter `{key}` ({}) would span pages {start_page}-{end_page}",
                chapter.title
            ));
            continue;
        }
        if let Some(previous) = plan.sections.last()
            && previous.end_page >= start_page
        {
            plan.problems.push(format!(
                "chapter `{key}` ({}) overlaps the chapter before it",
                chapter.title
            ));
            continue;
        }
        plan.sections.push(PlannedSection {
            title: chapter.title.clone(),
            start_page,
            end_page,
        });
    }
    Ok(plan)
}

struct Chapter {
    title: String,
    start: i64,
    end: Option<i64>,
}

fn parse_chapter(value: &Value) -> Option<Chapter> {
    let parts = value.as_array()?;
    let title = parts.first()?.as_str()?.to_string();
    let start = parts.get(1)?.as_i64()?;
    let end = match parts.len() {
        2 => None,
        3 => Some(parts[2].as_i64()?),
        _ => return None,
    };
    Some(Chapter { title, start, end })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(title: &str, start_page: i64, end_page: i64) -> PlannedSection {
        PlannedSection {
            title: title.to_string(),
            start_page,
            end_page,
        }
    }

    #[test]
    fn current_shape_chapters_end_where_the_next_begins() {
        let plan = plan(r#"{"chapters": {"2": ["Two", 10], "1": ["One", 3]}}"#, Some(42)).unwrap();
        assert_eq!(plan.sections, vec![section("One", 3, 9), section("Two", 10, 42)]);
        assert_eq!(plan.page_offset, 0);
        assert!(plan.problems.is_empty());
    }

    #[test]
    fn earlier_shape_is_shifted_into_pdf_pages() {
        let plan = plan(
            r#"{"chapters": {"1": ["One", 1, 5], "2": ["Two", 6, 9]}, "page_offset": 14}"#,
            None,
        )
        .unwrap();
        assert_eq!(
            plan.sections,
            vec![section("One", 15, 19), section("Two", 20, 23)]
        );
        assert_eq!(plan.page_offset, 14);
    }

    #[test]
    fn invalid_chapters_are_reported_not_written() {
        let plan = plan(r#"{"chapters": {"1": ["A", 5], "2": ["B", 5], "3": "x"}}"#, None).unwrap();
        assert_eq!(plan.sections, vec![section("B", 5, 5)]);
        assert_eq!(plan.problems.len(), 2);
    }
}
