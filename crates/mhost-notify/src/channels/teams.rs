use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::error;

use crate::channel::NotifyChannel;
use crate::event::{NotifyEvent, Severity};

/// Microsoft Teams incoming webhook notification channel using Adaptive Cards.
pub struct TeamsChannel {
    pub webhook_url: String,
    pub name: String,
    client: reqwest::Client,
}

impl TeamsChannel {
    pub fn new(name: impl Into<String>, webhook_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            webhook_url: webhook_url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Build a Microsoft Teams Adaptive Card payload.
    pub fn build_payload(event: &NotifyEvent) -> Value {
        let severity_color = match event.severity {
            Severity::Critical => "attention",
            Severity::Warning => "warning",
            Severity::Info => "good",
        };

        let timestamp = event.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string();
        let title = format!("{} — {}", event.event_type, event.process_name);

        json!({
            "type": "message",
            "attachments": [
                {
                    "contentType": "application/vnd.microsoft.card.adaptive",
                    "content": {
                        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                        "type": "AdaptiveCard",
                        "version": "1.4",
                        "body": [
                            {
                                "type": "TextBlock",
                                "text": title,
                                "weight": "Bolder",
                                "size": "Medium",
                                "color": severity_color
                            },
                            {
                                "type": "FactSet",
                                "facts": [
                                    {
                                        "title": "Severity",
                                        "value": event.severity.to_string()
                                    },
                                    {
                                        "title": "Process",
                                        "value": event.process_name
                                    },
                                    {
                                        "title": "Message",
                                        "value": event.message
                                    },
                                    {
                                        "title": "Time",
                                        "value": timestamp
                                    }
                                ]
                            }
                        ]
                    }
                }
            ]
        })
    }
}

#[async_trait]
impl NotifyChannel for TeamsChannel {
    async fn send(&self, event: &NotifyEvent) -> Result<(), String> {
        let payload = Self::build_payload(event);

        let response = self
            .client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Teams request failed: {e}"))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(channel = %self.name, %status, %body, "Teams send failed");
            Err(format!("Teams webhook error {status}: {body}"))
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
    fn test_critical_payload_has_attention_color() {
        let event = NotifyEvent::new(EventType::Crash, "api", "msg", Severity::Critical);
        let payload = TeamsChannel::build_payload(&event);
        let color = &payload["attachments"][0]["content"]["body"][0]["color"];
        assert_eq!(color, "attention");
    }

    #[test]
    fn test_warning_payload_has_warning_color() {
        let event = NotifyEvent::new(EventType::HealthFail, "api", "msg", Severity::Warning);
        let payload = TeamsChannel::build_payload(&event);
        let color = &payload["attachments"][0]["content"]["body"][0]["color"];
        assert_eq!(color, "warning");
    }

    #[test]
    fn test_info_payload_has_good_color() {
        let event = NotifyEvent::new(EventType::Deploy, "api", "msg", Severity::Info);
        let payload = TeamsChannel::build_payload(&event);
        let color = &payload["attachments"][0]["content"]["body"][0]["color"];
        assert_eq!(color, "good");
    }

    #[test]
    fn test_payload_contains_process_name_in_facts() {
        let event = NotifyEvent::new(EventType::Restart, "my-svc", "restarting", Severity::Info);
        let payload = TeamsChannel::build_payload(&event);
        let text = payload.to_string();
        assert!(text.contains("my-svc"));
    }
}
