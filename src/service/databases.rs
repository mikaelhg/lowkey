//! Databases: named containers of documents, owned by a user.

use super::{Caller, ServiceError};
use crate::domain::{DbName, Username};
use crate::storage::FsStore;

pub struct Databases {
    store: FsStore,
}

impl Databases {
    pub fn new(store: FsStore) -> Self {
        Self { store }
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
        Ok(self.store.create_db(user, db).await?)
    }

    /// Deletes a database and all its documents. The owner or the admin.
    pub async fn delete(
        &self,
        caller: &Caller,
        user: &Username,
        db: &DbName,
    ) -> Result<(), ServiceError> {
        caller.require_owner_or_admin(user)?;
        Ok(self.store.delete_db(user, db).await?)
    }
}
