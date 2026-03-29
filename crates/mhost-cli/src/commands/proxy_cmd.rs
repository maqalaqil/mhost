use mhost_ipc::IpcClient;

use crate::output;

pub async fn run(client: &IpcClient) -> Result<(), String> {
    let resp = client
        .call("proxy.list", serde_json::json!(null))
        .await
        .map_err(|e| format!("Failed: {e}"))?;
    if let Some(err) = resp.error {
        output::print_error(&err.message);
    } else if let Some(result) = resp.result {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).unwrap_or_default()
        );
    }
    Ok(())
}
