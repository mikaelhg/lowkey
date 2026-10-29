//! Documents: validated bodies stored in a database.

use std::sync::Arc;

use serde_json::Value;

use super::{Caller, Quotas, ServiceError};
use crate::domain::{ContentType, DbName, DocName, Document, JsonPath, Username};
use crate::storage::{FsStore, StorageError, Usage};

/// Whether a write created a new document or replaced an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    Created,
    Replaced,
}

pub struct Documents {
    store: FsStore,
    quotas: Arc<Quotas>,
}

impl Documents {
    pub fn new(store: FsStore, quotas: Arc<Quotas>) -> Self {
        Self { store, quotas }
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

    /// Reads the value at `path` inside a JSON document. Public.
    pub async fn read_value(
        &self,
        user: &Username,
        db: &DbName,
        doc: &DocName,
        path: &JsonPath,
    ) -> Result<Value, ServiceError> {
        let document = self.store.read_doc(user, db, doc).await?;
        if document.content_type != ContentType::Json {
            return Err(ServiceError::NotJson);
        }
        // Bodies are validated on write, so a parse failure means corruption.
        let value: Value = serde_json::from_slice(&document.body).map_err(|error| {
            ServiceError::Storage(StorageError::Corrupt {
                path: format!("{user}/{db}/{doc}").into(),
                reason: error.to_string(),
            })
        })?;
        path.lookup(&value).cloned().ok_or(ServiceError::NotFound)
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

        let mut usage = self.quotas.lock(user).await;
        let new_doc = doc_usage(document.body.len() as u64);
        let (written, after) = match self.store.doc_size(user, db, doc).await? {
            Some(old_size) => (
                Written::Replaced,
                usage.minus(doc_usage(old_size)).plus(new_doc),
            ),
            None => (Written::Created, usage.plus(new_doc)),
        };
        self.quotas.check(*usage, after)?;
        self.store.write_doc(user, db, doc, document).await?;
        *usage = after;
        Ok(written)
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

        let mut usage = self.quotas.lock(user).await;
        let after = usage.plus(doc_usage(document.body.len() as u64));
        self.quotas.check(*usage, after)?;
        self.store.write_doc(user, db, &doc, document).await?;
        *usage = after;
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
        let mut usage = self.quotas.lock(user).await;
        let size = self
            .store
            .doc_size(user, db, doc)
            .await?
            .ok_or(ServiceError::NotFound)?;
        self.store.delete_doc(user, db, doc).await?;
        *usage = usage.minus(doc_usage(size));
        Ok(())
    }
}

/// What a single document of `bytes` bytes counts against a quota.
fn doc_usage(bytes: u64) -> Usage {
    Usage {
        documents: 1,
        bytes,
        ..Usage::default()
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
