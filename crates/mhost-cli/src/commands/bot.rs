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

pub async fn run_enable(paths: &MhostPaths) -> Result<(), String> {
    let config =
        BotConfig::load(&paths.bot_config()).ok_or("Bot not configured. Run: mhost bot setup")?;
    if config.token.is_empty() {
        return Err("Bot token is empty".into());
    }

    println!("  Starting {} bot...", config.platform);
    let bot = mhost_bot::TelegramBot::new(config, paths.clone());
    bot.run().await
}

pub fn run_disable(paths: &MhostPaths) -> Result<(), String> {
    let mut config = BotConfig::load(&paths.bot_config()).ok_or("Bot not configured")?;
    config.enabled = false;
    config.save(&paths.bot_config())?;
    print_success("Bot disabled");
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

fn prompt(label: &str) -> String {
    print!("  {label}: ");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}
