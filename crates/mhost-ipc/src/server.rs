use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use mhost_core::protocol::{RpcRequest, RpcResponse};
use tokio::sync::Notify;

use crate::codec::{read_request, write_response};
use crate::transport::{bind, IpcStream};

// ---------------------------------------------------------------------------
// Handler type
// ---------------------------------------------------------------------------

/// A type-erased async handler: takes an `RpcRequest`, returns an `RpcResponse`.
pub type HandlerFn =
    Arc<dyn Fn(RpcRequest) -> Pin<Box<dyn Future<Output = RpcResponse> + Send>> + Send + Sync>;

// ---------------------------------------------------------------------------
// IpcServer
// ---------------------------------------------------------------------------

pub struct IpcServer {
    socket_path: PathBuf,
    shutdown: Arc<Notify>,
}

impl IpcServer {
    /// Create a new server that will bind to `socket_path`.
    pub fn new(socket_path: &Path) -> Self {
        Self {
            socket_path: socket_path.to_path_buf(),
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Return a handle that can be used to trigger graceful shutdown.
    pub fn shutdown_handle(&self) -> Arc<Notify> {
        Arc::clone(&self.shutdown)
    }

    /// Start the accept loop.  Returns when a shutdown is requested.
    pub async fn run(&self, handler: HandlerFn) {
        let listener = bind(&self.socket_path).expect("bind IPC socket");

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let handler = Arc::clone(&handler);
                            tokio::spawn(handle_connection(stream, handler));
                        }
                        Err(e) => {
                            tracing::error!("IpcServer accept error: {}", e);
                        }
                    }
                }
                _ = self.shutdown.notified() => {
                    tracing::info!("IpcServer shutting down");
                    break;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Per-connection task
// ---------------------------------------------------------------------------

/// Read requests from `stream` in a loop, call `handler`, and write responses back.
async fn handle_connection(mut stream: IpcStream, handler: HandlerFn) {
    loop {
        match read_request(&mut stream).await {
            Ok(req) => {
                let resp = handler(req).await;
                if let Err(e) = write_response(&mut stream, &resp).await {
                    tracing::warn!("IpcServer write_response error: {}", e);
                    break;
                }
            }
            Err(_) => {
                // EOF or parse error — close the connection silently.
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mhost_core::protocol::{RpcError, RpcResponse};
    use serde_json::json;

    use crate::client::IpcClient;

    // -- Server-client roundtrip: ping handler, unknown method error ---------

    #[tokio::test]
    async fn test_server_client_roundtrip() {
        let socket_path = std::path::Path::new("/tmp/mhost-server-roundtrip-test.sock");

        let server = IpcServer::new(socket_path);
        let shutdown = server.shutdown_handle();

        // Start server in background.
        tokio::spawn(async move {
            let handler: HandlerFn = Arc::new(|req: RpcRequest| {
                Box::pin(async move {
                    if req.method == "daemon.ping" {
                        RpcResponse::success(req.id, json!("pong"))
                    } else {
                        RpcResponse::error(
                            req.id,
                            RpcError::new(-32601, format!("unknown method: {}", req.method)),
                        )
                    }
                })
            });
            server.run(handler).await;
        });

        // Wait for the server to bind.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client = IpcClient::new(socket_path);

        // --- ping -> pong ---
        let resp = client
            .call("daemon.ping", json!(null))
            .await
            .expect("call ping");
        assert!(resp.error.is_none(), "ping should not return an error");
        assert_eq!(resp.result, Some(json!("pong")));

        // --- unknown method -> error ---
        let resp = client
            .call("unknown.method", json!(null))
            .await
            .expect("call unknown");
        assert!(
            resp.result.is_none(),
            "unknown method should not return result"
        );
        let err = resp.error.expect("should have error field");
        assert_eq!(err.code, -32601);
        assert!(err.message.contains("unknown method"));

        // Shut the server down cleanly.
        shutdown.notify_one();
    }
}
