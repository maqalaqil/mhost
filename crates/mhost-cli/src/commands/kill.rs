use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_success};

/// Shutdown the daemon process.
pub async fn run(client: &IpcClient) -> Result<(), String> {
    let resp = client
        .call(methods::DAEMON_KILL, json!({}))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Kill failed: {}", err.message));
    } else {
        print_success("Daemon shutdown initiated.");
    }

    Ok(())
}
