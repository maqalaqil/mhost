use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_success};

/// Scale a process to the given number of instances (cluster alias for scale).
pub async fn run(client: &IpcClient, name: &str, instances: u32) -> Result<(), String> {
    let params = json!({ "name": name, "instances": instances });

    let resp = client
        .call(methods::PROCESS_CLUSTER, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Failed to cluster '{}': {}", name, err.message));
    } else {
        print_success(&format!("Clustered '{name}' to {instances} instance(s)"));
    }

    Ok(())
}
