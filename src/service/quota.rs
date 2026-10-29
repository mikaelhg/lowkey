//! Per-user storage quotas.
//!
//! Usage is counted from disk at startup and kept up to date in memory. Each
//! user has their own lock: a write holds it while it checks the quota,
//! writes and records the new usage, so concurrent writes can't overshoot.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::ServiceError;
use crate::domain::Username;
use crate::storage::Usage;

/// The most each user may store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaLimits {
    pub max_bytes: u64,
    pub max_documents: u64,
    pub max_databases: u64,
}

pub struct Quotas {
    limits: QuotaLimits,
    users: Mutex<HashMap<Username, Arc<AsyncMutex<Usage>>>>,
}

impl Quotas {
    /// Starts tracking from the usage found on disk.
    pub fn new(limits: QuotaLimits, usage: HashMap<Username, Usage>) -> Self {
        let users = usage
            .into_iter()
            .map(|(user, usage)| (user, Arc::new(AsyncMutex::new(usage))))
            .collect();
        Self {
            limits,
            users: Mutex::new(users),
        }
    }

    /// Locks a user's usage. Hold the guard across the quota check, the write
    /// and the update.
    pub async fn lock(&self, user: &Username) -> OwnedMutexGuard<Usage> {
        let usage = self.users().entry(user.clone()).or_default().clone();
        usage.lock_owned().await
    }

    /// Refuses a change that would push any counter past its limit. A change
    /// that doesn't grow a counter is always allowed, so users over a lowered
    /// limit can still clean up.
    pub fn check(&self, before: Usage, after: Usage) -> Result<(), ServiceError> {
        let exceeds = |before: u64, after: u64, max: u64| after > max && after > before;
        let limits = self.limits;
        if exceeds(before.databases, after.databases, limits.max_databases) {
            return Err(quota_exceeded(limits.max_databases, "databases"));
        }
        if exceeds(before.documents, after.documents, limits.max_documents) {
            return Err(quota_exceeded(limits.max_documents, "documents"));
        }
        if exceeds(before.bytes, after.bytes, limits.max_bytes) {
            return Err(quota_exceeded(limits.max_bytes, "bytes"));
        }
        Ok(())
    }

    /// Stops tracking a deleted user.
    pub fn forget(&self, user: &Username) {
        self.users().remove(user);
    }

    fn users(&self) -> std::sync::MutexGuard<'_, HashMap<Username, Arc<AsyncMutex<Usage>>>> {
        self.users.lock().expect("quota map lock is never poisoned")
    }
}

fn quota_exceeded(max: u64, what: &str) -> ServiceError {
    ServiceError::QuotaExceeded(format!("quota exceeded: at most {max} {what} per user"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: QuotaLimits = QuotaLimits {
        max_bytes: 100,
        max_documents: 10,
        max_databases: 2,
    };

    fn usage(databases: u64, documents: u64, bytes: u64) -> Usage {
        Usage {
            databases,
            documents,
            bytes,
        }
    }

    #[test]
    fn allows_up_to_the_limit_and_refuses_beyond() {
        let quotas = Quotas::new(LIMITS, HashMap::new());
        assert!(quotas.check(usage(1, 9, 50), usage(2, 10, 100)).is_ok());
        assert!(quotas.check(usage(2, 0, 0), usage(3, 0, 0)).is_err());
        assert!(quotas.check(usage(0, 10, 0), usage(0, 11, 0)).is_err());
        assert!(quotas.check(usage(0, 0, 100), usage(0, 0, 101)).is_err());
    }

    #[test]
    fn users_over_a_lowered_limit_can_shrink() {
        let quotas = Quotas::new(LIMITS, HashMap::new());
        assert!(quotas.check(usage(5, 50, 500), usage(5, 50, 400)).is_ok());
        assert!(quotas.check(usage(5, 50, 500), usage(5, 50, 501)).is_err());
    }

    #[tokio::test]
    async fn starts_from_disk_usage_and_forgets_deleted_users() {
        let alice: Username = "alice".parse().unwrap();
        let quotas = Quotas::new(LIMITS, HashMap::from([(alice.clone(), usage(1, 2, 3))]));
        assert_eq!(*quotas.lock(&alice).await, usage(1, 2, 3));
        quotas.forget(&alice);
        assert_eq!(*quotas.lock(&alice).await, Usage::default());
    }
}
