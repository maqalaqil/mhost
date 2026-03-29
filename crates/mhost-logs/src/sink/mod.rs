pub mod elasticsearch;
pub mod gelf;
pub mod loki;
pub mod syslog;

use crate::parser::LogEntry;
use async_trait::async_trait;

/// Trait implemented by every log forwarding sink.
///
/// Sinks receive parsed `LogEntry` values and forward them to an external
/// destination (Graylog, Loki, Elasticsearch, syslog, …).
#[async_trait]
pub trait LogSink: Send + Sync {
    /// Forward a single log entry to the sink.
    async fn send(&self, entry: &LogEntry) -> Result<(), String>;

    /// Return `true` when this sink should receive entries from `process`.
    fn matches(&self, process: &str) -> bool;
}
