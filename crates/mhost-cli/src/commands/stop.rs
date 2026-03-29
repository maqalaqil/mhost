use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_success};

/// Stop a process by name or all processes when `target` is `"all"`.
pub async fn run(client: &IpcClient, target: &str) -> Result<(), String> {
    let params = if target == "all" {
        json!({ "all": true })
    } else {
        json!({ "name": target })
    };

    let resp = client
        .call(methods::PROCESS_STOP, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Failed to stop '{target}': {}", err.message));
    } else {
        print_success(&format!("Stopped '{target}'"));
    }

    Ok(())
}
