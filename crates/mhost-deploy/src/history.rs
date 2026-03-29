use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;

pub struct DeployRecord {
    pub env: String,
    pub commit_hash: String,
    pub timestamp: DateTime<Utc>,
    pub status: String,
    pub message: Option<String>,
}

pub struct DeployHistory {
    conn: Connection,
}

impl DeployHistory {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        let history = Self { conn };
        history.init_tables();
        Ok(history)
    }

    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        let history = Self { conn };
        history.init_tables();
        Ok(history)
    }

    fn init_tables(&self) {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS deploys (
                    id          INTEGER PRIMARY KEY AUTOINCREMENT,
                    env         TEXT NOT NULL,
                    commit_hash TEXT NOT NULL,
                    timestamp   TEXT NOT NULL,
                    status      TEXT NOT NULL,
                    message     TEXT
                );",
            )
            .expect("deploy history table init");
    }

    pub fn record(
        &self,
        env: &str,
        commit: &str,
        status: &str,
        message: Option<&str>,
    ) {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO deploys (env, commit_hash, timestamp, status, message)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![env, commit, now, status, message],
            )
            .expect("insert deploy record");
    }

    pub fn list(&self, env: &str, limit: u32) -> Vec<DeployRecord> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT env, commit_hash, timestamp, status, message
                 FROM deploys
                 WHERE env = ?1
                 ORDER BY id DESC
                 LIMIT ?2",
            )
            .expect("prepare list query");

        stmt.query_map(params![env, limit], |row| {
            let ts_str: String = row.get(2)?;
            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            Ok(DeployRecord {
                env: row.get(0)?,
                commit_hash: row.get(1)?,
                timestamp,
                status: row.get(3)?,
                message: row.get(4)?,
            })
        })
        .expect("query deploys")
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn last_successful(&self, env: &str) -> Option<DeployRecord> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT env, commit_hash, timestamp, status, message
                 FROM deploys
                 WHERE env = ?1 AND status = 'success'
                 ORDER BY id DESC
                 LIMIT 1",
            )
            .ok()?;

        let result = stmt
            .query_map(params![env], |row| {
                let ts_str: String = row.get(2)?;
                let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                Ok(DeployRecord {
                    env: row.get(0)?,
                    commit_hash: row.get(1)?,
                    timestamp,
                    status: row.get(3)?,
                    message: row.get(4)?,
                })
            })
            .ok()?
            .filter_map(|r| r.ok())
            .next();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_list() {
        let history = DeployHistory::in_memory().unwrap();

        history.record("production", "abc123", "success", None);
        history.record("production", "def456", "failed", Some("build error"));
        history.record("staging", "xyz789", "success", None);

        let prod_records = history.list("production", 10);
        assert_eq!(prod_records.len(), 2);
        // newest first
        assert_eq!(prod_records[0].commit_hash, "def456");
        assert_eq!(prod_records[1].commit_hash, "abc123");

        let staging_records = history.list("staging", 10);
        assert_eq!(staging_records.len(), 1);
        assert_eq!(staging_records[0].commit_hash, "xyz789");
    }

    #[test]
    fn list_respects_limit() {
        let history = DeployHistory::in_memory().unwrap();
        for i in 0..5 {
            history.record("production", &format!("commit{}", i), "success", None);
        }
        let records = history.list("production", 3);
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn last_successful_returns_most_recent_success() {
        let history = DeployHistory::in_memory().unwrap();

        history.record("production", "first_success", "success", None);
        history.record("production", "then_failed", "failed", Some("oops"));
        history.record("production", "second_success", "success", None);

        let last = history.last_successful("production").unwrap();
        assert_eq!(last.commit_hash, "second_success");
        assert_eq!(last.status, "success");
    }

    #[test]
    fn last_successful_returns_none_when_no_success() {
        let history = DeployHistory::in_memory().unwrap();
        history.record("production", "bad_commit", "failed", Some("error"));

        let last = history.last_successful("production");
        assert!(last.is_none());
    }

    #[test]
    fn last_successful_returns_none_for_unknown_env() {
        let history = DeployHistory::in_memory().unwrap();
        let last = history.last_successful("nonexistent");
        assert!(last.is_none());
    }

    #[test]
    fn record_with_message() {
        let history = DeployHistory::in_memory().unwrap();
        history.record("production", "commit_abc", "success", Some("rollback"));

        let records = history.list("production", 10);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].message.as_deref(), Some("rollback"));
    }

    #[test]
    fn list_with_limit_one_returns_newest() {
        let history = DeployHistory::in_memory().unwrap();
        history.record("production", "first", "success", None);
        history.record("production", "second", "success", None);
        history.record("production", "third", "success", None);

        let records = history.list("production", 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].commit_hash, "third", "limit=1 should return newest");
    }

    #[test]
    fn list_for_unknown_env_is_empty() {
        let history = DeployHistory::in_memory().unwrap();
        history.record("production", "abc", "success", None);

        let records = history.list("staging", 10);
        assert!(records.is_empty(), "unknown env should return empty list");
    }

    #[test]
    fn multiple_environments_are_isolated() {
        let history = DeployHistory::in_memory().unwrap();
        history.record("production", "prod1", "success", None);
        history.record("production", "prod2", "success", None);
        history.record("staging", "stage1", "failed", Some("lint error"));
        history.record("dev", "dev1", "success", None);

        assert_eq!(history.list("production", 10).len(), 2);
        assert_eq!(history.list("staging", 10).len(), 1);
        assert_eq!(history.list("dev", 10).len(), 1);
    }

    #[test]
    fn last_successful_ignores_other_envs() {
        let history = DeployHistory::in_memory().unwrap();
        history.record("staging", "staging_good", "success", None);
        // production has no success records.

        let last = history.last_successful("production");
        assert!(last.is_none(), "production has no successes");
    }

    #[test]
    fn last_successful_returns_none_when_empty() {
        let history = DeployHistory::in_memory().unwrap();
        let last = history.last_successful("production");
        assert!(last.is_none());
    }

    #[test]
    fn deploy_ordering_newest_first() {
        let history = DeployHistory::in_memory().unwrap();
        for i in 0..5 {
            history.record("production", &format!("commit_{i}"), "success", None);
        }
        let records = history.list("production", 10);
        assert_eq!(records[0].commit_hash, "commit_4", "newest should come first");
        assert_eq!(records[4].commit_hash, "commit_0", "oldest should be last");
    }

    #[test]
    fn record_with_no_message_stores_none() {
        let history = DeployHistory::in_memory().unwrap();
        history.record("production", "hash1", "success", None);

        let records = history.list("production", 10);
        assert_eq!(records.len(), 1);
        assert!(records[0].message.is_none(), "message should be None");
    }

    #[test]
    fn last_successful_skips_all_failures() {
        let history = DeployHistory::in_memory().unwrap();
        history.record("production", "fail1", "failed", Some("error"));
        history.record("production", "fail2", "failed", Some("error2"));

        let last = history.last_successful("production");
        assert!(last.is_none(), "all deploys failed, should be None");
    }
}
