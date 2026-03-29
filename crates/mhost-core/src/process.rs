use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{MhostError, Result};

// ---------------------------------------------------------------------------
// ProcessStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Stopped,
    Starting,
    Online,
    Stopping,
    Errored,
}

impl ProcessStatus {
    /// Returns `true` if transitioning `self` → `next` is a valid state move.
    pub fn can_transition_to(&self, next: &ProcessStatus) -> bool {
        use ProcessStatus::*;
        matches!(
            (self, next),
            (Stopped, Starting)
                | (Starting, Online)
                | (Starting, Errored)
                | (Online, Stopping)
                | (Online, Errored)
                | (Stopping, Stopped)
                | (Stopping, Errored)
                | (Errored, Starting)
        )
    }

    /// Returns a colour name suitable for terminal output.
    pub fn display_color(&self) -> &'static str {
        match self {
            ProcessStatus::Stopped => "red",
            ProcessStatus::Starting => "yellow",
            ProcessStatus::Online => "green",
            ProcessStatus::Stopping => "yellow",
            ProcessStatus::Errored => "red",
        }
    }
}

impl fmt::Display for ProcessStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ProcessStatus::Stopped => "stopped",
            ProcessStatus::Starting => "starting",
            ProcessStatus::Online => "online",
            ProcessStatus::Stopping => "stopping",
            ProcessStatus::Errored => "errored",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// ProcessConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: HashMap<String, String>,
    pub instances: u32,
    pub max_memory_mb: Option<u64>,
    pub max_restarts: u32,
    pub min_uptime_ms: u64,
    pub restart_delay_ms: u64,
    pub grace_period_ms: u64,
    pub cron_restart: Option<String>,
    pub interpreter: Option<String>,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            command: String::new(),
            args: Vec::new(),
            cwd: None,
            env: HashMap::new(),
            instances: 1,
            max_memory_mb: None,
            max_restarts: 15,
            min_uptime_ms: 1000,
            restart_delay_ms: 100,
            grace_period_ms: 5000,
            cron_restart: None,
            interpreter: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ProcessInfo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub id: String,
    pub config: ProcessConfig,
    pub status: ProcessStatus,
    pub pid: Option<u32>,
    pub instance: u32,
    pub restart_count: u32,
    pub uptime_started: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub last_restart: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub memory_bytes: Option<u64>,
    pub cpu_percent: Option<f32>,
}

impl ProcessInfo {
    /// Create a new `ProcessInfo` in the `Stopped` state.
    pub fn new(config: ProcessConfig, instance: u32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config,
            status: ProcessStatus::Stopped,
            pid: None,
            instance,
            restart_count: 0,
            uptime_started: None,
            created_at: Utc::now(),
            last_restart: None,
            exit_code: None,
            memory_bytes: None,
            cpu_percent: None,
        }
    }

    /// Returns a **new** `ProcessInfo` with `status` set to `next`.
    /// Errors if the transition is invalid (immutable pattern — `self` is
    /// never mutated).
    pub fn transition_to(&self, next: ProcessStatus) -> Result<ProcessInfo> {
        if !self.status.can_transition_to(&next) {
            return Err(MhostError::Config(format!(
                "Invalid process state transition: {} → {}",
                self.status, next
            )));
        }

        let mut updated = self.clone();
        updated.status = next;
        Ok(updated)
    }

    /// Elapsed seconds since the process was marked `Online`, or `None` if
    /// there is no `uptime_started` timestamp.
    pub fn uptime_seconds(&self) -> Option<i64> {
        self.uptime_started
            .map(|started| (Utc::now() - started).num_seconds())
    }

    /// Human-readable uptime string, e.g. `"2h 03m 14s"`.
    /// Returns `"N/A"` when the process has not started yet.
    pub fn format_uptime(&self) -> String {
        let total = match self.uptime_seconds() {
            None => return "N/A".to_string(),
            Some(s) => s,
        };

        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        let seconds = total % 60;

        if hours > 0 {
            format!("{}h {:02}m {:02}s", hours, minutes, seconds)
        } else if minutes > 0 {
            format!("{}m {:02}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Valid transitions ---------------------------------------------------

    #[test]
    fn test_valid_stopped_to_starting() {
        assert!(ProcessStatus::Stopped.can_transition_to(&ProcessStatus::Starting));
    }

    #[test]
    fn test_valid_starting_to_online() {
        assert!(ProcessStatus::Starting.can_transition_to(&ProcessStatus::Online));
    }

    #[test]
    fn test_valid_starting_to_errored() {
        assert!(ProcessStatus::Starting.can_transition_to(&ProcessStatus::Errored));
    }

    #[test]
    fn test_valid_online_to_stopping() {
        assert!(ProcessStatus::Online.can_transition_to(&ProcessStatus::Stopping));
    }

    #[test]
    fn test_valid_online_to_errored() {
        assert!(ProcessStatus::Online.can_transition_to(&ProcessStatus::Errored));
    }

    #[test]
    fn test_valid_stopping_to_stopped() {
        assert!(ProcessStatus::Stopping.can_transition_to(&ProcessStatus::Stopped));
    }

    #[test]
    fn test_valid_stopping_to_errored() {
        assert!(ProcessStatus::Stopping.can_transition_to(&ProcessStatus::Errored));
    }

    #[test]
    fn test_valid_errored_to_starting() {
        assert!(ProcessStatus::Errored.can_transition_to(&ProcessStatus::Starting));
    }

    // -- Invalid transitions -------------------------------------------------

    #[test]
    fn test_invalid_stopped_to_online() {
        assert!(!ProcessStatus::Stopped.can_transition_to(&ProcessStatus::Online));
    }

    #[test]
    fn test_invalid_online_to_starting() {
        assert!(!ProcessStatus::Online.can_transition_to(&ProcessStatus::Starting));
    }

    #[test]
    fn test_invalid_stopped_to_errored() {
        assert!(!ProcessStatus::Stopped.can_transition_to(&ProcessStatus::Errored));
    }

    #[test]
    fn test_invalid_errored_to_stopped() {
        assert!(!ProcessStatus::Errored.can_transition_to(&ProcessStatus::Stopped));
    }

    #[test]
    fn test_invalid_online_to_stopped() {
        assert!(!ProcessStatus::Online.can_transition_to(&ProcessStatus::Stopped));
    }

    // -- ProcessInfo transitions ---------------------------------------------

    #[test]
    fn test_process_info_transition() {
        let config = ProcessConfig {
            name: "test-app".to_string(),
            command: "node".to_string(),
            ..Default::default()
        };
        let info = ProcessInfo::new(config, 0);
        assert_eq!(info.status, ProcessStatus::Stopped);

        let starting = info.transition_to(ProcessStatus::Starting).unwrap();
        assert_eq!(starting.status, ProcessStatus::Starting);

        let online = starting.transition_to(ProcessStatus::Online).unwrap();
        assert_eq!(online.status, ProcessStatus::Online);
    }

    #[test]
    fn test_process_info_invalid_transition_returns_error() {
        let config = ProcessConfig::default();
        let info = ProcessInfo::new(config, 0);
        let result = info.transition_to(ProcessStatus::Online);
        assert!(result.is_err());
    }

    // -- Immutability --------------------------------------------------------

    #[test]
    fn test_transition_original_unchanged() {
        let config = ProcessConfig {
            name: "immutable-test".to_string(),
            command: "echo".to_string(),
            ..Default::default()
        };
        let original = ProcessInfo::new(config, 0);
        let _transitioned = original.transition_to(ProcessStatus::Starting).unwrap();
        // original must still be Stopped
        assert_eq!(original.status, ProcessStatus::Stopped);
    }

    // -- Display strings -----------------------------------------------------

    #[test]
    fn test_status_display_strings() {
        assert_eq!(ProcessStatus::Stopped.to_string(), "stopped");
        assert_eq!(ProcessStatus::Starting.to_string(), "starting");
        assert_eq!(ProcessStatus::Online.to_string(), "online");
        assert_eq!(ProcessStatus::Stopping.to_string(), "stopping");
        assert_eq!(ProcessStatus::Errored.to_string(), "errored");
    }

    // -- Default config values -----------------------------------------------

    #[test]
    fn test_default_config_values() {
        let cfg = ProcessConfig::default();
        assert_eq!(cfg.instances, 1);
        assert_eq!(cfg.max_restarts, 15);
        assert_eq!(cfg.min_uptime_ms, 1000);
        assert_eq!(cfg.restart_delay_ms, 100);
        assert_eq!(cfg.grace_period_ms, 5000);
        assert!(cfg.cron_restart.is_none());
        assert!(cfg.interpreter.is_none());
    }

    // -- Serialisation roundtrip ---------------------------------------------

    #[test]
    fn test_process_info_serialization_roundtrip() {
        let config = ProcessConfig {
            name: "api".to_string(),
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            ..Default::default()
        };
        let info = ProcessInfo::new(config, 0);
        let json = serde_json::to_string(&info).expect("serialize");
        let decoded: ProcessInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.config.name, "api");
        assert_eq!(decoded.status, ProcessStatus::Stopped);
    }

    // -- format_uptime with no start time ------------------------------------

    #[test]
    fn test_format_uptime_no_start_time() {
        let config = ProcessConfig::default();
        let info = ProcessInfo::new(config, 0);
        assert_eq!(info.format_uptime(), "N/A");
    }
}
