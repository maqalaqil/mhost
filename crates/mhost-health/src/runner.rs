use std::time::Duration;

use mhost_core::health::{HealthCheckKind, HealthConfig, HealthStatus};
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep;
use tracing::{debug, info};

use crate::http_probe::run_http_check;
use crate::script_probe::run_script_check;
use crate::tcp_probe::run_tcp_check;

// ---------------------------------------------------------------------------
// HealthEvent
// ---------------------------------------------------------------------------

/// An event emitted after each probe attempt.
#[derive(Debug, Clone)]
pub struct HealthEvent {
    pub process_name: String,
    pub instance: u32,
    pub status: HealthStatus,
}

// ---------------------------------------------------------------------------
// HealthCheckRunner
// ---------------------------------------------------------------------------

/// Runs health probes on a configurable interval and emits [`HealthEvent`]s
/// via a channel.  Stops cleanly when the shutdown signal fires.
pub struct HealthCheckRunner {
    process_name: String,
    instance: u32,
    config: HealthConfig,
}

impl HealthCheckRunner {
    pub fn new(process_name: String, instance: u32, config: HealthConfig) -> Self {
        Self {
            process_name,
            instance,
            config,
        }
    }

    /// Run the health check loop until `shutdown` fires.
    ///
    /// The probe is dispatched, consecutive failure count is tracked against
    /// `config.retries`, and a [`HealthEvent`] is sent after every check.
    pub async fn run(
        self,
        tx: mpsc::Sender<HealthEvent>,
        mut shutdown: oneshot::Receiver<()>,
    ) {
        let interval = Duration::from_millis(self.config.interval_ms);
        let mut consecutive_failures: u32 = 0;

        info!(
            "Health check runner starting for {}[{}]",
            self.process_name, self.instance
        );

        loop {
            // Run the appropriate probe.
            let raw_status = run_probe(&self.config).await;

            // Apply retry logic: only report Unhealthy after retries are exhausted.
            let status = match raw_status {
                HealthStatus::Healthy => {
                    consecutive_failures = 0;
                    HealthStatus::Healthy
                }
                HealthStatus::Unhealthy => {
                    consecutive_failures += 1;
                    if consecutive_failures >= self.config.retries {
                        debug!(
                            "{}[{}] unhealthy after {} consecutive failures",
                            self.process_name, self.instance, consecutive_failures
                        );
                        HealthStatus::Unhealthy
                    } else {
                        // Still within retry budget — report as Unknown (pending retry).
                        HealthStatus::Unknown
                    }
                }
                other => {
                    consecutive_failures = 0;
                    other
                }
            };

            let event = HealthEvent {
                process_name: self.process_name.clone(),
                instance: self.instance,
                status,
            };

            // Send the event; if the receiver is gone, stop the loop.
            if tx.send(event).await.is_err() {
                debug!(
                    "Health event receiver dropped for {}[{}]; stopping runner",
                    self.process_name, self.instance
                );
                return;
            }

            // Wait for the next interval or a shutdown signal.
            tokio::select! {
                _ = sleep(interval) => {}
                _ = &mut shutdown => {
                    info!(
                        "Health check runner shutting down for {}[{}]",
                        self.process_name, self.instance
                    );
                    return;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// run_probe (dispatch helper)
// ---------------------------------------------------------------------------

async fn run_probe(config: &HealthConfig) -> HealthStatus {
    match &config.kind {
        HealthCheckKind::Http { .. } => run_http_check(config).await,
        HealthCheckKind::Tcp { .. } => run_tcp_check(config).await,
        HealthCheckKind::Script { .. } => run_script_check(config).await,
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
    async fn test_runner_sends_healthy_event_for_true_script() {
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "true".to_string(),
            },
            interval_ms: 100,
            timeout_ms: 5_000,
            retries: 1,
        };

        let runner = HealthCheckRunner::new("test-proc".to_string(), 0, config);
        let (tx, mut rx) = mpsc::channel(8);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(async move {
            runner.run(tx, shutdown_rx).await;
        });

        // Receive the first event.
        let event = rx.recv().await.expect("should receive event");
        assert_eq!(event.process_name, "test-proc");
        assert_eq!(event.instance, 0);
        assert_eq!(event.status, HealthStatus::Healthy);

        // Shut down the runner.
        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_runner_stops_on_shutdown() {
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "true".to_string(),
            },
            interval_ms: 10_000, // Very long interval — shutdown should fire first.
            timeout_ms: 1_000,
            retries: 1,
        };

        let runner = HealthCheckRunner::new("shutdown-test".to_string(), 0, config);
        let (tx, _rx) = mpsc::channel(8);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let handle = tokio::spawn(async move {
            runner.run(tx, shutdown_rx).await;
        });

        // Signal immediate shutdown.
        let _ = shutdown_tx.send(());

        // The task should complete quickly (well within 1 second).
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("runner should stop after shutdown")
            .expect("runner task should not panic");
    }

    #[test]
    fn runner_construction_stores_fields() {
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "true".to_string(),
            },
            interval_ms: 5_000,
            timeout_ms: 2_000,
            retries: 3,
        };
        let runner = HealthCheckRunner::new("my-service".to_string(), 7, config.clone());
        assert_eq!(runner.process_name, "my-service");
        assert_eq!(runner.instance, 7);
        assert_eq!(runner.config, config);
    }

    #[test]
    fn health_event_fields_are_correct() {
        let event = HealthEvent {
            process_name: "api".to_string(),
            instance: 2,
            status: mhost_core::health::HealthStatus::Healthy,
        };
        assert_eq!(event.process_name, "api");
        assert_eq!(event.instance, 2);
        assert_eq!(event.status, mhost_core::health::HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_runner_retries_before_unhealthy() {
        // retries = 3: first two failures should produce Unknown, third → Unhealthy.
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "false".to_string(),
            },
            interval_ms: 10,
            timeout_ms: 5_000,
            retries: 3,
        };

        let runner = HealthCheckRunner::new("retry-test".to_string(), 0, config);
        let (tx, mut rx) = mpsc::channel(16);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(async move {
            runner.run(tx, shutdown_rx).await;
        });

        let evt1 = rx.recv().await.unwrap();
        let evt2 = rx.recv().await.unwrap();
        let evt3 = rx.recv().await.unwrap();

        // First two failures are still within retry budget → Unknown.
        assert_eq!(evt1.status, mhost_core::health::HealthStatus::Unknown);
        assert_eq!(evt2.status, mhost_core::health::HealthStatus::Unknown);
        // Third failure exceeds retries → Unhealthy.
        assert_eq!(evt3.status, mhost_core::health::HealthStatus::Unhealthy);

        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_runner_multiple_probes_in_sequence() {
        let config = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "true".to_string(),
            },
            interval_ms: 10,
            timeout_ms: 5_000,
            retries: 1,
        };

        let runner = HealthCheckRunner::new("seq-test".to_string(), 1, config);
        let (tx, mut rx) = mpsc::channel(16);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(async move {
            runner.run(tx, shutdown_rx).await;
        });

        // Collect 3 consecutive healthy events.
        for _ in 0..3 {
            let evt = rx.recv().await.expect("should receive event");
            assert_eq!(evt.status, mhost_core::health::HealthStatus::Healthy);
            assert_eq!(evt.process_name, "seq-test");
            assert_eq!(evt.instance, 1);
        }

        let _ = shutdown_tx.send(());
    }
}
