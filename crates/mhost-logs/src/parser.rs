use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

/// Severity level of a log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    /// Parse a level string, case-insensitively.
    /// Accepts: TRACE/trace, DEBUG/debug, INFO/info, WARN/warn/WARNING/warning,
    /// ERROR/error/ERR/err, FATAL/fatal/CRIT/critical.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "TRACE" => Some(Self::Trace),
            "DEBUG" | "DBG" => Some(Self::Debug),
            "INFO" | "INFORMATION" => Some(Self::Info),
            "WARN" | "WARNING" => Some(Self::Warn),
            "ERROR" | "ERR" => Some(Self::Error),
            "FATAL" | "CRIT" | "CRITICAL" => Some(Self::Fatal),
            _ => None,
        }
    }

    /// Return the canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single structured log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: Option<LogLevel>,
    pub message: String,
    pub process_name: String,
    pub instance: u32,
    /// Extra key-value fields extracted from JSON (excludes level/message/timestamp).
    pub fields: HashMap<String, Value>,
    /// The original raw line before parsing.
    pub raw_line: String,
}

/// Try to extract a timestamp from a JSON value at the given key.
fn extract_timestamp(obj: &serde_json::Map<String, Value>, key: &str) -> Option<DateTime<Utc>> {
    let val = obj.get(key)?;
    match val {
        Value::String(s) => s.parse::<DateTime<Utc>>().ok(),
        Value::Number(n) => {
            // Unix epoch seconds (float or int)
            let secs = n.as_f64()?;
            #[allow(clippy::cast_possible_truncation)]
            let secs_i64 = secs as i64;
            DateTime::from_timestamp(secs_i64, 0)
        }
        _ => None,
    }
}

/// Parse a single raw log line into a `LogEntry`.
///
/// If the line is a JSON object we extract well-known keys; otherwise the
/// entire line becomes the message and level/fields are empty.
pub fn parse_line(raw: &str, process_name: &str, instance: u32) -> LogEntry {
    // Attempt JSON parse first.
    if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(raw) {
        let level = obj
            .get("level")
            .or_else(|| obj.get("severity"))
            .and_then(|v| v.as_str())
            .and_then(LogLevel::from_str);

        let message = obj
            .get("message")
            .or_else(|| obj.get("msg"))
            .and_then(|v| v.as_str())
            .unwrap_or(raw)
            .to_owned();

        let timestamp = extract_timestamp(&obj, "timestamp")
            .or_else(|| extract_timestamp(&obj, "time"))
            .unwrap_or_else(Utc::now);

        // All remaining keys become fields (skip the ones we already consumed).
        let well_known = ["level", "severity", "message", "msg", "timestamp", "time"];
        let fields = obj
            .into_iter()
            .filter(|(k, _)| !well_known.contains(&k.as_str()))
            .collect();

        return LogEntry {
            timestamp,
            level,
            message,
            process_name: process_name.to_owned(),
            instance,
            fields,
            raw_line: raw.to_owned(),
        };
    }

    // Plain-text fallback.
    LogEntry {
        timestamp: Utc::now(),
        level: None,
        message: raw.to_owned(),
        process_name: process_name.to_owned(),
        instance,
        fields: HashMap::new(),
        raw_line: raw.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_text() {
        let entry = parse_line("hello world", "myapp", 0);
        assert_eq!(entry.message, "hello world");
        assert_eq!(entry.raw_line, "hello world");
        assert!(entry.level.is_none());
        assert!(entry.fields.is_empty());
        assert_eq!(entry.process_name, "myapp");
        assert_eq!(entry.instance, 0);
    }

    #[test]
    fn parse_json_line() {
        let raw = r#"{"level":"error","message":"something went wrong","timestamp":"2024-01-15T10:00:00Z"}"#;
        let entry = parse_line(raw, "svc", 1);
        assert_eq!(entry.level, Some(LogLevel::Error));
        assert_eq!(entry.message, "something went wrong");
        assert_eq!(entry.process_name, "svc");
        assert_eq!(entry.instance, 1);
        assert_eq!(raw, entry.raw_line);
    }

    #[test]
    fn parse_json_with_nested_fields() {
        let raw = r#"{"level":"info","msg":"started","request_id":"abc123","meta":{"user":42}}"#;
        let entry = parse_line(raw, "api", 2);
        assert_eq!(entry.level, Some(LogLevel::Info));
        assert_eq!(entry.message, "started");
        assert!(entry.fields.contains_key("request_id"));
        assert!(entry.fields.contains_key("meta"));
        assert_eq!(entry.fields["request_id"], Value::String("abc123".into()));
    }

    #[test]
    fn detect_log_level_variants() {
        assert_eq!(LogLevel::from_str("ERROR"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("error"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("ERR"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("warn"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("WARNING"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("TRACE"), Some(LogLevel::Trace));
        assert_eq!(LogLevel::from_str("DEBUG"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("INFO"), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_str("FATAL"), Some(LogLevel::Fatal));
        assert_eq!(LogLevel::from_str("CRITICAL"), Some(LogLevel::Fatal));
        assert_eq!(LogLevel::from_str("unknown"), None);
    }
}
