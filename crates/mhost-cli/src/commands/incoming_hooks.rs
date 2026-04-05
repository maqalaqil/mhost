use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::output::{print_error, print_success};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
struct Hook {
    id: String,
    token: String,
    action: String,
    process: String,
    created_at: String,
    last_triggered: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct HooksFile {
    hooks: Vec<Hook>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const HOOKS_FILE: &str = "incoming-hooks.json";
const HOOKS_PORT: u16 = 19516;

fn hooks_path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".mhost").join(HOOKS_FILE))
        .ok_or_else(|| "Could not determine home directory".to_string())
}

fn load_hooks() -> Result<HooksFile, String> {
    let path = hooks_path()?;
    if !path.exists() {
        return Ok(HooksFile { hooks: vec![] });
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read hooks file: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse hooks file: {e}"))
}

fn save_hooks(hooks_file: &HooksFile) -> Result<(), String> {
    let path = hooks_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(hooks_file)
        .map_err(|e| format!("Failed to serialize: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write hooks file: {e}"))
}

fn generate_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let suffix: u64 = rng.gen_range(100_000..999_999);
    format!("hook_{suffix:x}")
}

fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..24).map(|_| rng.gen::<u8>()).collect();
    bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
}

fn webhook_url(id: &str, token: &str) -> String {
    format!("http://localhost:{HOOKS_PORT}/hooks/{id}?token={token}")
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

pub fn run_create(action: &str, process: &str) -> Result<(), String> {
    let valid_actions = ["restart", "stop", "start", "delete", "reload"];
    if !valid_actions.contains(&action) {
        return Err(format!(
            "Invalid action '{action}'. Valid actions: {}",
            valid_actions.join(", ")
        ));
    }

    let mut hooks_file = load_hooks()?;

    let id = generate_id();
    let token = generate_token();
    let url = webhook_url(&id, &token);

    let hook = Hook {
        id: id.clone(),
        token: token.clone(),
        action: action.to_string(),
        process: process.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        last_triggered: None,
    };

    let updated_hooks = {
        let mut new_hooks = hooks_file.hooks.clone();
        new_hooks.push(hook);
        new_hooks
    };
    hooks_file.hooks = updated_hooks;
    save_hooks(&hooks_file)?;

    println!();
    print_success("Webhook created!");
    println!("  URL:    {}", url.cyan());
    println!("  ID:     {}", id.dimmed());
    println!("  Action: {} {}", action.white().bold(), process.white());
    println!();

    Ok(())
}

pub fn run_list() -> Result<(), String> {
    let hooks_file = load_hooks()?;

    println!("\n  {} Incoming Webhooks\n", "Hooks".white().bold());

    if hooks_file.hooks.is_empty() {
        println!("  {}  {}", "○".dimmed(), "No webhooks configured".dimmed());
        println!(
            "     Create one: {}",
            "mhost hooks create --action restart --process api".cyan()
        );
        println!();
        return Ok(());
    }

    // Header
    println!(
        "  {}  {}  {}  {}  {}",
        format!("{:<16}", "ID").dimmed(),
        format!("{:<10}", "Action").bold(),
        format!("{:<14}", "Process").bold(),
        format!("{:<48}", "URL").dimmed(),
        "Last Triggered".dimmed(),
    );
    println!("  {}", "─".repeat(110).dimmed());

    for hook in &hooks_file.hooks {
        let url = webhook_url(&hook.id, &hook.token);
        let triggered = hook.last_triggered.as_deref().unwrap_or("never");

        println!(
            "  {}  {}  {}  {}  {}",
            format!("{:<16}", hook.id).cyan(),
            format!("{:<10}", hook.action).white(),
            format!("{:<14}", hook.process).white(),
            format!("{url:<48}").dimmed(),
            triggered.dimmed(),
        );
    }
    println!();

    Ok(())
}

pub fn run_remove(id: &str) -> Result<(), String> {
    let mut hooks_file = load_hooks()?;

    let original_len = hooks_file.hooks.len();
    let filtered: Vec<Hook> = hooks_file
        .hooks
        .iter()
        .filter(|h| h.id != id)
        .cloned()
        .collect();

    if filtered.len() == original_len {
        return Err(format!("Webhook '{id}' not found"));
    }

    hooks_file.hooks = filtered;
    save_hooks(&hooks_file)?;

    print_success(&format!("Webhook '{id}' removed"));
    Ok(())
}

pub fn run_test(id: &str) -> Result<(), String> {
    let hooks_file = load_hooks()?;

    let hook = hooks_file
        .hooks
        .iter()
        .find(|h| h.id == id)
        .ok_or_else(|| format!("Webhook '{id}' not found"))?;

    println!(
        "\n  {} Simulating webhook: {} {}\n",
        "▸".cyan(),
        hook.action.white().bold(),
        hook.process.white(),
    );

    // Execute the action by shelling out to mhost
    let status = std::process::Command::new("mhost")
        .arg(&hook.action)
        .arg(&hook.process)
        .status()
        .map_err(|e| {
            format!(
                "Failed to execute mhost {} {}: {e}",
                hook.action, hook.process
            )
        })?;

    if status.success() {
        print_success(&format!(
            "Webhook test complete: {} {}",
            hook.action, hook.process
        ));
    } else {
        print_error(&format!(
            "Webhook action failed with exit code: {}",
            status.code().unwrap_or(-1)
        ));
    }

    // Update last_triggered timestamp
    let mut updated_file = load_hooks()?;
    let now = chrono::Utc::now().to_rfc3339();
    let updated_hooks: Vec<Hook> = updated_file
        .hooks
        .iter()
        .map(|h| {
            if h.id == id {
                Hook {
                    last_triggered: Some(now.clone()),
                    ..h.clone()
                }
            } else {
                h.clone()
            }
        })
        .collect();
    updated_file.hooks = updated_hooks;
    save_hooks(&updated_file)?;

    Ok(())
}
