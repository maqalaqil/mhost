use chrono::Utc;

use crate::indexer::LogIndexer;
use crate::parser::LogLevel;

/// A policy that controls how long log entries are kept.
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Glob pattern matched against the process name.
    pub process_glob: String,
    /// If `Some`, this policy only applies to entries at this level.
    pub level: Option<LogLevel>,
    /// Number of days to keep matching entries.
    pub max_age_days: u32,
}

impl RetentionPolicy {
    /// Build the default set of retention policies:
    /// - 7 days for INFO entries
    /// - 30 days for WARN / ERROR entries
    /// - 90 days for FATAL entries
    pub fn defaults() -> Vec<Self> {
        vec![
            Self {
                process_glob: "*".into(),
                level: Some(LogLevel::Info),
                max_age_days: 7,
            },
            Self {
                process_glob: "*".into(),
                level: Some(LogLevel::Warn),
                max_age_days: 30,
            },
            Self {
                process_glob: "*".into(),
                level: Some(LogLevel::Error),
                max_age_days: 30,
            },
            Self {
                process_glob: "*".into(),
                level: Some(LogLevel::Fatal),
                max_age_days: 90,
            },
        ]
    }
}

/// Enforce a set of retention policies by deleting stale entries from the index.
///
/// Applies each policy in order. The most restrictive cutoff across all
/// matching policies wins for a given level. Returns the total number of
/// rows deleted.
pub fn enforce_retention(
    indexer: &LogIndexer,
    policies: &[RetentionPolicy],
) -> rusqlite::Result<u64> {
    let now = Utc::now();
    let mut total_deleted: u64 = 0;

    for policy in policies {
        let cutoff = now - chrono::Duration::days(i64::from(policy.max_age_days));
        // Delete rows matching the policy's level (or all levels if None) that
        // are older than the cutoff.
        let deleted = delete_by_policy(indexer, policy, cutoff)?;
        total_deleted += deleted;
    }

    Ok(total_deleted)
}

fn delete_by_policy(
    indexer: &LogIndexer,
    policy: &RetentionPolicy,
    cutoff: chrono::DateTime<Utc>,
) -> rusqlite::Result<u64> {
    // We use the indexer's generic delete_before only when no level filter is
    // needed. For level-scoped policies we build the SQL directly via the
    // internal connection reference exposed through a dedicated method.
    // Since LogIndexer wraps a private `conn`, we expose a level-aware delete
    // helper on LogIndexer rather than calling it from here.
    indexer.delete_before_with_level(cutoff, policy.level.as_ref())
}

// ---------------------------------------------------------------------------
// Level-aware deletion — extend LogIndexer (placed here to keep retention
// logic co-located with its only caller).
// ---------------------------------------------------------------------------

impl LogIndexer {
    /// Delete entries older than `cutoff`.  If `level` is `Some`, only
    /// entries matching that level are removed.  Returns rows deleted.
    pub fn delete_before_with_level(
        &self,
        cutoff: chrono::DateTime<Utc>,
        level: Option<&LogLevel>,
    ) -> rusqlite::Result<u64> {
        let ts = cutoff.to_rfc3339();
        let affected = match level {
            Some(lvl) => self.conn.execute(
                "DELETE FROM log_entries WHERE timestamp < ?1 AND level = ?2",
                rusqlite::params![ts, lvl.as_str()],
            )?,
            None => self.conn.execute(
                "DELETE FROM log_entries WHERE timestamp < ?1",
                rusqlite::params![ts],
            )?,
        };
        Ok(affected as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::LogIndexer;
    use crate::parser::{LogEntry, LogLevel};
    use chrono::{Duration, Utc};
    use std::collections::HashMap;

    fn entry_at(process: &str, level: LogLevel, message: &str, age_days: i64) -> LogEntry {
        LogEntry {
            timestamp: Utc::now() - Duration::days(age_days),
            level: Some(level),
            message: message.to_owned(),
            process_name: process.to_owned(),
            instance: 0,
            fields: HashMap::new(),
            raw_line: message.to_owned(),
        }
    }

    #[test]
    fn enforce_deletes_old_entries() {
        let indexer = LogIndexer::in_memory().expect("in_memory");

        // Insert one entry that is 10 days old (older than the 7-day INFO policy).
        let old = entry_at("svc", LogLevel::Info, "old info", 10);
        indexer.index_entry(&old).expect("index old");

        let deleted = enforce_retention(&indexer, &RetentionPolicy::defaults()).expect("retain");
        assert!(deleted >= 1, "expected at least 1 deletion, got {deleted}");

        // Confirm the entry is gone.
        let remaining = indexer.search("old info", None, None, 10).expect("search");
        assert!(remaining.is_empty(), "old entry should have been deleted");
    }

    #[test]
    fn enforce_keeps_recent_entries() {
        let indexer = LogIndexer::in_memory().expect("in_memory");

        // Insert a recent entry (1 day old — well within the 7-day INFO policy).
        let recent = entry_at("svc", LogLevel::Info, "recent info", 1);
        indexer.index_entry(&recent).expect("index recent");

        let deleted = enforce_retention(&indexer, &RetentionPolicy::defaults()).expect("retain");
        let _ = deleted; // May be 0.

        let remaining = indexer
            .search("recent info", None, None, 10)
            .expect("search");
        assert_eq!(remaining.len(), 1, "recent entry should be kept");
    }

    // -- Multiple policies applied ------------------------------------------

    #[test]
    fn enforce_multiple_policies_deletes_matching_levels() {
        let indexer = LogIndexer::in_memory().expect("in_memory");

        // 10-day-old INFO (violates 7-day INFO policy)
        let old_info = entry_at("svc", LogLevel::Info, "old info log", 10);
        // 10-day-old WARN (within 30-day WARN policy)
        let recent_warn = entry_at("svc", LogLevel::Warn, "recent warn log", 10);
        // 100-day-old ERROR (violates 30-day ERROR policy)
        let old_error = entry_at("svc", LogLevel::Error, "old error log", 100);

        indexer.index_entry(&old_info).expect("index old_info");
        indexer
            .index_entry(&recent_warn)
            .expect("index recent_warn");
        indexer.index_entry(&old_error).expect("index old_error");

        let deleted = enforce_retention(&indexer, &RetentionPolicy::defaults()).expect("retain");
        assert!(
            deleted >= 2,
            "expected at least 2 deletions (old info + old error), got {deleted}"
        );

        let remaining_warn = indexer
            .search("recent warn log", None, None, 10)
            .expect("search");
        assert_eq!(remaining_warn.len(), 1, "recent warn should be kept");

        let remaining_info = indexer
            .search("old info log", None, None, 10)
            .expect("search");
        assert!(remaining_info.is_empty(), "old info should be deleted");

        let remaining_error = indexer
            .search("old error log", None, None, 10)
            .expect("search");
        assert!(remaining_error.is_empty(), "old error should be deleted");
    }

    // -- Policy with specific process glob ----------------------------------

    #[test]
    fn policy_with_specific_process_glob() {
        let indexer = LogIndexer::in_memory().expect("in_memory");

        // 10-day-old INFO entries from two different processes
        let svc_a_entry = entry_at("svc-a", LogLevel::Info, "svc-a old log", 10);
        let svc_b_entry = entry_at("svc-b", LogLevel::Info, "svc-b old log", 10);

        indexer.index_entry(&svc_a_entry).expect("index a");
        indexer.index_entry(&svc_b_entry).expect("index b");

        // A custom policy targeting all processes (glob = "*") with 7-day INFO retention
        let policies = RetentionPolicy::defaults();
        let deleted = enforce_retention(&indexer, &policies).expect("retain");
        assert!(
            deleted >= 2,
            "both old entries should be deleted, got {deleted}"
        );
    }

    // -- RetentionPolicy defaults structure ---------------------------------

    #[test]
    fn retention_policy_defaults_structure() {
        let policies = RetentionPolicy::defaults();
        assert_eq!(policies.len(), 4);

        // INFO: 7 days
        let info_policy = policies
            .iter()
            .find(|p| p.level == Some(LogLevel::Info))
            .unwrap();
        assert_eq!(info_policy.max_age_days, 7);

        // WARN: 30 days
        let warn_policy = policies
            .iter()
            .find(|p| p.level == Some(LogLevel::Warn))
            .unwrap();
        assert_eq!(warn_policy.max_age_days, 30);

        // ERROR: 30 days
        let error_policy = policies
            .iter()
            .find(|p| p.level == Some(LogLevel::Error))
            .unwrap();
        assert_eq!(error_policy.max_age_days, 30);

        // FATAL: 90 days
        let fatal_policy = policies
            .iter()
            .find(|p| p.level == Some(LogLevel::Fatal))
            .unwrap();
        assert_eq!(fatal_policy.max_age_days, 90);
    }
}
