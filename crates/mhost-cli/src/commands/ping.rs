use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_success};

/// Ping the daemon to confirm it is alive.
pub async fn run(client: &IpcClient) -> Result<(), String> {
    let resp = client
        .call(methods::DAEMON_PING, json!({}))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Ping failed: {}", err.message));
    } else {
        let msg = resp
            .result
            .as_ref()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "pong".to_string());
        print_success(&msg);
    }

    Ok(())
}
