use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use tokio::sync::mpsc;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// ProcessMetrics
// ---------------------------------------------------------------------------

/// A snapshot of resource usage for a single process at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetrics {
    pub pid: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// collect_once
// ---------------------------------------------------------------------------

/// Poll sysinfo for a single PID and return a `ProcessMetrics` snapshot.
/// Returns `None` when the PID is not found on the current system.
pub fn collect_once(pid: u32) -> Option<ProcessMetrics> {
    let refresh_kind =
        RefreshKind::new().with_processes(ProcessRefreshKind::new().with_cpu().with_memory());
    let mut sys = System::new_with_specifics(refresh_kind);

    // sysinfo requires at least two CPU polls to produce a non-zero reading.
    // Perform a second refresh so the first call still returns a useful (if
    // zero) CPU reading during tests.
    let kind = ProcessRefreshKind::new().with_cpu().with_memory();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, false, kind);

    let sysinfo_pid = Pid::from_u32(pid);
    let proc = sys.process(sysinfo_pid)?;

    Some(ProcessMetrics {
        pid,
        cpu_percent: proc.cpu_usage(),
        memory_bytes: proc.memory(),
        timestamp: Utc::now(),
    })
}

// ---------------------------------------------------------------------------
// MetricsCollector
// ---------------------------------------------------------------------------

/// Background collector that polls a set of PIDs at a configurable interval
/// and forwards `ProcessMetrics` snapshots over a channel.
pub struct MetricsCollector {
    interval: Duration,
    pids: Vec<u32>,
    tx: mpsc::Sender<ProcessMetrics>,
}

impl MetricsCollector {
    /// Create a new collector.
    ///
    /// * `interval` — how often to poll each PID
    /// * `pids` — list of process IDs to monitor
    /// * `tx` — channel to send metrics into
    pub fn new(interval: Duration, pids: Vec<u32>, tx: mpsc::Sender<ProcessMetrics>) -> Self {
        Self { interval, pids, tx }
    }

    /// Spawn a Tokio task that polls indefinitely.
    /// The task stops when either the channel is closed or the handle is dropped.
    pub fn start(self) {
        tokio::spawn(async move {
            loop {
                let refresh_kind = RefreshKind::new()
                    .with_processes(ProcessRefreshKind::new().with_cpu().with_memory());
                let mut sys = System::new_with_specifics(refresh_kind);
                let kind = ProcessRefreshKind::new().with_cpu().with_memory();
                sys.refresh_processes_specifics(ProcessesToUpdate::All, false, kind);

                for &pid in &self.pids {
                    let sysinfo_pid = Pid::from_u32(pid);
                    if let Some(proc) = sys.process(sysinfo_pid) {
                        let metrics = ProcessMetrics {
                            pid,
                            cpu_percent: proc.cpu_usage(),
                            memory_bytes: proc.memory(),
                            timestamp: Utc::now(),
                        };
                        debug!(pid, "collected metrics");
                        if self.tx.send(metrics).await.is_err() {
                            warn!("metrics channel closed; stopping collector");
                            return;
                        }
                    } else {
                        debug!(pid, "PID not found during poll");
                    }
                }

                tokio::time::sleep(self.interval).await;
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::process;

    #[test]
    fn collect_once_current_process_has_memory() {
        let pid = process::id();
        let metrics =
            collect_once(pid).expect("current process should always be visible to sysinfo");
        assert!(
            metrics.memory_bytes > 0,
            "memory_bytes should be > 0 for running process, got {}",
            metrics.memory_bytes
        );
        assert_eq!(metrics.pid, pid);
        // timestamp should be recent (within last 5 seconds)
        let age = Utc::now() - metrics.timestamp;
        assert!(age.num_seconds() < 5);
    }

    #[test]
    fn collect_once_unknown_pid_returns_none() {
        // PID 0 is never a user process; on Linux/macOS it is the idle task.
        // sysinfo will not expose it to user space.
        assert!(collect_once(0).is_none());
    }

    #[tokio::test]
    async fn background_collector_sends_metrics() {
        let pid = process::id();
        let (tx, mut rx) = mpsc::channel(16);
        let collector = MetricsCollector::new(Duration::from_millis(50), vec![pid], tx);
        collector.start();

        let metrics = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("should receive within timeout")
            .expect("channel should not be closed");

        assert_eq!(metrics.pid, pid);
    }
}
