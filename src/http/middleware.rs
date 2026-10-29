//! Request-level middleware: identifying the client and limiting its rate.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;

use super::AppState;
use super::rate_limit::too_many_requests;

/// The client's address, stored in request extensions for later extractors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(pub IpAddr);

/// Refuses requests from clients over their request rate.
pub async fn limit_requests(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip());
    let forwarded = state
        .throttle
        .trust_forwarded_for
        .then(|| last_forwarded_for(request.headers()))
        .flatten();
    let ip = forwarded
        .or(peer)
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));

    if !state.throttle.requests.hit(ip) {
        return too_many_requests();
    }
    request.extensions_mut().insert(ClientIp(ip));
    next.run(request).await
}

/// The last `X-Forwarded-For` entry: the address our own proxy saw. Earlier
/// entries are supplied by the client and can't be trusted.
fn last_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    let value = headers.get_all("x-forwarded-for").iter().next_back()?;
    value.to_str().ok()?.rsplit(',').next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(values: &[&'static str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append("x-forwarded-for", HeaderValue::from_static(value));
        }
        headers
    }

    #[test]
    fn takes_the_last_forwarded_address() {
        let ip = |values| last_forwarded_for(&headers(values));
        assert_eq!(ip(&["203.0.113.9"]), "203.0.113.9".parse().ok());
        assert_eq!(ip(&["1.1.1.1, 203.0.113.9"]), "203.0.113.9".parse().ok());
        assert_eq!(ip(&["1.1.1.1", "2001:db8::1"]), "2001:db8::1".parse().ok());
        assert_eq!(ip(&["not an address"]), None);
        assert_eq!(ip(&[]), None);
    }
}
