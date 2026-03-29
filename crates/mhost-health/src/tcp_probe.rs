use std::time::Duration;

use mhost_core::health::{HealthCheckKind, HealthConfig, HealthStatus};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Run a single TCP health check.  Returns [`HealthStatus::Healthy`] if a
/// TCP connection can be established within the configured timeout,
/// [`HealthStatus::Unhealthy`] on connection failure or timeout, and
/// [`HealthStatus::Disabled`] if the config is not a TCP kind.
pub async fn run_tcp_check(config: &HealthConfig) -> HealthStatus {
    let (host, port) = match &config.kind {
        HealthCheckKind::Tcp { host, port } => (host.clone(), *port),
        _ => {
            debug!("run_tcp_check called with non-TCP config; returning Disabled");
            return HealthStatus::Disabled;
        }
    };

    let addr = format!("{host}:{port}");
    let duration = Duration::from_millis(config.timeout_ms);

    match timeout(duration, TcpStream::connect(&addr)).await {
        Ok(Ok(_stream)) => {
            debug!("TCP health check {} -> connected (healthy)", addr);
            HealthStatus::Healthy
        }
        Ok(Err(err)) => {
            warn!("TCP health check {} failed: {}", addr, err);
            HealthStatus::Unhealthy
        }
        Err(_elapsed) => {
            warn!(
                "TCP health check {} timed out after {}ms",
                addr, config.timeout_ms
            );
            HealthStatus::Unhealthy
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mhost_core::health::HealthCheckKind;

    #[tokio::test]
    async fn test_tcp_closed_port_returns_unhealthy() {
        // Port 1 is effectively always refused on any dev machine.
        let config = HealthConfig {
            kind: HealthCheckKind::Tcp {
                host: "127.0.0.1".to_string(),
                port: 1,
            },
            interval_ms: 5_000,
            timeout_ms: 500,
            retries: 1,
        };
        assert_eq!(run_tcp_check(&config).await, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_tcp_wrong_kind_returns_disabled() {
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "true".to_string(),
            },
            interval_ms: 5_000,
            timeout_ms: 1_000,
            retries: 1,
        };
        assert_eq!(run_tcp_check(&config).await, HealthStatus::Disabled);
    }

    #[tokio::test]
    async fn test_tcp_timeout_on_non_routable_address_returns_unhealthy() {
        // 192.0.2.0/24 is TEST-NET-1 (RFC 5737) — packets are dropped, not refused,
        // so the connection attempt will hang until our very short timeout fires.
        let config = HealthConfig {
            kind: HealthCheckKind::Tcp {
                host: "192.0.2.1".to_string(),
                port: 9999,
            },
            interval_ms: 5_000,
            timeout_ms: 200, // very short to keep the test fast
            retries: 1,
        };
        assert_eq!(run_tcp_check(&config).await, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_tcp_http_kind_returns_disabled() {
        let config = HealthConfig {
            kind: HealthCheckKind::Http {
                url: "http://localhost/health".to_string(),
                expected_status: 200,
            },
            interval_ms: 5_000,
            timeout_ms: 1_000,
            retries: 1,
        };
        assert_eq!(run_tcp_check(&config).await, HealthStatus::Disabled);
    }
}
