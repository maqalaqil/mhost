use async_trait::async_trait;
use glob::Pattern;
use serde_json::{json, Value};

use crate::parser::{LogEntry, LogLevel};
use crate::sink::LogSink;

// ── Transport ────────────────────────────────────────────────────────────────

/// Network transport used by GELF.
#[derive(Debug, Clone)]
pub enum GelfTransport {
    Udp,
    Tcp,
}

// ── Sink ─────────────────────────────────────────────────────────────────────

/// Forwards log entries in GELF 1.1 format to a Graylog-compatible endpoint.
pub struct GelfSink {
    pub host: String,
    pub port: u16,
    pub transport: GelfTransport,
    /// Glob pattern matched against `entry.process_name`.
    pub process_filter: String,
}

impl GelfSink {
    /// Create a new `GelfSink`.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        transport: GelfTransport,
        process_filter: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            transport,
            process_filter: process_filter.into(),
        }
    }

    /// Build a GELF 1.1 JSON payload from a `LogEntry`.
    ///
    /// Spec: <https://go2docs.graylog.org/current/getting_in_log_data/gelf.htm>
    pub fn build_payload(entry: &LogEntry) -> Value {
        let level = gelf_severity(entry.level.as_ref());
        let timestamp_secs = entry.timestamp.timestamp() as f64
            + (entry.timestamp.timestamp_subsec_millis() as f64 / 1000.0);

        let mut payload = json!({
            "version": "1.1",
            "host": &entry.process_name,
            "short_message": &entry.message,
            "timestamp": timestamp_secs,
            "level": level,
            "_process": &entry.process_name,
            "_instance": entry.instance,
        });

        // Extra structured fields — prefix with underscore per GELF spec.
        if let Some(obj) = payload.as_object_mut() {
            for (k, v) in &entry.fields {
                let key = format!("_{k}");
                obj.insert(key, v.clone());
            }
        }

        payload
    }
}

/// Map `LogLevel` to syslog severity numbers (GELF uses the same scale).
fn gelf_severity(level: Option<&LogLevel>) -> u8 {
    match level {
        Some(LogLevel::Fatal) => 0,
        Some(LogLevel::Error) => 3,
        Some(LogLevel::Warn) => 4,
        Some(LogLevel::Info) => 6,
        Some(LogLevel::Debug) => 7,
        Some(LogLevel::Trace) => 7,
        None => 6, // default to informational
    }
}

#[async_trait]
impl LogSink for GelfSink {
    async fn send(&self, entry: &LogEntry) -> Result<(), String> {
        use std::net::UdpSocket;
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpStream;

        let payload = serde_json::to_vec(&Self::build_payload(entry)).map_err(|e| e.to_string())?;

        let addr = format!("{}:{}", self.host, self.port);

        match self.transport {
            GelfTransport::Udp => {
                let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
                socket.send_to(&payload, &addr).map_err(|e| e.to_string())?;
            }
            GelfTransport::Tcp => {
                let mut stream = TcpStream::connect(&addr).await.map_err(|e| e.to_string())?;
                // GELF/TCP frames are null-byte terminated.
                let mut framed = payload.clone();
                framed.push(0u8);
                stream.write_all(&framed).await.map_err(|e| e.to_string())?;
            }
        }

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

    fn make_sink(filter: &str) -> GelfSink {
        GelfSink::new("127.0.0.1", 12201, GelfTransport::Udp, filter)
    }

    #[test]
    fn gelf_payload_version_and_fields() {
        let raw = r#"{"level":"error","message":"oh no","timestamp":"2024-01-15T10:00:00Z","request_id":"xyz"}"#;
        let entry = parse_line(raw, "api-service", 1);
        let payload = GelfSink::build_payload(&entry);

        assert_eq!(payload["version"], "1.1");
        assert_eq!(payload["short_message"], "oh no");
        assert_eq!(payload["host"], "api-service");
        assert_eq!(payload["level"], 3u64); // ERROR → 3
        assert_eq!(payload["_instance"], 1u64);
        assert_eq!(payload["_request_id"], "xyz");
    }

    #[test]
    fn gelf_payload_plain_text_defaults_to_info() {
        let entry = parse_line("hello world", "worker", 0);
        let payload = GelfSink::build_payload(&entry);

        assert_eq!(payload["version"], "1.1");
        assert_eq!(payload["short_message"], "hello world");
        assert_eq!(payload["level"], 6u64); // None → 6 (informational)
    }

    #[test]
    fn gelf_severity_levels() {
        assert_eq!(gelf_severity(Some(&LogLevel::Fatal)), 0);
        assert_eq!(gelf_severity(Some(&LogLevel::Error)), 3);
        assert_eq!(gelf_severity(Some(&LogLevel::Warn)), 4);
        assert_eq!(gelf_severity(Some(&LogLevel::Info)), 6);
        assert_eq!(gelf_severity(Some(&LogLevel::Debug)), 7);
        assert_eq!(gelf_severity(Some(&LogLevel::Trace)), 7);
        assert_eq!(gelf_severity(None), 6);
    }

    #[test]
    fn glob_matching_exact() {
        let sink = make_sink("api-service");
        assert!(sink.matches("api-service"));
        assert!(!sink.matches("worker"));
    }

    #[test]
    fn glob_matching_wildcard() {
        let sink = make_sink("api-*");
        assert!(sink.matches("api-service"));
        assert!(sink.matches("api-gateway"));
        assert!(!sink.matches("worker"));
    }

    #[test]
    fn glob_matching_question_mark() {
        let sink = make_sink("worker-?");
        assert!(sink.matches("worker-1"));
        assert!(sink.matches("worker-2"));
        assert!(!sink.matches("worker-10"));
        assert!(!sink.matches("api-service"));
    }

    #[test]
    fn glob_matching_star_star() {
        let sink = make_sink("*");
        assert!(sink.matches("anything"));
        assert!(sink.matches("api-service"));
    }
}
