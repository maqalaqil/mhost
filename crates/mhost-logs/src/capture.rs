use std::io;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader as TokioBufReader};
use tokio::sync::broadcast;

use crate::ring::RingBuffer;
use crate::writer::LogWriter;

/// Broadcast log capture — reads lines from an async stream, writes to a
/// file and a ring buffer, and broadcasts each line to all subscribers.
pub struct LogCapture {
    broadcaster: broadcast::Sender<String>,
}

impl LogCapture {
    /// Create a new `LogCapture` with the given broadcast channel buffer size.
    pub fn new(buffer_size: usize) -> Self {
        let (broadcaster, _) = broadcast::channel(buffer_size);
        Self { broadcaster }
    }

    /// Subscribe to the broadcast channel.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.broadcaster.subscribe()
    }

    /// Read lines from `reader`, write each to `writer` and `ring`, then broadcast.
    /// Returns when the stream is exhausted or an I/O error occurs.
    pub async fn capture_stream<R: AsyncRead + Unpin>(
        &self,
        reader: R,
        writer: &mut LogWriter,
        ring: &mut RingBuffer,
    ) -> io::Result<()> {
        let mut lines = TokioBufReader::new(reader).lines();
        while let Some(line) = lines.next_line().await? {
            writer.write_line(&line)?;
            ring.push(line.clone());
            // Ignore send errors — no active subscribers is not a failure.
            let _ = self.broadcaster.send(line);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_capture_broadcast() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log_path = dir.path().join("capture.log");

        let capture = LogCapture::new(16);
        let mut rx = capture.subscribe();

        let mut writer = LogWriter::new(&log_path, 1024 * 1024, 3).expect("writer");
        let mut ring = RingBuffer::new(100);

        let input = b"line one\nline two\nline three\n" as &[u8];
        capture
            .capture_stream(input, &mut writer, &mut ring)
            .await
            .expect("capture");

        // Ring buffer should hold all three lines.
        assert_eq!(ring.lines(), vec!["line one", "line two", "line three"]);

        // All lines should have been broadcast.
        assert_eq!(rx.recv().await.expect("recv 1"), "line one");
        assert_eq!(rx.recv().await.expect("recv 2"), "line two");
        assert_eq!(rx.recv().await.expect("recv 3"), "line three");
    }
}
