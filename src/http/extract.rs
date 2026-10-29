//! Extracting the [`Caller`] from HTTP Basic auth.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderValue, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use super::AppState;
use crate::service::{Caller, Credentials, ServiceError};

impl FromRequestParts<AppState> for Caller {
    type Rejection = ServiceError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let credentials = match parts.headers.get(header::AUTHORIZATION) {
            None => None,
            Some(value) => Some(parse_basic(value).ok_or(ServiceError::Unauthorized)?),
        };
        state.auth.authenticate(credentials).await
    }
}

/// Parses `Basic base64(username:password)`.
fn parse_basic(value: &HeaderValue) -> Option<Credentials> {
    let (scheme, encoded) = value.to_str().ok()?.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = String::from_utf8(BASE64.decode(encoded.trim()).ok()?).ok()?;
    let (username, password) = decoded.split_once(':')?;
    Some(Credentials {
        username: username.to_owned(),
        password: password.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: &str) -> Option<(String, String)> {
        parse_basic(&HeaderValue::from_str(value).unwrap()).map(|c| (c.username, c.password))
    }

    #[test]
    fn parses_basic_credentials() {
        // "alice:pass:word" — the password may contain colons.
        assert_eq!(
            parse("Basic YWxpY2U6cGFzczp3b3Jk"),
            Some(("alice".into(), "pass:word".into()))
        );
        assert_eq!(
            parse("basic YWxpY2U6cGFzczp3b3Jk").map(|c| c.0),
            Some("alice".into())
        );
    }

    #[test]
    fn rejects_malformed_headers() {
        assert_eq!(parse("Bearer YWxpY2U6cGFzcw=="), None);
        assert_eq!(parse("Basic not-base64!"), None);
        assert_eq!(parse("Basic YWxpY2U="), None); // "alice", no colon
        assert_eq!(parse("Basic"), None);
    }
}
