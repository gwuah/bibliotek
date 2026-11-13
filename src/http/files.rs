use std::path::Path;

use axum::body::Body;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use tokio_util::io::ReaderStream;

use crate::blob::BlobError;
use crate::error::Error;

pub(super) async fn stream(path: &Path, content_type: &str, disposition: String) -> Result<Response, Error> {
    let file = tokio::fs::File::open(path).await.map_err(local_file_error)?;
    let length = file.metadata().await.map_err(local_file_error)?.len();
    let headers = [
        (header::CONTENT_TYPE, content_type.to_string()),
        (header::CONTENT_LENGTH, length.to_string()),
        (header::CONTENT_DISPOSITION, disposition),
    ];
    Ok((headers, Body::from_stream(ReaderStream::new(file))).into_response())
}

pub(super) fn inline(filename: &str) -> String {
    disposition("inline", filename)
}

pub(super) fn attachment(filename: &str) -> String {
    disposition("attachment", filename)
}

fn disposition(kind: &str, filename: &str) -> String {
    let ascii: String = filename
        .chars()
        .map(|c| match c {
            ' ' => ' ',
            '"' | '\\' => '_',
            c if c.is_ascii_graphic() => c,
            _ => '_',
        })
        .collect();
    let encoded: String = filename
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect();
    format!("{kind}; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

fn local_file_error(error: std::io::Error) -> Error {
    match error.kind() {
        std::io::ErrorKind::NotFound => BlobError::ObjectNotFound.into(),
        _ => BlobError::Provider(error.into()).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{attachment, inline};

    #[test]
    fn dispositions_survive_quotes_and_non_ascii_names() {
        assert_eq!(
            inline("Weep Not, \"Child\".pdf"),
            "inline; filename=\"Weep Not, _Child_.pdf\"; filename*=UTF-8''Weep%20Not%2C%20%22Child%22.pdf"
        );
        assert_eq!(
            inline("Ngũgĩ.pdf"),
            "inline; filename=\"Ng_g_.pdf\"; filename*=UTF-8''Ng%C5%A9g%C4%A9.pdf"
        );
        assert_eq!(
            attachment("liotek-manual-20261113T020304Z.sqlite"),
            "attachment; filename=\"liotek-manual-20261113T020304Z.sqlite\"; \
             filename*=UTF-8''liotek-manual-20261113T020304Z.sqlite"
        );
    }
}
