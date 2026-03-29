use mhost_ipc::IpcClient;

use crate::output;

pub async fn run(client: &IpcClient, env: &str) -> Result<(), String> {
    output::print_success(&format!("Deploying '{env}'..."));
    let resp = client
        .call("deploy.execute", serde_json::json!({"env": env}))
        .await
        .map_err(|e| format!("Failed: {e}"))?;
    if let Some(err) = resp.error {
        output::print_error(&err.message);
    } else {
        output::print_success(&format!("Deploy '{env}' complete"));
    }
    Ok(())
}
