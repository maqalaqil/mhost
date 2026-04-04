use colored::Colorize;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

pub async fn run(client: &IpcClient, app: &str, port: Option<u16>) -> Result<(), String> {
    // Get process info to find the port
    let actual_port = if let Some(p) = port {
        p
    } else {
        // Try to detect from process config
        let resp = client
            .call(methods::PROCESS_INFO, serde_json::json!({"name": app}))
            .await
            .map_err(|e| e.to_string())?;
        let result = resp.result.unwrap_or_default();
        let procs = result
            .get("processes")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();
        let env_port = procs
            .first()
            .and_then(|p| p["config"]["env"].as_object())
            .and_then(|env| env.get("PORT"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u16>().ok());
        env_port.unwrap_or(3000)
    };

    println!(
        "\n  {} Sharing '{}' (port {})\n",
        "🌐".cyan(),
        app.white().bold(),
        actual_port
    );
    println!("  To expose this to the internet, use one of:\n");
    println!(
        "  {} {}",
        "ngrok:".bold(),
        format!("ngrok http {actual_port}").cyan()
    );
    println!(
        "  {} {}",
        "cloudflared:".bold(),
        format!("cloudflared tunnel --url http://localhost:{actual_port}").cyan()
    );
    println!(
        "  {} {}",
        "localtunnel:".bold(),
        format!("npx localtunnel --port {actual_port}").cyan()
    );
    println!(
        "  {} {}",
        "bore:".bold(),
        format!("bore local {actual_port} --to bore.pub").cyan()
    );
    println!(
        "\n  Or via SSH: {}",
        format!("ssh -R 80:localhost:{actual_port} serveo.net").cyan()
    );
    println!();
    Ok(())
}
