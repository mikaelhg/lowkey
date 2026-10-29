//! Databases: named containers of documents, owned by a user.

use std::sync::Arc;

use super::{Caller, Quotas, ServiceError};
use crate::domain::{DbName, Username};
use crate::storage::{FsStore, Usage};

pub struct Databases {
    store: FsStore,
    quotas: Arc<Quotas>,
}

impl Databases {
    pub fn new(store: FsStore, quotas: Arc<Quotas>) -> Self {
        Self { store, quotas }
    }

    /// Lists a user's databases. Public.
    pub async fn list(&self, user: &Username) -> Result<Vec<DbName>, ServiceError> {
        Ok(self.store.list_dbs(user).await?)
    }

    /// Creates an empty database. The owner or the admin.
    pub async fn create(
        &self,
        caller: &Caller,
        user: &Username,
        db: &DbName,
    ) -> Result<(), ServiceError> {
        caller.require_owner_or_admin(user)?;
        let mut usage = self.quotas.lock(user).await;
        let after = usage.plus(Usage {
            databases: 1,
            ..Usage::default()
        });
        self.quotas.check(*usage, after)?;
        self.store.create_db(user, db).await?;
        *usage = after;
        Ok(())
    }

    /// Deletes a database and all its documents. The owner or the admin.
    pub async fn delete(
        &self,
        caller: &Caller,
        user: &Username,
        db: &DbName,
    ) -> Result<(), ServiceError> {
        caller.require_owner_or_admin(user)?;
        let mut usage = self.quotas.lock(user).await;
        let freed = self.store.db_usage(user, db).await?;
        self.store.delete_db(user, db).await?;
        *usage = usage.minus(freed);
        Ok(())
    }
}
