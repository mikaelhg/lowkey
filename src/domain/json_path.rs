//! Paths to values inside JSON documents.
//!
//! A path is a sequence of keys separated by `/`, as in a JSON Pointer
//! (RFC 6901) without the leading slash: `or/1` is the second element of the
//! array under `or`. Inside a key, `~1` stands for `/` and `~0` for `~`.

use std::str::FromStr;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid path: '~' must be followed by '0' or '1'")]
pub struct JsonPathError;

/// A validated path into a JSON document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonPath {
    /// The equivalent JSON Pointer, with its leading slash.
    pointer: String,
}

impl JsonPath {
    /// Finds the value at this path, if there is one. Array indexes must be
    /// plain decimal numbers without leading zeros.
    pub fn lookup<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        value.pointer(&self.pointer)
    }
}

impl FromStr for JsonPath {
    type Err = JsonPathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return Err(JsonPathError);
            }
        }
        Ok(Self {
            pointer: format!("/{s}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn lookup(path: &str, value: &Value) -> Option<Value> {
        path.parse::<JsonPath>().unwrap().lookup(value).cloned()
    }

    #[test]
    fn walks_objects_and_arrays() {
        let doc = json!({"or": ["you", "know"], "a": {"b": {"c": null}}});
        assert_eq!(lookup("or", &doc), Some(json!(["you", "know"])));
        assert_eq!(lookup("or/0", &doc), Some(json!("you")));
        assert_eq!(lookup("a/b/c", &doc), Some(Value::Null));
    }

    #[test]
    fn missing_values_are_none() {
        let doc = json!({"or": ["you", "know"], "s": "text"});
        for path in ["nope", "or/2", "or/-1", "or/01", "or/+1", "or/x", "s/0"] {
            assert_eq!(lookup(path, &doc), None, "{path}");
        }
    }

    #[test]
    fn escapes_select_keys_with_slashes_and_tildes() {
        let doc = json!({"a/b": 1, "~c": 2, "~1": 3, "": 4});
        assert_eq!(lookup("a~1b", &doc), Some(json!(1)));
        assert_eq!(lookup("~0c", &doc), Some(json!(2)));
        assert_eq!(lookup("~01", &doc), Some(json!(3)));
        assert_eq!(lookup("", &doc), Some(json!(4)));
    }

    #[test]
    fn rejects_malformed_escapes() {
        for path in ["~", "a~", "~2", "a/~x/b"] {
            assert_eq!(path.parse::<JsonPath>(), Err(JsonPathError), "{path}");
        }
    }
}
