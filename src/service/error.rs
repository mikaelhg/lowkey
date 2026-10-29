use crate::domain::{JsonPathError, NameError};
use crate::storage::StorageError;

/// What can go wrong in the service layer, independent of any protocol.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("authentication required")]
    Unauthorized,
    #[error("not allowed")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("already exists")]
    Conflict,
    #[error("{0}")]
    InvalidInput(String),
    #[error(
        "unsupported content type; use application/json, text/plain or application/octet-stream"
    )]
    UnsupportedContentType,
    #[error("document is not JSON, so it has no keys")]
    NotJson,
    #[error("storage failure: {0}")]
    Storage(StorageError),
}

impl From<StorageError> for ServiceError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::NotFound => Self::NotFound,
            StorageError::AlreadyExists => Self::Conflict,
            other => Self::Storage(other),
        }
    }
}

impl From<NameError> for ServiceError {
    fn from(error: NameError) -> Self {
        Self::InvalidInput(error.to_string())
    }
}

impl From<JsonPathError> for ServiceError {
    fn from(error: JsonPathError) -> Self {
        Self::InvalidInput(error.to_string())
    }
}
