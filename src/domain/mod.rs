//! Domain types and rules. Pure: no I/O and no async.

mod document;
mod json_path;
mod name;

pub use document::{ContentType, Document};
pub use json_path::{JsonPath, JsonPathError};
pub use name::{DbName, DocName, NameError, Username};
