//! Lowkey, a low resource key/value HTTP database.

pub mod config;
pub mod domain;
pub mod http;
pub mod service;
pub mod storage;

use std::sync::Arc;

use axum::Router;

use crate::config::Config;
use crate::service::Services;
use crate::storage::{FsStore, StorageError};

/// Opens the data directory and builds the complete application router.
pub async fn build_app(config: &Config) -> Result<Router, StorageError> {
    let store = FsStore::open(&config.data_dir).await?;
    let services = Services::new(store, config.admin_password_hash.clone());
    Ok(http::router(Arc::new(services)))
}
