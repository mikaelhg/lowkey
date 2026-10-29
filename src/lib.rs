//! Lowkey, a low resource key/value HTTP database.

pub mod config;
pub mod domain;
pub mod http;
pub mod service;
pub mod storage;

use axum::Router;

use crate::config::Config;
use crate::service::{QuotaLimits, Quotas, Services};
use crate::storage::{FsStore, StorageError};

/// Opens the data directory and builds the complete application router.
pub async fn build_app(config: &Config) -> Result<Router, StorageError> {
    let store = FsStore::open(&config.data_dir).await?;
    let limits = QuotaLimits {
        max_bytes: config.limits.max_bytes_per_user,
        max_documents: config.limits.max_documents_per_user,
        max_databases: config.limits.max_databases_per_user,
    };
    let quotas = Quotas::new(limits, store.disk_usage().await?);
    let services = Services::new(store, config.admin_password_hash.clone(), quotas);
    Ok(http::router(services, config))
}
