//! Domain types and rules. Pure: no I/O and no async.

mod document;
mod name;

pub use document::{ContentType, Document};
pub use name::{DbName, DocName, NameError, Username};
