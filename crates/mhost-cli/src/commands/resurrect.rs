use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_success};

/// Restore all previously saved processes.
pub async fn run(client: &IpcClient) -> Result<(), String> {
    let resp = client
        .call(methods::PROCESS_RESURRECT, json!({}))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Failed to resurrect processes: {}", err.message));
    } else {
        print_success("Processes resurrected.");
    }

    Ok(())
}
