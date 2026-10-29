//! User account management.

use std::sync::Arc;

use super::password_work::run_password_work;
use super::{Caller, Quotas, ServiceError, hash_password};
use crate::domain::Username;
use crate::storage::{FsStore, UserRecord};

/// Longest accepted password, in bytes. Keeps hashing cost bounded.
const MAX_PASSWORD_LEN: usize = 1024;

pub struct Users {
    store: FsStore,
    quotas: Arc<Quotas>,
}

impl Users {
    pub fn new(store: FsStore, quotas: Arc<Quotas>) -> Self {
        Self { store, quotas }
    }

    /// Creates a user. Admin only.
    pub async fn create(
        &self,
        caller: &Caller,
        user: &Username,
        password: &str,
    ) -> Result<(), ServiceError> {
        caller.require_admin()?;
        let record = new_record(password).await?;
        Ok(self.store.create_user(user, &record).await?)
    }

    /// Replaces a user's password. The user themself or the admin.
    pub async fn change_password(
        &self,
        caller: &Caller,
        user: &Username,
        password: &str,
    ) -> Result<(), ServiceError> {
        caller.require_owner_or_admin(user)?;
        let record = new_record(password).await?;
        Ok(self.store.write_user(user, &record).await?)
    }

    /// Deletes a user and all their data. The user themself or the admin.
    pub async fn delete(&self, caller: &Caller, user: &Username) -> Result<(), ServiceError> {
        caller.require_owner_or_admin(user)?;
        self.store.delete_user(user).await?;
        self.quotas.forget(user);
        Ok(())
    }
}

/// Validates a password and hashes it off the async runtime.
async fn new_record(password: &str) -> Result<UserRecord, ServiceError> {
    if password.is_empty() {
        return Err(ServiceError::InvalidInput(
            "password must not be empty".into(),
        ));
    }
    if password.len() > MAX_PASSWORD_LEN {
        return Err(ServiceError::InvalidInput(format!(
            "password must be at most {MAX_PASSWORD_LEN} bytes"
        )));
    }
    let password = password.to_owned();
    let password_hash = run_password_work(move || hash_password(&password)).await;
    Ok(UserRecord { password_hash })
}
