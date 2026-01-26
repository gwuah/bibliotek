/// Parse comma-separated keywords into a vector of lowercase strings
pub fn parse_keywords(keywords: &str) -> Vec<String> {
    keywords
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Infer a category from PDF subject and keywords metadata
pub fn infer_category_from_metadata(subject: Option<&str>, keywords: Option<&str>) -> Option<String> {
    let text = format!("{} {}", subject.unwrap_or(""), keywords.unwrap_or("")).to_lowercase();

    if text.contains("mathematics") || text.contains("math") {
        Some("mathematics".to_string())
    } else if text.contains("physics") {
        Some("physics".to_string())
    } else if text.contains("biology") {
        Some("biology".to_string())
    } else if text.contains("chemistry") {
        Some("chemistry".to_string())
    } else if text.contains("computer") || text.contains("programming") {
        Some("computers".to_string())
    } else if text.contains("philosophy") {
        Some("philosophy".to_string())
    } else if text.contains("literature") || text.contains("fiction") {
        Some("literature".to_string())
    } else if text.contains("history") {
        Some("history".to_string())
    } else if text.contains("economics") {
        Some("economics".to_string())
    } else if text.contains("psychology") {
        Some("psychology".to_string())
    } else {
        None
    }
}
