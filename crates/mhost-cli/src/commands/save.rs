use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_success};

/// Save the current process list to disk so it can be resurrected later.
pub async fn run(client: &IpcClient) -> Result<(), String> {
    let resp = client
        .call(methods::PROCESS_SAVE, json!({}))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Failed to save: {}", err.message));
    } else {
        print_success("Process list saved.");
    }

    Ok(())
}
