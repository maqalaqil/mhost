use serde_json::json;

use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

/// Display current CPU, memory, and uptime metrics for a process.
pub async fn show(client: &IpcClient, name: &str) -> Result<(), String> {
    let params = json!({ "name": name });

    let resp = client
        .call(methods::METRICS_SHOW, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        return Err(format!("Daemon error: {}", err.message));
    }

    let result = resp.result.unwrap_or(serde_json::Value::Null);
    let empty = vec![];
    let metrics = result
        .get("metrics")
        .and_then(|m| m.as_array())
        .unwrap_or(&empty);

    if metrics.is_empty() {
        println!("No metrics available for '{}'.", name);
        return Ok(());
    }

    println!(
        "{:<6} {:<20} {:>10} {:>12} {:>12}",
        "INST", "NAME", "CPU%", "MEM (MB)", "UPTIME"
    );
    println!("{}", "-".repeat(64));

    for m in metrics {
        let instance = m.get("instance").and_then(|v| v.as_u64()).unwrap_or(0);
        let process_name = m.get("name").and_then(|v| v.as_str()).unwrap_or(name);
        let cpu = m.get("cpu_percent").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let mem_mb = m.get("memory_mb").and_then(|v| v.as_u64()).unwrap_or(0);
        let uptime_ms = m.get("uptime_ms").and_then(|v| v.as_u64()).unwrap_or(0);

        let uptime_display = format_uptime(uptime_ms);

        println!(
            "{:<6} {:<20} {:>10.1} {:>12} {:>12}",
            instance, process_name, cpu, mem_mb, uptime_display
        );
    }

    Ok(())
}

/// Show time-series metric history for a process.
pub async fn history(
    client: &IpcClient,
    name: &str,
    metric: &str,
    since: &str,
) -> Result<(), String> {
    let params = json!({
        "name": name,
        "metric": metric,
        "since": since,
    });

    let resp = client
        .call(methods::METRICS_HISTORY, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        return Err(format!("Daemon error: {}", err.message));
    }

    let result = resp.result.unwrap_or(serde_json::Value::Null);
    let empty = vec![];
    let series = result
        .get("series")
        .and_then(|s| s.as_array())
        .unwrap_or(&empty);

    if series.is_empty() {
        println!(
            "No history available for '{}' metric on '{}'.",
            metric, name
        );
        return Ok(());
    }

    println!("{:<24} {:>12}", "timestamp", metric);
    println!("{}", "-".repeat(38));

    for point in series {
        let ts = point.get("ts").and_then(|v| v.as_str()).unwrap_or("?");
        let value = point.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
        println!("{:<24} {:>12.2}", ts, value);
    }

    Ok(())
}

/// Start the Prometheus exporter endpoint on the daemon.
pub async fn start_prometheus(client: &IpcClient, listen: &str) -> Result<(), String> {
    let params = json!({ "listen": listen });

    let resp = client
        .call(methods::METRICS_START_PROMETHEUS, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        return Err(format!("Daemon error: {}", err.message));
    }

    println!("Prometheus exporter acknowledged on {}.", listen);
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_uptime(ms: u64) -> String {
    if ms == 0 {
        return "0s".to_string();
    }
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;
    let days = hours / 24;

    if days > 0 {
        format!("{}d {}h", days, hours % 24)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins % 60)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs % 60)
    } else {
        format!("{}s", secs)
    }
}
