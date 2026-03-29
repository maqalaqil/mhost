use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// A single audit record written for every bot command invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub user_id: i64,
    pub username: String,
    /// Full command string, e.g. `"/stop api-server"`.
    pub command: String,
    /// `"ok"` on success, otherwise an error message.
    pub result: String,
    /// `"telegram"` or `"discord"`.
    pub platform: String,
}

/// Append-only JSONL audit log backed by a file on disk.
pub struct AuditLog {
    path: PathBuf,
}

impl AuditLog {
    /// Create a new `AuditLog` that writes to `path`.
    ///
    /// The file (and its parent directories) are created on the first
    /// [`log`] call if they do not already exist.
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    /// Append `entry` as a single JSON line.
    ///
    /// Errors are silently ignored to avoid disrupting normal bot operation.
    pub fn log(&self, entry: &AuditEntry) {
        let line = serde_json::to_string(entry).unwrap_or_default();
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{}", line);
        }
    }

    /// Return up to `count` most-recent entries (newest first).
    pub fn recent(&self, count: usize) -> Vec<AuditEntry> {
        std::fs::read_to_string(&self.path)
            .ok()
            .map(|content| {
                content
                    .lines()
                    .rev()
                    .take(count)
                    .filter_map(|l| serde_json::from_str(l).ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn tmp_path(name: &str) -> PathBuf {
        env::temp_dir().join(format!("mhost-bot-audit-test-{name}.jsonl"))
    }

    fn sample_entry(command: &str, result: &str) -> AuditEntry {
        AuditEntry {
            timestamp: Utc::now(),
            user_id: 100,
            username: "alice".into(),
            command: command.into(),
            result: result.into(),
            platform: "telegram".into(),
        }
    }

    // -----------------------------------------------------------------------
    // log + recent
    // -----------------------------------------------------------------------

    #[test]
    fn test_log_and_retrieve_entry() {
        let path = tmp_path("log-retrieve");
        let _ = std::fs::remove_file(&path);

        let log = AuditLog::new(&path);
        let entry = sample_entry("/status", "ok");
        log.log(&entry);

        let entries = log.recent(10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].command, "/status");
        assert_eq!(entries[0].result, "ok");
        assert_eq!(entries[0].user_id, 100);
        assert_eq!(entries[0].username, "alice");
        assert_eq!(entries[0].platform, "telegram");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_recent_returns_last_n() {
        let path = tmp_path("recent-n");
        let _ = std::fs::remove_file(&path);

        let log = AuditLog::new(&path);
        for i in 0..10_u32 {
            log.log(&sample_entry(&format!("/cmd{i}"), "ok"));
        }

        let entries = log.recent(3);
        assert_eq!(entries.len(), 3);
        // recent() reverses lines so newest first
        assert_eq!(entries[0].command, "/cmd9");
        assert_eq!(entries[1].command, "/cmd8");
        assert_eq!(entries[2].command, "/cmd7");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_recent_on_empty_file_returns_empty() {
        let path = tmp_path("empty");
        let _ = std::fs::remove_file(&path);

        let log = AuditLog::new(&path);
        assert!(log.recent(5).is_empty());
    }

    #[test]
    fn test_recent_on_missing_file_returns_empty() {
        let path = tmp_path("no-such-file-xyz999");
        let log = AuditLog::new(&path);
        assert!(log.recent(10).is_empty());
    }

    #[test]
    fn test_multiple_entries_appended() {
        let path = tmp_path("multi-append");
        let _ = std::fs::remove_file(&path);

        let log = AuditLog::new(&path);
        log.log(&sample_entry("/start api", "ok"));
        log.log(&sample_entry("/stop api", "ok"));
        log.log(&sample_entry("/restart api", "Error: not found"));

        let entries = log.recent(100);
        assert_eq!(entries.len(), 3);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_log_creates_parent_directories() {
        let base = env::temp_dir().join("mhost-bot-audit-nested-test");
        let _ = std::fs::remove_dir_all(&base);

        let path = base.join("sub").join("audit.jsonl");
        let log = AuditLog::new(&path);
        log.log(&sample_entry("/health api", "ok"));

        assert!(path.exists(), "log file should be created with parent dirs");
        let _ = std::fs::remove_dir_all(&base);
    }
}
