use async_trait::async_trait;
use chrono::Utc;
use glob::Pattern;
use reqwest::Client;
use serde_json::json;

use crate::parser::LogEntry;
use crate::sink::LogSink;

/// Forwards log entries to an Elasticsearch cluster via the Bulk API.
pub struct ElasticsearchSink {
    pub url: String,
    /// Index name, optionally containing `{date}` which is expanded to
    /// `YYYY.MM.DD` at send time.
    pub index: String,
    /// Glob pattern matched against `entry.process_name`.
    pub process_filter: String,
    client: Client,
}

impl ElasticsearchSink {
    /// Create a new `ElasticsearchSink`.
    pub fn new(
        url: impl Into<String>,
        index: impl Into<String>,
        process_filter: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            index: index.into(),
            process_filter: process_filter.into(),
            client: Client::new(),
        }
    }

    /// Expand `{date}` in the index template to today's `YYYY.MM.DD`.
    pub fn resolve_index(&self) -> String {
        let today = Utc::now().format("%Y.%m.%d").to_string();
        self.index.replace("{date}", &today)
    }

    /// Resolve the index using the provided date string instead of today.
    /// Useful in tests where a deterministic date is needed.
    pub fn resolve_index_with_date(&self, date: &str) -> String {
        self.index.replace("{date}", date)
    }

    /// Build an NDJSON bulk request body (action line + document line).
    pub fn build_ndjson(entry: &LogEntry, resolved_index: &str) -> String {
        let action = json!({ "index": { "_index": resolved_index } });
        let doc = json!({
            "@timestamp": entry.timestamp.to_rfc3339(),
            "level": entry.level.as_ref().map(|l| l.as_str()),
            "message": entry.message,
            "process": entry.process_name,
            "instance": entry.instance,
            "fields": entry.fields,
            "raw_line": entry.raw_line,
        });

        format!("{}\n{}\n", action, doc)
    }
}

#[async_trait]
impl LogSink for ElasticsearchSink {
    async fn send(&self, entry: &LogEntry) -> Result<(), String> {
        let resolved = self.resolve_index();
        let bulk_url = format!(
            "{}/{}/_bulk",
            self.url.trim_end_matches('/'),
            resolved
        );
        let body = Self::build_ndjson(entry, &resolved);

        self.client
            .post(&bulk_url)
            .header("Content-Type", "application/x-ndjson")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("Elasticsearch send error: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Elasticsearch HTTP error: {e}"))?;

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

    fn make_sink(index: &str) -> ElasticsearchSink {
        ElasticsearchSink::new("http://localhost:9200", index, "*")
    }

    #[test]
    fn date_template_expansion() {
        let sink = make_sink("logs-{date}");
        let resolved = sink.resolve_index_with_date("2024.01.15");
        assert_eq!(resolved, "logs-2024.01.15");
    }

    #[test]
    fn no_date_template_unchanged() {
        let sink = make_sink("logs-static");
        let resolved = sink.resolve_index_with_date("2024.01.15");
        assert_eq!(resolved, "logs-static");
    }

    #[test]
    fn ndjson_format_two_lines() {
        let raw = r#"{"level":"info","message":"started","timestamp":"2024-01-15T10:00:00Z"}"#;
        let entry = parse_line(raw, "api", 0);
        let ndjson = ElasticsearchSink::build_ndjson(&entry, "logs-2024.01.15");

        let lines: Vec<&str> = ndjson.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 2, "NDJSON must have exactly 2 lines");

        // First line: bulk action
        let action: serde_json::Value =
            serde_json::from_str(lines[0]).expect("action is valid JSON");
        assert_eq!(action["index"]["_index"], "logs-2024.01.15");

        // Second line: document
        let doc: serde_json::Value =
            serde_json::from_str(lines[1]).expect("doc is valid JSON");
        assert_eq!(doc["message"], "started");
        assert_eq!(doc["level"], "INFO");
        assert_eq!(doc["process"], "api");
    }

    #[test]
    fn ndjson_plain_text_entry() {
        let entry = parse_line("plain text log", "worker", 2);
        let ndjson = ElasticsearchSink::build_ndjson(&entry, "logs-2024.01.15");

        let lines: Vec<&str> = ndjson.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 2);

        let doc: serde_json::Value =
            serde_json::from_str(lines[1]).expect("doc is valid JSON");
        assert_eq!(doc["message"], "plain text log");
        assert_eq!(doc["level"], serde_json::Value::Null);
        assert_eq!(doc["instance"], 2u64);
    }

    #[test]
    fn glob_matching() {
        let sink = ElasticsearchSink::new(
            "http://localhost:9200",
            "logs-{date}",
            "worker-*",
        );
        assert!(sink.matches("worker-1"));
        assert!(sink.matches("worker-main"));
        assert!(!sink.matches("api-service"));
    }
}
