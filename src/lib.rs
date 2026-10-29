//! Lowkey, a low resource key/value HTTP database.

pub mod config;
pub mod domain;
pub mod http;
pub mod storage;

use axum::Router;

use crate::config::Config;

/// Builds the complete application router from a config.
pub fn build_app(_config: &Config) -> Router {
    http::router()
}
