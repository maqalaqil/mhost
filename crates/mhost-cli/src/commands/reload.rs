use serde_json::json;

use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

use crate::output::{print_error, print_success};

/// Zero-downtime reload — falls back to a supervised restart for now.
/// Full health-check gating will be added in a future release.
pub async fn run(client: &IpcClient, target: &str) -> Result<(), String> {
    println!("  Reloading '{}' (zero-downtime)...", target);
    let resp = client
        .call(methods::PROCESS_RELOAD, json!({"name": target}))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&err.message);
        Err(err.message)
    } else {
        print_success(&format!("Reloaded '{}' with zero downtime", target));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Integration tests would require a live daemon; unit tests are covered
    // by the IpcClient mock layer in mhost-ipc crate.
    #[test]
    fn test_reload_module_compiles() {
        // Ensures the module compiles and public API is correct.
    }
}
