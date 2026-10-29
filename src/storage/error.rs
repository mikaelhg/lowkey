use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("not found")]
    NotFound,
    #[error("already exists")]
    AlreadyExists,
    #[error("corrupt data in {path}: {reason}")]
    Corrupt { path: PathBuf, reason: String },
    #[error(transparent)]
    Io(io::Error),
}

impl From<io::Error> for StorageError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            _ => Self::Io(error),
        }
    }
}
