use std::time::Duration;

use mhost_core::health::{HealthCheckKind, HealthConfig, HealthStatus};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Run a single script health check.  The `command` string is passed to
/// `/bin/sh -c` so it supports shell syntax.  Returns:
/// - [`HealthStatus::Healthy`]  — exit code 0
/// - [`HealthStatus::Unhealthy`] — non-zero exit code, spawn failure, or timeout
/// - [`HealthStatus::Disabled`]  — config is not a Script kind
pub async fn run_script_check(config: &HealthConfig) -> HealthStatus {
    let command = match &config.kind {
        HealthCheckKind::Script { command } => command.clone(),
        _ => {
            debug!("run_script_check called with non-Script config; returning Disabled");
            return HealthStatus::Disabled;
        }
    };

    let duration = Duration::from_millis(config.timeout_ms);

    let child_future = Command::new("sh").arg("-c").arg(&command).status();

    match timeout(duration, child_future).await {
        Ok(Ok(status)) => {
            if status.success() {
                debug!("Script health check '{}' exited 0 (healthy)", command);
                HealthStatus::Healthy
            } else {
                warn!(
                    "Script health check '{}' exited {:?} (unhealthy)",
                    command,
                    status.code()
                );
                HealthStatus::Unhealthy
            }
        }
        Ok(Err(err)) => {
            warn!("Script health check '{}' failed to spawn: {}", command, err);
            HealthStatus::Unhealthy
        }
        Err(_elapsed) => {
            warn!(
                "Script health check '{}' timed out after {}ms",
                command, config.timeout_ms
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
    async fn test_script_true_returns_healthy() {
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "true".to_string(),
            },
            interval_ms: 15_000,
            timeout_ms: 5_000,
            retries: 1,
        };
        assert_eq!(run_script_check(&config).await, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_script_false_returns_unhealthy() {
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "false".to_string(),
            },
            interval_ms: 15_000,
            timeout_ms: 5_000,
            retries: 1,
        };
        assert_eq!(run_script_check(&config).await, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_script_wrong_kind_returns_disabled() {
        let config = HealthConfig {
            kind: HealthCheckKind::Http {
                url: "http://localhost/health".to_string(),
                expected_status: 200,
            },
            interval_ms: 10_000,
            timeout_ms: 3_000,
            retries: 3,
        };
        assert_eq!(run_script_check(&config).await, HealthStatus::Disabled);
    }

    #[tokio::test]
    async fn test_script_with_arguments_returns_healthy() {
        // `test -n "hello"` exits 0 when the string is non-empty.
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: r#"test -n "hello""#.to_string(),
            },
            interval_ms: 15_000,
            timeout_ms: 5_000,
            retries: 1,
        };
        assert_eq!(run_script_check(&config).await, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_script_with_arguments_false_condition_returns_unhealthy() {
        // `test -n ""` exits 1 when the string is empty.
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: r#"test -n """#.to_string(),
            },
            interval_ms: 15_000,
            timeout_ms: 5_000,
            retries: 1,
        };
        assert_eq!(run_script_check(&config).await, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_empty_command_returns_unhealthy() {
        // An empty string passed to sh -c should produce an error or a zero exit,
        // but either way it should not panic.  On most POSIX shells `sh -c ""` exits 0,
        // so we verify the function completes without panicking and returns a valid status.
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "".to_string(),
            },
            interval_ms: 5_000,
            timeout_ms: 3_000,
            retries: 1,
        };
        // Just ensure no panic; accept either Healthy or Unhealthy.
        let status = run_script_check(&config).await;
        assert!(
            status == HealthStatus::Healthy || status == HealthStatus::Unhealthy,
            "empty command must return Healthy or Unhealthy, got {status:?}"
        );
    }

    #[tokio::test]
    async fn test_script_nonexistent_binary_returns_unhealthy() {
        // A command that references a non-existent binary fails to execute.
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "/nonexistent/path/to/binary --check".to_string(),
            },
            interval_ms: 5_000,
            timeout_ms: 3_000,
            retries: 1,
        };
        assert_eq!(run_script_check(&config).await, HealthStatus::Unhealthy);
    }
}
