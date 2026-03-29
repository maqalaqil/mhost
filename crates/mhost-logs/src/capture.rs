use std::io;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader as TokioBufReader};
use tokio::sync::{broadcast, Mutex};

use crate::indexer::LogIndexer;
use crate::parser::parse_line;
use crate::ring::RingBuffer;
use crate::sink::LogSink;
use crate::writer::LogWriter;

/// Broadcast log capture — reads lines from an async stream, writes to a
/// file and a ring buffer, broadcasts each line, optionally indexes it, and
/// fans out to any registered log sinks.
///
/// `LogIndexer` wraps a `rusqlite::Connection` which is `!Sync`.  We store it
/// behind `Arc<Mutex<…>>` so that `LogCapture` itself is `Send + Sync` and can
/// live inside an `Arc<RwLock<…>>` as the daemon requires.
pub struct LogCapture {
    broadcaster: broadcast::Sender<String>,
    indexer: Option<Arc<Mutex<LogIndexer>>>,
    sinks: Vec<Box<dyn LogSink>>,
}

impl LogCapture {
    /// Create a new `LogCapture` with the given broadcast channel buffer size
    /// and no indexer or sinks.
    pub fn new(buffer_size: usize) -> Self {
        let (broadcaster, _) = broadcast::channel(buffer_size);
        Self {
            broadcaster,
            indexer: None,
            sinks: Vec::new(),
        }
    }

    /// Attach a `LogIndexer` — each parsed entry will be inserted into it.
    ///
    /// The indexer is wrapped in `Arc<Mutex<…>>` to make `LogCapture: Sync`.
    pub fn with_indexer(mut self, indexer: LogIndexer) -> Self {
        self.indexer = Some(Arc::new(Mutex::new(indexer)));
        self
    }

    /// Register a log sink.
    pub fn add_sink(mut self, sink: Box<dyn LogSink>) -> Self {
        self.sinks.push(sink);
        self
    }

    /// Subscribe to the broadcast channel.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.broadcaster.subscribe()
    }

    /// Read lines from `reader`, writing each to `writer` and `ring`, then
    /// broadcasting, indexing (if an indexer is attached), and forwarding to
    /// matching sinks.
    ///
    /// Returns when the stream is exhausted or an I/O error occurs.
    ///
    /// `process_name` and `instance` are passed to `parse_line` to build
    /// structured `LogEntry` values for indexing and sinks.
    pub async fn capture_stream<R: AsyncRead + Unpin>(
        &self,
        reader: R,
        writer: &mut LogWriter,
        ring: &mut RingBuffer,
        process_name: &str,
        instance: u32,
    ) -> io::Result<()> {
        let mut lines = TokioBufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            // 1. Persist to rolling file and ring buffer.
            writer.write_line(&line)?;
            ring.push(line.clone());

            // 2. Broadcast to subscribers (ignore lagged receivers).
            let _ = self.broadcaster.send(line.clone());

            // 3. Parse into a structured entry for indexer / sinks.
            let entry = parse_line(&line, process_name, instance);

            // 4. Index entry if an indexer is attached; log but don't fail on
            //    indexing errors so a broken SQLite path never kills capture.
            if let Some(ref indexer) = self.indexer {
                let idx = indexer.lock().await;
                if let Err(e) = idx.index_entry(&entry) {
                    tracing::warn!(
                        process = process_name,
                        error = %e,
                        "failed to index log entry"
                    );
                }
            }

            // 5. Fan out to registered sinks that match this process.
            for sink in &self.sinks {
                if sink.matches(process_name) {
                    if let Err(e) = sink.send(&entry).await {
                        tracing::warn!(
                            process = process_name,
                            error = %e,
                            "log sink send failed"
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn make_capture(buffer: usize) -> (LogCapture, broadcast::Receiver<String>) {
        let cap = LogCapture::new(buffer);
        let rx = cap.subscribe();
        (cap, rx)
    }

    async fn run_capture(
        cap: &LogCapture,
        input: &[u8],
        process: &str,
        instance: u32,
    ) -> (LogWriter, RingBuffer) {
        let dir = tempfile::tempdir().expect("tempdir");
        let log_path = dir.path().join("capture.log");
        let mut writer = LogWriter::new(&log_path, 1024 * 1024, 3).expect("writer");
        let mut ring = RingBuffer::new(100);

        cap.capture_stream(input, &mut writer, &mut ring, process, instance)
            .await
            .expect("capture");

        (writer, ring)
    }

    // ── Basic broadcast + ring buffer ─────────────────────────────────────

    #[tokio::test]
    async fn test_log_capture_broadcast() {
        let (cap, mut rx) = make_capture(16);
        let input = b"line one\nline two\nline three\n" as &[u8];
        let (_, ring) = run_capture(&cap, input, "svc", 0).await;

        assert_eq!(ring.lines(), vec!["line one", "line two", "line three"]);

        assert_eq!(rx.recv().await.expect("recv 1"), "line one");
        assert_eq!(rx.recv().await.expect("recv 2"), "line two");
        assert_eq!(rx.recv().await.expect("recv 3"), "line three");
    }

    // ── Indexer integration ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_capture_with_indexer() {
        let indexer = LogIndexer::in_memory().expect("indexer");
        let cap = LogCapture::new(16).with_indexer(indexer);
        let rx = cap.subscribe();
        drop(rx); // not testing broadcast here

        let raw = r#"{"level":"error","message":"db failure","timestamp":"2024-01-15T10:00:00Z"}"#;
        let input = raw.as_bytes();
        run_capture(&cap, input, "api", 0).await;

        let results = cap
            .indexer
            .as_ref()
            .unwrap()
            .lock()
            .await
            .search("db", None, None, 10)
            .expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].message, "db failure");
        assert_eq!(results[0].process_name, "api");
    }

    // ── Sink fan-out ──────────────────────────────────────────────────────

    /// A simple in-memory sink that records entries it receives.
    struct CollectorSink {
        filter: String,
        collected: std::sync::Arc<tokio::sync::Mutex<Vec<String>>>,
    }

    impl CollectorSink {
        fn new(filter: &str) -> (Self, std::sync::Arc<tokio::sync::Mutex<Vec<String>>>) {
            let collected = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
            let sink = Self {
                filter: filter.to_owned(),
                collected: collected.clone(),
            };
            (sink, collected)
        }
    }

    #[async_trait::async_trait]
    impl LogSink for CollectorSink {
        async fn send(&self, entry: &crate::parser::LogEntry) -> Result<(), String> {
            self.collected.lock().await.push(entry.message.clone());
            Ok(())
        }

        fn matches(&self, process: &str) -> bool {
            glob::Pattern::new(&self.filter)
                .map(|p| p.matches(process))
                .unwrap_or(false)
        }
    }

    #[tokio::test]
    async fn test_capture_fans_out_to_matching_sink() {
        let (collector, collected) = CollectorSink::new("api-*");
        let cap = LogCapture::new(16).add_sink(Box::new(collector));

        let input = b"hello from api\n";
        run_capture(&cap, input, "api-service", 0).await;

        let msgs = collected.lock().await;
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], "hello from api");
    }

    #[tokio::test]
    async fn test_capture_skips_non_matching_sink() {
        let (collector, collected) = CollectorSink::new("worker-*");
        let cap = LogCapture::new(16).add_sink(Box::new(collector));

        let input = b"hello from api\n";
        run_capture(&cap, input, "api-service", 0).await;

        let msgs = collected.lock().await;
        assert!(msgs.is_empty(), "sink should not receive unmatched process");
    }

    #[tokio::test]
    async fn test_capture_multiple_sinks() {
        let (c1, col1) = CollectorSink::new("*");
        let (c2, col2) = CollectorSink::new("*");

        let cap = LogCapture::new(16)
            .add_sink(Box::new(c1))
            .add_sink(Box::new(c2));

        let input = b"broadcast\n";
        run_capture(&cap, input, "any-service", 0).await;

        assert_eq!(col1.lock().await.len(), 1);
        assert_eq!(col2.lock().await.len(), 1);
    }
}
