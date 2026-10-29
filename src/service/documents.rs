//! Documents: validated bodies stored in a database.

use super::{Caller, ServiceError};
use crate::domain::{ContentType, DbName, DocName, Document, Username};
use crate::storage::FsStore;

/// Whether a write created a new document or replaced an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    Created,
    Replaced,
}

pub struct Documents {
    store: FsStore,
}

impl Documents {
    pub fn new(store: FsStore) -> Self {
        Self { store }
    }

    /// Lists the documents in a database. Public.
    pub async fn list(&self, user: &Username, db: &DbName) -> Result<Vec<DocName>, ServiceError> {
        Ok(self.store.list_docs(user, db).await?)
    }

    /// Reads a document. Public.
    pub async fn read(
        &self,
        user: &Username,
        db: &DbName,
        doc: &DocName,
    ) -> Result<Document, ServiceError> {
        Ok(self.store.read_doc(user, db, doc).await?)
    }

    /// Stores a document under a chosen name, replacing any existing one.
    /// The owner or the admin.
    pub async fn put(
        &self,
        caller: &Caller,
        user: &Username,
        db: &DbName,
        doc: &DocName,
        document: &Document,
    ) -> Result<Written, ServiceError> {
        caller.require_owner_or_admin(user)?;
        validate(document)?;
        let replaced = self.store.write_doc(user, db, doc, document).await?;
        Ok(match replaced {
            Some(_) => Written::Replaced,
            None => Written::Created,
        })
    }

    /// Stores a document under a generated, time-ordered name and returns
    /// that name. The owner or the admin.
    pub async fn push(
        &self,
        caller: &Caller,
        user: &Username,
        db: &DbName,
        document: &Document,
    ) -> Result<DocName, ServiceError> {
        caller.require_owner_or_admin(user)?;
        validate(document)?;
        let doc: DocName = ulid::Ulid::generate()
            .to_string()
            .parse()
            .expect("a ULID is a valid document name");
        self.store.write_doc(user, db, &doc, document).await?;
        Ok(doc)
    }

    /// Deletes a document. The owner or the admin.
    pub async fn delete(
        &self,
        caller: &Caller,
        user: &Username,
        db: &DbName,
        doc: &DocName,
    ) -> Result<(), ServiceError> {
        caller.require_owner_or_admin(user)?;
        Ok(self.store.delete_doc(user, db, doc).await?)
    }
}

/// Checks that a body matches its declared content type.
fn validate(document: &Document) -> Result<(), ServiceError> {
    match document.content_type {
        ContentType::Json => serde_json::from_slice::<serde::de::IgnoredAny>(&document.body)
            .map(|_| ())
            .map_err(|error| {
                ServiceError::InvalidInput(format!("body is not valid JSON: {error}"))
            }),
        ContentType::Text => std::str::from_utf8(&document.body)
            .map(|_| ())
            .map_err(|_| ServiceError::InvalidInput("text body is not valid UTF-8".into())),
        ContentType::Binary => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    fn document(content_type: ContentType, body: &'static [u8]) -> Document {
        Document {
            content_type,
            body: Bytes::from_static(body),
        }
    }

    #[test]
    fn json_must_parse() {
        assert!(validate(&document(ContentType::Json, br#"{"a": [1, null]}"#)).is_ok());
        assert!(validate(&document(ContentType::Json, b"\"just a string\"")).is_ok());
        assert!(validate(&document(ContentType::Json, b"{")).is_err());
        assert!(validate(&document(ContentType::Json, b"")).is_err());
    }

    #[test]
    fn text_must_be_utf8_and_binary_is_anything() {
        assert!(validate(&document(ContentType::Text, "héllo".as_bytes())).is_ok());
        assert!(validate(&document(ContentType::Text, b"\xff")).is_err());
        assert!(validate(&document(ContentType::Binary, b"\xff\x00")).is_ok());
    }
}
