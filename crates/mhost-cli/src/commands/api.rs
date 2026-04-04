use std::fs;
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHasher};
use chrono::Utc;
use clap::Subcommand;
use colored::Colorize;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::output;

// ---------------------------------------------------------------------------
// CLI structure
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum ApiCommands {
    /// Start the API server.
    Start {
        /// Port to listen on.
        #[arg(long, default_value = "19516")]
        port: u16,
        /// Address to bind to.
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
    },
    /// Stop the API server.
    Stop,
    /// Show API server status.
    Status,
    /// Manage API tokens.
    Token {
        #[command(subcommand)]
        action: TokenAction,
    },
    /// Manage webhooks.
    Webhook {
        #[command(subcommand)]
        action: WebhookAction,
    },
}

#[derive(Subcommand)]
pub enum TokenAction {
    /// Create a new API token.
    Create {
        /// Human-readable name for the token.
        #[arg(long)]
        name: String,
        /// Role: admin, operator, or viewer.
        #[arg(long)]
        role: String,
        /// Expiry duration (e.g. "30d", "12h"). Omit for no expiry.
        #[arg(long)]
        expires: Option<String>,
    },
    /// List all API tokens.
    List,
    /// Revoke an API token by ID.
    Revoke {
        /// Token ID (e.g. tok_XXXXXXXX).
        id: String,
    },
}

#[derive(Subcommand)]
pub enum WebhookAction {
    /// Register a new webhook endpoint.
    Add {
        /// URL to receive webhook payloads.
        #[arg(long)]
        url: String,
        /// Comma-separated event names to subscribe to.
        #[arg(long)]
        events: String,
        /// Optional HMAC signing secret.
        #[arg(long)]
        secret: Option<String>,
    },
    /// List all registered webhooks.
    List,
    /// Remove a webhook by ID.
    Remove {
        /// Webhook ID (e.g. wh_XXXXXXXX).
        id: String,
    },
    /// Send a test payload to a webhook.
    Test {
        /// Webhook ID.
        id: String,
    },
    /// Show recent webhook delivery failures.
    Failures,
}

// ---------------------------------------------------------------------------
// Persistence types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenRecord {
    id: String,
    name: String,
    role: String,
    hash: String,
    created: String,
    expires: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenStore {
    tokens: Vec<TokenRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WebhookRecord {
    id: String,
    url: String,
    events: Vec<String>,
    secret: Option<String>,
    created: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WebhookStore {
    webhooks: Vec<WebhookRecord>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn mhost_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".mhost");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create ~/.mhost: {e}"))?;
    }
    Ok(dir)
}

fn tokens_path() -> Result<PathBuf, String> {
    Ok(mhost_dir()?.join("api-tokens.json"))
}

fn webhooks_path() -> Result<PathBuf, String> {
    Ok(mhost_dir()?.join("webhooks.json"))
}

fn load_tokens() -> Result<TokenStore, String> {
    let path = tokens_path()?;
    if !path.exists() {
        return Ok(TokenStore { tokens: vec![] });
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("Failed to read tokens: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("Failed to parse tokens: {e}"))
}

fn save_tokens(store: &TokenStore) -> Result<(), String> {
    let path = tokens_path()?;
    let json =
        serde_json::to_string_pretty(store).map_err(|e| format!("Failed to serialize: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write tokens: {e}"))
}

fn load_webhooks() -> Result<WebhookStore, String> {
    let path = webhooks_path()?;
    if !path.exists() {
        return Ok(WebhookStore { webhooks: vec![] });
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("Failed to read webhooks: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("Failed to parse webhooks: {e}"))
}

fn save_webhooks(store: &WebhookStore) -> Result<(), String> {
    let path = webhooks_path()?;
    let json =
        serde_json::to_string_pretty(store).map_err(|e| format!("Failed to serialize: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write webhooks: {e}"))
}

fn random_hex(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub async fn run(cmd: ApiCommands) -> Result<(), String> {
    match cmd {
        ApiCommands::Start { port, bind } => run_start(port, &bind),
        ApiCommands::Stop => run_stop(),
        ApiCommands::Status => run_status(None),
        ApiCommands::Token { action } => match action {
            TokenAction::Create {
                name,
                role,
                expires,
            } => run_token_create(&name, &role, expires.as_deref()),
            TokenAction::List => run_token_list(),
            TokenAction::Revoke { id } => run_token_revoke(&id),
        },
        ApiCommands::Webhook { action } => match action {
            WebhookAction::Add {
                url,
                events,
                secret,
            } => run_webhook_add(&url, &events, secret.as_deref()),
            WebhookAction::List => run_webhook_list(),
            WebhookAction::Remove { id } => run_webhook_remove(&id),
            WebhookAction::Test { id } => run_webhook_test(&id).await,
            WebhookAction::Failures => run_webhook_failures(),
        },
    }
}

// ---------------------------------------------------------------------------
// Server commands (start / stop / status)
// ---------------------------------------------------------------------------

fn run_start(port: u16, bind: &str) -> Result<(), String> {
    // Check if already running
    if TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(500),
    )
    .is_ok()
    {
        println!(
            "{} API server is already running on port {}",
            "!".yellow(),
            port.to_string().cyan()
        );
        return Ok(());
    }

    println!(
        "{} Starting API server on {}:{}",
        ">>".green(),
        bind.cyan(),
        port.to_string().cyan()
    );
    println!(
        "{}",
        "   (The API server will be launched by the daemon.)".dimmed()
    );
    // In a full implementation this would send an IPC message to the daemon
    // to spawn the API server process. For now we print the intent.
    output::print_success(&format!("API server start requested on {bind}:{port}"));
    Ok(())
}

fn run_stop() -> Result<(), String> {
    println!("{} Stopping API server...", ">>".green());
    output::print_success("API server stop requested");
    Ok(())
}

fn run_status(port: Option<u16>) -> Result<(), String> {
    let port = port.unwrap_or(19516);
    let addr = format!("127.0.0.1:{port}");
    match TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_millis(500)) {
        Ok(_) => println!(
            "{} API server is {} on port {}",
            "\u{2714}".green(),
            "running".green().bold(),
            port.to_string().cyan()
        ),
        Err(_) => println!(
            "{} API server is {} (port {})",
            "\u{2718}".red(),
            "not running".red().bold(),
            port.to_string().cyan()
        ),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Token commands
// ---------------------------------------------------------------------------

fn run_token_create(name: &str, role: &str, expires: Option<&str>) -> Result<(), String> {
    // Validate role
    let valid_roles = ["admin", "operator", "viewer"];
    if !valid_roles.contains(&role) {
        return Err(format!(
            "Invalid role '{}'. Must be one of: {}",
            role,
            valid_roles.join(", ")
        ));
    }

    let token_id = format!("tok_{}", random_hex(8));
    let raw_secret = format!("mhost_{}_{}", token_id, random_hex(24));

    // Hash the secret with argon2
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(raw_secret.as_bytes(), &salt)
        .map_err(|e| format!("Failed to hash token: {e}"))?
        .to_string();

    let now = Utc::now();
    let expires_at = expires.map(|e| {
        // Simple duration parse: "30d", "12h"
        let (num_str, unit) = e.split_at(e.len().saturating_sub(1));
        let num: i64 = num_str.parse().unwrap_or(30);
        let duration = match unit {
            "d" => chrono::Duration::days(num),
            "h" => chrono::Duration::hours(num),
            "m" => chrono::Duration::minutes(num),
            _ => chrono::Duration::days(num),
        };
        (now + duration).to_rfc3339()
    });

    let record = TokenRecord {
        id: token_id.clone(),
        name: name.to_owned(),
        role: role.to_owned(),
        hash,
        created: now.to_rfc3339(),
        expires: expires_at,
    };

    let store = load_tokens()?;
    let updated = TokenStore {
        tokens: {
            let mut t = store.tokens.clone();
            t.push(record);
            t
        },
    };
    save_tokens(&updated)?;

    println!();
    println!("{} Token created successfully!", "\u{2714}".green());
    println!();
    println!("  {} {}", "ID:".dimmed(), token_id.cyan());
    println!("  {} {}", "Name:".dimmed(), name.cyan());
    println!("  {} {}", "Role:".dimmed(), role.cyan());
    println!();
    println!("  {} {}", "Secret:".dimmed(), raw_secret.yellow());
    println!();
    println!(
        "{}",
        "  \u{26a0}  Save this secret now — it will not be shown again!"
            .yellow()
            .bold()
    );
    println!();

    Ok(())
}

fn run_token_list() -> Result<(), String> {
    let store = load_tokens()?;

    if store.tokens.is_empty() {
        println!("{} No API tokens configured.", "i".cyan());
        return Ok(());
    }

    println!();
    println!(
        "  {:<16} {:<20} {:<10} {}",
        "ID".bold(),
        "NAME".bold(),
        "ROLE".bold(),
        "CREATED".bold()
    );
    println!("  {}", "-".repeat(70));

    for tok in &store.tokens {
        let created_short = tok.created.get(..10).unwrap_or(&tok.created);
        println!(
            "  {:<16} {:<20} {:<10} {}",
            tok.id.cyan(),
            tok.name,
            tok.role,
            created_short.dimmed()
        );
    }
    println!();

    Ok(())
}

fn run_token_revoke(id: &str) -> Result<(), String> {
    let store = load_tokens()?;

    let original_len = store.tokens.len();
    let remaining: Vec<TokenRecord> = store
        .tokens
        .iter()
        .filter(|t| t.id != id)
        .cloned()
        .collect();

    if remaining.len() == original_len {
        return Err(format!("Token '{id}' not found"));
    }

    let updated = TokenStore { tokens: remaining };
    save_tokens(&updated)?;

    println!("{} Token {} revoked.", "\u{2714}".green(), id.cyan());
    Ok(())
}

// ---------------------------------------------------------------------------
// Webhook commands
// ---------------------------------------------------------------------------

fn run_webhook_add(url: &str, events: &str, secret: Option<&str>) -> Result<(), String> {
    let webhook_id = format!("wh_{}", random_hex(8));
    let event_list: Vec<String> = events.split(',').map(|s| s.trim().to_owned()).collect();

    let record = WebhookRecord {
        id: webhook_id.clone(),
        url: url.to_owned(),
        events: event_list.clone(),
        secret: secret.map(|s| s.to_owned()),
        created: Utc::now().to_rfc3339(),
    };

    let store = load_webhooks()?;
    let updated = WebhookStore {
        webhooks: {
            let mut w = store.webhooks.clone();
            w.push(record);
            w
        },
    };
    save_webhooks(&updated)?;

    println!();
    println!("{} Webhook registered!", "\u{2714}".green());
    println!("  {} {}", "ID:".dimmed(), webhook_id.cyan());
    println!("  {} {}", "URL:".dimmed(), url.cyan());
    println!("  {} {}", "Events:".dimmed(), event_list.join(", ").cyan());
    println!();

    Ok(())
}

fn run_webhook_list() -> Result<(), String> {
    let store = load_webhooks()?;

    if store.webhooks.is_empty() {
        println!("{} No webhooks configured.", "i".cyan());
        return Ok(());
    }

    println!();
    println!(
        "  {:<20} {:<40} {}",
        "ID".bold(),
        "URL".bold(),
        "EVENTS".bold()
    );
    println!("  {}", "-".repeat(80));

    for wh in &store.webhooks {
        println!(
            "  {:<20} {:<40} {}",
            wh.id.cyan(),
            wh.url,
            wh.events.join(", ").dimmed()
        );
    }
    println!();

    Ok(())
}

fn run_webhook_remove(id: &str) -> Result<(), String> {
    let store = load_webhooks()?;

    let original_len = store.webhooks.len();
    let remaining: Vec<WebhookRecord> = store
        .webhooks
        .iter()
        .filter(|w| w.id != id)
        .cloned()
        .collect();

    if remaining.len() == original_len {
        return Err(format!("Webhook '{id}' not found"));
    }

    let updated = WebhookStore {
        webhooks: remaining,
    };
    save_webhooks(&updated)?;

    println!("{} Webhook {} removed.", "\u{2714}".green(), id.cyan());
    Ok(())
}

async fn run_webhook_test(id: &str) -> Result<(), String> {
    let store = load_webhooks()?;

    let webhook = store
        .webhooks
        .iter()
        .find(|w| w.id == id)
        .ok_or_else(|| format!("Webhook '{id}' not found"))?;

    println!(
        "{} Sending test payload to {}...",
        ">>".green(),
        webhook.url.cyan()
    );

    let payload = serde_json::json!({
        "event": "test",
        "timestamp": Utc::now().to_rfc3339(),
        "data": {
            "message": "This is a test webhook delivery from mhost."
        }
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&webhook.url)
        .json(&payload)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = response.status();
    if status.is_success() {
        println!(
            "{} Test delivered — HTTP {}",
            "\u{2714}".green(),
            status.as_u16().to_string().green()
        );
    } else {
        println!(
            "{} Server returned HTTP {}",
            "\u{2718}".red(),
            status.as_u16().to_string().red()
        );
    }

    Ok(())
}

fn run_webhook_failures() -> Result<(), String> {
    let failures_path = mhost_dir()?.join("webhook-failures.json");
    if !failures_path.exists() {
        println!("{} No webhook delivery failures recorded.", "i".cyan());
        return Ok(());
    }

    let data =
        fs::read_to_string(&failures_path).map_err(|e| format!("Failed to read failures: {e}"))?;
    println!("{data}");
    Ok(())
}
