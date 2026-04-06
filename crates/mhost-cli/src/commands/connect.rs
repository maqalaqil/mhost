use colored::Colorize;

pub fn run_connect(name: Option<&str>) {
    println!("\n  {} mhost Cloud Connect\n", "mhost".bold());

    // Load cloud auth
    let home = dirs::home_dir().unwrap();
    let auth_path = home.join(".mhost").join("cloud-auth.json");
    if !auth_path.exists() {
        println!(
            "  {} Not logged in. Run {} first.",
            "✖".red(),
            "mhost login".cyan()
        );
        return;
    }

    let auth: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&auth_path).unwrap_or_default())
            .unwrap_or_default();

    let api_token = auth["api_token"].as_str().unwrap_or("");
    let api_url = auth["api_url"]
        .as_str()
        .unwrap_or("https://api.mhostai.com");

    if api_token.is_empty() {
        println!(
            "  {} Invalid credentials. Run {} again.",
            "✖".red(),
            "mhost login".cyan()
        );
        return;
    }

    // Gather server info
    let host = hostname();
    let server_name = name.unwrap_or(&host).to_string();
    let os_info = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    let mhost_version = env!("CARGO_PKG_VERSION");

    println!("  Server:    {}", server_name.cyan());
    println!("  OS:        {os_info}");
    println!("  mhost:     v{mhost_version}");
    println!();

    // Register with cloud
    let body = serde_json::json!({
        "name": server_name,
        "region": "auto",
        "os": os_info,
        "mhost_version": mhost_version,
    });

    match reqwest::blocking::Client::new()
        .post(format!("{api_url}/servers/register"))
        .header("Authorization", format!("Bearer {api_token}"))
        .json(&body)
        .send()
    {
        Ok(resp) => {
            if let Ok(data) = resp.json::<serde_json::Value>() {
                if data["ok"].as_bool() == Some(true) {
                    let server_id = data["server_id"].as_str().unwrap_or("");
                    let ws_token = data["ws_token"].as_str().unwrap_or("");

                    // Build updated auth data with server info
                    let auth_data = serde_json::json!({
                        "api_token": auth["api_token"],
                        "api_url": auth["api_url"],
                        "ws_url": auth["ws_url"],
                        "connected_at": auth["connected_at"],
                        "server_id": server_id,
                        "ws_token": ws_token,
                        "server_name": server_name,
                    });
                    std::fs::write(
                        &auth_path,
                        serde_json::to_string_pretty(&auth_data).unwrap(),
                    )
                    .ok();

                    println!("  {} Registered as {}", "✓".green(), server_id.cyan());
                    println!("  {} Cloud sync will start with daemon", "✓".green());
                    println!(
                        "\n  Dashboard: {}",
                        format!("https://app.mhostai.com/servers/{server_id}").cyan()
                    );
                    println!();
                } else {
                    let err = data["error"].as_str().unwrap_or("Unknown error");
                    println!("  {} Registration failed: {err}", "✖".red());
                }
            }
        }
        Err(e) => {
            println!("  {} Could not reach cloud API: {e}", "✖".red());
        }
    }
}

pub fn run_disconnect() {
    let home = dirs::home_dir().unwrap();
    let auth_path = home.join(".mhost").join("cloud-auth.json");
    if auth_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&auth_path) {
            if let Ok(auth) = serde_json::from_str::<serde_json::Value>(&data) {
                // Build new auth without server fields (immutable approach)
                let cleaned = serde_json::json!({
                    "api_token": auth["api_token"],
                    "api_url": auth["api_url"],
                    "ws_url": auth["ws_url"],
                    "connected_at": auth["connected_at"],
                });
                std::fs::write(&auth_path, serde_json::to_string_pretty(&cleaned).unwrap()).ok();
            }
        }
        println!("  {} Disconnected from mhost Cloud", "✓".green());
        println!("  Server deregistered. Cloud sync stopped.");
    } else {
        println!("  Not connected to mhost Cloud");
    }
}

fn hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
