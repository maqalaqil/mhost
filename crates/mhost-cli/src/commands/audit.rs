use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use colored::Colorize;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Audit entry
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct AuditEntry {
    timestamp: String,
    action: String,
    process: String,
    source: String,
    details: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn audit_log_path() -> PathBuf {
    dirs::home_dir()
        .expect("Cannot determine home directory")
        .join(".mhost")
        .join("audit.jsonl")
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty duration string".into());
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let value: i64 = num_str
        .parse()
        .map_err(|_| format!("Invalid duration number: '{num_str}'"))?;

    match unit {
        "s" => Ok(Duration::seconds(value)),
        "m" => Ok(Duration::minutes(value)),
        "h" => Ok(Duration::hours(value)),
        "d" => Ok(Duration::days(value)),
        _ => Err(format!(
            "Unknown duration unit '{unit}'. Use s, m, h, or d."
        )),
    }
}

fn parse_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    ts.parse::<DateTime<Utc>>().ok()
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

pub fn run(process_filter: Option<&str>, since: Option<&str>, limit: usize) -> Result<(), String> {
    let path = audit_log_path();

    if !path.exists() {
        println!("No audit log found at {}", path.display());
        println!(
            "  {}",
            "The audit log will be created when commands are executed.".dimmed()
        );
        return Ok(());
    }

    let content = fs::read_to_string(&path).map_err(|e| format!("Cannot read audit log: {e}"))?;

    let cutoff: Option<DateTime<Utc>> = match since {
        Some(s) => {
            let dur = parse_duration(s)?;
            Some(Utc::now() - dur)
        }
        None => None,
    };

    let entries: Vec<AuditEntry> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<AuditEntry>(line).ok())
        .filter(|entry| {
            if let Some(ref filter) = process_filter {
                if entry.process != *filter {
                    return false;
                }
            }
            if let Some(ref cutoff_ts) = cutoff {
                if let Some(ts) = parse_timestamp(&entry.timestamp) {
                    if ts < *cutoff_ts {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    let display_entries: Vec<&AuditEntry> = entries.iter().rev().take(limit).collect();

    if display_entries.is_empty() {
        println!("No audit entries match the given filters.");
        return Ok(());
    }

    println!(
        "\n  {:<24} {:<12} {:<16} {:<10} {}",
        "Time".bold(),
        "Action".bold(),
        "Process".bold(),
        "Source".bold(),
        "Details".bold(),
    );
    println!("  {}", "─".repeat(80).dimmed());

    for entry in &display_entries {
        let colored_action = match entry.action.as_str() {
            "start" | "restart" => entry.action.green().to_string(),
            "stop" | "delete" => entry.action.red().to_string(),
            "crash" | "error" => entry.action.red().bold().to_string(),
            _ => entry.action.yellow().to_string(),
        };

        println!(
            "  {:<24} {:<24} {:<16} {:<10} {}",
            entry.timestamp.dimmed(),
            colored_action,
            entry.process.cyan(),
            entry.source,
            entry.details.dimmed(),
        );
    }

    println!();
    println!("  {} {} entries shown", "ℹ".blue(), display_entries.len());
    println!();
    Ok(())
}
