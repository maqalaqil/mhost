//! Resolve a process target — accepts a name, index (0, 1, 2...), or ID prefix.

use mhost_core::process::ProcessInfo;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

/// Resolve a target string to a process name.
/// Accepts: process name ("api"), index ("0", "1"), ID prefix ("a2db"), or "all".
pub async fn resolve_target(client: &IpcClient, target: &str) -> Result<String, String> {
    // Pass-through for "all"
    if target == "all" {
        return Ok("all".into());
    }

    // If it's a number, resolve by index
    if let Ok(idx) = target.parse::<usize>() {
        let processes = fetch_process_list(client).await?;
        if idx >= processes.len() {
            return Err(format!(
                "Index {idx} out of range (0-{})",
                processes.len().saturating_sub(1)
            ));
        }
        return Ok(processes[idx].config.name.clone());
    }

    // If it's a short string (< 6 chars) that looks like a hex ID prefix, try to resolve
    if target.len() <= 8 && target.len() >= 2 && target.chars().all(|c| c.is_ascii_hexdigit()) {
        let processes = fetch_process_list(client).await?;
        let matches: Vec<&ProcessInfo> = processes
            .iter()
            .filter(|p| p.id.starts_with(target))
            .collect();
        if matches.len() == 1 {
            return Ok(matches[0].config.name.clone());
        }
        // If no match by ID, fall through to use as name
    }

    // Use as-is (process name)
    Ok(target.to_string())
}

async fn fetch_process_list(client: &IpcClient) -> Result<Vec<ProcessInfo>, String> {
    let resp = client
        .call(methods::PROCESS_LIST, serde_json::json!(null))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;
    let result = resp.result.unwrap_or(serde_json::Value::Array(vec![]));
    let list = if let Some(arr) = result.get("processes") {
        arr.clone()
    } else {
        result
    };
    serde_json::from_value(list).map_err(|e| format!("Parse error: {e}"))
}
