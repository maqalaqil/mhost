use std::fs;

use colored::Colorize;
use mhost_core::paths::MhostPaths;
use rusqlite::{params, Connection};

// ---------------------------------------------------------------------------
// mhost rollback-process <name>
// ---------------------------------------------------------------------------

pub fn run_rollback(paths: &MhostPaths, process: &str) -> Result<(), String> {
    println!(
        "\n  {} Rollback for process '{}'",
        "↩".bold(),
        process.cyan()
    );
    println!("  {}", "─".repeat(50).dimmed());

    // Show recent history from the database
    show_recent_history(paths, process)?;

    // Show saved config from dump.json if available
    show_saved_config(paths, process)?;

    println!(
        "\n  {} Config rollback requires a running daemon connection.",
        "ℹ".blue()
    );
    println!(
        "  {} In a future release, `mhost rollback-process {}` will:",
        "→".dimmed(),
        process
    );
    println!("    1. Show the last config change diff");
    println!("    2. Ask for confirmation");
    println!("    3. Apply the previous config via the daemon");
    println!();

    Ok(())
}

// ---------------------------------------------------------------------------
// mhost config-history <name>
// ---------------------------------------------------------------------------

pub fn run_config_history(paths: &MhostPaths, process: &str) -> Result<(), String> {
    println!(
        "\n  {} Config history for '{}'",
        "📋".bold(),
        process.cyan()
    );
    println!("  {}", "─".repeat(50).dimmed());

    // Show events from the database
    show_recent_history(paths, process)?;

    // Show current saved config
    show_saved_config(paths, process)?;

    println!();
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn show_recent_history(paths: &MhostPaths, process: &str) -> Result<(), String> {
    let db_path = paths.db();

    if !db_path.exists() {
        println!(
            "  {} No database found — daemon may not have run yet.",
            "!".yellow()
        );
        return Ok(());
    }

    let conn = Connection::open(&db_path).map_err(|e| format!("Cannot open database: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT event_type, COALESCE(message, ''), timestamp \
             FROM events \
             WHERE process_name = ?1 \
             ORDER BY timestamp DESC \
             LIMIT 20",
        )
        .map_err(|e| format!("Database query error: {e}"))?;

    let rows: Vec<(String, String, String)> = stmt
        .query_map(params![process], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("Failed to query events: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        println!("  No event history found for '{process}'.");
        return Ok(());
    }

    println!("\n  {}", "Recent events:".bold());
    println!(
        "  {:<12} {:<28} {}",
        "Event".dimmed(),
        "Timestamp".dimmed(),
        "Message".dimmed()
    );

    for (event_type, message, timestamp) in &rows {
        let colored_event = match event_type.as_str() {
            "started" | "online" => event_type.green().to_string(),
            "stopped" | "deleted" => event_type.red().to_string(),
            "errored" | "crashed" => event_type.red().bold().to_string(),
            "restarted" | "config_changed" => event_type.yellow().to_string(),
            _ => event_type.normal().to_string(),
        };
        println!("  {colored_event:<24} {timestamp:<28} {message}");
    }

    Ok(())
}

fn show_saved_config(paths: &MhostPaths, process: &str) -> Result<(), String> {
    let dump_path = paths.dump_file();

    if !dump_path.exists() {
        println!(
            "\n  {} No saved dump file found. Run `mhost save` first.",
            "ℹ".blue()
        );
        return Ok(());
    }

    let content =
        fs::read_to_string(&dump_path).map_err(|e| format!("Cannot read dump file: {e}"))?;

    let dump: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid dump file: {e}"))?;

    // The dump file is typically an array of process configs
    let processes = dump.as_array().ok_or("Dump file is not an array")?;

    let saved_config = processes.iter().find(|p| {
        p.get("name")
            .and_then(|n| n.as_str())
            .map(|n| n == process)
            .unwrap_or(false)
    });

    match saved_config {
        Some(config) => {
            let pretty =
                serde_json::to_string_pretty(config).unwrap_or_else(|_| config.to_string());
            println!("\n  {}", "Saved config (from dump.json):".bold());
            for line in pretty.lines() {
                println!("  {}", line.dimmed());
            }
        }
        None => {
            println!(
                "\n  {} No saved config found for '{}' in dump file.",
                "ℹ".blue(),
                process
            );
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn test_find_process_in_dump_array() {
        let dump: serde_json::Value = serde_json::from_str(
            r#"[{"name":"api","command":"node server.js"},{"name":"worker","command":"python worker.py"}]"#,
        )
        .unwrap();

        let processes = dump.as_array().unwrap();
        let found = processes.iter().find(|p| {
            p.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n == "api")
                .unwrap_or(false)
        });
        assert!(found.is_some());
        assert_eq!(
            found.unwrap().get("command").unwrap().as_str().unwrap(),
            "node server.js"
        );
    }

    #[test]
    fn test_find_process_not_in_dump() {
        let dump: serde_json::Value =
            serde_json::from_str(r#"[{"name":"api","command":"node server.js"}]"#).unwrap();
        let processes = dump.as_array().unwrap();
        let found = processes.iter().find(|p| {
            p.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n == "nonexistent")
                .unwrap_or(false)
        });
        assert!(found.is_none());
    }
}
