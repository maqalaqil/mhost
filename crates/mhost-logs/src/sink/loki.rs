use std::collections::HashMap;

use async_trait::async_trait;
use glob::Pattern;
use reqwest::Client;
use serde_json::json;

use crate::parser::LogEntry;
use crate::sink::LogSink;

/// Forwards log entries to a Grafana Loki push endpoint.
pub struct LokiSink {
    pub url: String,
    /// Glob pattern matched against `entry.process_name`.
    pub process_filter: String,
    /// Additional stream labels merged with the default ones.
    pub extra_labels: HashMap<String, String>,
    client: Client,
}

impl LokiSink {
    /// Create a new `LokiSink`.
    pub fn new(
        url: impl Into<String>,
        process_filter: impl Into<String>,
        extra_labels: HashMap<String, String>,
    ) -> Self {
        Self {
            url: url.into(),
            process_filter: process_filter.into(),
            extra_labels,
            client: Client::new(),
        }
    }

    /// Build the Loki push payload for a single entry.
    ///
    /// Format: `{"streams":[{"stream":{labels},"values":[["timestamp_ns","line"]]}]}`
    pub fn build_payload(entry: &LogEntry, extra_labels: &HashMap<String, String>) -> serde_json::Value {
        let mut labels = HashMap::new();
        labels.insert("process".to_owned(), entry.process_name.clone());
        labels.insert(
            "level".to_owned(),
            entry
                .level
                .as_ref()
                .map(|l| l.as_str().to_lowercase())
                .unwrap_or_else(|| "unknown".to_owned()),
        );

        for (k, v) in extra_labels {
            labels.insert(k.clone(), v.clone());
        }

        let timestamp_ns = entry.timestamp.timestamp_nanos_opt().unwrap_or(0).to_string();

        json!({
            "streams": [{
                "stream": labels,
                "values": [[timestamp_ns, entry.raw_line]]
            }]
        })
    }
}

#[async_trait]
impl LogSink for LokiSink {
    async fn send(&self, entry: &LogEntry) -> Result<(), String> {
        let push_url = format!("{}/loki/api/v1/push", self.url.trim_end_matches('/'));
        let payload = Self::build_payload(entry, &self.extra_labels);

        self.client
            .post(&push_url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Loki send error: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Loki HTTP error: {e}"))?;

        Ok(())
    }

    fn matches(&self, process: &str) -> bool {
        Pattern::new(&self.process_filter)
            .map(|p| p.matches(process))
            .unwrap_or(false)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_line;

    #[test]
    fn loki_payload_structure() {
        let entry = parse_line("hello loki", "my-service", 0);
        let extra: HashMap<String, String> = HashMap::new();
        let payload = LokiSink::build_payload(&entry, &extra);

        let streams = payload["streams"].as_array().expect("streams array");
        assert_eq!(streams.len(), 1);

        let stream = &streams[0];
        assert_eq!(stream["stream"]["process"], "my-service");
        assert_eq!(stream["stream"]["level"], "unknown");

        let values = stream["values"].as_array().expect("values array");
        assert_eq!(values.len(), 1);
        assert_eq!(values[0][1], "hello loki");
    }

    #[test]
    fn loki_payload_with_level() {
        let raw = r#"{"level":"error","message":"fail","timestamp":"2024-01-15T10:00:00Z"}"#;
        let entry = parse_line(raw, "api", 0);
        let extra: HashMap<String, String> = HashMap::new();
        let payload = LokiSink::build_payload(&entry, &extra);

        let stream_labels = &payload["streams"][0]["stream"];
        assert_eq!(stream_labels["level"], "error");
        assert_eq!(stream_labels["process"], "api");
    }

    #[test]
    fn loki_payload_extra_labels_merged() {
        let entry = parse_line("msg", "svc", 0);
        let mut extra = HashMap::new();
        extra.insert("env".to_owned(), "production".to_owned());
        extra.insert("region".to_owned(), "us-east-1".to_owned());
        let payload = LokiSink::build_payload(&entry, &extra);

        let labels = &payload["streams"][0]["stream"];
        assert_eq!(labels["env"], "production");
        assert_eq!(labels["region"], "us-east-1");
    }

    #[test]
    fn loki_payload_timestamp_is_nanoseconds_string() {
        let entry = parse_line("ts test", "svc", 0);
        let extra: HashMap<String, String> = HashMap::new();
        let payload = LokiSink::build_payload(&entry, &extra);

        let ts = payload["streams"][0]["values"][0][0]
            .as_str()
            .expect("timestamp string");
        // A valid nanosecond timestamp is a long integer string.
        assert!(ts.parse::<i64>().is_ok(), "timestamp is not an integer: {ts}");
    }

    #[test]
    fn glob_matching() {
        let sink = LokiSink::new("http://localhost:3100", "api-*", HashMap::new());
        assert!(sink.matches("api-service"));
        assert!(!sink.matches("worker"));
    }
}
