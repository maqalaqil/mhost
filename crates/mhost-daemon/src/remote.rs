#![allow(dead_code)]
use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::{info, warn};

pub struct RemoteConfig {
    pub enabled: bool,
    pub listen: SocketAddr,
    pub cert_path: String,
    pub key_path: String,
    pub ca_path: String,
}

pub struct RemoteListener;

impl RemoteListener {
    /// Start a remote mTLS listener. For now, this is a stub that logs the intent.
    /// Full mTLS implementation requires loading certs and using tokio-rustls.
    pub async fn start(config: RemoteConfig) -> Result<(), String> {
        if !config.enabled {
            return Ok(());
        }
        info!(listen = %config.listen, "Remote mTLS API enabled (stub)");
        // In production: load server cert+key, require client cert signed by CA
        // Use same JSON-RPC codec as IPC
        // For now, just bind and accept (no TLS)
        let listener = TcpListener::bind(config.listen)
            .await
            .map_err(|e| format!("Remote listener bind failed: {e}"))?;
        info!("Remote API listening on {}", config.listen);

        loop {
            match listener.accept().await {
                Ok((_stream, addr)) => {
                    warn!(addr = %addr, "Remote connection received (mTLS not yet implemented)");
                    // Future: wrap with TLS, handle JSON-RPC
                }
                Err(e) => {
                    warn!(error = %e, "Remote accept failed");
                }
            }
        }
    }
}
