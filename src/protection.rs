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
    pub fn new(
        owner_password: impl Into<String>,
        user_password: impl Into<String>,
        permissions: Permissions,
    ) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Permissions;

    #[test]
    fn test_policy_new_with_both_passwords() {
        let policy = StandardProtectionPolicy::new("owner", "user", Permissions::all_allowed());
        assert_eq!(policy.user_password, Some("user".into()));
        assert_eq!(policy.owner_password, "owner");
        assert_eq!(policy.permissions, Permissions::all_allowed());
    }

    #[test]
    fn test_policy_owner_only() {
        let policy = StandardProtectionPolicy::owner_password("owner", Permissions::all_allowed());
        assert_eq!(policy.user_password, None);
        assert_eq!(policy.owner_password, "owner");
    }

    #[test]
    fn test_policy_default() {
        let policy = StandardProtectionPolicy::default();
        assert_eq!(policy.user_password, None);
        assert!(policy.owner_password.is_empty());
    }

    #[test]
    fn test_policy_new_into_string() {
        let policy = StandardProtectionPolicy::new(
            String::from("owner_pass"),
            String::from("user_pass"),
            Permissions::none_allowed(),
        );
        assert_eq!(policy.user_password, Some("user_pass".into()));
        let perms = Permissions::none_allowed();
        assert_eq!(policy.permissions, perms);
    }

    #[test]
    fn test_policy_restricted() {
        let perms = Permissions::none_allowed();
        let policy = StandardProtectionPolicy::new("o", "u", perms);
        assert!(!policy.permissions.can_fill_forms());
        assert!(!policy.permissions.can_print());
    }
}
