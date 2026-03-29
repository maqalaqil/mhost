use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::error;

use crate::channel::NotifyChannel;
use crate::event::{NotifyEvent, Severity};

/// Slack incoming webhook notification channel.
pub struct SlackChannel {
    pub webhook_url: String,
    pub name: String,
    client: reqwest::Client,
}

impl SlackChannel {
    pub fn new(name: impl Into<String>, webhook_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            webhook_url: webhook_url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Build a Slack Block Kit message body for the event.
    pub fn build_payload(event: &NotifyEvent) -> Value {
        let color = match event.severity {
            Severity::Critical => "#E01E5A",
            Severity::Warning => "#ECB22E",
            Severity::Info => "#2EB67D",
        };

        let header_text = format!(
            "{} {} — {}",
            severity_emoji(&event.severity),
            event.event_type,
            event.process_name,
        );

        let timestamp = event.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string();

        json!({
            "attachments": [
                {
                    "color": color,
                    "blocks": [
                        {
                            "type": "header",
                            "text": {
                                "type": "plain_text",
                                "text": header_text,
                                "emoji": true
                            }
                        },
                        {
                            "type": "section",
                            "fields": [
                                {
                                    "type": "mrkdwn",
                                    "text": format!("*Severity:*\n{}", event.severity)
                                },
                                {
                                    "type": "mrkdwn",
                                    "text": format!("*Process:*\n{}", event.process_name)
                                },
                                {
                                    "type": "mrkdwn",
                                    "text": format!("*Message:*\n{}", event.message)
                                },
                                {
                                    "type": "mrkdwn",
                                    "text": format!("*Time:*\n{}", timestamp)
                                }
                            ]
                        }
                    ]
                }
            ]
        })
    }
}

fn severity_emoji(severity: &Severity) -> &'static str {
    match severity {
        Severity::Critical => "🔴",
        Severity::Warning => "🟡",
        Severity::Info => "🟢",
    }
}

#[async_trait]
impl NotifyChannel for SlackChannel {
    async fn send(&self, event: &NotifyEvent) -> Result<(), String> {
        let payload = Self::build_payload(event);

        let response = self
            .client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Slack request failed: {e}"))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(channel = %self.name, %status, %body, "Slack send failed");
            Err(format!("Slack webhook error {status}: {body}"))
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

    fn make_event(severity: Severity) -> NotifyEvent {
        NotifyEvent::new(EventType::Crash, "api-service", "Crashed", severity)
    }

    #[test]
    fn test_critical_payload_has_red_color() {
        let event = make_event(Severity::Critical);
        let payload = SlackChannel::build_payload(&event);
        let color = &payload["attachments"][0]["color"];
        assert_eq!(color, "#E01E5A");
    }

    #[test]
    fn test_warning_payload_has_yellow_color() {
        let event = make_event(Severity::Warning);
        let payload = SlackChannel::build_payload(&event);
        let color = &payload["attachments"][0]["color"];
        assert_eq!(color, "#ECB22E");
    }

    #[test]
    fn test_info_payload_has_green_color() {
        let event = make_event(Severity::Info);
        let payload = SlackChannel::build_payload(&event);
        let color = &payload["attachments"][0]["color"];
        assert_eq!(color, "#2EB67D");
    }

    #[test]
    fn test_payload_contains_process_name() {
        let event = make_event(Severity::Info);
        let payload = SlackChannel::build_payload(&event);
        let text = payload.to_string();
        assert!(text.contains("api-service"));
    }

    #[test]
    fn test_payload_header_contains_event_type() {
        let event = make_event(Severity::Critical);
        let payload = SlackChannel::build_payload(&event);
        let header = &payload["attachments"][0]["blocks"][0]["text"]["text"];
        assert!(header.to_string().contains("Crash"));
    }
}
