//! User account management.

use super::{Caller, ServiceError, hash_password};
use crate::domain::Username;
use crate::storage::{FsStore, UserRecord};

/// Longest accepted password, in bytes. Keeps hashing cost bounded.
const MAX_PASSWORD_LEN: usize = 1024;

pub struct Users {
    store: FsStore,
}

impl Users {
    pub fn new(store: FsStore) -> Self {
        Self { store }
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
        Ok(self.store.delete_user(user).await?)
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
    let password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .expect("password hashing does not panic");
    Ok(UserRecord { password_hash })
}
