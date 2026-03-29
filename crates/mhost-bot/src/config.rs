use serde::{Deserialize, Serialize};
use std::path::Path;

/// Full configuration for the mhost Telegram/Discord bot.
///
/// Serialised to / deserialised from `~/.mhost/bot.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    pub enabled: bool,
    /// `"telegram"` or `"discord"`
    pub platform: String,
    pub token: String,
    pub permissions: Permissions,
    /// Require `/confirm` before executing destructive commands.
    pub confirm_destructive: bool,
    /// Automatically forward daemon alerts to admin users.
    pub auto_alerts: bool,
    /// Maximum commands a single user may issue per minute.
    pub rate_limit: u32,
}

/// Per-role user lists.  A user may appear in **at most one** list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub admins: Vec<i64>,
    #[serde(default)]
    pub operators: Vec<i64>,
    #[serde(default)]
    pub viewers: Vec<i64>,
    #[serde(default)]
    pub blocked: Vec<i64>,
}

/// Access level assigned to a user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// Explicitly denied — all commands rejected.
    Blocked,
    /// Not in any list — all commands rejected.
    Unknown,
    /// Read-only: status / logs / health.
    Viewer,
    /// Start / stop / restart / scale / logs / health / deploy.
    Operator,
    /// Full access.
    Admin,
}

// ---------------------------------------------------------------------------
// Permissions helpers
// ---------------------------------------------------------------------------

impl Permissions {
    /// Determine the [`Role`] of `user_id`.
    pub fn get_role(&self, user_id: i64) -> Role {
        if self.blocked.contains(&user_id) {
            return Role::Blocked;
        }
        if self.admins.contains(&user_id) {
            return Role::Admin;
        }
        if self.operators.contains(&user_id) {
            return Role::Operator;
        }
        if self.viewers.contains(&user_id) {
            return Role::Viewer;
        }
        Role::Unknown
    }

    /// Add or **move** a user to the given role list.
    /// Passing [`Role::Unknown`] is a no-op.
    pub fn add_user(&mut self, user_id: i64, role: Role) {
        self.remove_user(user_id);
        match role {
            Role::Admin => self.admins.push(user_id),
            Role::Operator => self.operators.push(user_id),
            Role::Viewer => self.viewers.push(user_id),
            Role::Blocked => self.blocked.push(user_id),
            Role::Unknown => {}
        }
    }

    /// Remove a user from **all** role lists.
    pub fn remove_user(&mut self, user_id: i64) {
        self.admins.retain(|&id| id != user_id);
        self.operators.retain(|&id| id != user_id);
        self.viewers.retain(|&id| id != user_id);
        self.blocked.retain(|&id| id != user_id);
    }
}

// ---------------------------------------------------------------------------
// BotConfig defaults / persistence
// ---------------------------------------------------------------------------

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            platform: "telegram".into(),
            token: String::new(),
            permissions: Permissions {
                admins: vec![],
                operators: vec![],
                viewers: vec![],
                blocked: vec![],
            },
            confirm_destructive: true,
            auto_alerts: true,
            rate_limit: 30,
        }
    }
}

impl BotConfig {
    /// Load from a JSON file.  Returns `None` when the file is absent or
    /// cannot be parsed.
    pub fn load(path: &Path) -> Option<Self> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
    }

    /// Persist as pretty-printed JSON.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        std::fs::write(path, json).map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Command-level authorisation
// ---------------------------------------------------------------------------

/// Returns `true` when a user with `role` is permitted to run `command`.
pub fn command_allowed(role: Role, command: &str) -> bool {
    match role {
        Role::Blocked | Role::Unknown => false,
        Role::Admin => true,
        Role::Operator => matches!(
            command,
            "status"
                | "start"
                | "stop"
                | "restart"
                | "scale"
                | "logs"
                | "health"
                | "deploy"
                | "help"
        ),
        Role::Viewer => matches!(command, "status" | "logs" | "health" | "help"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        env::temp_dir().join(format!("mhost-bot-config-test-{name}.json"))
    }

    // -----------------------------------------------------------------------
    // Role detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_admin_role_detected() {
        let perms = Permissions {
            admins: vec![1],
            operators: vec![],
            viewers: vec![],
            blocked: vec![],
        };
        assert_eq!(perms.get_role(1), Role::Admin);
    }

    #[test]
    fn test_operator_role_detected() {
        let perms = Permissions {
            admins: vec![],
            operators: vec![2],
            viewers: vec![],
            blocked: vec![],
        };
        assert_eq!(perms.get_role(2), Role::Operator);
    }

    #[test]
    fn test_viewer_role_detected() {
        let perms = Permissions {
            admins: vec![],
            operators: vec![],
            viewers: vec![3],
            blocked: vec![],
        };
        assert_eq!(perms.get_role(3), Role::Viewer);
    }

    #[test]
    fn test_blocked_role_detected() {
        let perms = Permissions {
            admins: vec![],
            operators: vec![],
            viewers: vec![],
            blocked: vec![4],
        };
        assert_eq!(perms.get_role(4), Role::Blocked);
    }

    #[test]
    fn test_unknown_role_for_unregistered_user() {
        let perms = Permissions {
            admins: vec![1],
            operators: vec![2],
            viewers: vec![3],
            blocked: vec![4],
        };
        assert_eq!(perms.get_role(99), Role::Unknown);
    }

    // -----------------------------------------------------------------------
    // blocked takes priority over other lists
    // -----------------------------------------------------------------------

    #[test]
    fn test_blocked_takes_priority_over_admin() {
        let perms = Permissions {
            admins: vec![10],
            operators: vec![],
            viewers: vec![],
            blocked: vec![10],
        };
        // blocked is checked first in get_role
        assert_eq!(perms.get_role(10), Role::Blocked);
    }

    // -----------------------------------------------------------------------
    // add_user / remove_user
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_user_moves_between_roles() {
        let mut perms = Permissions {
            admins: vec![],
            operators: vec![5],
            viewers: vec![],
            blocked: vec![],
        };
        // Move 5 from operator → admin
        perms.add_user(5, Role::Admin);
        assert_eq!(perms.get_role(5), Role::Admin);
        assert!(!perms.operators.contains(&5));
    }

    #[test]
    fn test_add_user_unknown_is_noop() {
        let mut perms = Permissions {
            admins: vec![6],
            operators: vec![],
            viewers: vec![],
            blocked: vec![],
        };
        perms.add_user(6, Role::Unknown);
        // Should have been removed (remove_user runs) and not re-added
        assert_eq!(perms.get_role(6), Role::Unknown);
    }

    #[test]
    fn test_remove_user_clears_all_lists() {
        let mut perms = Permissions {
            admins: vec![7],
            operators: vec![7],
            viewers: vec![7],
            blocked: vec![7],
        };
        perms.remove_user(7);
        assert_eq!(perms.get_role(7), Role::Unknown);
    }

    // -----------------------------------------------------------------------
    // command_allowed
    // -----------------------------------------------------------------------

    #[test]
    fn test_admin_allowed_any_command() {
        for cmd in &[
            "status", "start", "stop", "restart", "scale", "logs", "health", "deploy", "ai",
            "help", "custom",
        ] {
            assert!(
                command_allowed(Role::Admin, cmd),
                "admin should allow {cmd}"
            );
        }
    }

    #[test]
    fn test_operator_allowed_commands() {
        for cmd in &[
            "status", "start", "stop", "restart", "scale", "logs", "health", "deploy", "help",
        ] {
            assert!(
                command_allowed(Role::Operator, cmd),
                "operator should allow {cmd}"
            );
        }
    }

    #[test]
    fn test_operator_denied_ai() {
        assert!(
            !command_allowed(Role::Operator, "ai"),
            "operator should not access ai"
        );
    }

    #[test]
    fn test_viewer_allowed_read_only() {
        for cmd in &["status", "logs", "health", "help"] {
            assert!(
                command_allowed(Role::Viewer, cmd),
                "viewer should allow {cmd}"
            );
        }
    }

    #[test]
    fn test_viewer_denied_start_stop_restart() {
        for cmd in &["start", "stop", "restart", "scale", "deploy", "ai"] {
            assert!(
                !command_allowed(Role::Viewer, cmd),
                "viewer should not allow {cmd}"
            );
        }
    }

    #[test]
    fn test_blocked_denied_all() {
        for cmd in &["status", "logs", "help", "start"] {
            assert!(
                !command_allowed(Role::Blocked, cmd),
                "blocked should deny {cmd}"
            );
        }
    }

    #[test]
    fn test_unknown_denied_all() {
        for cmd in &["status", "logs", "help", "start"] {
            assert!(
                !command_allowed(Role::Unknown, cmd),
                "unknown should deny {cmd}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Config default values
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_config_values() {
        let cfg = BotConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.platform, "telegram");
        assert!(cfg.token.is_empty());
        assert!(cfg.confirm_destructive);
        assert!(cfg.auto_alerts);
        assert_eq!(cfg.rate_limit, 30);
    }

    // -----------------------------------------------------------------------
    // Save / load roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_save_and_load_roundtrip() {
        let path = tmp_path("roundtrip");
        let _ = std::fs::remove_file(&path);

        let cfg = BotConfig {
            enabled: true,
            platform: "discord".into(),
            token: "tok-abc".into(),
            permissions: Permissions {
                admins: vec![1, 2],
                operators: vec![3],
                viewers: vec![4],
                blocked: vec![5],
            },
            confirm_destructive: false,
            auto_alerts: false,
            rate_limit: 10,
        };

        cfg.save(&path).expect("save should succeed");
        let loaded = BotConfig::load(&path).expect("load should return Some");

        assert_eq!(loaded.enabled, cfg.enabled);
        assert_eq!(loaded.platform, cfg.platform);
        assert_eq!(loaded.token, cfg.token);
        assert_eq!(loaded.rate_limit, cfg.rate_limit);
        assert_eq!(loaded.permissions.admins, cfg.permissions.admins);
        assert_eq!(loaded.permissions.operators, cfg.permissions.operators);
        assert_eq!(loaded.permissions.viewers, cfg.permissions.viewers);
        assert_eq!(loaded.permissions.blocked, cfg.permissions.blocked);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_returns_none_for_missing_file() {
        let path = tmp_path("does-not-exist-xyz999");
        assert!(BotConfig::load(&path).is_none());
    }

    #[test]
    fn test_load_returns_none_for_invalid_json() {
        let path = tmp_path("bad-json");
        std::fs::write(&path, b"not json!!!").unwrap();
        assert!(BotConfig::load(&path).is_none());
        let _ = std::fs::remove_file(&path);
    }
}
