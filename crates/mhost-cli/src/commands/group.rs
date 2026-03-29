use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_success};

/// Start all processes belonging to a group.
pub async fn start(client: &IpcClient, group: &str) -> Result<(), String> {
    let params = json!({ "group": group });

    let resp = client
        .call(methods::GROUP_START, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!(
            "Failed to start group '{}': {}",
            group, err.message
        ));
    } else {
        print_success(&format!("Started group '{}'", group));
    }

    Ok(())
}

/// Stop all processes belonging to a group.
pub async fn stop(client: &IpcClient, group: &str) -> Result<(), String> {
    let params = json!({ "group": group });

    let resp = client
        .call(methods::GROUP_STOP, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!(
            "Failed to stop group '{}': {}",
            group, err.message
        ));
    } else {
        print_success(&format!("Stopped group '{}'", group));
    }

    Ok(())
}
