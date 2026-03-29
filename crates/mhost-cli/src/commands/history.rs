use colored::Colorize;
use mhost_core::paths::MhostPaths;
use rusqlite::{params, Connection};

use crate::output::print_error;

/// Show event history for a process directly from the SQLite database.
pub fn run(paths: &MhostPaths, name: &str) -> Result<(), String> {
    let db_path = paths.db();

    if !db_path.exists() {
        print_error("No database found — has the daemon ever run?");
        return Ok(());
    }

    let conn = Connection::open(&db_path)
        .map_err(|e| format!("Cannot open database: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT event_type, COALESCE(message, ''), timestamp \
             FROM events \
             WHERE process_name = ?1 \
             ORDER BY timestamp DESC \
             LIMIT 100",
        )
        .map_err(|e| format!("Database query error: {e}"))?;

    let rows: Vec<(String, String, String)> = stmt
        .query_map(params![name], |row| {
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
        println!("No history found for '{}'.", name);
        return Ok(());
    }

    println!(
        "{:<12} {:<30} {}",
        "event".bold(),
        "timestamp".bold(),
        "message".bold()
    );
    println!("{}", "─".repeat(70).dimmed());

    for (event_type, message, timestamp) in &rows {
        let colored_event = match event_type.as_str() {
            "started" | "online" => event_type.green().to_string(),
            "stopped" | "deleted" => event_type.red().to_string(),
            "errored" | "crashed" => event_type.red().bold().to_string(),
            "restarted" => event_type.yellow().to_string(),
            _ => event_type.normal().to_string(),
        };

        println!("{:<24} {:<30} {}", colored_event, timestamp, message);
    }

    Ok(())
}
