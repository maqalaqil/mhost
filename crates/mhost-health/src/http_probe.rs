use std::time::Duration;

use mhost_core::health::{HealthCheckKind, HealthConfig, HealthStatus};
use tracing::{debug, warn};

/// Run a single HTTP health check.  Returns [`HealthStatus::Healthy`] if the
/// server responds with the expected status code within the configured timeout,
/// [`HealthStatus::Unhealthy`] on any error or unexpected status, and
/// [`HealthStatus::Disabled`] if the config is not an HTTP kind.
pub async fn run_http_check(config: &HealthConfig) -> HealthStatus {
    let (url, expected_status) = match &config.kind {
        HealthCheckKind::Http { url, expected_status } => (url.clone(), *expected_status),
        _ => {
            debug!("run_http_check called with non-HTTP config; returning Disabled");
            return HealthStatus::Disabled;
        }
    };

    let timeout = Duration::from_millis(config.timeout_ms);

    let client = match reqwest::Client::builder()
        .timeout(timeout)
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            warn!("Failed to build HTTP client: {}", err);
            return HealthStatus::Unhealthy;
        }
    };

    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            if status == expected_status {
                debug!("HTTP health check {} -> {} (healthy)", url, status);
                HealthStatus::Healthy
            } else {
                warn!(
                    "HTTP health check {} -> {} (expected {})",
                    url, status, expected_status
                );
                HealthStatus::Unhealthy
            }
        }
        Err(err) => {
            warn!("HTTP health check {} failed: {}", url, err);
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
    async fn test_http_wrong_kind_returns_disabled() {
        let config = HealthConfig {
            kind: HealthCheckKind::Tcp {
                host: "127.0.0.1".to_string(),
                port: 9999,
            },
            interval_ms: 5_000,
            timeout_ms: 1_000,
            retries: 1,
        };
        assert_eq!(run_http_check(&config).await, HealthStatus::Disabled);
    }

    #[tokio::test]
    async fn test_http_unreachable_returns_unhealthy() {
        // Port 1 should be unreachable / refused on any dev machine.
        let config = HealthConfig {
            kind: HealthCheckKind::Http {
                url: "http://127.0.0.1:1/health".to_string(),
                expected_status: 200,
            },
            interval_ms: 5_000,
            timeout_ms: 500,
            retries: 1,
        };
        assert_eq!(run_http_check(&config).await, HealthStatus::Unhealthy);
    }
}
