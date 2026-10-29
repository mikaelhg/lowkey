//! The API route table.

use axum::Router;
use axum::routing::get;

use super::handlers;

pub fn routes() -> Router {
    Router::new().route("/healthz", get(handlers::healthz))
}
