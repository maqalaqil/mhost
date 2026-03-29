use colored::Colorize;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::print_error;

/// Print health status for each instance of a process.
pub async fn run(client: &IpcClient, name: &str) -> Result<(), String> {
    let resp = client
        .call(methods::HEALTH_STATUS, json!({ "name": name }))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!(
            "Failed to get health for '{}': {}",
            name, err.message
        ));
        return Ok(());
    }

    let result = resp.result.ok_or("Empty response from daemon")?;

    let health_list = result
        .get("health")
        .and_then(|v| v.as_array())
        .ok_or("Unexpected response format")?;

    if health_list.is_empty() {
        println!("{}", "No instances found.".dimmed());
        return Ok(());
    }

    println!(
        "{:<4} {:<20} {:<8} {:<12}",
        "inst".bold(),
        "name".bold(),
        "id".bold(),
        "health".bold(),
    );
    println!("{}", "─".repeat(50).dimmed());

    for entry in health_list {
        let inst = entry.get("instance").and_then(|v| v.as_u64()).unwrap_or(0);
        let proc_name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| &s[..4.min(s.len())])
            .unwrap_or("-");
        let health = entry
            .get("health_status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let health_display = match health {
            "healthy" => health.green().to_string(),
            "unhealthy" => health.red().to_string(),
            "disabled" => health.dimmed().to_string(),
            _ => health.yellow().to_string(),
        };

        println!(
            "{:<4} {:<20} {:<8} {:<12}",
            inst, proc_name, id, health_display,
        );
    }

    Ok(())
}
