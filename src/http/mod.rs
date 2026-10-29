//! HTTP layer: translates requests into service calls and results into
//! responses. Contains no business logic.

mod error;
mod extract;
mod handlers;
mod routes;

use std::sync::Arc;

use axum::Router;
use tower_http::trace::TraceLayer;

use crate::service::Services;

/// Shared state handed to every handler.
pub type AppState = Arc<Services>;

/// The full HTTP router with middleware applied.
pub fn router(services: AppState) -> Router {
    routes::routes()
        .with_state(services)
        .layer(TraceLayer::new_for_http())
}
