use std::fs::File;
use std::io::{BufRead, BufReader};

use serde_json::json;

use mhost_core::paths::MhostPaths;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

/// Tail log lines for a process (reads from file).
///
/// - `name`       — process name
/// - `lines`      — how many tail lines to show (0 = all)
/// - `err_stream` — if true read stderr log, otherwise stdout log
/// - `grep`       — optional substring filter
pub fn run(
    paths: &MhostPaths,
    name: &str,
    lines: usize,
    err_stream: bool,
    grep: Option<&str>,
) -> Result<(), String> {
    // Instance 0 is the canonical instance for single-instance processes.
    let log_path = if err_stream {
        paths.process_err_log(name, 0)
    } else {
        paths.process_out_log(name, 0)
    };

    let file = File::open(&log_path)
        .map_err(|e| format!("Cannot open log '{}': {e}", log_path.display()))?;

    let reader = BufReader::new(file);
    let all_lines: Vec<String> = reader
        .lines()
        .filter_map(|l| l.ok())
        .filter(|l| grep.map(|g| l.contains(g)).unwrap_or(true))
        .collect();

    let start = if lines == 0 || all_lines.len() <= lines {
        0
    } else {
        all_lines.len() - lines
    };

    for line in &all_lines[start..] {
        println!("{}", line);
    }

    Ok(())
}

/// Full-text search across log entries via the daemon (LOG_SEARCH RPC).
///
/// - `name`    — process name (or "*" for all)
/// - `query`   — full-text search expression
/// - `filter`  — optional SQL-style WHERE clause applied server-side
/// - `since`   — time range, e.g. "1h", "24h", "7d"
/// - `format`  — output format: "text" (default) or "json"
pub async fn search(
    client: &IpcClient,
    name: &str,
    query: &str,
    filter: Option<&str>,
    since: Option<&str>,
    format: &str,
) -> Result<(), String> {
    let params = json!({
        "name": name,
        "query": query,
        "filter": filter.unwrap_or(""),
        "since": since.unwrap_or("24h"),
        "format": format,
    });

    let resp = client
        .call(methods::LOG_SEARCH, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        return Err(format!("Daemon error: {}", err.message));
    }

    let result = resp.result.unwrap_or(serde_json::Value::Null);
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
    } else {
        let empty = vec![];
        let results = result
            .get("results")
            .and_then(|r| r.as_array())
            .unwrap_or(&empty);

        if results.is_empty() {
            println!("No log entries found.");
        } else {
            for entry in results {
                if let Some(line) = entry.as_str() {
                    println!("{}", line);
                } else {
                    println!("{}", entry);
                }
            }
        }
    }

    Ok(())
}

/// Count log entries grouped by a field via the daemon (LOG_COUNT_BY RPC).
///
/// - `name`     — process name (or "*" for all)
/// - `field`    — field to group by, e.g. "level", "hour"
/// - `since`    — time range, e.g. "24h"
pub async fn count_by(
    client: &IpcClient,
    name: &str,
    field: &str,
    since: Option<&str>,
) -> Result<(), String> {
    let params = json!({
        "name": name,
        "field": field,
        "since": since.unwrap_or("24h"),
    });

    let resp = client
        .call(methods::LOG_COUNT_BY, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        return Err(format!("Daemon error: {}", err.message));
    }

    let result = resp.result.unwrap_or(serde_json::Value::Null);
    let empty = vec![];
    let buckets = result
        .get("buckets")
        .and_then(|b| b.as_array())
        .unwrap_or(&empty);

    if buckets.is_empty() {
        println!("No data.");
    } else {
        println!("{:<20} {:>10}", field, "count");
        println!("{}", "-".repeat(32));
        for bucket in buckets {
            let key = bucket.get("key").and_then(|k| k.as_str()).unwrap_or("?");
            let count = bucket.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
            println!("{:<20} {:>10}", key, count);
        }
    }

    Ok(())
}

/// Validate that the log path exists without reading it (used in tests).
#[allow(dead_code)]
pub fn log_exists(paths: &MhostPaths, name: &str, err_stream: bool) -> bool {
    let p = if err_stream {
        paths.process_err_log(name, 0)
    } else {
        paths.process_out_log(name, 0)
    };
    p.exists()
}
