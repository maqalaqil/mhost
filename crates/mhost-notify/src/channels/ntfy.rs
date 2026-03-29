use async_trait::async_trait;
use tracing::error;

use crate::channel::NotifyChannel;
use crate::event::{NotifyEvent, Severity};

/// Ntfy.sh push notification channel.
pub struct NtfyChannel {
    pub url: String,
    pub topic: String,
    pub name: String,
    client: reqwest::Client,
}

impl NtfyChannel {
    pub fn new(
        name: impl Into<String>,
        url: impl Into<String>,
        topic: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            topic: topic.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Map severity to ntfy priority (1=min, 5=max).
    pub fn priority(severity: &Severity) -> &'static str {
        match severity {
            Severity::Critical => "5",
            Severity::Warning => "3",
            Severity::Info => "1",
        }
    }

    /// Map event type to ntfy tags (emoji tags).
    pub fn tags(event: &NotifyEvent) -> String {
        use crate::event::EventType;
        let tag = match event.event_type {
            EventType::Crash => "rotating_light",
            EventType::Restart => "arrows_counterclockwise",
            EventType::Oom => "boom",
            EventType::Deploy => "rocket",
            EventType::HealthFail => "warning",
            EventType::Recovered => "white_check_mark",
        };
        tag.to_string()
    }

    /// Build the ntfy endpoint URL.
    pub fn endpoint_url(&self) -> String {
        format!("{}/{}", self.url.trim_end_matches('/'), self.topic)
    }
}

#[async_trait]
impl NotifyChannel for NtfyChannel {
    async fn send(&self, event: &NotifyEvent) -> Result<(), String> {
        let url = self.endpoint_url();
        let title = format!("[{}] {} — {}", event.severity, event.event_type, event.process_name);
        let priority = Self::priority(&event.severity);
        let tags = Self::tags(event);

        let response = self
            .client
            .post(&url)
            .header("Title", &title)
            .header("Priority", priority)
            .header("Tags", &tags)
            .body(event.message.clone())
            .send()
            .await
            .map_err(|e| format!("Ntfy request failed: {e}"))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(channel = %self.name, %status, %body, "Ntfy send failed");
            Err(format!("Ntfy error {status}: {body}"))
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

    fn make_channel() -> NtfyChannel {
        NtfyChannel::new("ntfy-test", "https://ntfy.sh", "my-topic")
    }

    #[test]
    fn test_endpoint_url_joins_url_and_topic() {
        let channel = make_channel();
        assert_eq!(channel.endpoint_url(), "https://ntfy.sh/my-topic");
    }

    #[test]
    fn test_endpoint_url_trailing_slash_removed() {
        let channel = NtfyChannel::new("n", "https://ntfy.sh/", "alerts");
        assert_eq!(channel.endpoint_url(), "https://ntfy.sh/alerts");
    }

    #[test]
    fn test_critical_priority_is_5() {
        assert_eq!(NtfyChannel::priority(&Severity::Critical), "5");
    }

    #[test]
    fn test_warning_priority_is_3() {
        assert_eq!(NtfyChannel::priority(&Severity::Warning), "3");
    }

    #[test]
    fn test_info_priority_is_1() {
        assert_eq!(NtfyChannel::priority(&Severity::Info), "1");
    }

    #[test]
    fn test_crash_tag_is_rotating_light() {
        let event = NotifyEvent::new(EventType::Crash, "svc", "msg", Severity::Critical);
        assert_eq!(NtfyChannel::tags(&event), "rotating_light");
    }

    #[test]
    fn test_recovered_tag_is_check_mark() {
        let event = NotifyEvent::new(EventType::Recovered, "svc", "msg", Severity::Info);
        assert_eq!(NtfyChannel::tags(&event), "white_check_mark");
    }
}
