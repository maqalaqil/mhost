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

    fn entry_at(
        process: &str,
        level: LogLevel,
        message: &str,
        age_days: i64,
    ) -> LogEntry {
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

        let remaining = indexer.search("recent info", None, None, 10).expect("search");
        assert_eq!(remaining.len(), 1, "recent entry should be kept");
    }
}
