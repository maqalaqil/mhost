use std::io::{self, Write};

use mhost_core::paths::MhostPaths;
use mhost_core::process::ProcessConfig;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

use crate::output::{print_error, print_success};

// ─── Setup Wizard ────────────────────────────────────────────────────────────

pub fn run_setup(paths: &MhostPaths) -> Result<(), String> {
    // Load existing config if any — use as defaults
    let agent_path = paths.root().join("agent.json");
    let existing: serde_json::Value = std::fs::read_to_string(&agent_path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();

    let cur_provider = existing["provider"].as_str().unwrap_or("");
    let cur_key = existing["api_key"].as_str().unwrap_or("");
    let cur_model = existing["model"].as_str().unwrap_or("");
    let cur_tg_token = existing["telegram_token"].as_str().unwrap_or("");
    let cur_tg_chat = existing["telegram_chat_id"].as_str().unwrap_or("");
    let cur_autonomy = existing["autonomy"].as_str().unwrap_or("supervised");

    let has_existing = !cur_provider.is_empty();

    println!("\n  mhost Agent Setup");
    if has_existing {
        println!("  (Press Enter to keep current value)\n");
    } else {
        println!();
    }

    // Provider
    let provider_hint = if cur_provider == "claude" {
        "2"
    } else if !cur_provider.is_empty() {
        "1"
    } else {
        ""
    };
    println!("  LLM provider:  1) OpenAI  2) Claude");
    let choice = prompt_default("Provider (1-2)", provider_hint);
    let (provider, fallback_model) = match choice.as_str() {
        "1" | "openai" => ("openai", "gpt-4o"),
        "2" | "claude" | "anthropic" => ("claude", "claude-sonnet-4-20250514"),
        "" if !cur_provider.is_empty() => (cur_provider, cur_model),
        _ => return Err("Invalid choice — enter 1 or 2".into()),
    };

    // API key — mask current value
    let key_hint = if cur_key.len() > 8 {
        format!("{}...{}", &cur_key[..4], &cur_key[cur_key.len() - 4..])
    } else if !cur_key.is_empty() {
        "****".into()
    } else {
        String::new()
    };
    let api_key = if key_hint.is_empty() {
        let k = prompt("API key");
        if k.is_empty() {
            return Err("API key is required".into());
        }
        k
    } else {
        let k = prompt_default("API key", &key_hint);
        if k == key_hint || k.is_empty() {
            cur_key.to_string()
        } else {
            k
        }
    };

    let model_default = if !cur_model.is_empty() {
        cur_model
    } else {
        fallback_model
    };
    let model = prompt_default("Model", model_default);

    let tg_token = if cur_tg_token.is_empty() {
        prompt("Telegram bot token")
    } else {
        let hint = format!("{}...", &cur_tg_token[..cur_tg_token.len().min(10)]);
        let v = prompt_default("Telegram bot token", &hint);
        if v == hint || v.is_empty() {
            cur_tg_token.to_string()
        } else {
            v
        }
    };

    let tg_chat = if cur_tg_chat.is_empty() {
        prompt("Telegram chat ID")
    } else {
        prompt_default("Telegram chat ID", cur_tg_chat)
    };

    let autonomy = prompt_default("Autonomy (autonomous/supervised/manual)", cur_autonomy);
    if !matches!(autonomy.as_str(), "autonomous" | "supervised" | "manual") {
        return Err(format!("Unknown autonomy level '{autonomy}'"));
    }

    let config = serde_json::json!({
        "enabled": true,
        "provider": provider,
        "api_key": api_key,
        "model": model,
        "telegram_token": tg_token,
        "telegram_chat_id": tg_chat,
        "autonomy": autonomy,
        "allowed_actions": ["restart", "scale", "logs", "info", "list", "save", "start"],
        "blocked_actions": ["delete", "kill"],
        "confirm_destructive": true,
        "max_actions_per_hour": 20,
        "observe_interval_seconds": 30,
        "conversation_history_limit": 20
    });

    if let Some(parent) = agent_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create config dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(&config).map_err(|e| format!("Serialize: {e}"))?;
    std::fs::write(&agent_path, json).map_err(|e| format!("Write: {e}"))?;

    print_success(&format!("Agent configured (autonomy: {autonomy})"));
    println!("  Config: {}", agent_path.display());
    println!("  Start:  mhost agent start");
    Ok(())
}

// ─── Start ───────────────────────────────────────────────────────────────────

pub async fn run_start(paths: &MhostPaths, client: &IpcClient) -> Result<(), String> {
    let agent_config = paths.root().join("agent.json");
    if !agent_config.exists() {
        return Err("Agent not configured. Run: mhost agent setup".into());
    }

    let script = crate::embedded::ensure_agent()?;

    let proc_config = ProcessConfig {
        name: "mhost-agent".into(),
        command: "node".into(),
        args: vec![script.to_string_lossy().to_string()],
        max_restarts: 100,
        ..Default::default()
    };

    let params = serde_json::to_value(&proc_config).map_err(|e| format!("Serialize error: {e}"))?;
    let resp = client
        .call(methods::PROCESS_START, params)
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Failed to start agent: {}", err.message));
    } else {
        print_success("Agent started as 'mhost-agent'");
        println!("  Status: mhost info mhost-agent");
        println!("  Logs:   mhost logs mhost-agent");
        println!("  Stop:   mhost agent stop");
    }
    Ok(())
}

// ─── Stop ────────────────────────────────────────────────────────────────────

pub async fn run_stop(client: &IpcClient) -> Result<(), String> {
    let resp = client
        .call(
            methods::PROCESS_STOP,
            serde_json::json!({ "name": "mhost-agent" }),
        )
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&err.message);
    } else {
        print_success("Agent stopped");
    }
    Ok(())
}

// ─── Status ──────────────────────────────────────────────────────────────────

pub fn run_status(paths: &MhostPaths) -> Result<(), String> {
    let agent_config = paths.root().join("agent.json");
    if !agent_config.exists() {
        println!("  Agent not configured. Run: mhost agent setup");
        return Ok(());
    }

    let content =
        std::fs::read_to_string(&agent_config).map_err(|e| format!("Cannot read config: {e}"))?;
    let cfg: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid config JSON: {e}"))?;

    println!("\n  mhost Agent\n");
    println!(
        "  Provider:        {}",
        cfg["provider"].as_str().unwrap_or("?")
    );
    println!(
        "  Model:           {}",
        cfg["model"].as_str().unwrap_or("?")
    );
    println!(
        "  Autonomy:        {}",
        cfg["autonomy"].as_str().unwrap_or("?")
    );
    println!(
        "  Max actions/hr:  {}",
        cfg["max_actions_per_hour"].as_u64().unwrap_or(0)
    );
    println!(
        "  Observe every:   {}s",
        cfg["observe_interval_seconds"].as_u64().unwrap_or(0)
    );
    println!("  Blocked actions: {}", cfg["blocked_actions"]);
    println!("  Config:          {}", agent_config.display());
    println!();
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

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
