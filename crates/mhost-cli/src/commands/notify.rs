use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::output::{print_error, print_success};

// ─── Config Types ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyConfig {
    pub channels: HashMap<String, ChannelConfig>,
    pub global_events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ChannelConfig {
    #[serde(rename = "telegram")]
    Telegram {
        bot_token: String,
        chat_id: String,
        events: Vec<String>,
        enabled: bool,
    },
    #[serde(rename = "slack")]
    Slack {
        webhook_url: String,
        channel: Option<String>,
        events: Vec<String>,
        enabled: bool,
    },
    #[serde(rename = "discord")]
    Discord {
        webhook_url: String,
        events: Vec<String>,
        enabled: bool,
    },
    #[serde(rename = "webhook")]
    Webhook {
        url: String,
        headers: HashMap<String, String>,
        events: Vec<String>,
        enabled: bool,
    },
}

const ALL_EVENTS: &[&str] = &[
    "crash",
    "restart",
    "errored",
    "stopped",
    "recovered",
    "health_fail",
    "high_restarts",
    "5xx_error",
    "oom_kill",
    "deploy_success",
    "deploy_fail",
];

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            channels: HashMap::new(),
            global_events: ALL_EVENTS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

// ─── Config IO ──────────────────────────────────────────────────

fn load_config(path: &Path) -> NotifyConfig {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => NotifyConfig::default(),
    }
}

fn save_config(path: &Path, config: &NotifyConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, json).map_err(|e| format!("Failed to write config: {e}"))
}

fn prompt(label: &str) -> String {
    print!("  {label}: ");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn prompt_default(label: &str, default: &str) -> String {
    print!("  {label} [{default}]: ");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let val = input.trim();
    if val.is_empty() {
        default.to_string()
    } else {
        val.to_string()
    }
}

fn prompt_events() -> Vec<String> {
    println!("\n  Available alert events:");
    for (i, event) in ALL_EVENTS.iter().enumerate() {
        println!("    {:<3} {}", i + 1, event);
    }
    println!("    *   All events");
    let selection = prompt("Select events (comma-separated numbers, or * for all)");
    if selection.trim() == "*" {
        return ALL_EVENTS.iter().map(|s| s.to_string()).collect();
    }
    selection
        .split(',')
        .filter_map(|s| {
            let idx: usize = s.trim().parse().ok()?;
            ALL_EVENTS.get(idx.checked_sub(1)?).map(|s| s.to_string())
        })
        .collect()
}

// ─── Commands ───────────────────────────────────────────────────

pub fn run_setup(config_path: &Path) -> Result<(), String> {
    let mut config = load_config(config_path);

    println!("\n  mhost Notification Setup\n");
    println!("  Select channel type:");
    println!("    1) Telegram");
    println!("    2) Slack");
    println!("    3) Discord");
    println!("    4) Generic Webhook");
    println!();

    let choice = prompt("Channel type (1-4)");

    let (name, channel) = match choice.as_str() {
        "1" => setup_telegram(),
        "2" => setup_slack(),
        "3" => setup_discord(),
        "4" => setup_webhook(),
        _ => return Err("Invalid choice. Use 1-4.".into()),
    }?;

    config.channels.insert(name.clone(), channel);
    save_config(config_path, &config)?;

    print_success(&format!("Channel '{name}' configured and saved"));
    println!("  Config: {}", config_path.display());
    println!();
    println!("  To test:  mhost notify test {name}");
    println!("  To start: mhost notify start");
    Ok(())
}

fn setup_telegram() -> Result<(String, ChannelConfig), String> {
    println!("\n  Telegram Setup");
    println!("  ─────────────────────────────");
    println!("  1. Message @BotFather on Telegram");
    println!("  2. Send /newbot and follow the instructions");
    println!("  3. Copy the bot token below");
    println!("  4. Message your bot, then get your chat ID from @userinfobot");
    println!();

    let bot_token = prompt("Bot token");
    if bot_token.is_empty() {
        return Err("Bot token is required".into());
    }

    let chat_id = prompt("Chat ID");
    if chat_id.is_empty() {
        return Err("Chat ID is required".into());
    }

    let name = prompt_default("Channel name", "telegram");
    let events = prompt_events();

    Ok((
        name,
        ChannelConfig::Telegram {
            bot_token,
            chat_id,
            events,
            enabled: true,
        },
    ))
}

fn setup_slack() -> Result<(String, ChannelConfig), String> {
    println!("\n  Slack Setup");
    println!("  ─────────────────────────────");
    println!("  1. Go to api.slack.com/apps");
    println!("  2. Create app > Incoming Webhooks > Add New Webhook");
    println!("  3. Copy the webhook URL below");
    println!();

    let webhook_url = prompt("Webhook URL");
    if webhook_url.is_empty() || !webhook_url.starts_with("https://") {
        return Err("Valid webhook URL is required (starts with https://)".into());
    }

    let channel = prompt_default("Slack channel", "#alerts");
    let channel = if channel.is_empty() {
        None
    } else {
        Some(channel)
    };

    let name = prompt_default("Channel name", "slack");
    let events = prompt_events();

    Ok((
        name,
        ChannelConfig::Slack {
            webhook_url,
            channel,
            events,
            enabled: true,
        },
    ))
}

fn setup_discord() -> Result<(String, ChannelConfig), String> {
    println!("\n  Discord Setup");
    println!("  ─────────────────────────────");
    println!("  1. Open Discord > Server Settings > Integrations > Webhooks");
    println!("  2. Create Webhook > Copy Webhook URL");
    println!();

    let webhook_url = prompt("Webhook URL");
    if webhook_url.is_empty() || !webhook_url.starts_with("https://") {
        return Err("Valid webhook URL is required".into());
    }

    let name = prompt_default("Channel name", "discord");
    let events = prompt_events();

    Ok((
        name,
        ChannelConfig::Discord {
            webhook_url,
            events,
            enabled: true,
        },
    ))
}

fn setup_webhook() -> Result<(String, ChannelConfig), String> {
    println!("\n  Generic Webhook Setup");
    println!("  ─────────────────────────────");

    let url = prompt("Webhook URL");
    if url.is_empty() {
        return Err("URL is required".into());
    }

    let mut headers = HashMap::new();
    println!("  Add custom headers (leave empty to skip):");
    loop {
        let key = prompt("Header name (empty to finish)");
        if key.is_empty() {
            break;
        }
        let value = prompt(&format!("  Value for '{key}'"));
        headers.insert(key, value);
    }

    let name = prompt_default("Channel name", "webhook");
    let events = prompt_events();

    Ok((
        name,
        ChannelConfig::Webhook {
            url,
            headers,
            events,
            enabled: true,
        },
    ))
}

pub fn run_list(config_path: &Path) -> Result<(), String> {
    let config = load_config(config_path);

    if config.channels.is_empty() {
        println!("  No notification channels configured.");
        println!("  Run: mhost notify setup");
        return Ok(());
    }

    println!("\n  Configured Notification Channels\n");
    println!("  {:<15} {:<12} {:<10} Events", "Name", "Type", "Status");
    println!("  {}", "─".repeat(60));

    for (name, channel) in &config.channels {
        let (ch_type, enabled, events) = match channel {
            ChannelConfig::Telegram {
                enabled, events, ..
            } => ("telegram", *enabled, events),
            ChannelConfig::Slack {
                enabled, events, ..
            } => ("slack", *enabled, events),
            ChannelConfig::Discord {
                enabled, events, ..
            } => ("discord", *enabled, events),
            ChannelConfig::Webhook {
                enabled, events, ..
            } => ("webhook", *enabled, events),
        };
        let status = if enabled { "active" } else { "disabled" };
        let event_str = if events.len() == ALL_EVENTS.len() {
            "all".to_string()
        } else {
            events.join(", ")
        };
        println!("  {name:<15} {ch_type:<12} {status:<10} {event_str}");
    }
    println!();
    Ok(())
}

pub async fn run_test(config_path: &Path, channel_name: &str) -> Result<(), String> {
    let config = load_config(config_path);

    let channel = config
        .channels
        .get(channel_name)
        .ok_or_else(|| format!("Channel '{channel_name}' not found. Run: mhost notify list"))?;

    println!("  Sending test notification to '{channel_name}'...");

    match channel {
        ChannelConfig::Telegram {
            bot_token, chat_id, ..
        } => send_telegram_test(bot_token, chat_id).await,
        ChannelConfig::Slack { webhook_url, .. } => send_slack_test(webhook_url).await,
        ChannelConfig::Discord { webhook_url, .. } => send_discord_test(webhook_url).await,
        ChannelConfig::Webhook { url, headers, .. } => send_webhook_test(url, headers).await,
    }
}

async fn send_telegram_test(bot_token: &str, chat_id: &str) -> Result<(), String> {
    let url = format!("https://api.telegram.org/bot{bot_token}/sendMessage");
    let body = serde_json::json!({
        "chat_id": chat_id,
        "text": "✅ *mhost notification test*\n\nThis is a test message from `mhost notify test`\\.\nYour Telegram alerts are configured correctly\\!",
        "parse_mode": "MarkdownV2"
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if resp.status().is_success() {
        print_success("Test message sent to Telegram!");
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!("Telegram API error {status}: {text}"))
    }
}

async fn send_slack_test(webhook_url: &str) -> Result<(), String> {
    let body = serde_json::json!({
        "blocks": [
            {
                "type": "header",
                "text": { "type": "plain_text", "text": "mhost Notification Test" }
            },
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": ":white_check_mark: This is a test message from `mhost notify test`.\nYour Slack alerts are configured correctly!"
                }
            }
        ]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(webhook_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if resp.status().is_success() {
        print_success("Test message sent to Slack!");
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!("Slack API error {status}: {text}"))
    }
}

async fn send_discord_test(webhook_url: &str) -> Result<(), String> {
    let body = serde_json::json!({
        "embeds": [{
            "title": "mhost Notification Test",
            "description": "This is a test message from `mhost notify test`.\nYour Discord alerts are configured correctly!",
            "color": 3066993
        }]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(webhook_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if resp.status().is_success() || resp.status().as_u16() == 204 {
        print_success("Test message sent to Discord!");
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!("Discord API error {status}: {text}"))
    }
}

async fn send_webhook_test(url: &str, headers: &HashMap<String, String>) -> Result<(), String> {
    let body = serde_json::json!({
        "event": "test",
        "source": "mhost",
        "message": "This is a test notification from mhost notify test",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    let client = reqwest::Client::new();
    let mut req = client.post(url).json(&body);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if resp.status().is_success() {
        print_success("Test message sent to webhook!");
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(format!("Webhook error {status}: {text}"))
    }
}

pub fn run_remove(config_path: &Path, channel_name: &str) -> Result<(), String> {
    let mut config = load_config(config_path);

    if config.channels.remove(channel_name).is_none() {
        return Err(format!("Channel '{channel_name}' not found"));
    }

    save_config(config_path, &config)?;
    print_success(&format!("Channel '{channel_name}' removed"));
    Ok(())
}

pub fn run_enable(config_path: &Path, channel_name: &str, enable: bool) -> Result<(), String> {
    let mut config = load_config(config_path);

    let channel = config
        .channels
        .get_mut(channel_name)
        .ok_or_else(|| format!("Channel '{channel_name}' not found"))?;

    match channel {
        ChannelConfig::Telegram { enabled, .. }
        | ChannelConfig::Slack { enabled, .. }
        | ChannelConfig::Discord { enabled, .. }
        | ChannelConfig::Webhook { enabled, .. } => *enabled = enable,
    }

    save_config(config_path, &config)?;
    let action = if enable { "enabled" } else { "disabled" };
    print_success(&format!("Channel '{channel_name}' {action}"));
    Ok(())
}

pub fn run_events(config_path: &Path, channel_name: Option<&str>) -> Result<(), String> {
    let config = load_config(config_path);

    println!("\n  Alert Events Reference\n");
    println!("  {:<18} Description", "Event");
    println!("  {}", "─".repeat(55));
    println!("  {:<18} Process crashed (non-zero exit)", "crash");
    println!("  {:<18} Process auto-restarted by mhost", "restart");
    println!(
        "  {:<18} Process hit max restarts (circuit breaker)",
        "errored"
    );
    println!("  {:<18} Process was stopped", "stopped");
    println!(
        "  {:<18} Process came back online after failure",
        "recovered"
    );
    println!("  {:<18} Health check probe failed", "health_fail");
    println!("  {:<18} Process restarted 5+ times", "high_restarts");
    println!("  {:<18} Health endpoint returned HTTP 5xx", "5xx_error");
    println!("  {:<18} Process killed due to memory limit", "oom_kill");
    println!("  {:<18} Deploy completed successfully", "deploy_success");
    println!("  {:<18} Deploy failed", "deploy_fail");

    if let Some(name) = channel_name {
        if let Some(channel) = config.channels.get(name) {
            let events = match channel {
                ChannelConfig::Telegram { events, .. }
                | ChannelConfig::Slack { events, .. }
                | ChannelConfig::Discord { events, .. }
                | ChannelConfig::Webhook { events, .. } => events,
            };
            println!("\n  Channel '{name}' subscribed to:");
            for e in events {
                println!("    [x] {e}");
            }
            let missing: Vec<&&str> = ALL_EVENTS
                .iter()
                .filter(|e| !events.contains(&e.to_string()))
                .collect();
            for e in missing {
                println!("    [ ] {e}");
            }
        }
    }

    println!();
    Ok(())
}

/// Start the built-in notifier daemon as an mhost-managed process.
pub async fn run_start(config_path: &Path, client: &mhost_ipc::IpcClient) -> Result<(), String> {
    let config = load_config(config_path);

    if config.channels.is_empty() {
        return Err("No channels configured. Run: mhost notify setup".into());
    }

    // Get the notifier script (embedded in binary, extracted to ~/.mhost/)
    let notifier_script = crate::embedded::ensure_notifier()?;

    // Build env vars from the first enabled telegram channel
    let mut env_vars = HashMap::new();
    for ch in config.channels.values() {
        match ch {
            ChannelConfig::Telegram {
                bot_token,
                chat_id,
                enabled,
                ..
            } if *enabled => {
                env_vars.insert("MHOST_TELEGRAM_TOKEN".to_string(), bot_token.clone());
                env_vars.insert("MHOST_TELEGRAM_CHAT".to_string(), chat_id.clone());
                break;
            }
            _ => {}
        }
    }

    let proc_config = mhost_core::process::ProcessConfig {
        name: "mhost-notifier".to_string(),
        command: "node".to_string(),
        args: vec![notifier_script.to_string_lossy().to_string()],
        env: env_vars,
        max_restarts: 100,
        ..Default::default()
    };

    let params = serde_json::to_value(&proc_config).map_err(|e| format!("Serialize: {e}"))?;
    let resp = client
        .call(mhost_core::protocol::methods::PROCESS_START, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!(
            "Failed to start notifier: {err_msg}",
            err_msg = err.message
        ));
    } else {
        print_success("Notifier started as 'mhost-notifier' process");
        println!("  View status: mhost info mhost-notifier");
        println!("  View logs:   mhost logs mhost-notifier");
    }

    Ok(())
}
