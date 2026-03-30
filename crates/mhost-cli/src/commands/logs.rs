use std::fs::File;
use std::io::{BufRead, BufReader};

use colored::Colorize;
use serde_json::json;

use mhost_core::paths::MhostPaths;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

/// Show logs for ALL processes (when no name given).
pub fn run_all(
    paths: &MhostPaths,
    lines: usize,
    err_stream: bool,
    grep: Option<&str>,
) -> Result<(), String> {
    let logs_dir = paths.logs_dir();
    if !logs_dir.exists() {
        println!("  {}  {}", "○".dimmed(), "No logs yet".dimmed());
        return Ok(());
    }

    let suffix = if err_stream { "-err.log" } else { "-out.log" };
    let mut found = false;

    let mut entries: Vec<_> = std::fs::read_dir(&logs_dir)
        .map_err(|e| format!("Cannot read logs dir: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(suffix))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let fname = entry.file_name().to_string_lossy().to_string();
        // Extract process name from filename: "api-server-0-out.log" -> "api-server"
        let name = fname
            .strip_suffix(suffix)
            .unwrap_or(&fname)
            .rsplit_once('-')
            .map(|(name, _instance)| name)
            .unwrap_or(&fname);

        let file = match std::fs::File::open(entry.path()) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = std::io::BufReader::new(file);
        let all_lines: Vec<String> = reader
            .lines()
            .map_while(|l| l.ok())
            .filter(|l| grep.map(|g| l.contains(g)).unwrap_or(true))
            .collect();

        if all_lines.is_empty() {
            continue;
        }
        found = true;

        let per_process = lines.min(20); // cap per-process in "all" mode
        let start = all_lines.len().saturating_sub(per_process);
        let showing = all_lines.len() - start;

        let stream_label = if err_stream { "stderr" } else { "stdout" };
        println!();
        println!(
            "  {} {} {} {} {} {}",
            "▸".cyan().bold(),
            name.white().bold(),
            "│".dimmed(),
            stream_label.dimmed(),
            "│".dimmed(),
            format!("{showing} lines").dimmed(),
        );
        println!("  {}", "─".repeat(72).dimmed());

        for (i, line) in all_lines[start..].iter().enumerate() {
            let line_num = format!("{:>4}", start + i + 1).dimmed();
            let formatted = format_log_line(line);
            println!("  {} {} {}", line_num, "│".dimmed(), formatted);
        }
        println!("  {}", "─".repeat(72).dimmed());
    }

    if !found {
        println!(
            "  {}  {}",
            "○".dimmed(),
            "No log output from any process".dimmed()
        );
    }

    println!();
    Ok(())
}

/// Tail log lines for a process (reads from file). Accepts name or ID prefix.
pub fn run(
    paths: &MhostPaths,
    name_or_id: &str,
    lines: usize,
    err_stream: bool,
    grep: Option<&str>,
) -> Result<(), String> {
    // Try direct name first
    let log_path = if err_stream {
        paths.process_err_log(name_or_id, 0)
    } else {
        paths.process_out_log(name_or_id, 0)
    };

    // If not found by name, try to find by scanning log files for an ID prefix match
    let (log_path, name) = if log_path.exists() {
        (log_path, name_or_id.to_string())
    } else {
        resolve_log_by_id(paths, name_or_id, err_stream)?
    };

    let file = File::open(&log_path)
        .map_err(|e| format!("Cannot open log '{}': {e}", log_path.display()))?;

    let reader = BufReader::new(file);
    let all_lines: Vec<String> = reader
        .lines()
        .map_while(|l| l.ok())
        .filter(|l| grep.map(|g| l.contains(g)).unwrap_or(true))
        .collect();

    if all_lines.is_empty() {
        // If stdout is empty and user didn't explicitly ask for stderr, auto-check stderr
        if !err_stream {
            let err_path = paths.process_err_log(&name, 0);
            if err_path.exists() {
                let err_file = File::open(&err_path).ok();
                let has_stderr = err_file
                    .map(|f| BufReader::new(f).lines().next().is_some())
                    .unwrap_or(false);
                if has_stderr {
                    println!(
                        "  {}  No stdout for '{}', showing {} instead:",
                        "○".dimmed(),
                        name.cyan(),
                        "stderr".yellow()
                    );
                    println!();
                    return run(paths, &name, lines, true, grep);
                }
            }
        }
        println!(
            "  {}  No log output yet for '{}'",
            "○".dimmed(),
            name.cyan()
        );
        return Ok(());
    }

    let start = if lines == 0 || all_lines.len() <= lines {
        0
    } else {
        all_lines.len() - lines
    };

    let stream_label = if err_stream { "stderr" } else { "stdout" };
    let showing = all_lines.len() - start;

    // Header
    println!();
    println!(
        "  {} {} {} {} {} {}",
        "▸".cyan().bold(),
        name.white().bold(),
        "│".dimmed(),
        stream_label.dimmed(),
        "│".dimmed(),
        format!("{showing} lines").dimmed(),
    );
    println!("  {}", "─".repeat(72).dimmed());

    for (i, line) in all_lines[start..].iter().enumerate() {
        let line_num = format!("{:>4}", start + i + 1).dimmed();
        let formatted = format_log_line(line);
        println!("  {} {} {}", line_num, "│".dimmed(), formatted);
    }

    println!("  {}", "─".repeat(72).dimmed());
    println!();
    Ok(())
}

/// Format a single log line with color based on content.
fn format_log_line(line: &str) -> String {
    // Try to parse as JSON for structured logs
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
        return format_json_log(&json);
    }

    // Plain text — color by keyword detection
    let lower = line.to_lowercase();
    if lower.contains("error") || lower.contains("fatal") || lower.contains("panic") {
        line.red().to_string()
    } else if lower.contains("warn") {
        line.yellow().to_string()
    } else if lower.contains("debug") || lower.contains("trace") {
        line.dimmed().to_string()
    } else {
        line.to_string()
    }
}

/// Format a JSON log line with colored fields.
fn format_json_log(json: &serde_json::Value) -> String {
    let level = json
        .get("level")
        .or_else(|| json.get("severity"))
        .and_then(|v| v.as_str())
        .unwrap_or("info");

    let message = json
        .get("message")
        .or_else(|| json.get("msg"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let timestamp = json
        .get("timestamp")
        .or_else(|| json.get("time"))
        .or_else(|| json.get("ts"))
        .and_then(|v| v.as_str())
        .map(|t| {
            // Show just time portion if it's an ISO timestamp
            if t.contains('T') {
                t.split('T')
                    .nth(1)
                    .unwrap_or(t)
                    .trim_end_matches('Z')
                    .split('.')
                    .next()
                    .unwrap_or(t)
            } else {
                t
            }
        });

    let level_badge = match level.to_lowercase().as_str() {
        "error" | "err" | "fatal" | "crit" | "critical" => {
            format!("{}", " ERR ".on_red().white().bold())
        }
        "warn" | "warning" => format!("{}", " WRN ".on_yellow().black().bold()),
        "info" => format!("{}", " INF ".on_cyan().black().bold()),
        "debug" | "dbg" => format!("{}", " DBG ".on_white().black()),
        "trace" => format!("{}", " TRC ".dimmed()),
        _ => format!("{}", format!(" {level:^3} ").dimmed()),
    };

    let time_str = timestamp
        .map(|t| format!("{} ", t.dimmed()))
        .unwrap_or_default();

    // Collect extra fields (not level/message/timestamp)
    let extras: Vec<String> = json
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter(|(k, _)| {
                    !matches!(
                        k.as_str(),
                        "level" | "severity" | "message" | "msg" | "timestamp" | "time" | "ts"
                    )
                })
                .map(|(k, v)| {
                    let val = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    format!("{}={}", k.dimmed(), val.cyan())
                })
                .collect()
        })
        .unwrap_or_default();

    let extra_str = if extras.is_empty() {
        String::new()
    } else {
        format!(" {}", extras.join(" "))
    };

    format!("{time_str}{level_badge} {message}{extra_str}")
}

// ---------------------------------------------------------------------------
// Daemon-backed search / count_by (unchanged)
// ---------------------------------------------------------------------------

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
        println!(
            "{}",
            serde_json::to_string_pretty(&result).unwrap_or_default()
        );
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
                    println!("{line}");
                } else {
                    println!("{entry}");
                }
            }
        }
    }

    Ok(())
}

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
            println!("{key:<20} {count:>10}");
        }
    }

    Ok(())
}

/// Try to find a log file by matching a process ID prefix against log filenames.
/// Falls back to checking the daemon's state DB for ID → name mapping.
fn resolve_log_by_id(
    paths: &MhostPaths,
    id_prefix: &str,
    err_stream: bool,
) -> Result<(std::path::PathBuf, String), String> {
    // Scan log directory for files and try to match
    let logs_dir = paths.logs_dir();
    let _ = err_stream; // used for log path selection below

    if logs_dir.exists() {
        // Try the state DB to resolve ID -> name
        let db_path = paths.db();
        if db_path.exists() {
            if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                let query = "SELECT name FROM processes WHERE id LIKE ?1 LIMIT 1";
                let pattern = format!("{id_prefix}%");
                if let Ok(name) = conn.query_row(query, [&pattern], |row| row.get::<_, String>(0)) {
                    let log_path = if err_stream {
                        paths.process_err_log(&name, 0)
                    } else {
                        paths.process_out_log(&name, 0)
                    };
                    if log_path.exists() {
                        return Ok((log_path, name));
                    }
                }
            }
        }
    }

    Err(format!(
        "No logs found for '{id_prefix}'. Use process name or run: mhost list"
    ))
}

#[allow(dead_code)]
pub fn log_exists(paths: &MhostPaths, name: &str, err_stream: bool) -> bool {
    let p = if err_stream {
        paths.process_err_log(name, 0)
    } else {
        paths.process_out_log(name, 0)
    };
    p.exists()
}
