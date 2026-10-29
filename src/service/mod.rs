//! Service layer: the application's behavior. Decides who may do what and
//! enforces the rules, using the storage layer for persistence. Knows nothing
//! about HTTP.

mod auth;
mod error;
mod users;

pub use auth::{Auth, Credentials, hash_password, is_password_hash};
pub use error::ServiceError;
pub use users::Users;

use crate::domain::Username;
use crate::storage::FsStore;

/// Who is making a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    Anonymous,
    User(Username),
    Admin,
}

impl Caller {
    pub fn require_admin(&self) -> Result<(), ServiceError> {
        match self {
            Self::Admin => Ok(()),
            Self::User(_) => Err(ServiceError::Forbidden),
            Self::Anonymous => Err(ServiceError::Unauthorized),
        }
    }

    /// Allows `owner` themself and the admin.
    pub fn require_owner_or_admin(&self, owner: &Username) -> Result<(), ServiceError> {
        match self {
            Self::Admin => Ok(()),
            Self::User(user) if user == owner => Ok(()),
            Self::User(_) => Err(ServiceError::Forbidden),
            Self::Anonymous => Err(ServiceError::Unauthorized),
        }
    }
}

/// All services, sharing one store.
pub struct Services {
    pub auth: Auth,
    pub users: Users,
}

impl Services {
    pub fn new(store: FsStore, admin_password_hash: Option<String>) -> Self {
        Self {
            auth: Auth::new(store.clone(), admin_password_hash),
            users: Users::new(store),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_and_admin_pass_the_owner_check() {
        let alice: Username = "alice".parse().unwrap();
        let bob: Username = "bob".parse().unwrap();
        assert!(
            Caller::User(alice.clone())
                .require_owner_or_admin(&alice)
                .is_ok()
        );
        assert!(Caller::Admin.require_owner_or_admin(&alice).is_ok());
        assert!(matches!(
            Caller::User(bob).require_owner_or_admin(&alice),
            Err(ServiceError::Forbidden)
        ));
        assert!(matches!(
            Caller::Anonymous.require_owner_or_admin(&alice),
            Err(ServiceError::Unauthorized)
        ));
    }
}
