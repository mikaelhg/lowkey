//! Storage layer: persists users, databases and documents as plain files.
//!
//! It knows the directory layout and how to write files safely, and nothing
//! about permissions, quotas or content validation.
//!
//! ```text
//! <root>/<user>/user.json         account record
//! <root>/<user>/<db>/             database directory
//! <root>/<user>/<db>/<doc>        content type line, then the raw body
//! ```
//!
//! Every write goes to a temporary file that is synced and renamed into
//! place, so a crash never leaves a half-written file. Temporary files and
//! pending deletions start with `.`, which no valid name can, and are cleaned
//! up when the store is opened.

mod error;
mod fs;

use serde::{Deserialize, Serialize};

pub use error::StorageError;
pub use fs::FsStore;

/// What is stored about a user account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserRecord {
    pub password_hash: String,
}

/// How much a user currently stores. `bytes` counts document bodies only.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub databases: u64,
    pub documents: u64,
    pub bytes: u64,
}
