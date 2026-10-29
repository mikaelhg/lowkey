//! Validated names for users, databases and documents.
//!
//! A name is 1 to 64 ASCII letters, digits, `_` or `-`. Because dots and
//! slashes are impossible, every name is safe to use as a single path segment.

use std::fmt;
use std::str::FromStr;

/// Longest allowed name, in bytes.
pub const MAX_NAME_LEN: usize = 64;

/// Usernames that would clash with fixed routes or the admin account.
const RESERVED_USERNAMES: [&str; 2] = ["admin", "users"];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("name must not be empty")]
    Empty,
    #[error("name must be at most {MAX_NAME_LEN} characters")]
    TooLong,
    #[error("name may only contain letters, digits, '_' and '-'")]
    InvalidCharacter,
    #[error("name is reserved")]
    Reserved,
}

fn validate(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > MAX_NAME_LEN {
        return Err(NameError::TooLong);
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    if !name.chars().all(allowed) {
        return Err(NameError::InvalidCharacter);
    }
    Ok(())
}

/// The name of a user account.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Username(String);

impl FromStr for Username {
    type Err = NameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate(s)?;
        if RESERVED_USERNAMES.contains(&s) {
            return Err(NameError::Reserved);
        }
        Ok(Self(s.to_owned()))
    }
}

/// The name of a database owned by a user.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DbName(String);

impl FromStr for DbName {
    type Err = NameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate(s)?;
        Ok(Self(s.to_owned()))
    }
}

/// The name of a document inside a database.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocName(String);

impl FromStr for DocName {
    type Err = NameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate(s)?;
        Ok(Self(s.to_owned()))
    }
}

impl AsRef<str> for Username {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for DbName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for DocName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for DbName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for DocName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_letters_digits_underscore_and_dash() {
        assert!("Foo_bar-42".parse::<DbName>().is_ok());
        assert!("a".repeat(MAX_NAME_LEN).parse::<DocName>().is_ok());
    }

    #[test]
    fn rejects_empty_and_too_long() {
        assert_eq!("".parse::<DbName>(), Err(NameError::Empty));
        assert_eq!(
            "a".repeat(MAX_NAME_LEN + 1).parse::<DbName>(),
            Err(NameError::TooLong)
        );
    }

    #[test]
    fn rejects_anything_that_could_escape_a_directory() {
        for name in [".", "..", "a/b", "a\\b", ".hidden", "a.b", "é", "a b"] {
            assert_eq!(
                name.parse::<DocName>(),
                Err(NameError::InvalidCharacter),
                "{name:?}"
            );
        }
    }

    #[test]
    fn reserved_names_are_only_reserved_for_users() {
        assert_eq!("admin".parse::<Username>(), Err(NameError::Reserved));
        assert_eq!("users".parse::<Username>(), Err(NameError::Reserved));
        assert!("users".parse::<DbName>().is_ok());
    }
}
