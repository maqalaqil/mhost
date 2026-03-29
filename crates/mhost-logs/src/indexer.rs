use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use crate::parser::{LogEntry, LogLevel};

/// SQLite FTS5-backed log indexer.
pub struct LogIndexer {
    pub(crate) conn: Connection,
}

impl LogIndexer {
    /// Open (or create) a persistent database at `path`.
    pub fn new(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        let indexer = Self { conn };
        indexer.create_schema()?;
        Ok(indexer)
    }

    /// Create an in-memory database (useful for tests).
    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let indexer = Self { conn };
        indexer.create_schema()?;
        Ok(indexer)
    }

    fn create_schema(&self) -> rusqlite::Result<()> {
        // Full-content FTS5 table — lets us query, delete, and update rows.
        self.conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS log_entries USING fts5(
                process_name,
                instance UNINDEXED,
                level,
                message,
                fields_json,
                raw_line,
                timestamp UNINDEXED
            );",
        )
    }

    /// Insert a `LogEntry` into the index.
    pub fn index_entry(&self, entry: &LogEntry) -> rusqlite::Result<()> {
        let level_str = entry
            .level
            .as_ref()
            .map(|l| l.as_str().to_owned())
            .unwrap_or_default();

        let fields_json = serde_json::to_string(&entry.fields).unwrap_or_else(|_| "{}".into());
        let ts = entry.timestamp.to_rfc3339();

        self.conn.execute(
            "INSERT INTO log_entries
                (process_name, instance, level, message, fields_json, raw_line, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.process_name,
                entry.instance,
                level_str,
                entry.message,
                fields_json,
                entry.raw_line,
                ts,
            ],
        )?;
        Ok(())
    }

    /// Full-text search across all indexed columns.
    ///
    /// Optionally filter by `process` name and/or entries after `since`.
    /// Returns up to `limit` matching entries.
    pub fn search(
        &self,
        query: &str,
        process: Option<&str>,
        since: Option<DateTime<Utc>>,
        limit: usize,
    ) -> rusqlite::Result<Vec<LogEntry>> {
        // Build the WHERE clause dynamically.
        let mut conditions = vec![format!("log_entries MATCH '{}'", query.replace('\'', "''"))];
        let mut extra_conditions: Vec<String> = Vec::new();

        if let Some(p) = process {
            extra_conditions.push(format!("process_name = '{}'", p.replace('\'', "''")));
        }
        if let Some(s) = since {
            extra_conditions.push(format!("timestamp >= '{}'", s.to_rfc3339()));
        }

        let match_clause = conditions.remove(0);
        let where_clause = if extra_conditions.is_empty() {
            match_clause
        } else {
            format!("{match_clause} AND {}", extra_conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT process_name, instance, level, message, fields_json, raw_line, timestamp
             FROM log_entries
             WHERE {where_clause}
             LIMIT {limit}"
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            let process_name: String = row.get(0)?;
            let instance: u32 = row.get(1)?;
            let level_str: String = row.get(2)?;
            let message: String = row.get(3)?;
            let fields_json: String = row.get(4)?;
            let raw_line: String = row.get(5)?;
            let ts_str: String = row.get(6)?;
            Ok((
                process_name,
                instance,
                level_str,
                message,
                fields_json,
                raw_line,
                ts_str,
            ))
        })?;

        let mut entries = Vec::new();
        for row in rows {
            let (process_name, instance, level_str, message, fields_json, raw_line, ts_str) = row?;

            let level = if level_str.is_empty() {
                None
            } else {
                LogLevel::from_str(&level_str)
            };

            let fields: HashMap<String, serde_json::Value> =
                serde_json::from_str(&fields_json).unwrap_or_default();

            let timestamp = ts_str
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());

            entries.push(LogEntry {
                timestamp,
                level,
                message,
                process_name,
                instance,
                fields,
                raw_line,
            });
        }

        Ok(entries)
    }

    /// Count entries grouped by a field value.
    ///
    /// `field` must be one of `process_name`, `level`, or `instance`.
    /// Optionally filter by process and/or `since`.
    pub fn count_by(
        &self,
        field: &str,
        process: Option<&str>,
        since: Option<DateTime<Utc>>,
    ) -> rusqlite::Result<Vec<(String, u64)>> {
        // Allowlist field names to prevent SQL injection.
        let safe_field = match field {
            "process_name" | "level" | "instance" => field,
            _ => return Err(rusqlite::Error::InvalidQuery),
        };

        let mut conditions: Vec<String> = Vec::new();
        if let Some(p) = process {
            conditions.push(format!("process_name = '{}'", p.replace('\'', "''")));
        }
        if let Some(s) = since {
            conditions.push(format!("timestamp >= '{}'", s.to_rfc3339()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT {safe_field}, COUNT(*) FROM log_entries {where_clause} GROUP BY {safe_field}"
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            let key: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((key, count as u64))
        })?;

        rows.collect()
    }

    /// Delete entries older than `cutoff`. Returns the number of rows deleted.
    pub fn delete_before(&self, cutoff: DateTime<Utc>) -> rusqlite::Result<u64> {
        let ts = cutoff.to_rfc3339();
        let affected = self.conn.execute(
            "DELETE FROM log_entries WHERE timestamp < ?1",
            params![ts],
        )?;
        Ok(affected as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_line;

    fn make_entry(process: &str, level: &str, message: &str) -> LogEntry {
        let raw = format!(
            r#"{{"level":"{level}","message":"{message}","timestamp":"2024-01-15T10:00:00Z"}}"#
        );
        parse_line(&raw, process, 0)
    }

    #[test]
    fn index_and_search() {
        let indexer = LogIndexer::in_memory().expect("in_memory");

        let entry = make_entry("myapp", "ERROR", "database connection failed");
        indexer.index_entry(&entry).expect("index");

        let results = indexer
            .search("database", None, None, 10)
            .expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].message, "database connection failed");
    }

    #[test]
    fn search_with_process_filter() {
        let indexer = LogIndexer::in_memory().expect("in_memory");

        indexer
            .index_entry(&make_entry("app-a", "ERROR", "crash in app-a"))
            .expect("index a");
        indexer
            .index_entry(&make_entry("app-b", "ERROR", "crash in app-b"))
            .expect("index b");

        let results = indexer
            .search("crash", Some("app-a"), None, 10)
            .expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].process_name, "app-a");
    }

    #[test]
    fn count_by_level() {
        let indexer = LogIndexer::in_memory().expect("in_memory");

        indexer
            .index_entry(&make_entry("svc", "ERROR", "err 1"))
            .expect("index 1");
        indexer
            .index_entry(&make_entry("svc", "ERROR", "err 2"))
            .expect("index 2");
        indexer
            .index_entry(&make_entry("svc", "INFO", "info 1"))
            .expect("index 3");

        let mut counts = indexer
            .count_by("level", Some("svc"), None)
            .expect("count_by");
        counts.sort_by_key(|(k, _)| k.clone());

        assert_eq!(counts.len(), 2);
        let error_count = counts.iter().find(|(k, _)| k == "ERROR").map(|(_, v)| *v);
        assert_eq!(error_count, Some(2));
        let info_count = counts.iter().find(|(k, _)| k == "INFO").map(|(_, v)| *v);
        assert_eq!(info_count, Some(1));
    }
}
