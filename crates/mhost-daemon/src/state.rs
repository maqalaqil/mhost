#![allow(dead_code, clippy::type_complexity)]
use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqlResult, Row};

use mhost_core::process::{ProcessConfig, ProcessInfo, ProcessStatus};

// ---------------------------------------------------------------------------
// StateStore
// ---------------------------------------------------------------------------

/// Thread-safe SQLite state store.
///
/// The inner `Connection` is wrapped in `Mutex` so that `StateStore` is both
/// `Send` and `Sync`, which lets it be held across `.await` points in async
/// supervisor methods without violating Rust's safety rules.
pub struct StateStore {
    conn: Mutex<Connection>,
}

impl StateStore {
    /// Open (or create) a SQLite database at `path`.
    pub fn open(path: &Path) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_tables()?;
        Ok(store)
    }

    /// Create an in-memory database (useful for tests).
    pub fn in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_tables()?;
        Ok(store)
    }

    /// Create the `processes` and `events` tables if they do not exist.
    pub fn init_tables(&self) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS processes (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                instance INTEGER NOT NULL,
                config_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'stopped',
                pid INTEGER,
                restart_count INTEGER NOT NULL DEFAULT 0,
                uptime_started TEXT,
                created_at TEXT NOT NULL,
                last_restart TEXT,
                exit_code INTEGER,
                UNIQUE(name, instance)
            );
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                process_name TEXT NOT NULL,
                event_type TEXT NOT NULL,
                message TEXT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                metadata_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_events_process ON events(process_name);
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
            "#,
        )
    }

    // -----------------------------------------------------------------------
    // Process operations
    // -----------------------------------------------------------------------

    /// Insert or update a process record keyed on (name, instance).
    pub fn upsert_process(&self, info: &ProcessInfo) -> SqlResult<()> {
        let config_json = serde_json::to_string(&info.config)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        let status_str = info.status.to_string();
        let created_at = info.created_at.to_rfc3339();
        let uptime_started = info.uptime_started.map(|dt| dt.to_rfc3339());
        let last_restart = info.last_restart.map(|dt| dt.to_rfc3339());

        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO processes
                (id, name, instance, config_json, status, pid, restart_count,
                 uptime_started, created_at, last_restart, exit_code)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(name, instance) DO UPDATE SET
                id             = excluded.id,
                config_json    = excluded.config_json,
                status         = excluded.status,
                pid            = excluded.pid,
                restart_count  = excluded.restart_count,
                uptime_started = excluded.uptime_started,
                created_at     = excluded.created_at,
                last_restart   = excluded.last_restart,
                exit_code      = excluded.exit_code
            "#,
            params![
                info.id,
                info.config.name,
                info.instance,
                config_json,
                status_str,
                info.pid,
                info.restart_count,
                uptime_started,
                created_at,
                last_restart,
                info.exit_code,
            ],
        )?;
        Ok(())
    }

    /// Retrieve a single process by (name, instance).
    pub fn get_process(&self, name: &str, instance: u32) -> SqlResult<Option<ProcessInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, instance, config_json, status, pid, restart_count,
                    uptime_started, created_at, last_restart, exit_code
             FROM processes WHERE name = ?1 AND instance = ?2",
        )?;

        let mut rows = stmt.query(params![name, instance])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_process_info(row)?)),
            None => Ok(None),
        }
    }

    /// Return all process records.
    pub fn list_processes(&self) -> SqlResult<Vec<ProcessInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, instance, config_json, status, pid, restart_count,
                    uptime_started, created_at, last_restart, exit_code
             FROM processes ORDER BY name, instance",
        )?;

        let rows = stmt.query_map([], row_to_process_info)?;
        rows.collect()
    }

    /// Delete all process records with the given name. Returns rows affected.
    pub fn delete_process(&self, name: &str) -> SqlResult<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM processes WHERE name = ?1", params![name])
    }

    /// Delete every process record. Returns rows affected.
    pub fn delete_all(&self) -> SqlResult<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM processes", [])
    }

    // -----------------------------------------------------------------------
    // Event operations
    // -----------------------------------------------------------------------

    /// Insert an event row for `process_name`.
    pub fn log_event(
        &self,
        process_name: &str,
        event_type: &str,
        message: Option<&str>,
    ) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (process_name, event_type, message) VALUES (?1, ?2, ?3)",
            params![process_name, event_type, message],
        )?;
        Ok(())
    }

    /// Return the most-recent `limit` events for `process_name`.
    /// Each tuple is (event_type, message, metadata_json, timestamp).
    pub fn get_events(
        &self,
        process_name: &str,
        limit: usize,
    ) -> SqlResult<Vec<(String, String, Option<String>, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT event_type, COALESCE(message, ''), metadata_json, timestamp
             FROM events
             WHERE process_name = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![process_name, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        rows.collect()
    }
}

// ---------------------------------------------------------------------------
// Row deserialiser helper
// ---------------------------------------------------------------------------

fn row_to_process_info(row: &Row) -> SqlResult<ProcessInfo> {
    let id: String = row.get(0)?;
    let _name: String = row.get(1)?;
    let instance: u32 = row.get::<_, i64>(2)? as u32;
    let config_json: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let pid: Option<u32> = row.get::<_, Option<i64>>(5)?.map(|v| v as u32);
    let restart_count: u32 = row.get::<_, i64>(6)? as u32;
    let uptime_started_str: Option<String> = row.get(7)?;
    let created_at_str: String = row.get(8)?;
    let last_restart_str: Option<String> = row.get(9)?;
    let exit_code: Option<i32> = row.get(10)?;

    let config: ProcessConfig = serde_json::from_str(&config_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let status = match status_str.as_str() {
        "online" => ProcessStatus::Online,
        "starting" => ProcessStatus::Starting,
        "stopping" => ProcessStatus::Stopping,
        "errored" => ProcessStatus::Errored,
        _ => ProcessStatus::Stopped,
    };

    let created_at: DateTime<Utc> = created_at_str.parse().unwrap_or_else(|_| Utc::now());
    let uptime_started: Option<DateTime<Utc>> = uptime_started_str.and_then(|s| s.parse().ok());
    let last_restart: Option<DateTime<Utc>> = last_restart_str.and_then(|s| s.parse().ok());

    Ok(ProcessInfo {
        id,
        config,
        status,
        pid,
        instance,
        restart_count,
        uptime_started,
        created_at,
        last_restart,
        exit_code,
        memory_bytes: None,
        cpu_percent: None,
        health_status: mhost_core::HealthStatus::Unknown,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mhost_core::process::ProcessConfig;

    fn make_config(name: &str) -> ProcessConfig {
        ProcessConfig {
            name: name.to_string(),
            command: "echo".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn upsert_and_get() {
        let store = StateStore::in_memory().unwrap();
        let info = ProcessInfo::new(make_config("api"), 0);
        store.upsert_process(&info).unwrap();

        let retrieved = store.get_process("api", 0).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, info.id);
        assert_eq!(retrieved.config.name, "api");
        assert_eq!(retrieved.status, ProcessStatus::Stopped);
    }

    #[test]
    fn list_processes() {
        let store = StateStore::in_memory().unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("api"), 0))
            .unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("worker"), 0))
            .unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("worker"), 1))
            .unwrap();

        let list = store.list_processes().unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn delete_process() {
        let store = StateStore::in_memory().unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("api"), 0))
            .unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("worker"), 0))
            .unwrap();

        let deleted = store.delete_process("api").unwrap();
        assert_eq!(deleted, 1);

        let list = store.list_processes().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].config.name, "worker");
    }

    #[test]
    fn upsert_updates_existing() {
        let store = StateStore::in_memory().unwrap();
        let info = ProcessInfo::new(make_config("api"), 0);
        store.upsert_process(&info).unwrap();

        let updated = ProcessInfo {
            status: ProcessStatus::Online,
            pid: Some(1234),
            restart_count: 2,
            ..info
        };
        store.upsert_process(&updated).unwrap();

        let retrieved = store.get_process("api", 0).unwrap().unwrap();
        assert_eq!(retrieved.status, ProcessStatus::Online);
        assert_eq!(retrieved.pid, Some(1234));
        assert_eq!(retrieved.restart_count, 2);

        assert_eq!(store.list_processes().unwrap().len(), 1);
    }

    #[test]
    fn events() {
        let store = StateStore::in_memory().unwrap();
        store
            .log_event("api", "started", Some("process started"))
            .unwrap();
        store
            .log_event("api", "stopped", Some("exit code 0"))
            .unwrap();
        store.log_event("worker", "started", None).unwrap();

        let events = store.get_events("api", 10).unwrap();
        assert_eq!(events.len(), 2);
        // Most recent first
        assert_eq!(events[0].0, "stopped");
        assert_eq!(events[1].0, "started");
    }

    #[test]
    fn get_nonexistent() {
        let store = StateStore::in_memory().unwrap();
        let result = store.get_process("nonexistent", 0).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn delete_all_removes_every_process() {
        let store = StateStore::in_memory().unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("api"), 0))
            .unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("worker"), 0))
            .unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("worker"), 1))
            .unwrap();

        let affected = store.delete_all().unwrap();
        assert_eq!(affected, 3);

        let list = store.list_processes().unwrap();
        assert!(list.is_empty(), "expected empty list after delete_all");
    }

    #[test]
    fn delete_all_on_empty_store_returns_zero() {
        let store = StateStore::in_memory().unwrap();
        let affected = store.delete_all().unwrap();
        assert_eq!(affected, 0);
    }

    #[test]
    fn multiple_events_for_same_process() {
        let store = StateStore::in_memory().unwrap();

        store.log_event("api", "started", Some("boot")).unwrap();
        store.log_event("api", "restarted", Some("crash")).unwrap();
        store
            .log_event("api", "restarted", Some("crash again"))
            .unwrap();
        store.log_event("api", "stopped", Some("graceful")).unwrap();

        let events = store.get_events("api", 100).unwrap();
        assert_eq!(events.len(), 4, "expected 4 events for api");
    }

    #[test]
    fn events_are_newest_first() {
        let store = StateStore::in_memory().unwrap();

        store.log_event("svc", "started", None).unwrap();
        store.log_event("svc", "crashed", None).unwrap();
        store.log_event("svc", "restarted", None).unwrap();

        let events = store.get_events("svc", 10).unwrap();
        // ORDER BY id DESC — newest inserted first.
        assert_eq!(events[0].0, "restarted");
        assert_eq!(events[1].0, "crashed");
        assert_eq!(events[2].0, "started");
    }

    #[test]
    fn events_limit_is_respected() {
        let store = StateStore::in_memory().unwrap();

        for i in 0..10 {
            store
                .log_event("svc", "tick", Some(&format!("tick {i}")))
                .unwrap();
        }

        let events = store.get_events("svc", 3).unwrap();
        assert_eq!(events.len(), 3, "limit should cap result at 3");
    }

    #[test]
    fn events_for_different_processes_are_independent() {
        let store = StateStore::in_memory().unwrap();

        store.log_event("api", "started", None).unwrap();
        store.log_event("worker", "started", None).unwrap();
        store.log_event("worker", "stopped", None).unwrap();

        let api_events = store.get_events("api", 10).unwrap();
        let worker_events = store.get_events("worker", 10).unwrap();

        assert_eq!(api_events.len(), 1);
        assert_eq!(worker_events.len(), 2);
    }

    #[test]
    fn upsert_updates_status_pid_and_restart_count() {
        let store = StateStore::in_memory().unwrap();
        let info = ProcessInfo::new(make_config("svc"), 0);
        store.upsert_process(&info).unwrap();

        // Change several fields at once.
        let updated = ProcessInfo {
            status: ProcessStatus::Errored,
            pid: Some(9999),
            restart_count: 5,
            exit_code: Some(1),
            ..info
        };
        store.upsert_process(&updated).unwrap();

        let retrieved = store.get_process("svc", 0).unwrap().unwrap();
        assert_eq!(retrieved.status, ProcessStatus::Errored);
        assert_eq!(retrieved.pid, Some(9999));
        assert_eq!(retrieved.restart_count, 5);
        assert_eq!(retrieved.exit_code, Some(1));
        // Still only one row.
        assert_eq!(store.list_processes().unwrap().len(), 1);
    }

    #[test]
    fn delete_process_does_not_affect_others() {
        let store = StateStore::in_memory().unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("a"), 0))
            .unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("b"), 0))
            .unwrap();
        store
            .upsert_process(&ProcessInfo::new(make_config("b"), 1))
            .unwrap();

        let deleted = store.delete_process("b").unwrap();
        assert_eq!(deleted, 2);

        let list = store.list_processes().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].config.name, "a");
    }
}
