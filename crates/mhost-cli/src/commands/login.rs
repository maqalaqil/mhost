use colored::Colorize;

const CLOUD_API: &str = "https://api.mhostai.com";

pub fn run_login() {
    println!("\n  {} mhost Cloud Login\n", "mhost".bold());

    // Step 1: Get device code from cloud API
    let api_url = std::env::var("MHOST_CLOUD_API").unwrap_or_else(|_| CLOUD_API.to_string());

    println!("  Requesting device code...");

    match reqwest::blocking::Client::new()
        .post(format!("{api_url}/auth/device/code"))
        .send()
    {
        Ok(resp) => {
            if let Ok(data) = resp.json::<serde_json::Value>() {
                let code = data["code"].as_str().unwrap_or("????-????");
                let device_id = data["device_id"].as_str().unwrap_or("");
                let verification_url = data["verification_url"]
                    .as_str()
                    .unwrap_or("https://app.mhostai.com/auth/link");

                println!("  {}", "─".repeat(40).bright_black());
                println!("  Open this URL in your browser:\n");
                println!("    {}", verification_url.cyan());
                println!("\n  Enter this code:\n");
                println!("    {}\n", code.bold().yellow());
                println!("  {}", "─".repeat(40).bright_black());
                println!("  Waiting for approval...");

                // Try to open browser
                let _ = open_browser(verification_url);

                // Step 2: Poll for approval
                let client = reqwest::blocking::Client::new();
                for _ in 0..300 {
                    // 10 minutes max (2s × 300)
                    std::thread::sleep(std::time::Duration::from_secs(2));

                    if let Ok(resp) = client
                        .get(format!("{api_url}/auth/device/poll?device_id={device_id}"))
                        .send()
                    {
                        if let Ok(poll) = resp.json::<serde_json::Value>() {
                            if poll["status"].as_str() == Some("approved") {
                                if let Some(token) = poll["api_token"].as_str() {
                                    save_cloud_auth(token);
                                    println!("\n  {} Logged in to mhost Cloud!", "✓".green());
                                    println!("  Token saved to ~/.mhost/cloud-auth.json\n");
                                    println!(
                                        "  Next: run {} to link this server",
                                        "mhost connect".cyan()
                                    );
                                    return;
                                }
                            }
                        }
                    }
                }
                println!("\n  {} Timed out waiting for approval.", "✖".red());
            }
        }
        Err(_) => {
            println!(
                "  {} Could not reach mhost Cloud API at {api_url}",
                "✖".red()
            );
            println!("  Check your internet connection or try again later.\n");
        }
    }
}

pub fn run_logout() {
    let home = dirs::home_dir().unwrap();
    let auth_path = home.join(".mhost").join("cloud-auth.json");
    if auth_path.exists() {
        std::fs::remove_file(&auth_path).ok();
        println!("  {} Logged out from mhost Cloud", "✓".green());
    } else {
        println!("  Not logged in to mhost Cloud");
    }
}

fn save_cloud_auth(token: &str) {
    let home = dirs::home_dir().unwrap();
    let dir = home.join(".mhost");
    std::fs::create_dir_all(&dir).ok();
    let auth = serde_json::json!({
        "api_token": token,
        "api_url": CLOUD_API,
        "ws_url": "wss://ws.mhostai.com",
        "connected_at": chrono::Utc::now().to_rfc3339(),
    });
    let path = dir.join("cloud-auth.json");
    std::fs::write(&path, serde_json::to_string_pretty(&auth).unwrap()).ok();
    // Set permissions 0600 on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok();
    }
}

fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn().ok();
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn().ok();
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", url])
            .spawn()
            .ok();
    }
    Ok(())
}
