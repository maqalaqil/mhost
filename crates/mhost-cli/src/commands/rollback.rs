use mhost_ipc::IpcClient;

use crate::output;

pub async fn run(client: &IpcClient, env: &str) -> Result<(), String> {
    output::print_success(&format!("Rolling back '{env}'..."));
    let resp = client
        .call("deploy.rollback", serde_json::json!({"env": env}))
        .await
        .map_err(|e| format!("Failed: {e}"))?;
    if let Some(err) = resp.error {
        output::print_error(&err.message);
    } else {
        output::print_success(&format!("Rollback '{env}' complete"));
    }
    Ok(())
}
