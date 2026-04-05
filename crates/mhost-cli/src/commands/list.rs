use mhost_core::process::ProcessInfo;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_process_table};

/// List all registered processes, optionally filtered by tag.
pub async fn run(client: &IpcClient, tag: Option<&str>) -> Result<(), String> {
    let resp = client
        .call(methods::PROCESS_LIST, json!({}))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Failed to list processes: {}", err.message));
        return Ok(());
    }

    let result = resp.result.unwrap_or(serde_json::Value::Array(vec![]));
    // Handler returns {"processes": [...]} — extract the array
    let process_list = if let Some(arr) = result.get("processes") {
        arr.clone()
    } else {
        result
    };
    let all_processes: Vec<ProcessInfo> = serde_json::from_value(process_list)
        .map_err(|e| format!("Failed to parse process list: {e}"))?;

    let processes: Vec<ProcessInfo> = match tag {
        Some(t) => all_processes
            .into_iter()
            .filter(|p| p.config.tags.contains(&t.to_string()))
            .collect(),
        None => all_processes,
    };

    print_process_table(&processes);
    Ok(())
}
