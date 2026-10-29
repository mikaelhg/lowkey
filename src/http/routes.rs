//! The API route table.

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post, put};

use super::{AppState, handlers};

/// All routes. Document bodies are capped at `max_document_bytes`; other
/// bodies (small forms) keep axum's default limit.
pub fn routes(max_document_bytes: usize) -> Router<AppState> {
    let document_limit = DefaultBodyLimit::max(max_document_bytes);
    Router::new()
        .route("/healthz", get(handlers::healthz))
        // Accounts
        .route("/users", post(handlers::create_user))
        .route("/users/{user}", delete(handlers::delete_user))
        .route("/users/{user}/password", put(handlers::change_password))
        // Databases
        .route("/{user}", get(handlers::list_dbs).post(handlers::create_db))
        // Documents
        .route(
            "/{user}/{db}",
            get(handlers::list_docs)
                .post(handlers::push_doc)
                .delete(handlers::delete_db)
                .layer(document_limit),
        )
        .route(
            "/{user}/{db}/{doc}",
            get(handlers::get_doc)
                .put(handlers::put_doc)
                .delete(handlers::delete_doc)
                .layer(document_limit),
        )
        .route("/{user}/{db}/{doc}/{*path}", get(handlers::get_value))
}
