use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mhost_core::protocol::{RpcRequest, RpcResponse};
use serde_json::Value;

use crate::codec::{read_response, write_request};
use crate::transport::connect;

// ---------------------------------------------------------------------------
// IpcClient
// ---------------------------------------------------------------------------

/// Lightweight JSON-RPC client that opens a new Unix-socket connection per call.
pub struct IpcClient {
    socket_path: PathBuf,
    counter: Arc<AtomicU64>,
}

impl IpcClient {
    /// Create a new client targeting the daemon socket at `socket_path`.
    pub fn new(socket_path: &Path) -> Self {
        Self {
            socket_path: socket_path.to_path_buf(),
            counter: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Send a JSON-RPC request and return the response.
    /// A fresh connection is opened for every call.
    pub async fn call(&self, method: &str, params: Value) -> io::Result<RpcResponse> {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        let req = RpcRequest::new(id, method, params);

        let mut stream = connect(&self.socket_path).await?;
        write_request(&mut stream, &req).await?;
        read_response(&mut stream).await
    }

    /// Return `true` when the daemon socket file is present on disk.
    pub fn is_daemon_running(&self) -> bool {
        self.socket_path.exists()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Client detects missing daemon (nonexistent socket path) -------------

    #[tokio::test]
    async fn test_client_detects_missing_daemon() {
        let client = IpcClient::new(Path::new("/tmp/mhost-no-such-daemon.sock"));
        assert!(!client.is_daemon_running(), "daemon should not be detected");
    }
}
