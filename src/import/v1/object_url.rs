//! Bibliotek's `get_s3_url` built one of three shapes, percent-encoding each key
//! segment:
//!
//! ```text
//! https://<bucket>.t3.storage.dev/<key>      service "t3"
//! https://<bucket>.s3.amazonaws.com/<key>    service "s3"
//! https://<service>.storage.dev/<key>        any other service (no bucket in the URL)
//! ```

use crate::blob::ObjectKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectReference {
    pub host: String,
    pub bucket: Option<String>,
    pub key: ObjectKey,
}

impl ObjectReference {
    pub fn parse(url: &str) -> Result<ObjectReference, String> {
        let rest = url
            .strip_prefix("https://")
            .ok_or_else(|| "not an https URL".to_string())?;
        let (host, encoded_key) = rest
            .split_once('/')
            .ok_or_else(|| "the URL has no object path".to_string())?;
        if encoded_key.contains(['?', '#']) {
            return Err("the URL has a query or fragment, which Bibliotek never produced".to_string());
        }

        let bucket = if let Some(bucket) = host.strip_suffix(".t3.storage.dev") {
            Some(bucket)
        } else if let Some(bucket) = host.strip_suffix(".s3.amazonaws.com") {
            Some(bucket)
        } else if host
            .strip_suffix(".storage.dev")
            .is_some_and(|service| !service.is_empty())
        {
            None
        } else {
            return Err(format!("`{host}` is not a host Bibliotek wrote"));
        };
        if bucket.is_some_and(str::is_empty) {
            return Err("the URL names no bucket".to_string());
        }

        let key = encoded_key
            .split('/')
            .map(percent_decode)
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        let key = ObjectKey::parse(key).map_err(|error| error.to_string())?;
        Ok(ObjectReference {
            host: host.to_string(),
            bucket: bucket.map(str::to_string),
            key,
        })
    }

    /// Bibliotek keys are `<16-character signature>_<file name>`.
    pub fn original_filename(&self) -> String {
        let name = self.key.as_str().rsplit('/').next().unwrap_or_default();
        match name.split_at_checked(16) {
            Some((signature, rest))
                if signature.chars().all(|c| c.is_ascii_alphanumeric())
                    && rest.len() > 1
                    && rest.starts_with('_') =>
            {
                rest[1..].to_string()
            }
            _ => name.to_string(),
        }
    }
}

fn percent_decode(segment: &str) -> Result<String, String> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = segment
                .get(index + 1..index + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or_else(|| format!("`{segment}` has a malformed percent escape"))?;
            decoded.push(hex);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| format!("`{segment}` does not decode to UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_bibliotek_wrote_parses() {
        let t3 = ObjectReference::parse(
            "https://bibliotek.t3.storage.dev/0123456789abcdef_My%20Book%20%28v2%29.pdf",
        )
        .unwrap();
        assert_eq!(t3.bucket.as_deref(), Some("bibliotek"));
        assert_eq!(t3.key.as_str(), "0123456789abcdef_My Book (v2).pdf");
        assert_eq!(t3.original_filename(), "My Book (v2).pdf");

        let s3 = ObjectReference::parse("https://books.s3.amazonaws.com/a/b%2Fc.pdf").unwrap();
        assert_eq!(s3.bucket.as_deref(), Some("books"));
        assert_eq!(s3.key.as_str(), "a/b/c.pdf");

        let generic = ObjectReference::parse("https://minio.storage.dev/k%C3%A9y.pdf").unwrap();
        assert_eq!(generic.bucket, None);
        assert_eq!(generic.key.as_str(), "kéy.pdf");
        assert_eq!(generic.original_filename(), "kéy.pdf");
    }

    #[test]
    fn anything_else_is_reported_not_guessed() {
        for url in [
            "http://bibliotek.t3.storage.dev/a.pdf",
            "https://example.com/a.pdf",
            "https://bibliotek.t3.storage.dev/",
            "https://.t3.storage.dev/a.pdf",
            "https://bibliotek.t3.storage.dev/a%2.pdf",
            "https://bibliotek.t3.storage.dev/a.pdf?x=1",
            "https://bibliotek.t3.storage.dev/%FF.pdf",
        ] {
            assert!(ObjectReference::parse(url).is_err(), "{url} should not parse");
        }
    }
}
