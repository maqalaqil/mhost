use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::event_bus::ProcessEvent;

type HmacSha256 = Hmac<Sha256>;

/// Configuration for a single outbound webhook endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Unique identifier in the format "wh_XXXXXXXX", auto-generated.
    pub id: String,
    /// Target URL to POST events to.
    pub url: String,
    /// Event types to subscribe to, e.g. `["crash", "restart", "*"]`.
    pub events: Vec<String>,
    /// Optional HMAC-SHA256 signing secret.
    pub secret: Option<String>,
    /// Whether this webhook is active. Defaults to `true`.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Number of consecutive delivery failures. Defaults to `0`.
    #[serde(default)]
    pub failure_count: u32,
}

fn default_enabled() -> bool {
    true
}

/// Persistent file format: `{ "webhooks": [...] }`.
#[derive(Debug, Serialize, Deserialize)]
struct WebhookFile {
    webhooks: Vec<WebhookConfig>,
}

/// Outbound webhook dispatcher that manages webhook registrations and
/// delivers process events to registered endpoints.
pub struct WebhookDispatcher {
    webhooks: Mutex<Vec<WebhookConfig>>,
    config_path: PathBuf,
    failures_path: PathBuf,
    client: Client,
}

impl Default for WebhookDispatcher {
    fn default() -> Self {
        Self {
            webhooks: Mutex::new(Vec::new()),
            config_path: PathBuf::from("/dev/null"),
            failures_path: PathBuf::from("/dev/null"),
            client: Client::new(),
        }
    }
}

impl WebhookDispatcher {
    /// Loads webhook configuration from the given JSON file.
    /// If the file does not exist, starts with an empty list.
    pub fn load(config_path: PathBuf, failures_path: PathBuf) -> Result<Self, String> {
        let webhooks = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| format!("Failed to read webhook config: {e}"))?;
            let file: WebhookFile = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse webhook config: {e}"))?;
            file.webhooks
        } else {
            Vec::new()
        };

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

        Ok(Self {
            webhooks: Mutex::new(webhooks),
            config_path,
            failures_path,
            client,
        })
    }

    /// Persists the current webhook list to disk.
    fn save(&self) -> Result<(), String> {
        let hooks = self.webhooks.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let file = WebhookFile {
            webhooks: hooks.clone(),
        };
        let content = serde_json::to_string_pretty(&file)
            .map_err(|e| format!("Failed to serialize webhook config: {e}"))?;
        std::fs::write(&self.config_path, content)
            .map_err(|e| format!("Failed to write webhook config: {e}"))?;
        Ok(())
    }

    /// Adds a new webhook. Auto-generates the `id` if it is empty.
    /// Saves the updated list and returns the assigned id.
    pub fn add(&self, config: WebhookConfig) -> Result<String, String> {
        let id = if config.id.is_empty() {
            generate_webhook_id()
        } else {
            config.id.clone()
        };

        let webhook = WebhookConfig {
            id: id.clone(),
            url: config.url,
            events: config.events,
            secret: config.secret,
            enabled: config.enabled,
            failure_count: config.failure_count,
        };

        {
            let mut hooks = self.webhooks.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
            hooks.push(webhook);
        }

        self.save()?;
        info!(webhook_id = %id, "Webhook added");
        Ok(id)
    }

    /// Removes a webhook by id. Returns an error if the id is not found.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut hooks = self.webhooks.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let original_len = hooks.len();
        hooks.retain(|h| h.id != id);
        if hooks.len() == original_len {
            return Err(format!("Webhook not found: {id}"));
        }
        drop(hooks);
        self.save()?;
        info!(webhook_id = %id, "Webhook removed");
        Ok(())
    }

    /// Returns a cloned list of all registered webhooks.
    pub fn list(&self) -> Vec<WebhookConfig> {
        self.webhooks
            .lock()
            .map(|hooks| hooks.clone())
            .unwrap_or_default()
    }

    /// Dispatches an event to all enabled webhooks whose event filter
    /// matches. Each delivery is spawned as an independent async task.
    pub fn dispatch(&self, event: &ProcessEvent) {
        let hooks = match self.webhooks.lock() {
            Ok(h) => h.clone(),
            Err(e) => {
                error!("Failed to lock webhooks for dispatch: {e}");
                return;
            }
        };

        for hook in hooks {
            if !hook.enabled {
                continue;
            }
            if !event_matches(&hook.events, &event.event) {
                continue;
            }

            let client = self.client.clone();
            let event_clone = event.clone();
            let failures_path = self.failures_path.clone();

            tokio::spawn(async move {
                deliver_with_retry(&client, &hook, &event_clone, &failures_path).await;
            });
        }
    }

    /// Delivers an event to a specific webhook by id (useful for testing).
    pub fn dispatch_to(&self, id: &str, event: &ProcessEvent) -> Result<(), String> {
        let hooks = self.webhooks.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let hook = hooks
            .iter()
            .find(|h| h.id == id)
            .cloned()
            .ok_or_else(|| format!("Webhook not found: {id}"))?;
        drop(hooks);

        let client = self.client.clone();
        let event_clone = event.clone();
        let failures_path = self.failures_path.clone();

        tokio::spawn(async move {
            deliver_with_retry(&client, &hook, &event_clone, &failures_path).await;
        });

        Ok(())
    }
}

/// Checks whether a webhook's event filter matches the given event name.
fn event_matches(subscribed: &[String], event_name: &str) -> bool {
    subscribed.iter().any(|e| e == "*" || e == event_name)
}

/// Generates a webhook id in the format `wh_XXXXXXXX`.
fn generate_webhook_id() -> String {
    let short = &Uuid::new_v4().to_string()[..8];
    format!("wh_{short}")
}

/// Generates a delivery id in the format `del_XXXXXXXX`.
fn generate_delivery_id() -> String {
    let short = &Uuid::new_v4().to_string()[..8];
    format!("del_{short}")
}

/// Computes an HMAC-SHA256 signature of `body` using `secret`.
fn compute_hmac(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Attempts delivery up to 3 times with exponential backoff.
/// On final failure, appends an entry to the dead letter log.
async fn deliver_with_retry(
    client: &Client,
    hook: &WebhookConfig,
    event: &ProcessEvent,
    failures_path: &PathBuf,
) {
    let backoff = [5, 30, 120];

    for (attempt, delay_secs) in backoff.iter().enumerate() {
        match deliver_once(client, hook, event).await {
            Ok(()) => {
                info!(
                    webhook_id = %hook.id,
                    attempt = attempt + 1,
                    "Webhook delivery succeeded"
                );
                return;
            }
            Err(e) => {
                warn!(
                    webhook_id = %hook.id,
                    attempt = attempt + 1,
                    error = %e,
                    "Webhook delivery failed"
                );
                if attempt < backoff.len() - 1 {
                    tokio::time::sleep(std::time::Duration::from_secs(*delay_secs)).await;
                }
            }
        }
    }

    // All attempts exhausted — write to dead letter log.
    let dead_letter = serde_json::json!({
        "timestamp": Utc::now().to_rfc3339(),
        "webhook_id": hook.id,
        "url": hook.url,
        "event": event.event,
        "process": event.process,
    });

    if let Ok(line) = serde_json::to_string(&dead_letter) {
        use std::io::Write;
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(failures_path)
        {
            Ok(mut f) => {
                if let Err(e) = writeln!(f, "{line}") {
                    error!(
                        webhook_id = %hook.id,
                        error = %e,
                        "Failed to write dead letter entry"
                    );
                }
            }
            Err(e) => {
                error!(
                    webhook_id = %hook.id,
                    error = %e,
                    "Failed to open dead letter log"
                );
            }
        }
    }
}

/// Performs a single delivery attempt to the webhook endpoint.
async fn deliver_once(
    client: &Client,
    hook: &WebhookConfig,
    event: &ProcessEvent,
) -> Result<(), String> {
    let delivery_id = generate_delivery_id();
    let timestamp = Utc::now().to_rfc3339();

    let body = serde_json::json!({
        "id": delivery_id,
        "event": event.event,
        "process": event.process,
        "timestamp": timestamp,
        "data": event.detail,
    });

    let body_bytes = serde_json::to_vec(&body)
        .map_err(|e| format!("Failed to serialize event body: {e}"))?;

    let mut request = client
        .post(&hook.url)
        .header("Content-Type", "application/json")
        .header("X-Mhost-Event", &event.event)
        .header("X-Mhost-Delivery", &delivery_id)
        .header("X-Mhost-Timestamp", &timestamp)
        .timeout(std::time::Duration::from_secs(10));

    if let Some(ref secret) = hook.secret {
        let signature = compute_hmac(secret, &body_bytes);
        request = request.header(
            "X-Mhost-Signature",
            format!("sha256={signature}"),
        );
    }

    let response = request
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("Non-2xx response: {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hmac() {
        let sig1 = compute_hmac("my-secret", b"hello world");
        let sig2 = compute_hmac("my-secret", b"hello world");
        let sig3 = compute_hmac("my-secret", b"different payload");
        let sig4 = compute_hmac("other-secret", b"hello world");

        // Deterministic: same inputs produce same output.
        assert_eq!(sig1, sig2);

        // 64-character hex string (32 bytes).
        assert_eq!(sig1.len(), 64);
        assert!(sig1.chars().all(|c| c.is_ascii_hexdigit()));

        // Different inputs produce different hashes.
        assert_ne!(sig1, sig3);
        assert_ne!(sig1, sig4);
    }

    #[test]
    fn test_webhook_add_remove() {
        let dispatcher = WebhookDispatcher::default();

        let config = WebhookConfig {
            id: String::new(),
            url: "https://example.com/hook".into(),
            events: vec!["crash".into()],
            secret: None,
            enabled: true,
            failure_count: 0,
        };

        let id = dispatcher.add(config).expect("add should succeed");

        // ID starts with "wh_".
        assert!(id.starts_with("wh_"), "id should start with wh_, got: {id}");

        // List contains exactly one webhook.
        let list = dispatcher.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);

        // Remove it.
        dispatcher.remove(&id).expect("remove should succeed");

        // List is now empty.
        let list = dispatcher.list();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_remove_nonexistent() {
        let dispatcher = WebhookDispatcher::default();
        let result = dispatcher.remove("wh_nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
