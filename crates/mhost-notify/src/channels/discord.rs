use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::error;

use crate::channel::NotifyChannel;
use crate::event::{NotifyEvent, Severity};

/// Discord webhook notification channel using embeds.
pub struct DiscordChannel {
    pub webhook_url: String,
    pub name: String,
    client: reqwest::Client,
}

impl DiscordChannel {
    pub fn new(name: impl Into<String>, webhook_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            webhook_url: webhook_url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Map severity to a Discord embed color (decimal RGB).
    pub fn embed_color(severity: &Severity) -> u32 {
        match severity {
            Severity::Critical => 0xE01E5A, // red
            Severity::Warning => 0xECB22E,  // yellow
            Severity::Info => 0x2EB67D,     // green
        }
    }

    /// Build the Discord webhook JSON payload.
    pub fn build_payload(event: &NotifyEvent) -> Value {
        let color = Self::embed_color(&event.severity);
        let title = format!("{} — {}", event.event_type, event.process_name);
        let timestamp = event.timestamp.to_rfc3339();

        json!({
            "embeds": [
                {
                    "title": title,
                    "description": event.message,
                    "color": color,
                    "timestamp": timestamp,
                    "fields": [
                        {
                            "name": "Severity",
                            "value": event.severity.to_string(),
                            "inline": true
                        },
                        {
                            "name": "Process",
                            "value": event.process_name,
                            "inline": true
                        }
                    ],
                    "footer": {
                        "text": "mhost process manager"
                    }
                }
            ]
        })
    }
}

#[async_trait]
impl NotifyChannel for DiscordChannel {
    async fn send(&self, event: &NotifyEvent) -> Result<(), String> {
        let payload = Self::build_payload(event);

        let response = self
            .client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Discord request failed: {e}"))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(channel = %self.name, %status, %body, "Discord send failed");
            Err(format!("Discord webhook error {status}: {body}"))
        }
    }

    fn channel_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventType, NotifyEvent};

    #[test]
    fn test_embed_color_critical_is_red() {
        assert_eq!(DiscordChannel::embed_color(&Severity::Critical), 0xE01E5A);
    }

    #[test]
    fn test_embed_color_warning_is_yellow() {
        assert_eq!(DiscordChannel::embed_color(&Severity::Warning), 0xECB22E);
    }

    #[test]
    fn test_embed_color_info_is_green() {
        assert_eq!(DiscordChannel::embed_color(&Severity::Info), 0x2EB67D);
    }

    #[test]
    fn test_payload_has_embed_with_correct_color() {
        let event = NotifyEvent::new(EventType::Crash, "service", "msg", Severity::Critical);
        let payload = DiscordChannel::build_payload(&event);
        assert_eq!(payload["embeds"][0]["color"], 0xE01E5A_u64);
    }

    #[test]
    fn test_payload_title_contains_event_and_process() {
        let event = NotifyEvent::new(EventType::Deploy, "api", "deployed", Severity::Info);
        let payload = DiscordChannel::build_payload(&event);
        let title = payload["embeds"][0]["title"].as_str().unwrap();
        assert!(title.contains("Deploy"));
        assert!(title.contains("api"));
    }

    #[test]
    fn test_payload_has_timestamp() {
        let event = NotifyEvent::new(EventType::Restart, "svc", "restarting", Severity::Warning);
        let payload = DiscordChannel::build_payload(&event);
        let ts = payload["embeds"][0]["timestamp"].as_str().unwrap();
        assert!(!ts.is_empty());
    }
}
