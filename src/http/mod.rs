//! HTTP layer: translates requests into service calls and results into
//! responses. Contains no business logic.

mod error;
mod extract;
mod handlers;
mod middleware;
mod rate_limit;
mod routes;

use std::sync::Arc;

use axum::Router;
use axum::extract::FromRef;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::service::Services;
use rate_limit::RateLimiter;

/// Shared state handed to handlers and middleware. Handlers that only need
/// the services extract `State<Arc<Services>>`.
#[derive(Clone)]
pub struct AppState {
    services: Arc<Services>,
    throttle: Arc<Throttle>,
}

/// Per-client rate limits.
struct Throttle {
    requests: RateLimiter,
    failed_logins: RateLimiter,
    trust_forwarded_for: bool,
}

impl FromRef<AppState> for Arc<Services> {
    fn from_ref(state: &AppState) -> Self {
        state.services.clone()
    }
}

/// The full HTTP router with middleware applied.
pub fn router(services: Services, config: &Config) -> Router {
    let state = AppState {
        services: Arc::new(services),
        throttle: Arc::new(Throttle {
            requests: RateLimiter::per_minute(config.limits.requests_per_minute),
            failed_logins: RateLimiter::per_minute(config.limits.failed_logins_per_minute),
            trust_forwarded_for: config.trust_forwarded_for,
        }),
    };
    routes::routes(config.limits.max_document_bytes)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::limit_requests,
        ))
        .with_state(state)
        // Don't log every 5xx as an error: 507 is a user over quota, and
        // real failures are logged where they happen.
        .layer(TraceLayer::new_for_http().on_failure(()))
}
