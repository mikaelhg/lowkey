//! The API route table.

use axum::Router;
use axum::routing::{delete, get, post, put};

use super::{AppState, handlers};

pub fn routes() -> Router<AppState> {
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
                .delete(handlers::delete_db),
        )
        .route(
            "/{user}/{db}/{doc}",
            get(handlers::get_doc)
                .put(handlers::put_doc)
                .delete(handlers::delete_doc),
        )
}
