//! Password hashing and turning credentials into a [`Caller`].
//!
//! Argon2 is deliberately slow, too slow to run on every request. After a
//! successful login we remember a SHA-256 digest of the password for a few
//! minutes, together with the hash it was checked against. Changing the
//! password or deleting the user changes or removes that hash, so stale cache
//! entries never match.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use sha2::{Digest, Sha256};

use super::password_work::run_password_work;
use super::{Caller, ServiceError};
use crate::domain::Username;
use crate::storage::{FsStore, StorageError};

/// The login name of the admin account.
pub const ADMIN_USERNAME: &str = "admin";

/// How long a verified login is remembered.
const CACHE_TTL: Duration = Duration::from_secs(5 * 60);

/// A username and password, as sent by a client.
#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// Hashes a password with Argon2id and a random salt, as a PHC string.
pub fn hash_password(password: &str) -> String {
    Argon2::default()
        .hash_password(password.as_bytes())
        .expect("hashing with a random salt succeeds")
        .to_string()
}

/// Whether `hash` is a well-formed PHC password hash.
pub fn is_password_hash(hash: &str) -> bool {
    PasswordHash::new(hash).is_ok()
}

fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

struct CachedLogin {
    password_digest: [u8; 32],
    password_hash: String,
    expires: Instant,
}

/// Authenticates callers against stored users and the admin password.
pub struct Auth {
    store: FsStore,
    admin_password_hash: Option<String>,
    cache: Mutex<HashMap<String, CachedLogin>>,
}

impl Auth {
    pub fn new(store: FsStore, admin_password_hash: Option<String>) -> Self {
        Self {
            store,
            admin_password_hash,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Identifies the caller from their credentials. Wrong credentials are
    /// an error, never a silent downgrade to anonymous.
    pub async fn authenticate(&self, credentials: &Credentials) -> Result<Caller, ServiceError> {
        let (caller, password_hash) = self.lookup(&credentials.username).await?;
        if self.check_password(credentials, password_hash).await {
            Ok(caller)
        } else {
            Err(ServiceError::Unauthorized)
        }
    }

    /// Finds who a username belongs to and their current password hash.
    async fn lookup(&self, username: &str) -> Result<(Caller, String), ServiceError> {
        if username == ADMIN_USERNAME {
            let hash = self
                .admin_password_hash
                .clone()
                .ok_or(ServiceError::Unauthorized)?;
            return Ok((Caller::Admin, hash));
        }
        let user: Username = username.parse().map_err(|_| ServiceError::Unauthorized)?;
        match self.store.read_user(&user).await {
            Ok(record) => Ok((Caller::User(user), record.password_hash)),
            Err(StorageError::NotFound) => Err(ServiceError::Unauthorized),
            Err(error) => Err(error.into()),
        }
    }

    async fn check_password(&self, credentials: &Credentials, password_hash: String) -> bool {
        let digest: [u8; 32] = Sha256::digest(credentials.password.as_bytes()).into();
        if self.is_cached(&credentials.username, &digest, &password_hash) {
            return true;
        }

        let password = credentials.password.clone();
        let hash = password_hash.clone();
        let valid = run_password_work(move || verify_password(&password, &hash)).await;

        if valid {
            let login = CachedLogin {
                password_digest: digest,
                password_hash,
                expires: Instant::now() + CACHE_TTL,
            };
            self.cache().insert(credentials.username.clone(), login);
        }
        valid
    }

    fn is_cached(&self, username: &str, digest: &[u8; 32], password_hash: &str) -> bool {
        self.cache().get(username).is_some_and(|login| {
            login.password_digest == *digest
                && login.password_hash == password_hash
                && login.expires > Instant::now()
        })
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, HashMap<String, CachedLogin>> {
        self.cache
            .lock()
            .expect("auth cache lock is never poisoned")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_verify_only_their_own_password() {
        let hash = hash_password("secret");
        assert!(is_password_hash(&hash));
        assert!(verify_password("secret", &hash));
        assert!(!verify_password("Secret", &hash));
        assert!(!verify_password("secret", "not a hash"));
    }

    #[test]
    fn hashing_twice_uses_different_salts() {
        assert_ne!(hash_password("secret"), hash_password("secret"));
    }
}
