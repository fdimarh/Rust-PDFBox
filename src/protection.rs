
use crate::crypto::permissions::Permissions;

/// Defines an encryption policy for a PDF document.
/// This is used with `Document::protect` to set up encryption.
#[derive(Debug, Clone, Default)]
pub struct StandardProtectionPolicy {
    /// The user password. If `None`, no user password is set.
    pub user_password: Option<String>,
    /// The owner password.
    pub owner_password: String,
    /// Permissions for the encrypted document.
    pub permissions: Permissions,
}

impl StandardProtectionPolicy {
    /// Creates a new protection policy.
    pub fn new(owner_password: impl Into<String>, user_password: impl Into<String>, permissions: Permissions) -> Self {
        Self {
            user_password: Some(user_password.into()),
            owner_password: owner_password.into(),
            permissions,
        }
    }

    /// Creates a new protection policy with only an owner password.
    pub fn owner_password(owner_password: impl Into<String>, permissions: Permissions) -> Self {
        Self {
            user_password: None,
            owner_password: owner_password.into(),
            permissions,
        }
    }
}
