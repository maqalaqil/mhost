use async_trait::async_trait;
use glob::Pattern;

use crate::parser::{LogEntry, LogLevel};
use crate::sink::LogSink;

// ── Transport ────────────────────────────────────────────────────────────────

/// Network transport used to deliver syslog messages.
#[derive(Debug, Clone)]
pub enum SyslogTransport {
    Udp,
    Tcp,
}

// ── Sink ─────────────────────────────────────────────────────────────────────

/// Forwards log entries using the RFC 5424 syslog protocol.
pub struct SyslogSink {
    pub host: String,
    pub port: u16,
    pub transport: SyslogTransport,
    /// Syslog facility code (0–23).
    pub facility: u8,
    /// HOSTNAME field placed in the syslog header.
    pub hostname: String,
    /// Glob pattern matched against `entry.process_name`.
    pub process_filter: String,
}

impl SyslogSink {
    /// Create a new `SyslogSink`.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        transport: SyslogTransport,
        facility: u8,
        hostname: impl Into<String>,
        process_filter: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            transport,
            facility,
            hostname: hostname.into(),
            process_filter: process_filter.into(),
        }
    }

    /// Compute the RFC 5424 PRI value.
    ///
    /// `PRI = facility * 8 + severity`
    pub fn priority(facility: u8, level: Option<&LogLevel>) -> u8 {
        facility * 8 + syslog_severity(level)
    }

    /// Format a `LogEntry` as an RFC 5424 syslog message.
    ///
    /// ```text
    /// <PRI>VERSION TIMESTAMP HOSTNAME APP-NAME PROCID MSGID SD MSG
    /// ```
    pub fn format_message(&self, entry: &LogEntry) -> String {
        let pri = Self::priority(self.facility, entry.level.as_ref());
        let timestamp = entry.timestamp.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let app_name = &entry.process_name;
        let procid = entry.instance.to_string();

        // VERSION=1, MSGID="-" (nil), SD="-" (nil structured data)
        format!(
            "<{pri}>1 {timestamp} {hostname} {app_name} {procid} - - {msg}",
            pri = pri,
            timestamp = timestamp,
            hostname = self.hostname,
            app_name = app_name,
            procid = procid,
            msg = entry.message,
        )
    }
}

/// Map `LogLevel` to syslog severity (RFC 5424 Table 2).
fn syslog_severity(level: Option<&LogLevel>) -> u8 {
    match level {
        Some(LogLevel::Fatal) => 0, // Emergency
        Some(LogLevel::Error) => 3, // Error
        Some(LogLevel::Warn) => 4,  // Warning
        Some(LogLevel::Info) => 6,  // Informational
        Some(LogLevel::Debug) => 7, // Debug
        Some(LogLevel::Trace) => 7, // Debug
        None => 6,                  // Informational
    }
}

#[async_trait]
impl LogSink for SyslogSink {
    async fn send(&self, entry: &LogEntry) -> Result<(), String> {
        use std::net::UdpSocket;
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpStream;

        let message = self.format_message(entry);
        let addr = format!("{}:{}", self.host, self.port);

        match self.transport {
            SyslogTransport::Udp => {
                let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
                socket
                    .send_to(message.as_bytes(), &addr)
                    .map_err(|e| e.to_string())?;
            }
            SyslogTransport::Tcp => {
                let mut stream = TcpStream::connect(&addr)
                    .await
                    .map_err(|e| e.to_string())?;
                // RFC 6587 octet-counting framing: "<len> <msg>"
                let framed = format!("{} {message}", message.len() + 1);
                stream
                    .write_all(framed.as_bytes())
                    .await
                    .map_err(|e| e.to_string())?;
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

    fn make_sink(filter: &str) -> SyslogSink {
        SyslogSink::new(
            "127.0.0.1",
            514,
            SyslogTransport::Udp,
            1, // user-level messages
            "myhost",
            filter,
        )
    }

    // ── Priority calculation ──────────────────────────────────────────────

    #[test]
    fn priority_user_facility_error() {
        // facility=1 (user), severity=3 (error) → 1*8+3 = 11
        assert_eq!(SyslogSink::priority(1, Some(&LogLevel::Error)), 11);
    }

    #[test]
    fn priority_local0_info() {
        // facility=16 (local0), severity=6 (info) → 16*8+6 = 134
        assert_eq!(SyslogSink::priority(16, Some(&LogLevel::Info)), 134);
    }

    #[test]
    fn priority_none_level_defaults_to_informational() {
        // facility=0 (kernel), severity=6 (info) → 0*8+6 = 6
        assert_eq!(SyslogSink::priority(0, None), 6);
    }

    #[test]
    fn priority_fatal_maps_to_emergency() {
        // facility=1 (user), severity=0 (emergency) → 1*8+0 = 8
        assert_eq!(SyslogSink::priority(1, Some(&LogLevel::Fatal)), 8);
    }

    // ── RFC 5424 message format ───────────────────────────────────────────

    #[test]
    fn message_format_starts_with_pri_and_version() {
        let sink = make_sink("*");
        let raw = r#"{"level":"info","message":"service started","timestamp":"2024-01-15T10:00:00Z"}"#;
        let entry = parse_line(raw, "my-service", 3);
        let msg = sink.format_message(&entry);

        // <PRI>1 ...
        let pri = SyslogSink::priority(1, Some(&LogLevel::Info)); // 14
        assert!(msg.starts_with(&format!("<{pri}>1 ")));
    }

    #[test]
    fn message_format_contains_hostname_appname_procid() {
        let sink = make_sink("*");
        let entry = parse_line("boot", "myapp", 7);
        let msg = sink.format_message(&entry);

        assert!(msg.contains("myhost"), "hostname missing: {msg}");
        assert!(msg.contains("myapp"), "app-name missing: {msg}");
        assert!(msg.contains(" 7 "), "procid missing: {msg}");
    }

    #[test]
    fn message_format_ends_with_message_text() {
        let sink = make_sink("*");
        let entry = parse_line("the log message", "svc", 0);
        let msg = sink.format_message(&entry);

        assert!(msg.ends_with("the log message"), "message body missing: {msg}");
    }

    #[test]
    fn message_format_nil_fields() {
        let sink = make_sink("*");
        let entry = parse_line("plain", "svc", 0);
        let msg = sink.format_message(&entry);

        // MSGID and SD must be "-"
        // Format: <PRI>1 TIMESTAMP HOSTNAME APP PROCID - - MSG
        let parts: Vec<&str> = msg.splitn(8, ' ').collect();
        assert_eq!(parts.len(), 8);
        assert_eq!(parts[5], "-", "MSGID should be '-'");
        assert_eq!(parts[6], "-", "SD should be '-'");
    }

    // ── Glob matching ─────────────────────────────────────────────────────

    #[test]
    fn glob_matching() {
        let sink = make_sink("svc-*");
        assert!(sink.matches("svc-alpha"));
        assert!(!sink.matches("worker"));
    }
}
