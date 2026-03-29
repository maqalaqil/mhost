use mhost_ipc::IpcClient;

pub async fn run(client: &IpcClient) -> Result<(), String> {
    mhost_tui::run_tui(client)
        .await
        .map_err(|e| format!("TUI error: {e}"))
}
