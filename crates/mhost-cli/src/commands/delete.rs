use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_success};

/// Delete a process from the registry.
pub async fn run(client: &IpcClient, target: &str) -> Result<(), String> {
    let params = if target == "all" {
        json!({ "all": true })
    } else {
        json!({ "name": target })
    };

    let resp = client
        .call(methods::PROCESS_DELETE, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Failed to delete '{target}': {}", err.message));
    } else {
        print_success(&format!("Deleted '{target}'"));
    }

    Ok(())
}
