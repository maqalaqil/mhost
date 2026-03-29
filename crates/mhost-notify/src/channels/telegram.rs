use async_trait::async_trait;
use serde_json::json;
use tracing::error;

use crate::channel::NotifyChannel;
use crate::event::{NotifyEvent, Severity};

/// Telegram bot notification channel.
pub struct TelegramChannel {
    pub bot_token: String,
    pub chat_id: String,
    pub name: String,
    client: reqwest::Client,
}

impl TelegramChannel {
    pub fn new(
        name: impl Into<String>,
        bot_token: impl Into<String>,
        chat_id: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            bot_token: bot_token.into(),
            chat_id: chat_id.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Format the event as a MarkdownV2-escaped Telegram message.
    pub fn format_message(event: &NotifyEvent) -> String {
        let severity_emoji = match event.severity {
            Severity::Critical => "🔴",
            Severity::Warning => "🟡",
            Severity::Info => "🟢",
        };

        // MarkdownV2 requires escaping: _ * [ ] ( ) ~ ` > # + - = | { } . !
        let escaped_process = escape_markdownv2(&event.process_name);
        let escaped_message = escape_markdownv2(&event.message);
        let escaped_event = escape_markdownv2(&event.event_type.to_string());
        let escaped_severity = escape_markdownv2(&event.severity.to_string());
        let timestamp = event
            .timestamp
            .format("%Y\\-%-m\\-%-d %H:%M:%S UTC")
            .to_string();

        format!(
            "{severity_emoji} *{escaped_event}* \\| {escaped_process}\n\
            *Severity:* {escaped_severity}\n\
            *Message:* {escaped_message}\n\
            *Time:* {timestamp}"
        )
    }
}

/// Escape special characters for Telegram MarkdownV2.
fn escape_markdownv2(text: &str) -> String {
    let special = [
        '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    ];
    let mut result = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        if special.contains(&ch) {
            result.push('\\');
        }
        result.push(ch);
    }
    result
}

#[async_trait]
impl NotifyChannel for TelegramChannel {
    async fn send(&self, event: &NotifyEvent) -> Result<(), String> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);
        let text = Self::format_message(event);

        let body = json!({
            "chat_id": self.chat_id,
            "text": text,
            "parse_mode": "MarkdownV2"
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Telegram request failed: {e}"))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(channel = %self.name, %status, %body, "Telegram send failed");
            Err(format!("Telegram API error {status}: {body}"))
        }
    }

    fn channel_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventType, NotifyEvent, Severity};

    fn make_event(event_type: EventType, severity: Severity) -> NotifyEvent {
        NotifyEvent::new(event_type, "my-service", "Something happened", severity)
    }

    #[test]
    fn test_format_critical_message_contains_red_emoji() {
        let event = make_event(EventType::Crash, Severity::Critical);
        let msg = TelegramChannel::format_message(&event);
        assert!(msg.contains("🔴"));
        assert!(msg.contains("Crash"));
        assert!(msg.contains("my\\-service"));
        assert!(msg.contains("CRITICAL"));
    }

    #[test]
    fn test_format_warning_message_contains_yellow_emoji() {
        let event = make_event(EventType::HealthFail, Severity::Warning);
        let msg = TelegramChannel::format_message(&event);
        assert!(msg.contains("🟡"));
        assert!(msg.contains("WARNING"));
    }

    #[test]
    fn test_format_info_message_contains_green_emoji() {
        let event = make_event(EventType::Deploy, Severity::Info);
        let msg = TelegramChannel::format_message(&event);
        assert!(msg.contains("🟢"));
        assert!(msg.contains("INFO"));
    }

    #[test]
    fn test_escape_markdownv2_special_chars() {
        let escaped = escape_markdownv2("hello-world.test!");
        assert_eq!(escaped, "hello\\-world\\.test\\!");
    }

    #[test]
    fn test_format_message_contains_timestamp() {
        let event = make_event(EventType::Restart, Severity::Info);
        let msg = TelegramChannel::format_message(&event);
        assert!(msg.contains("UTC"));
        assert!(msg.contains("Time:"));
    }
}
