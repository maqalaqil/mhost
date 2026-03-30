use mhost_core::process::ProcessInfo;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::print_error;

/// Print the JSON configuration for a process.
pub async fn run(client: &IpcClient, name: &str) -> Result<(), String> {
    let resp = client
        .call(methods::PROCESS_INFO, json!({ "name": name }))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Process '{}' not found: {}", name, err.message));
        return Ok(());
    }

    let result = resp.result.ok_or("Empty response from daemon")?;
    let process_list = if let Some(arr) = result.get("processes") {
        arr.clone()
    } else {
        result
    };
    let infos: Vec<ProcessInfo> = serde_json::from_value(process_list)
        .map_err(|e| format!("Failed to parse process info: {e}"))?;

    if let Some(info) = infos.first() {
        let pretty = serde_json::to_string_pretty(&info.config)
            .map_err(|e| format!("Serialization error: {e}"))?;
        println!("{pretty}");
    }
    Ok(())
}
