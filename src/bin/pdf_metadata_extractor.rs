use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;
use serde::Serialize;
use std::path::Path;

#[derive(Parser)]
#[command(name = "pdf_metadata_extractor")]
#[command(about = "Extract metadata from PDF files")]
struct Cli {
    pdf_path: String,
    #[arg(short, long)]
    pretty: bool,
}

#[derive(Debug, Serialize)]
struct PdfMetadata {
    filename: String,
    title: Option<String>,
    author: Option<String>,
    subject: Option<String>,
    keywords: Option<String>,
    creator: Option<String>,
    producer: Option<String>,
    creation_date: Option<String>,
    modification_date: Option<String>,
}

impl Default for PdfMetadata {
    fn default() -> Self {
        Self {
            filename: String::new(),
            title: None,
            author: None,
            subject: None,
            keywords: None,
            creator: None,
            producer: None,
            creation_date: None,
            modification_date: None,
        }
    }
}

fn parse_pdf_date(pdf_date: &str) -> Option<String> {
    if pdf_date.is_empty() {
        return None;
    }

    let date_str = if pdf_date.starts_with("D:") {
        &pdf_date[2..]
    } else {
        pdf_date
    };

    let formats = [("%Y%m%d%H%M%S", 14), ("%Y%m%d%H%M", 12), ("%Y%m%d", 8)];

    for (format, required_len) in &formats {
        if date_str.len() >= *required_len {
            let slice = &date_str[..*required_len];
            let dt_result = if *format == "%Y%m%d" {
                chrono::NaiveDate::parse_from_str(slice, "%Y%m%d")
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
            } else {
                NaiveDateTime::parse_from_str(slice, format)
            };

            if let Ok(dt) = dt_result {
                let utc_dt = DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
                return Some(utc_dt.format("%Y-%m-%d %H:%M:%S UTC").to_string());
            }
        }
    }

    Some(pdf_date.to_string())
}

fn extract_metadata(path: &Path) -> Result<PdfMetadata> {
    let doc = lopdf::Document::load(path)
        .with_context(|| format!("Failed to load PDF: {}", path.display()))?;

    let mut metadata = PdfMetadata {
        filename: path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string(),
        ..Default::default()
    };

    if let Ok(info_ref) = doc.trailer.get(b"Info") {
        if let Ok(info_obj) = doc.get_object(info_ref.as_reference()?) {
            if let lopdf::Object::Dictionary(info_dict) = info_obj {
                let extract_string = |obj: &lopdf::Object| -> Option<String> {
                    let actual_obj = if let Ok(obj_ref) = obj.as_reference() {
                        doc.get_object(obj_ref).ok()?
                    } else {
                        obj
                    };

                    match actual_obj {
                        lopdf::Object::String(bytes, _) => String::from_utf8(bytes.clone()).ok(),
                        _ => None,
                    }
                };

                if let Ok(title) = info_dict.get(b"Title") {
                    metadata.title = extract_string(title);
                }

                if let Ok(author) = info_dict.get(b"Author") {
                    metadata.author = extract_string(author);
                }

                if let Ok(subject) = info_dict.get(b"Subject") {
                    metadata.subject = extract_string(subject);
                }

                if let Ok(keywords) = info_dict.get(b"Keywords") {
                    metadata.keywords = extract_string(keywords);
                }

                if let Ok(creator) = info_dict.get(b"Creator") {
                    metadata.creator = extract_string(creator);
                }

                if let Ok(producer) = info_dict.get(b"Producer") {
                    metadata.producer = extract_string(producer);
                }

                if let Ok(creation_date) = info_dict.get(b"CreationDate") {
                    if let Some(date_str) = extract_string(creation_date) {
                        metadata.creation_date = parse_pdf_date(&date_str);
                    }
                }

                if let Ok(mod_date) = info_dict.get(b"ModDate") {
                    if let Some(date_str) = extract_string(mod_date) {
                        metadata.modification_date = parse_pdf_date(&date_str);
                    }
                }
            }
        }
    }

    Ok(metadata)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let path = Path::new(&cli.pdf_path);

    if !path.exists() {
        anyhow::bail!("File does not exist: {}", path.display());
    }

    if !path.is_file() {
        anyhow::bail!("Path is not a file: {}", path.display());
    }

    let metadata = extract_metadata(path)?;

    if cli.pretty {
        println!("{}", serde_json::to_string_pretty(&metadata)?);
    } else {
        println!("{}", serde_json::to_string(&metadata)?);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pdf_date() {
        assert_eq!(
            parse_pdf_date("D:20231025143022"),
            Some("2023-10-25 14:30:22 UTC".to_string())
        );

        assert_eq!(
            parse_pdf_date("D:20231025"),
            Some("2023-10-25 00:00:00 UTC".to_string())
        );

        assert_eq!(parse_pdf_date(""), None);

        assert_eq!(
            parse_pdf_date("invalid_date"),
            Some("invalid_date".to_string())
        );
    }
}
