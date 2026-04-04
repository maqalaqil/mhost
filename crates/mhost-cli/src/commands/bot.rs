use crate::output::print_success;
use mhost_bot::{BotConfig, Role};
use mhost_core::MhostPaths;
use std::io::{self, Write};

pub fn run_setup(paths: &MhostPaths) -> Result<(), String> {
    println!("\n  mhost Bot Setup\n");
    println!("  Select platform:");
    println!("    1) Telegram");
    println!("    2) Discord");
    let choice = prompt("Platform (1-2)");
    let platform = match choice.as_str() {
        "1" => "telegram",
        "2" => "discord",
        _ => return Err("Invalid platform choice".into()),
    };

    let token = prompt("Bot token");
    if token.is_empty() {
        return Err("Token required".into());
    }

    let admin_id_str = prompt("Your user/chat ID (admin)");
    let admin_id: i64 = admin_id_str
        .parse()
        .map_err(|_| "Invalid ID: must be a number")?;

    let config = BotConfig {
        platform: platform.into(),
        token,
        enabled: true,
        permissions: mhost_bot::Permissions {
            admins: vec![admin_id],
            ..Default::default()
        },
        ..Default::default()
    };
    config.save(&paths.bot_config())?;

    print_success(&format!("Bot configured for {platform}"));
    println!("  Config: {}", paths.bot_config().display());
    println!("  Start:  mhost bot enable");
    Ok(())
}

pub async fn run_enable(paths: &MhostPaths, client: &mhost_ipc::IpcClient) -> Result<(), String> {
    let config =
        BotConfig::load(&paths.bot_config()).ok_or("Bot not configured. Run: mhost bot setup")?;
    if config.token.is_empty() {
        return Err("Bot token is empty".into());
    }

    // Create a small Node.js script that runs the bot via the mhost CLI
    // The bot is a Rust process, so we spawn mhostd-integrated bot as a managed process
    // For now, use a wrapper script that calls the Rust bot binary
    let bot_script = crate::embedded::ensure_script(
        "bot",
        "mhost-bot-runner.sh",
        &format!(
            "#!/bin/sh\nexec \"{}\" bot run-inline\n",
            std::env::current_exe()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|_| "mhost".into())
        ),
    )?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&bot_script, std::fs::Permissions::from_mode(0o755));
    }

    let proc_config = mhost_core::process::ProcessConfig {
        name: "mhost-bot".into(),
        command: "sh".into(),
        args: vec![bot_script.to_string_lossy().to_string()],
        max_restarts: 100,
        ..Default::default()
    };

    let params = serde_json::to_value(&proc_config).map_err(|e| format!("Serialize: {e}"))?;
    let resp = client
        .call(mhost_core::protocol::methods::PROCESS_START, params)
        .await
        .map_err(|e| format!("IPC: {e}"))?;

    if let Some(err) = resp.error {
        crate::output::print_error(&format!("Failed to start bot: {}", err.message));
    } else {
        print_success("Bot started as 'mhost-bot' (background)");
        println!("  Status: mhost list");
        println!("  Logs:   mhost logs mhost-bot");
        println!("  Stop:   mhost bot disable");
    }
    Ok(())
}

/// Run the bot inline (called by the wrapper script, not by users directly).
pub async fn run_inline(paths: &MhostPaths) -> Result<(), String> {
    let config = BotConfig::load(&paths.bot_config()).ok_or("Bot not configured")?;
    let bot = mhost_bot::TelegramBot::new(config, paths.clone());
    bot.run().await
}

pub async fn run_disable(paths: &MhostPaths) -> Result<(), String> {
    // Stop the managed bot process if running
    if paths.socket().exists() {
        let client = mhost_ipc::IpcClient::new(&paths.socket());
        let _ = client
            .call(
                mhost_core::protocol::methods::PROCESS_STOP,
                serde_json::json!({"name": "mhost-bot"}),
            )
            .await;
    }

    let mut config = BotConfig::load(&paths.bot_config()).ok_or("Bot not configured")?;
    config.enabled = false;
    config.save(&paths.bot_config())?;
    print_success("Bot stopped");
    Ok(())
}

pub fn run_status(paths: &MhostPaths) -> Result<(), String> {
    match BotConfig::load(&paths.bot_config()) {
        Some(c) => {
            println!("\n  Bot Status\n");
            println!("  Platform:            {}", c.platform);
            println!(
                "  Enabled:             {}",
                if c.enabled { "yes" } else { "no" }
            );
            println!("  Admins:              {:?}", c.permissions.admins);
            println!("  Operators:           {:?}", c.permissions.operators);
            println!("  Viewers:             {:?}", c.permissions.viewers);
            println!("  Rate limit:          {}/min", c.rate_limit);
            println!("  Confirm destructive: {}", c.confirm_destructive);
        }
        None => println!("  Bot not configured. Run: mhost bot setup"),
    }
    Ok(())
}

pub fn run_permissions(paths: &MhostPaths) -> Result<(), String> {
    let config = BotConfig::load(&paths.bot_config()).ok_or("Bot not configured")?;
    println!("\n  Bot Permissions\n");
    println!("  {:<12} {:<15} Users", "Role", "Commands");
    println!("  {}", "-".repeat(50));
    println!(
        "  {:<12} {:<15} {:?}",
        "Admin", "all", config.permissions.admins
    );
    println!(
        "  {:<12} {:<15} {:?}",
        "Operator", "start/stop/...", config.permissions.operators
    );
    println!(
        "  {:<12} {:<15} {:?}",
        "Viewer", "status/logs", config.permissions.viewers
    );
    println!(
        "  {:<12} {:<15} {:?}",
        "Blocked", "none", config.permissions.blocked
    );
    Ok(())
}

pub fn run_add_user(paths: &MhostPaths, user_id: i64, role: &str) -> Result<(), String> {
    let mut config = BotConfig::load(&paths.bot_config()).ok_or("Bot not configured")?;
    let r = match role {
        "admin" => Role::Admin,
        "operator" => Role::Operator,
        "viewer" => Role::Viewer,
        _ => {
            return Err(format!(
                "Invalid role: '{role}'. Use: admin, operator, viewer"
            ))
        }
    };
    config.permissions.add_user(user_id, r);
    config.save(&paths.bot_config())?;
    print_success(&format!("User {user_id} added as {role}"));
    Ok(())
}

pub fn run_remove_user(paths: &MhostPaths, user_id: i64) -> Result<(), String> {
    let mut config = BotConfig::load(&paths.bot_config()).ok_or("Bot not configured")?;
    config.permissions.remove_user(user_id);
    config.save(&paths.bot_config())?;
    print_success(&format!("User {user_id} removed"));
    Ok(())
}

pub fn run_logs(paths: &MhostPaths) -> Result<(), String> {
    let audit = mhost_bot::audit::AuditLog::new(&paths.root().join("bot-audit.jsonl"));
    let entries = audit.recent(20);
    if entries.is_empty() {
        println!("  No audit log entries.");
        return Ok(());
    }
    println!(
        "\n  {:<20} {:<10} {:<25} Result",
        "Timestamp", "User", "Command"
    );
    println!("  {}", "-".repeat(70));
    for e in &entries {
        println!(
            "  {:<20} {:<10} {:<25} {}",
            e.timestamp.format("%Y-%m-%d %H:%M"),
            e.user_id,
            e.command,
            e.result
        );
    }
    Ok(())
}

pub async fn run_chat_id(token: &str) -> Result<(), String> {
    println!("\n  Send /start to your bot in Telegram, then press Enter here...");
    let mut buf = String::new();
    let _ = io::stdin().read_line(&mut buf);

    println!("  Fetching chat ID...\n");

    let url = format!("https://api.telegram.org/bot{token}/getUpdates?timeout=1");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Telegram API error: {e}"))?;
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

    let results = body["result"].as_array();
    if let Some(updates) = results {
        let mut found = std::collections::HashMap::new();
        for update in updates {
            if let Some(chat) = update.get("message").and_then(|m| m.get("chat")) {
                let id = chat["id"].as_i64().unwrap_or(0);
                let name = chat["first_name"]
                    .as_str()
                    .or_else(|| chat["username"].as_str())
                    .unwrap_or("Unknown");
                if id != 0 {
                    found.insert(id, name.to_string());
                }
            }
        }
        if found.is_empty() {
            println!("  No messages found. Make sure you sent /start to the bot.");
        } else {
            for (id, name) in &found {
                println!("  ╔══════════════════════════════════════╗");
                println!("  ║  User: {:<29}║", name);
                println!("  ║  Chat ID: {:<27}║", id);
                println!("  ╚══════════════════════════════════════╝");
            }
            println!();
            println!("  Use this Chat ID in:");
            println!("    mhost notify setup");
            println!("    mhost agent setup");
            println!("    mhost bot setup");
        }
    } else {
        println!("  No updates from Telegram. Send /start to the bot first.");
    }
    println!();
    Ok(())
}

fn prompt(label: &str) -> String {
    print!("  {label}: ");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}
