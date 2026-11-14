use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Serialize;

const BOOK_DIR: &str = "book";
const BOOK_PAGE: &str = "book.html";
const BOOK_METADATA: &str = "book.toml";
const SHARED_DIR: &str = "reader";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Book {
    pub slug: String,
    pub title: String,
    pub author: String,
    pub translator: String,
    pub publisher: String,
    pub year: String,
    pub url: String,
}

pub fn page_path(slug: &str) -> String {
    format!("/{SHARED_DIR}/{slug}/{BOOK_DIR}/{BOOK_PAGE}")
}

pub async fn catalog(books_dir: &Path) -> anyhow::Result<Vec<Book>> {
    let mut entries = tokio::fs::read_dir(books_dir)
        .await
        .with_context(|| format!("cannot list {}", books_dir.display()))?;
    let mut books = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let Some(slug) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !is_book_slug(&slug) || !is_file(&entry.path().join(BOOK_DIR).join(BOOK_PAGE)).await {
            continue;
        }
        match read_book(&entry.path(), slug).await {
            Ok(book) => books.push(book),
            Err(error) => tracing::warn!(error = format!("{error:#}"), "skipping a reader book"),
        }
    }
    books.sort_by_cached_key(|book| {
        (
            surname(&book.author).to_lowercase(),
            book.author.clone(),
            book.title.clone(),
        )
    });
    Ok(books)
}

fn surname(author: &str) -> &str {
    author.split_whitespace().last().unwrap_or_default()
}

async fn read_book(folder: &Path, slug: String) -> anyhow::Result<Book> {
    let path = folder.join(BOOK_METADATA);
    let text = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("cannot read {}", path.display()))?;
    let table: toml::Table = text
        .parse()
        .with_context(|| format!("{} is not valid TOML", path.display()))?;
    let title = text_field(&table, "title");
    anyhow::ensure!(!title.is_empty(), "{} has no title", path.display());
    Ok(Book {
        url: page_path(&slug),
        slug,
        title,
        author: text_field(&table, "author"),
        translator: text_field(&table, "translator"),
        publisher: text_field(&table, "publisher"),
        year: text_field(&table, "year"),
    })
}

fn text_field(table: &toml::Table, name: &str) -> String {
    match table.get(name) {
        Some(toml::Value::String(text)) => text.trim().to_string(),
        Some(toml::Value::Integer(number)) => number.to_string(),
        _ => String::new(),
    }
}

fn is_book_slug(name: &str) -> bool {
    name != SHARED_DIR && !name.starts_with('.')
}

async fn is_file(path: &Path) -> bool {
    tokio::fs::metadata(path).await.is_ok_and(|meta| meta.is_file())
}

pub async fn servable_file(books_dir: &Path, request_path: &str) -> Option<PathBuf> {
    let segments: Vec<&str> = request_path.split('/').collect();
    if !segments.iter().all(|segment| is_plain_segment(segment)) || !is_public(&segments) {
        return None;
    }
    let root = tokio::fs::canonicalize(books_dir).await.ok()?;
    let file = tokio::fs::canonicalize(root.join(segments.join("/")))
        .await
        .ok()?;
    (file.starts_with(&root) && is_file(&file).await).then_some(file)
}

fn is_plain_segment(segment: &str) -> bool {
    !segment.is_empty() && !segment.starts_with('.') && !segment.contains('\\')
}

fn is_public(segments: &[&str]) -> bool {
    matches!(segments, [SHARED_DIR, _, ..] | [_, BOOK_DIR, _, ..])
}

#[cfg(test)]
mod tests {
    use super::is_public;

    #[test]
    fn only_the_shared_reader_and_built_books_are_public() {
        assert!(is_public(&["reader", "fit.js"]));
        assert!(is_public(&["reader", "fonts", "EBGaramond-normal.woff2"]));
        assert!(is_public(&["quine-two-dogmas", "book", "book.html"]));
        assert!(!is_public(&["quine-two-dogmas", "source.pdf"]));
        assert!(!is_public(&["quine-two-dogmas", "book.toml"]));
        assert!(!is_public(&["quine-two-dogmas", "work", "report.json"]));
        assert!(!is_public(&["quine-two-dogmas", "book"]));
        assert!(!is_public(&["reader"]));
    }
}
