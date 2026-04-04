use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Permission roles for API token access control.
///
/// Roles form a hierarchy: Admin > Operator > Viewer.
/// Higher roles inherit all permissions of lower roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer,
    Operator,
    Admin,
}

impl Role {
    /// Returns the numeric privilege level (higher = more permissions).
    fn level(self) -> u8 {
        match self {
            Role::Viewer => 0,
            Role::Operator => 1,
            Role::Admin => 2,
        }
    }

    /// Returns true if this role has at least the permissions of `required`.
    pub fn has_permission(self, required: Role) -> bool {
        self.level() >= required.level()
    }
}

impl PartialOrd for Role {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Role {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.level().cmp(&other.level())
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Role::Viewer => "viewer",
            Role::Operator => "operator",
            Role::Admin => "admin",
        };
        f.write_str(s)
    }
}

impl FromStr for Role {
    type Err = RoleParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "viewer" => Ok(Role::Viewer),
            "operator" => Ok(Role::Operator),
            "admin" => Ok(Role::Admin),
            _ => Err(RoleParseError(s.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoleParseError(String);

impl fmt::Display for RoleParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid role '{}': expected 'viewer', 'operator', or 'admin'",
            self.0
        )
    }
}

impl std::error::Error for RoleParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_ordering() {
        assert!(Role::Admin > Role::Operator);
        assert!(Role::Operator > Role::Viewer);
        assert!(Role::Admin > Role::Viewer);
    }

    #[test]
    fn test_role_equality() {
        assert_eq!(Role::Viewer, Role::Viewer);
        assert_eq!(Role::Operator, Role::Operator);
        assert_eq!(Role::Admin, Role::Admin);
        assert_ne!(Role::Viewer, Role::Admin);
    }

    #[test]
    fn test_has_permission() {
        assert!(Role::Admin.has_permission(Role::Admin));
        assert!(Role::Admin.has_permission(Role::Operator));
        assert!(Role::Admin.has_permission(Role::Viewer));

        assert!(!Role::Operator.has_permission(Role::Admin));
        assert!(Role::Operator.has_permission(Role::Operator));
        assert!(Role::Operator.has_permission(Role::Viewer));

        assert!(!Role::Viewer.has_permission(Role::Admin));
        assert!(!Role::Viewer.has_permission(Role::Operator));
        assert!(Role::Viewer.has_permission(Role::Viewer));
    }

    #[test]
    fn test_display() {
        assert_eq!(Role::Viewer.to_string(), "viewer");
        assert_eq!(Role::Operator.to_string(), "operator");
        assert_eq!(Role::Admin.to_string(), "admin");
    }

    #[test]
    fn test_from_str() {
        assert_eq!("viewer".parse::<Role>().unwrap(), Role::Viewer);
        assert_eq!("operator".parse::<Role>().unwrap(), Role::Operator);
        assert_eq!("admin".parse::<Role>().unwrap(), Role::Admin);
    }

    #[test]
    fn test_from_str_case_insensitive() {
        assert_eq!("VIEWER".parse::<Role>().unwrap(), Role::Viewer);
        assert_eq!("Operator".parse::<Role>().unwrap(), Role::Operator);
        assert_eq!("ADMIN".parse::<Role>().unwrap(), Role::Admin);
    }

    #[test]
    fn test_from_str_invalid() {
        let result = "superadmin".parse::<Role>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("superadmin"));
    }

    #[test]
    fn test_serde_roundtrip() {
        let role = Role::Operator;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"operator\"");
        let deserialized: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, role);
    }

    #[test]
    fn test_serde_all_variants() {
        for (role, expected) in [
            (Role::Viewer, "\"viewer\""),
            (Role::Operator, "\"operator\""),
            (Role::Admin, "\"admin\""),
        ] {
            let json = serde_json::to_string(&role).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_role_clone_and_copy() {
        let role = Role::Admin;
        let cloned = role.clone();
        let copied = role;
        assert_eq!(role, cloned);
        assert_eq!(role, copied);
    }
}
