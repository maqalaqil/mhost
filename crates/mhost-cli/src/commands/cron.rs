use std::str::FromStr;

use chrono::Utc;
use cron::Schedule;
use mhost_core::process::ProcessInfo;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::print_error;

/// Show all processes that have a `cron_restart` schedule configured.
pub async fn run(client: &IpcClient) -> Result<(), String> {
    let resp = client
        .call(methods::PROCESS_LIST, json!({}))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Failed to list processes: {}", err.message));
        return Ok(());
    }

    let result = resp.result.unwrap_or(serde_json::Value::Array(vec![]));
    let process_list = if let Some(arr) = result.get("processes") {
        arr.clone()
    } else {
        result
    };
    let processes: Vec<ProcessInfo> = serde_json::from_value(process_list)
        .map_err(|e| format!("Failed to parse process list: {e}"))?;

    let cron_processes: Vec<&ProcessInfo> = processes
        .iter()
        .filter(|p| p.config.cron_restart.is_some())
        .collect();

    if cron_processes.is_empty() {
        println!("No processes with cron_restart schedules found.");
        println!("Set cron_restart in your mhost.toml config, e.g.:");
        println!("  [process.cleanup]");
        println!("  cron_restart = \"0 3 * * *\"");
        return Ok(());
    }

    let now = Utc::now();

    println!();
    println!(
        "  {:<16} {:<20} {:<24} {}",
        "Process", "Schedule", "Next Run", "Status"
    );
    let sep = format!("  {}", "\u{2500}".repeat(72));
    println!("{sep}");

    for p in &cron_processes {
        let expr = p.config.cron_restart.as_deref().unwrap_or("");
        let next_run = compute_next_run(expr, &now);
        let status = &p.status;

        println!(
            "  {:<16} {:<20} {:<24} {}",
            p.config.name, expr, next_run, status
        );
    }

    println!("{sep}");
    println!();

    Ok(())
}

/// Parse a cron expression and compute the next fire time after `now`.
fn compute_next_run(expr: &str, now: &chrono::DateTime<Utc>) -> String {
    // The `cron` crate expects 6 or 7 field expressions (sec min hour dom mon dow [year]).
    // Standard 5-field cron (min hour dom mon dow) needs a seconds prefix.
    let full_expr = normalize_cron_expr(expr);

    match Schedule::from_str(&full_expr) {
        Ok(schedule) => match schedule.after(now).next() {
            Some(next) => next.format("%Y-%m-%d %H:%M").to_string(),
            None => "N/A".to_string(),
        },
        Err(_) => "invalid cron".to_string(),
    }
}

/// If the expression has 5 fields (standard cron), prepend "0" for seconds.
fn normalize_cron_expr(expr: &str) -> String {
    let field_count = expr.split_whitespace().count();
    if field_count == 5 {
        format!("0 {expr}")
    } else {
        expr.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_5_field() {
        assert_eq!(normalize_cron_expr("0 3 * * *"), "0 0 3 * * *");
    }

    #[test]
    fn test_normalize_6_field_unchanged() {
        let expr = "0 0 3 * * *";
        assert_eq!(normalize_cron_expr(expr), expr);
    }

    #[test]
    fn test_compute_next_run_valid() {
        let now = Utc::now();
        let result = compute_next_run("0 3 * * *", &now);
        // Should produce a date string, not "invalid cron"
        assert!(!result.contains("invalid"), "got: {result}");
        assert!(result.contains('-'), "expected date format, got: {result}");
    }

    #[test]
    fn test_compute_next_run_invalid() {
        let now = Utc::now();
        let result = compute_next_run("not a cron", &now);
        assert_eq!(result, "invalid cron");
    }

    #[test]
    fn test_cron_parse_5field() {
        // 5-field should be normalised to 6-field and parse successfully
        let normalized = normalize_cron_expr("*/5 * * * *");
        assert_eq!(normalized, "0 */5 * * * *");
        let schedule = Schedule::from_str(&normalized);
        assert!(
            schedule.is_ok(),
            "5-field cron should parse after normalisation"
        );
    }

    #[test]
    fn test_cron_parse_invalid() {
        let normalized = normalize_cron_expr("bad cron expr");
        let schedule = Schedule::from_str(&normalized);
        assert!(schedule.is_err(), "Invalid cron should fail to parse");
    }

    #[test]
    fn test_normalize_preserves_7_field() {
        let expr = "0 30 9 * * MON *";
        assert_eq!(normalize_cron_expr(expr), expr);
    }
}
