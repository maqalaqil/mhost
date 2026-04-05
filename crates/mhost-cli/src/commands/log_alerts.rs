use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::output::{print_error, print_success};

// ---------------------------------------------------------------------------
// Data model (immutable value objects)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogAlert {
    pub id: String,
    pub process: String,
    pub pattern: String,
    pub notify: String,
    pub cooldown_secs: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogAlertsFile {
    alerts: Vec<LogAlert>,
}

impl LogAlertsFile {
    fn empty() -> Self {
        Self { alerts: Vec::new() }
    }
}

// ---------------------------------------------------------------------------
// File helpers
// ---------------------------------------------------------------------------

fn alerts_path() -> std::path::PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".mhost")
        .join("log-alerts.json")
}

fn load_alerts(path: &Path) -> LogAlertsFile {
    if !path.exists() {
        return LogAlertsFile::empty();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(LogAlertsFile::empty)
}

fn save_alerts(path: &Path, file: &LogAlertsFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(file).map_err(|e| format!("Failed to serialize: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("Failed to write log-alerts.json: {e}"))
}

fn generate_id() -> String {
    let rand_part: String = uuid::Uuid::new_v4().to_string()[..8].to_string();
    format!("la_{rand_part}")
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Add a new log alert.
pub fn run_add(
    process: &str,
    pattern: &str,
    notify: &str,
    cooldown_secs: u64,
) -> Result<(), String> {
    let path = alerts_path();
    let current = load_alerts(&path);

    let alert = LogAlert {
        id: generate_id(),
        process: process.to_string(),
        pattern: pattern.to_string(),
        notify: notify.to_string(),
        cooldown_secs,
        enabled: true,
    };

    let updated = LogAlertsFile {
        alerts: {
            let mut new_alerts = current.alerts;
            new_alerts.push(alert.clone());
            new_alerts
        },
    };

    save_alerts(&path, &updated)?;
    print_success(&format!(
        "Added log alert {} for process '{}' (pattern: \"{}\", notify: {})",
        alert.id, process, pattern, notify
    ));

    Ok(())
}

/// List all configured log alerts.
pub fn run_list() -> Result<(), String> {
    let path = alerts_path();
    let file = load_alerts(&path);

    if file.alerts.is_empty() {
        println!("No log alerts configured.");
        println!(
            "Add one with: mhost log-alert add <process> --pattern \"error\" --notify telegram"
        );
        return Ok(());
    }

    println!(
        "{:<14} {:<12} {:<20} {:<12} {:<10} {}",
        "ID", "Process", "Pattern", "Notify", "Cooldown", "Enabled"
    );
    println!("{}", "-".repeat(78));

    for alert in &file.alerts {
        println!(
            "{:<14} {:<12} {:<20} {:<12} {:<10} {}",
            alert.id,
            alert.process,
            truncate(&alert.pattern, 18),
            alert.notify,
            format!("{}s", alert.cooldown_secs),
            if alert.enabled { "yes" } else { "no" },
        );
    }

    Ok(())
}

/// Remove a log alert by ID.
pub fn run_remove(id: &str) -> Result<(), String> {
    let path = alerts_path();
    let current = load_alerts(&path);

    let original_len = current.alerts.len();
    let remaining: Vec<LogAlert> = current.alerts.into_iter().filter(|a| a.id != id).collect();

    if remaining.len() == original_len {
        print_error(&format!("No alert found with ID '{id}'"));
        return Ok(());
    }

    let updated = LogAlertsFile { alerts: remaining };
    save_alerts(&path, &updated)?;
    print_success(&format!("Removed log alert '{id}'"));

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max - 3])
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_file_loads() {
        let f = LogAlertsFile::empty();
        assert!(f.alerts.is_empty());
    }

    #[test]
    fn test_generate_id_prefix() {
        let id = generate_id();
        assert!(id.starts_with("la_"));
        assert!(id.len() > 3);
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let result = truncate("a very long pattern string", 10);
        assert_eq!(result, "a very ...");
    }

    #[test]
    fn test_alert_id_format() {
        let id = generate_id();
        assert!(
            id.starts_with("la_"),
            "Alert ID should start with 'la_', got: {id}"
        );
        assert!(id.len() > 3, "Alert ID should have content after prefix");
    }

    #[test]
    fn test_alert_id_uniqueness() {
        let id1 = generate_id();
        let id2 = generate_id();
        assert_ne!(id1, id2, "Two generated IDs should be unique");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let alert = LogAlert {
            id: "la_test123".to_string(),
            process: "api".to_string(),
            pattern: "error|FATAL".to_string(),
            notify: "telegram".to_string(),
            cooldown_secs: 60,
            enabled: true,
        };
        let file = LogAlertsFile {
            alerts: vec![alert],
        };
        let json = serde_json::to_string(&file).unwrap();
        let parsed: LogAlertsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.alerts.len(), 1);
        assert_eq!(parsed.alerts[0].id, "la_test123");
    }
}
