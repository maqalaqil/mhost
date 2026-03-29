use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_success};

/// Scale a process to the given number of instances.
pub async fn run(client: &IpcClient, name: &str, instances: u32) -> Result<(), String> {
    let params = json!({ "name": name, "instances": instances });

    let resp = client
        .call(methods::PROCESS_SCALE, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Failed to scale '{}': {}", name, err.message));
    } else {
        print_success(&format!("Scaled '{}' to {} instance(s)", name, instances));
    }

    Ok(())
}
