use mhost_core::process::ProcessInfo;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::print_error;

/// Print the environment variables configured for a process.
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

    let info: ProcessInfo =
        serde_json::from_value(result).map_err(|e| format!("Failed to parse process info: {e}"))?;

    if info.config.env.is_empty() {
        println!("No environment variables set for '{name}'.");
    } else {
        let mut pairs: Vec<(&String, &String)> = info.config.env.iter().collect();
        pairs.sort_by_key(|(k, _)| k.as_str());
        for (k, v) in pairs {
            println!("{k}={v}");
        }
    }

    Ok(())
}
