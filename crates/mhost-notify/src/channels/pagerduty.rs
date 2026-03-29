use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::error;

use crate::channel::NotifyChannel;
use crate::event::{EventType, NotifyEvent, Severity};

/// PagerDuty Events API v2 channel.
pub struct PagerDutyChannel {
    pub routing_key: String,
    pub name: String,
    /// Override severity mapping: EventType string -> PD severity string
    pub severity_map: HashMap<String, String>,
    client: reqwest::Client,
}

impl PagerDutyChannel {
    pub fn new(
        name: impl Into<String>,
        routing_key: impl Into<String>,
        severity_map: HashMap<String, String>,
    ) -> Self {
        Self {
            name: name.into(),
            routing_key: routing_key.into(),
            severity_map,
            client: reqwest::Client::new(),
        }
    }

    /// Map mhost severity to PagerDuty severity string.
    pub fn map_severity(
        severity: &Severity,
        event_type: &EventType,
        overrides: &HashMap<String, String>,
    ) -> String {
        let key = event_type.to_string();
        if let Some(mapped) = overrides.get(&key) {
            return mapped.clone();
        }
        match severity {
            Severity::Critical => "critical".to_string(),
            Severity::Warning => "warning".to_string(),
            Severity::Info => "info".to_string(),
        }
    }

    /// Generate a stable dedup_key for a process + event combination.
    pub fn dedup_key(event: &NotifyEvent) -> String {
        format!("mhost-{}-{}", event.process_name, event.event_type)
    }

    /// Determine the PagerDuty action: trigger or resolve.
    pub fn pd_action(event: &NotifyEvent) -> &'static str {
        if event.event_type == EventType::Recovered {
            "resolve"
        } else {
            "trigger"
        }
    }

    /// Build the PagerDuty Events API v2 payload.
    pub fn build_payload(&self, event: &NotifyEvent) -> Value {
        let severity = Self::map_severity(&event.severity, &event.event_type, &self.severity_map);
        let action = Self::pd_action(event);
        let dedup_key = Self::dedup_key(event);
        let timestamp = event.timestamp.to_rfc3339();

        json!({
            "routing_key": self.routing_key,
            "event_action": action,
            "dedup_key": dedup_key,
            "payload": {
                "summary": format!("[{}] {} — {}", event.severity, event.event_type, event.process_name),
                "source": event.process_name,
                "severity": severity,
                "timestamp": timestamp,
                "custom_details": {
                    "message": event.message,
                    "event_type": event.event_type.to_string(),
                    "metadata": event.metadata,
                }
            }
        })
    }
}

#[async_trait]
impl NotifyChannel for PagerDutyChannel {
    async fn send(&self, event: &NotifyEvent) -> Result<(), String> {
        let payload = self.build_payload(event);

        let response = self
            .client
            .post("https://events.pagerduty.com/v2/enqueue")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("PagerDuty request failed: {e}"))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(channel = %self.name, %status, %body, "PagerDuty send failed");
            Err(format!("PagerDuty API error {status}: {body}"))
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

    fn make_channel() -> PagerDutyChannel {
        PagerDutyChannel::new("pd-test", "test-routing-key", HashMap::new())
    }

    #[test]
    fn test_recovered_event_uses_resolve_action() {
        let event = NotifyEvent::new(EventType::Recovered, "api", "back up", Severity::Info);
        assert_eq!(PagerDutyChannel::pd_action(&event), "resolve");
    }

    #[test]
    fn test_non_recovered_event_uses_trigger_action() {
        let event = NotifyEvent::new(EventType::Crash, "api", "down", Severity::Critical);
        assert_eq!(PagerDutyChannel::pd_action(&event), "trigger");
    }

    #[test]
    fn test_dedup_key_is_stable() {
        let event = NotifyEvent::new(EventType::Crash, "my-svc", "msg", Severity::Critical);
        let key1 = PagerDutyChannel::dedup_key(&event);
        let key2 = PagerDutyChannel::dedup_key(&event);
        assert_eq!(key1, key2);
        assert_eq!(key1, "mhost-my-svc-Crash");
    }

    #[test]
    fn test_severity_mapping_critical() {
        let overrides = HashMap::new();
        let result =
            PagerDutyChannel::map_severity(&Severity::Critical, &EventType::Crash, &overrides);
        assert_eq!(result, "critical");
    }

    #[test]
    fn test_severity_mapping_override() {
        let mut overrides = HashMap::new();
        overrides.insert("Crash".to_string(), "high".to_string());
        let result =
            PagerDutyChannel::map_severity(&Severity::Critical, &EventType::Crash, &overrides);
        assert_eq!(result, "high");
    }

    #[test]
    fn test_payload_structure_for_trigger() {
        let channel = make_channel();
        let event = NotifyEvent::new(EventType::Crash, "service", "crashed", Severity::Critical);
        let payload = channel.build_payload(&event);
        assert_eq!(payload["event_action"], "trigger");
        assert_eq!(payload["routing_key"], "test-routing-key");
        assert!(!payload["dedup_key"].as_str().unwrap().is_empty());
    }

    #[test]
    fn test_payload_structure_for_resolve() {
        let channel = make_channel();
        let event = NotifyEvent::new(EventType::Recovered, "service", "back up", Severity::Info);
        let payload = channel.build_payload(&event);
        assert_eq!(payload["event_action"], "resolve");
    }
}
