use std::io::{self, Write};

use mhost_core::paths::MhostPaths;
use mhost_core::process::ProcessConfig;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

use crate::output::{print_error, print_success};

// ─── Setup Wizard ────────────────────────────────────────────────────────────

pub fn run_setup(paths: &MhostPaths) -> Result<(), String> {
    println!("\n  mhost Agent Setup\n");

    println!("  Select LLM provider:");
    println!("    1) OpenAI (gpt-4o, gpt-4o-mini)");
    println!("    2) Claude (claude-sonnet-4-20250514)");
    println!();

    let choice = prompt("Provider (1-2)");
    let (provider, default_model) = match choice.as_str() {
        "1" => ("openai", "gpt-4o"),
        "2" => ("claude", "claude-sonnet-4-20250514"),
        _ => return Err("Invalid choice — enter 1 or 2".into()),
    };

    let api_key = prompt("API key (or env ref like ${OPENAI_API_KEY})");
    if api_key.is_empty() {
        return Err("API key is required".into());
    }

    let model = prompt_default("Model", default_model);
    let telegram_token = prompt("Telegram bot token (or ${MHOST_TELEGRAM_TOKEN})");
    let telegram_chat_id = prompt("Telegram chat ID (or ${MHOST_TELEGRAM_CHAT})");
    let autonomy = prompt_default(
        "Autonomy level (autonomous / supervised / manual)",
        "supervised",
    );

    if !matches!(autonomy.as_str(), "autonomous" | "supervised" | "manual") {
        return Err(format!(
            "Unknown autonomy level '{autonomy}'. Use: autonomous, supervised, or manual"
        ));
    }

    let config = serde_json::json!({
        "enabled": true,
        "provider": provider,
        "api_key": api_key,
        "model": model,
        "telegram_token": telegram_token,
        "telegram_chat_id": telegram_chat_id,
        "autonomy": autonomy,
        "allowed_actions": ["restart", "scale", "logs", "info", "list", "save", "start"],
        "blocked_actions": ["delete", "kill"],
        "confirm_destructive": true,
        "max_actions_per_hour": 20,
        "observe_interval_seconds": 30,
        "conversation_history_limit": 20
    });

    let agent_path = paths.root().join("agent.json");
    if let Some(parent) = agent_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create config dir: {e}"))?;
    }

    let json =
        serde_json::to_string_pretty(&config).map_err(|e| format!("Serialize error: {e}"))?;
    std::fs::write(&agent_path, json).map_err(|e| format!("Cannot write config: {e}"))?;

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

    let script = locate_agent_script()?;

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

/// Resolve the path to `examples/mhost-agent.js`, searching common locations.
fn locate_agent_script() -> Result<std::path::PathBuf, String> {
    let candidates: Vec<std::path::PathBuf> = vec![
        // Relative to cwd (useful during development)
        std::path::PathBuf::from("examples/mhost-agent.js"),
        // Relative to the CLI binary (installed layout: bin/mhost → ../../examples/…)
        std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.parent()
                    .map(|p| p.join("../../examples/mhost-agent.js"))
            })
            .unwrap_or_default(),
        // Absolute fallback for global npm installs
        std::path::PathBuf::from("/usr/local/lib/mhost/examples/mhost-agent.js"),
    ];

    candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| "Agent script not found. Ensure examples/mhost-agent.js exists.".to_string())
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
