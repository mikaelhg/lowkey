//! HTTP layer: translates requests into service calls and results into
//! responses. Contains no business logic.

mod handlers;
mod routes;

use axum::Router;
use tower_http::trace::TraceLayer;

/// The full HTTP router with middleware applied.
pub fn router() -> Router {
    routes::routes().layer(TraceLayer::new_for_http())
}
