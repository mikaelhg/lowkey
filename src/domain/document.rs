//! Stored documents and their content types.

use bytes::Bytes;

/// The kinds of documents Lowkey stores. Only JSON documents support key
/// lookups; the others are opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Json,
    Text,
    Binary,
}

impl ContentType {
    /// Parses a `Content-Type` header value, ignoring parameters such as
    /// `charset`. Returns `None` for unsupported types.
    pub fn from_mime(value: &str) -> Option<Self> {
        let essence = value.split(';').next().unwrap_or("").trim();
        match essence.to_ascii_lowercase().as_str() {
            "application/json" | "text/json" => Some(Self::Json),
            "text/plain" => Some(Self::Text),
            "application/octet-stream" => Some(Self::Binary),
            _ => None,
        }
    }

    /// The canonical MIME type, as sent in responses.
    pub fn as_mime(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Text => "text/plain; charset=utf-8",
            Self::Binary => "application/octet-stream",
        }
    }
}

/// A document body together with its content type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub content_type: ContentType,
    pub body: Bytes,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_types_and_aliases() {
        assert_eq!(
            ContentType::from_mime("application/json"),
            Some(ContentType::Json)
        );
        assert_eq!(ContentType::from_mime("text/json"), Some(ContentType::Json));
        assert_eq!(
            ContentType::from_mime("Text/Plain; charset=utf-8"),
            Some(ContentType::Text)
        );
        assert_eq!(
            ContentType::from_mime("application/octet-stream"),
            Some(ContentType::Binary)
        );
        assert_eq!(ContentType::from_mime("image/png"), None);
    }

    #[test]
    fn canonical_mime_round_trips() {
        for ty in [ContentType::Json, ContentType::Text, ContentType::Binary] {
            assert_eq!(ContentType::from_mime(ty.as_mime()), Some(ty));
        }
    }
}
