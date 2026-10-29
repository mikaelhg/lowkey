//! The API route table.

use axum::Router;
use axum::routing::{delete, get, post, put};

use super::{AppState, handlers};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(handlers::healthz))
        .route("/users", post(handlers::create_user))
        .route("/users/{user}", delete(handlers::delete_user))
        .route("/users/{user}/password", put(handlers::change_password))
}
