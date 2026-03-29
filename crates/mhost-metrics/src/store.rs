use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, Result as SqlResult};
use tracing::{debug, error};

// ---------------------------------------------------------------------------
// MetricsStore
// ---------------------------------------------------------------------------

/// SQLite-backed time-series store for process metrics.
pub struct MetricsStore {
    conn: Connection,
}

impl MetricsStore {
    /// Open (or create) a store at the given path.
    /// Pass `":memory:"` for in-process test databases.
    pub fn open(path: &str) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.create_table()?;
        Ok(store)
    }

    fn create_table(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS metrics_ts (
                process_name TEXT NOT NULL,
                metric       TEXT NOT NULL,
                value        REAL NOT NULL,
                timestamp    TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_metrics_ts_lookup
                ON metrics_ts (process_name, metric, timestamp);",
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Write
    // -----------------------------------------------------------------------

    /// Insert a single metric data point.
    pub fn record(
        &self,
        name: &str,
        metric: &str,
        value: f64,
        timestamp: DateTime<Utc>,
    ) -> SqlResult<()> {
        let ts = timestamp.to_rfc3339();
        debug!(name, metric, value, ts, "recording metric");
        self.conn.execute(
            "INSERT INTO metrics_ts (process_name, metric, value, timestamp)
             VALUES (?1, ?2, ?3, ?4)",
            params![name, metric, value, ts],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read
    // -----------------------------------------------------------------------

    /// Query recorded data points for a process/metric since `since`.
    ///
    /// When `interval_seconds > 0` the results are bucketed (downsampled) by
    /// averaging values that fall within the same `interval_seconds` bucket.
    /// When `interval_seconds == 0` every row is returned.
    pub fn query(
        &self,
        name: &str,
        metric: &str,
        since: DateTime<Utc>,
        interval_seconds: u64,
    ) -> SqlResult<Vec<(DateTime<Utc>, f64)>> {
        let since_str = since.to_rfc3339();

        if interval_seconds == 0 {
            // Return raw rows.
            let mut stmt = self.conn.prepare(
                "SELECT timestamp, value FROM metrics_ts
                 WHERE process_name = ?1 AND metric = ?2 AND timestamp >= ?3
                 ORDER BY timestamp ASC",
            )?;
            let rows = stmt
                .query_map(params![name, metric, since_str], |row| {
                    let ts_str: String = row.get(0)?;
                    let value: f64 = row.get(1)?;
                    Ok((ts_str, value))
                })?
                .filter_map(|r| match r {
                    Ok((ts_str, value)) => match DateTime::parse_from_rfc3339(&ts_str) {
                        Ok(dt) => Some((dt.with_timezone(&Utc), value)),
                        Err(e) => {
                            error!("failed to parse timestamp '{}': {}", ts_str, e);
                            None
                        }
                    },
                    Err(e) => {
                        error!("row error: {}", e);
                        None
                    }
                })
                .collect();
            return Ok(rows);
        }

        // Downsampled query: use SQLite integer division to bucket timestamps.
        // We cast the unix epoch seconds to an integer bucket, then average.
        let mut stmt = self.conn.prepare(
            "SELECT
                 (CAST(strftime('%s', timestamp) AS INTEGER) / ?4) * ?4 AS bucket,
                 AVG(value)
             FROM metrics_ts
             WHERE process_name = ?1 AND metric = ?2 AND timestamp >= ?3
             GROUP BY bucket
             ORDER BY bucket ASC",
        )?;

        let interval_i64 = interval_seconds as i64;
        let rows = stmt
            .query_map(params![name, metric, since_str, interval_i64], |row| {
                let bucket_epoch: i64 = row.get(0)?;
                let avg_value: f64 = row.get(1)?;
                Ok((bucket_epoch, avg_value))
            })?
            .filter_map(|r| match r {
                Ok((epoch, value)) => {
                    let dt = DateTime::from_timestamp(epoch, 0).unwrap_or_else(Utc::now);
                    Some((dt, value))
                }
                Err(e) => {
                    error!("row error: {}", e);
                    None
                }
            })
            .collect();

        Ok(rows)
    }

    // -----------------------------------------------------------------------
    // Cleanup
    // -----------------------------------------------------------------------

    /// Delete all rows older than `max_age_days` days.
    pub fn cleanup(&self, max_age_days: u32) -> SqlResult<usize> {
        let cutoff = Utc::now() - Duration::days(max_age_days as i64);
        let cutoff_str = cutoff.to_rfc3339();
        let deleted = self.conn.execute(
            "DELETE FROM metrics_ts WHERE timestamp < ?1",
            params![cutoff_str],
        )?;
        debug!(deleted, max_age_days, "cleaned up old metrics");
        Ok(deleted)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as CDuration;

    fn in_memory_store() -> MetricsStore {
        MetricsStore::open(":memory:").expect("in-memory store")
    }

    #[test]
    fn record_and_query_roundtrip() {
        let store = in_memory_store();
        let now = Utc::now();

        store
            .record("api", "cpu_percent", 42.5, now)
            .expect("record");

        let rows = store
            .query("api", "cpu_percent", now - CDuration::seconds(1), 0)
            .expect("query");

        assert_eq!(rows.len(), 1);
        let (ts, val) = &rows[0];
        assert!((val - 42.5).abs() < 1e-6);
        // timestamp should round-trip within a second
        assert!(((*ts - now).num_milliseconds()).abs() < 1000);
    }

    #[test]
    fn query_returns_only_matching_process_and_metric() {
        let store = in_memory_store();
        let now = Utc::now();

        store.record("api", "cpu_percent", 10.0, now).unwrap();
        store.record("worker", "cpu_percent", 20.0, now).unwrap();
        store.record("api", "memory_bytes", 1024.0, now).unwrap();

        let rows = store
            .query("api", "cpu_percent", now - CDuration::seconds(1), 0)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert!((rows[0].1 - 10.0).abs() < 1e-6);
    }

    #[test]
    fn query_since_filters_old_rows() {
        let store = in_memory_store();
        let old = Utc::now() - CDuration::hours(2);
        let recent = Utc::now();
        let cutoff = Utc::now() - CDuration::hours(1);

        store.record("api", "cpu_percent", 1.0, old).unwrap();
        store.record("api", "cpu_percent", 2.0, recent).unwrap();

        let rows = store.query("api", "cpu_percent", cutoff, 0).unwrap();
        assert_eq!(rows.len(), 1, "only the recent row should be returned");
        assert!((rows[0].1 - 2.0).abs() < 1e-6);
    }

    #[test]
    fn query_with_downsampling_averages_within_bucket() {
        let store = in_memory_store();
        // Use a fixed epoch-aligned base so both points land in the same 60s bucket.
        // epoch 0 is 1970-01-01T00:00:00Z — bucket = 0/60 * 60 = 0
        // epoch 30 is also bucket 0 (30/60 * 60 = 0 in integer division).
        let base = DateTime::from_timestamp(0, 0).unwrap();
        store.record("api", "cpu_percent", 10.0, base).unwrap();
        store
            .record("api", "cpu_percent", 20.0, base + CDuration::seconds(30))
            .unwrap();

        let rows = store
            .query("api", "cpu_percent", base - CDuration::seconds(1), 60)
            .unwrap();

        // Should collapse to 1 bucket with average 15.0
        assert_eq!(rows.len(), 1, "expected 1 bucket, got {}", rows.len());
        assert!((rows[0].1 - 15.0).abs() < 1e-6);
    }

    #[test]
    fn cleanup_removes_old_rows() {
        let store = in_memory_store();
        let old = Utc::now() - chrono::Duration::days(10);
        let recent = Utc::now();

        store.record("api", "cpu_percent", 1.0, old).unwrap();
        store.record("api", "cpu_percent", 2.0, recent).unwrap();

        let deleted = store.cleanup(5).expect("cleanup");
        assert_eq!(deleted, 1, "one old row should be removed");

        let rows = store
            .query(
                "api",
                "cpu_percent",
                Utc::now() - chrono::Duration::days(20),
                0,
            )
            .unwrap();
        assert_eq!(rows.len(), 1, "only recent row remains");
    }
}
