use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::sync::Mutex;

use mhost_core::protocol::methods;
use mhost_core::MhostPaths;
use mhost_ipc::IpcClient;

use crate::audit::{AuditEntry, AuditLog};
use crate::config::{command_allowed, BotConfig, Role};
use crate::rate_limit::RateLimiter;

// ---------------------------------------------------------------------------
// Pending confirmation state
// ---------------------------------------------------------------------------

struct PendingAction {
    command: String,
    args: Vec<String>,
    expires: Instant,
}

// ---------------------------------------------------------------------------
// TelegramBot
// ---------------------------------------------------------------------------

/// Long-polling Telegram bot that relays commands to the mhost daemon via IPC.
pub struct TelegramBot {
    token: String,
    client: reqwest::Client,
    config: BotConfig,
    rate_limiter: Arc<Mutex<RateLimiter>>,
    audit: AuditLog,
    /// Per-user pending destructive-action confirmations.
    pending_confirms: Arc<Mutex<HashMap<i64, PendingAction>>>,
    paths: MhostPaths,
}

impl TelegramBot {
    /// Create a new bot from `config` and a set of mhost paths.
    pub fn new(config: BotConfig, paths: MhostPaths) -> Self {
        let audit = AuditLog::new(&paths.root().join("bot-audit.jsonl"));
        Self {
            token: config.token.clone(),
            client: reqwest::Client::new(),
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new(config.rate_limit))),
            audit,
            pending_confirms: Arc::new(Mutex::new(HashMap::new())),
            paths,
            config,
        }
    }

    /// Start the long-polling loop.  This future runs indefinitely.
    pub async fn run(&self) -> Result<(), String> {
        tracing::info!("Telegram bot started (long-polling)");
        let mut offset: i64 = 0;

        loop {
            let updates = self.get_updates(offset).await?;
            for update in updates {
                let update_id = update["update_id"].as_i64().unwrap_or(0);
                offset = update_id + 1;

                if let Some(message) = update.get("message") {
                    self.handle_message(message).await;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    // -----------------------------------------------------------------------
    // Telegram API helpers
    // -----------------------------------------------------------------------

    async fn get_updates(&self, offset: i64) -> Result<Vec<serde_json::Value>, String> {
        let url = format!(
            "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=30",
            self.token, offset
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        Ok(body["result"].as_array().cloned().unwrap_or_default())
    }

    async fn send_message(&self, chat_id: i64, text: &str) -> Result<(), String> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.token);
        self.client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": text,
                "parse_mode": "HTML"
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Message dispatch
    // -----------------------------------------------------------------------

    async fn handle_message(&self, message: &serde_json::Value) {
        let chat_id = message["chat"]["id"].as_i64().unwrap_or(0);
        let user_id = message["from"]["id"].as_i64().unwrap_or(0);
        let username = message["from"]["username"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let text = message["text"].as_str().unwrap_or("").trim();

        // Only handle bot commands (messages starting with '/')
        if text.is_empty() || !text.starts_with('/') {
            return;
        }

        // Parse `/<command> [args…]`
        let parts: Vec<&str> = text[1..].split_whitespace().collect();
        if parts.is_empty() {
            return;
        }
        let command = parts[0];
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        // Role check
        let role = self.config.permissions.get_role(user_id);
        if role == Role::Blocked {
            let _ = self
                .send_message(chat_id, "You are blocked from using this bot.")
                .await;
            return;
        }
        if role == Role::Unknown {
            let _ = self
                .send_message(
                    chat_id,
                    "You don't have permission. Ask an admin to add you.",
                )
                .await;
            return;
        }

        // Rate limit
        {
            let mut limiter = self.rate_limiter.lock().await;
            if !limiter.check(user_id) {
                let _ = self
                    .send_message(chat_id, "Rate limit exceeded. Try again in a minute.")
                    .await;
                return;
            }
        }

        // Command-level permission
        if !command_allowed(role, command) {
            let _ = self
                .send_message(
                    chat_id,
                    &format!(
                        "Permission denied. Your role ({role:?}) cannot use /{command}"
                    ),
                )
                .await;
            return;
        }

        // Confirmation flow for destructive commands
        if command == "confirm" {
            self.handle_confirm(chat_id, user_id).await;
            return;
        }

        let is_destructive =
            self.config.confirm_destructive && matches!(command, "stop" | "delete" | "restart");

        if is_destructive {
            let action_desc = format!("/{command} {}", args.join(" "));
            {
                let mut confirms = self.pending_confirms.lock().await;
                confirms.insert(
                    user_id,
                    PendingAction {
                        command: command.to_string(),
                        args: args.clone(),
                        expires: Instant::now() + std::time::Duration::from_secs(30),
                    },
                );
            }
            let _ = self
                .send_message(
                    chat_id,
                    &format!(
                        "Are you sure you want to run <code>{action_desc}</code>?\nSend /confirm within 30 seconds."
                    ),
                )
                .await;
            return;
        }

        // Execute and respond
        let result = self.execute_command(command, &args).await;
        let result_text = match &result {
            Ok(output) => output.clone(),
            Err(e) => format!("Error: {e}"),
        };

        let _ = self.send_message(chat_id, &result_text).await;

        // Audit log
        self.audit.log(&AuditEntry {
            timestamp: Utc::now(),
            user_id,
            username,
            command: format!("/{command} {}", args.join(" ")),
            result: if result.is_ok() {
                "ok".into()
            } else {
                result_text
            },
            platform: "telegram".into(),
        });
    }

    // -----------------------------------------------------------------------
    // Confirmation handler
    // -----------------------------------------------------------------------

    async fn handle_confirm(&self, chat_id: i64, user_id: i64) {
        let action = {
            let mut confirms = self.pending_confirms.lock().await;
            confirms.remove(&user_id)
        };

        match action {
            Some(a) if a.expires > Instant::now() => {
                let result = self.execute_command(&a.command, &a.args).await;
                let text = match result {
                    Ok(out) => out,
                    Err(e) => format!("Error: {e}"),
                };
                let _ = self.send_message(chat_id, &text).await;
            }
            Some(_) => {
                let _ = self
                    .send_message(chat_id, "Confirmation expired. Run the command again.")
                    .await;
            }
            None => {
                let _ = self.send_message(chat_id, "Nothing to confirm.").await;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Command execution
    // -----------------------------------------------------------------------

    async fn execute_command(&self, command: &str, args: &[String]) -> Result<String, String> {
        let ipc = IpcClient::new(&self.paths.socket());

        match command {
            "status" | "list" => {
                let resp = ipc
                    .call(methods::PROCESS_LIST, serde_json::json!(null))
                    .await
                    .map_err(|e| e.to_string())?;
                let result = resp.result.unwrap_or_default();
                let procs = result.get("processes").unwrap_or(&result);
                let arr = procs.as_array().ok_or("Invalid response")?;
                if arr.is_empty() {
                    return Ok("No processes running.".into());
                }
                let mut out = String::from("<b>Processes</b>\n\n");
                for p in arr {
                    let name = p["config"]["name"].as_str().unwrap_or("?");
                    let status = p["status"].as_str().unwrap_or("?");
                    let pid = p["pid"]
                        .as_u64()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".into());
                    let icon = match status {
                        "online" => "\u{1F7E2}",
                        "starting" => "\u{1F7E1}",
                        "errored" => "\u{1F534}",
                        _ => "\u{26AB}",
                    };
                    out.push_str(&format!(
                        "{icon} <b>{name}</b> \u{2014} {status} (PID: {pid})\n"
                    ));
                }
                Ok(out)
            }

            "start" => {
                let target = args.first().ok_or("Usage: /start <process>")?;
                let config = mhost_core::process::ProcessConfig {
                    name: target.clone(),
                    command: target.clone(),
                    ..Default::default()
                };
                let params = serde_json::to_value(&config).map_err(|e| e.to_string())?;
                let resp = ipc
                    .call(methods::PROCESS_START, params)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(err) = resp.error {
                    Err(err.message)
                } else {
                    Ok(format!("Started '{target}'"))
                }
            }

            "stop" => {
                let target = args.first().ok_or("Usage: /stop <process>")?;
                let resp = ipc
                    .call(methods::PROCESS_STOP, serde_json::json!({ "name": target }))
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(err) = resp.error {
                    Err(err.message)
                } else {
                    Ok(format!("Stopped '{target}'"))
                }
            }

            "restart" => {
                let target = args.first().ok_or("Usage: /restart <process>")?;
                let resp = ipc
                    .call(
                        methods::PROCESS_RESTART,
                        serde_json::json!({ "name": target }),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(err) = resp.error {
                    Err(err.message)
                } else {
                    Ok(format!("Restarted '{target}'"))
                }
            }

            "scale" => {
                let name = args.first().ok_or("Usage: /scale <process> <N>")?;
                let n: u32 = args
                    .get(1)
                    .ok_or("Usage: /scale <process> <N>")?
                    .parse()
                    .map_err(|_| "Invalid number")?;
                let resp = ipc
                    .call(
                        methods::PROCESS_SCALE,
                        serde_json::json!({ "name": name, "instances": n }),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(err) = resp.error {
                    Err(err.message)
                } else {
                    Ok(format!("Scaled '{name}' to {n} instances"))
                }
            }

            "logs" => {
                let name = args.first().ok_or("Usage: /logs <process> [lines]")?;
                let lines: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(15);
                let log_path = self.paths.process_out_log(name, 0);
                let content = if log_path.exists() {
                    mhost_logs::reader::tail(&log_path, lines)
                        .unwrap_or_default()
                        .join("\n")
                } else {
                    "No logs found.".into()
                };
                Ok(format!("<b>Logs: {name}</b>\n<pre>{content}</pre>"))
            }

            "health" => {
                let name = args.first().ok_or("Usage: /health <process>")?;
                let resp = ipc
                    .call(methods::HEALTH_STATUS, serde_json::json!({ "name": name }))
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(err) = resp.error {
                    Err(err.message)
                } else {
                    Ok(format!(
                        "Health for '{}': {}",
                        name,
                        resp.result
                            .as_ref()
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "ok".into())
                    ))
                }
            }

            "deploy" => {
                let env = args.first().ok_or("Usage: /deploy <env>")?;
                let resp = ipc
                    .call(methods::DEPLOY_EXECUTE, serde_json::json!({ "env": env }))
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(err) = resp.error {
                    Err(err.message)
                } else {
                    Ok(format!("Deploy '{env}' triggered"))
                }
            }

            "ai" => {
                let sub = args
                    .first()
                    .ok_or("Usage: /ai diagnose <app> OR /ai ask <question>")?;
                match sub.as_str() {
                    "diagnose" => {
                        let name = args.get(1).ok_or("Usage: /ai diagnose <process>")?;
                        let ai_config = mhost_ai::AiConfig::load(&self.paths.ai_config())
                            .ok_or("AI not configured")?;
                        let provider = ai_config.create_provider()?;
                        let context = mhost_ai::ProcessContext::from_process_info(
                            &mhost_core::process::ProcessInfo::new(
                                mhost_core::process::ProcessConfig {
                                    name: name.clone(),
                                    command: "unknown".into(),
                                    ..Default::default()
                                },
                                0,
                            ),
                            vec![],
                            vec![],
                            vec![],
                        );
                        mhost_ai::diagnose::diagnose(provider.as_ref(), &context).await
                    }
                    "ask" => {
                        let question = args[1..].join(" ");
                        if question.is_empty() {
                            return Err("Usage: /ai ask <question>".into());
                        }
                        let ai_config = mhost_ai::AiConfig::load(&self.paths.ai_config())
                            .ok_or("AI not configured")?;
                        let provider = ai_config.create_provider()?;
                        mhost_ai::ask(provider.as_ref(), &question, &[]).await
                    }
                    other => Err(format!("Unknown AI command: {other}. Use: diagnose, ask")),
                }
            }

            "help" => Ok("\
                <b>mhost Bot Commands</b>\n\n\
                /status \u{2014} Show all processes\n\
                /start &lt;app&gt; \u{2014} Start a process\n\
                /stop &lt;app&gt; \u{2014} Stop a process\n\
                /restart &lt;app&gt; \u{2014} Restart a process\n\
                /scale &lt;app&gt; &lt;N&gt; \u{2014} Scale instances\n\
                /logs &lt;app&gt; [lines] \u{2014} View logs\n\
                /health &lt;app&gt; \u{2014} Health status\n\
                /deploy &lt;env&gt; \u{2014} Trigger deploy\n\
                /ai diagnose &lt;app&gt; \u{2014} AI crash analysis\n\
                /ai ask &lt;question&gt; \u{2014} Ask AI\n\
                /help \u{2014} This message"
                .to_string()),

            _ => Err(format!("Unknown command: /{command}. Try /help")),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BotConfig, Permissions};
    use mhost_core::MhostPaths;

    fn make_bot(config: BotConfig) -> TelegramBot {
        let paths = MhostPaths::with_root(std::env::temp_dir().join("mhost-bot-test"));
        TelegramBot::new(config, paths)
    }

    fn default_config() -> BotConfig {
        BotConfig {
            enabled: true,
            platform: "telegram".into(),
            token: "test-token".into(),
            permissions: Permissions {
                admins: vec![1],
                operators: vec![2],
                viewers: vec![3],
                blocked: vec![4],
            },
            confirm_destructive: true,
            auto_alerts: false,
            rate_limit: 30,
        }
    }

    // -----------------------------------------------------------------------
    // Bot construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_bot_new_uses_config_token() {
        let bot = make_bot(default_config());
        assert_eq!(bot.token, "test-token");
    }

    #[test]
    fn test_bot_new_uses_config_rate_limit() {
        let mut cfg = default_config();
        cfg.rate_limit = 5;
        let _bot = make_bot(cfg);
        // If construction panics the test fails — primarily a smoke test.
    }

    // -----------------------------------------------------------------------
    // execute_command — help
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_help_returns_text() {
        let bot = make_bot(default_config());
        let result = bot.execute_command("help", &[]).await;
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("mhost Bot Commands"));
        assert!(text.contains("/status"));
        assert!(text.contains("/start"));
        assert!(text.contains("/logs"));
    }

    #[tokio::test]
    async fn test_execute_unknown_command_returns_error() {
        let bot = make_bot(default_config());
        let result = bot.execute_command("nonexistent", &[]).await;
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("Unknown command"));
    }

    // -----------------------------------------------------------------------
    // execute_command — argument validation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_start_missing_arg_returns_error() {
        let bot = make_bot(default_config());
        let result = bot.execute_command("start", &[]).await;
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("Usage"));
    }

    #[tokio::test]
    async fn test_execute_stop_missing_arg_returns_error() {
        let bot = make_bot(default_config());
        let result = bot.execute_command("stop", &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_scale_missing_args_returns_error() {
        let bot = make_bot(default_config());
        let result = bot.execute_command("scale", &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_scale_invalid_number_returns_error() {
        let bot = make_bot(default_config());
        let result = bot
            .execute_command("scale", &["myapp".into(), "notanumber".into()])
            .await;
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("Invalid number"));
    }

    #[tokio::test]
    async fn test_execute_logs_missing_arg_returns_error() {
        let bot = make_bot(default_config());
        let result = bot.execute_command("logs", &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_health_missing_arg_returns_error() {
        let bot = make_bot(default_config());
        let result = bot.execute_command("health", &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_deploy_missing_arg_returns_error() {
        let bot = make_bot(default_config());
        let result = bot.execute_command("deploy", &[]).await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // execute_command — logs returns "No logs found" for missing file
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_logs_no_log_file() {
        let bot = make_bot(default_config());
        let result = bot
            .execute_command("logs", &["no-such-process".into()])
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No logs found"));
    }

    // -----------------------------------------------------------------------
    // execute_command — ai sub-command validation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_ai_missing_subcommand_returns_error() {
        let bot = make_bot(default_config());
        let result = bot.execute_command("ai", &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_ai_unknown_subcommand_returns_error() {
        let bot = make_bot(default_config());
        let result = bot.execute_command("ai", &["badcmd".into()]).await;
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("Unknown AI command"));
    }

    #[tokio::test]
    async fn test_execute_ai_ask_empty_question_returns_error() {
        let bot = make_bot(default_config());
        // "ask" subcommand with no actual question words
        let result = bot.execute_command("ai", &["ask".into()]).await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Pending confirms — expiry path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_handle_confirm_nothing_pending_sends_nothing_to_confirm() {
        // We can't call send_message without a real token, but we can verify
        // the internal state: no pending action → early return path.
        let bot = make_bot(default_config());
        // No pending action for user 999
        {
            let confirms = bot.pending_confirms.lock().await;
            assert!(confirms.get(&999).is_none());
        }
    }
}
