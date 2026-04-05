use mhost_core::process::ProcessInfo;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::print_error;

/// Show resource limits and current usage for a process.
pub async fn run(client: &IpcClient, name: &str) -> Result<(), String> {
    // Fetch process info to read config limits
    let info_resp = client
        .call(methods::PROCESS_INFO, json!({ "name": name }))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = info_resp.error {
        return Err(format!("Daemon error: {}", err.message));
    }

    let info_result = info_resp.result.unwrap_or(serde_json::Value::Null);
    let processes: Vec<ProcessInfo> = if let Some(arr) = info_result.get("instances") {
        serde_json::from_value(arr.clone())
            .map_err(|e| format!("Failed to parse process info: {e}"))?
    } else {
        let single: ProcessInfo = serde_json::from_value(info_result)
            .map_err(|e| format!("Failed to parse process info: {e}"))?;
        vec![single]
    };

    if processes.is_empty() {
        print_error(&format!("No instances found for '{name}'"));
        return Ok(());
    }

    // Fetch current metrics
    let metrics_resp = client
        .call(methods::METRICS_SHOW, json!({ "name": name }))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    let metrics_result = metrics_resp.result.unwrap_or(serde_json::Value::Null);
    let empty = vec![];
    let metrics = metrics_result
        .get("metrics")
        .and_then(|m| m.as_array())
        .unwrap_or(&empty);

    println!();
    println!("  Resource limits for '{name}'");
    let sep = format!("  {}", "\u{2500}".repeat(70));
    println!("{sep}");
    println!(
        "  {:<6} {:<14} {:<14} {:<14} {:<14} {}",
        "Inst", "CPU Limit", "CPU Current", "Mem Limit", "Mem Current", "Status"
    );
    println!("{sep}");

    for p in &processes {
        let cpu_limit_str = p.config.cpu_limit.as_deref().unwrap_or("none");
        let mem_limit_str = p
            .config
            .memory_limit_mb
            .map(|mb| format!("{mb} MB"))
            .unwrap_or_else(|| "none".to_string());

        // Find matching metric entry for this instance
        let metric = metrics.iter().find(|m| {
            m.get("instance").and_then(|v| v.as_u64()).map(|i| i as u32) == Some(p.instance)
        });

        let cpu_current = metric
            .and_then(|m| m.get("cpu_percent"))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{v:.1}%"))
            .unwrap_or_else(|| "N/A".to_string());

        let mem_current_mb = metric
            .and_then(|m| m.get("memory_mb"))
            .and_then(|v| v.as_u64());

        let mem_current_str = mem_current_mb
            .map(|mb| format!("{mb} MB"))
            .unwrap_or_else(|| "N/A".to_string());

        let status = compute_status(
            &p.config.cpu_limit,
            metric
                .and_then(|m| m.get("cpu_percent"))
                .and_then(|v| v.as_f64()),
            p.config.memory_limit_mb,
            mem_current_mb,
        );

        println!(
            "  {:<6} {:<14} {:<14} {:<14} {:<14} {}",
            p.instance, cpu_limit_str, cpu_current, mem_limit_str, mem_current_str, status
        );
    }

    println!("{sep}");
    println!();

    Ok(())
}

/// Determine status: OK, WARNING (>80% of limit), or EXCEEDED.
fn compute_status(
    cpu_limit: &Option<String>,
    cpu_current: Option<f64>,
    mem_limit_mb: Option<u64>,
    mem_current_mb: Option<u64>,
) -> &'static str {
    let cpu_exceeded = check_cpu_exceeded(cpu_limit, cpu_current);
    let mem_exceeded = check_mem_exceeded(mem_limit_mb, mem_current_mb);

    match (cpu_exceeded, mem_exceeded) {
        (LimitStatus::Exceeded, _) | (_, LimitStatus::Exceeded) => "EXCEEDED",
        (LimitStatus::Warning, _) | (_, LimitStatus::Warning) => "WARNING",
        _ => "OK",
    }
}

enum LimitStatus {
    Ok,
    Warning,
    Exceeded,
}

fn check_cpu_exceeded(limit: &Option<String>, current: Option<f64>) -> LimitStatus {
    let (limit, current) = match (limit, current) {
        (Some(l), Some(c)) => (l, c),
        _ => return LimitStatus::Ok,
    };

    let limit_val = parse_cpu_limit(limit);
    match limit_val {
        Some(lv) if current > lv => LimitStatus::Exceeded,
        Some(lv) if current > lv * 0.8 => LimitStatus::Warning,
        _ => LimitStatus::Ok,
    }
}

fn check_mem_exceeded(limit_mb: Option<u64>, current_mb: Option<u64>) -> LimitStatus {
    let (limit, current) = match (limit_mb, current_mb) {
        (Some(l), Some(c)) => (l, c),
        _ => return LimitStatus::Ok,
    };

    if current > limit {
        LimitStatus::Exceeded
    } else if current > limit * 80 / 100 {
        LimitStatus::Warning
    } else {
        LimitStatus::Ok
    }
}

/// Parse CPU limit string: "50%" -> 50.0, "1.0" -> 100.0 (cores to percent).
fn parse_cpu_limit(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.ends_with('%') {
        trimmed[..trimmed.len() - 1].parse::<f64>().ok()
    } else {
        // Assume cores: 1.0 = 100%, 0.5 = 50%
        trimmed.parse::<f64>().ok().map(|v| v * 100.0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_limit_percent() {
        assert_eq!(parse_cpu_limit("50%"), Some(50.0));
    }

    #[test]
    fn test_parse_cpu_limit_cores() {
        assert_eq!(parse_cpu_limit("1.0"), Some(100.0));
        assert_eq!(parse_cpu_limit("0.5"), Some(50.0));
    }

    #[test]
    fn test_parse_cpu_limit_invalid() {
        assert_eq!(parse_cpu_limit("abc"), None);
    }

    #[test]
    fn test_compute_status_ok() {
        let status = compute_status(&Some("50%".to_string()), Some(20.0), Some(512), Some(100));
        assert_eq!(status, "OK");
    }

    #[test]
    fn test_compute_status_warning() {
        let status = compute_status(&Some("50%".to_string()), Some(42.0), Some(512), Some(100));
        assert_eq!(status, "WARNING");
    }

    #[test]
    fn test_compute_status_exceeded() {
        let status = compute_status(&Some("50%".to_string()), Some(60.0), Some(512), Some(100));
        assert_eq!(status, "EXCEEDED");
    }

    #[test]
    fn test_compute_status_no_limits() {
        let status = compute_status(&None, Some(90.0), None, Some(999));
        assert_eq!(status, "OK");
    }

    #[test]
    fn test_compute_status_mem_exceeded() {
        let status = compute_status(&None, None, Some(256), Some(300));
        assert_eq!(status, "EXCEEDED");
    }

    #[test]
    fn test_check_mem_warning() {
        let result = check_mem_exceeded(Some(100), Some(85));
        assert!(matches!(result, LimitStatus::Warning));
    }
}
