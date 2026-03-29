use std::collections::HashMap;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::error;

use crate::channel::NotifyChannel;
use crate::event::NotifyEvent;

type HmacSha256 = Hmac<Sha256>;

/// Generic outbound webhook channel with optional HMAC-SHA256 signing.
pub struct WebhookChannel {
    pub url: String,
    pub headers: HashMap<String, String>,
    pub hmac_secret: Option<String>,
    pub name: String,
    client: reqwest::Client,
}

impl WebhookChannel {
    pub fn new(
        name: impl Into<String>,
        url: impl Into<String>,
        headers: HashMap<String, String>,
        hmac_secret: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            headers,
            hmac_secret,
            client: reqwest::Client::new(),
        }
    }

    /// Compute HMAC-SHA256 hex digest for the given body bytes.
    pub fn compute_hmac(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(body);
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }
}

#[async_trait]
impl NotifyChannel for WebhookChannel {
    async fn send(&self, event: &NotifyEvent) -> Result<(), String> {
        let body = serde_json::to_vec(event).map_err(|e| format!("Serialization error: {e}"))?;

        let mut request = self.client.post(&self.url);

        // Apply custom headers
        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        // Apply HMAC signature if configured
        if let Some(secret) = &self.hmac_secret {
            let signature = Self::compute_hmac(secret, &body);
            request = request.header("X-Signature", format!("sha256={signature}"));
        }

        let response = request
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("Webhook request failed: {e}"))?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let resp_body = response.text().await.unwrap_or_default();
            error!(channel = %self.name, %status, %resp_body, "Webhook send failed");
            Err(format!("Webhook error {status}: {resp_body}"))
        }
    }

    fn channel_name(&self) -> &str {
        &self.name
    }
}

// Implement Serialize for NotifyEvent to enable JSON body serialization
use serde::ser::{Serialize, SerializeStruct, Serializer};

impl Serialize for NotifyEvent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("NotifyEvent", 6)?;
        state.serialize_field("event_type", &self.event_type.to_string())?;
        state.serialize_field("process_name", &self.process_name)?;
        state.serialize_field("message", &self.message)?;
        state.serialize_field("severity", &self.severity.to_string())?;
        state.serialize_field("timestamp", &self.timestamp.to_rfc3339())?;
        state.serialize_field("metadata", &self.metadata)?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventType, NotifyEvent, Severity};

    #[test]
    fn test_hmac_signature_is_deterministic() {
        let secret = "mysecret";
        let body = b"hello world";
        let sig1 = WebhookChannel::compute_hmac(secret, body);
        let sig2 = WebhookChannel::compute_hmac(secret, body);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_hmac_signature_changes_with_different_body() {
        let secret = "mysecret";
        let sig1 = WebhookChannel::compute_hmac(secret, b"body1");
        let sig2 = WebhookChannel::compute_hmac(secret, b"body2");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_hmac_signature_changes_with_different_secret() {
        let body = b"same body";
        let sig1 = WebhookChannel::compute_hmac("secret1", body);
        let sig2 = WebhookChannel::compute_hmac("secret2", body);
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_hmac_signature_is_hex_encoded() {
        let sig = WebhookChannel::compute_hmac("key", b"data");
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(sig.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn test_notify_event_serializes_to_json() {
        let event = NotifyEvent::new(
            EventType::Crash,
            "my-service",
            "crashed",
            Severity::Critical,
        );
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"event_type\":\"Crash\""));
        assert!(json.contains("\"process_name\":\"my-service\""));
        assert!(json.contains("\"severity\":\"CRITICAL\""));
    }
}
