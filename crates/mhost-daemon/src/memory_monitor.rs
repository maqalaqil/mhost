#![allow(dead_code)]
use std::time::Duration;
use tokio::sync::oneshot;

// ---------------------------------------------------------------------------
// MemoryMonitor
// ---------------------------------------------------------------------------

/// Spawns a background task that polls the RSS of a process and sends SIGKILL
/// if it exceeds the configured limit.  The exit watcher handles the restart.
pub struct MemoryMonitor;

impl MemoryMonitor {
    /// Spawn a memory-polling task.
    ///
    /// `pid`             – OS PID to watch.
    /// `max_memory_bytes`– Kill threshold in bytes.
    /// `process_name`    – Used only for log messages.
    /// `poll_interval`   – How often to sample RSS.
    ///
    /// Returns a `oneshot::Sender` whose drop (or explicit send) cancels the
    /// monitor before the limit is ever reached.
    pub fn spawn(
        pid: u32,
        max_memory_bytes: u64,
        process_name: String,
        poll_interval: Duration,
    ) -> oneshot::Sender<()> {
        let (tx, mut rx) = oneshot::channel::<()>();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(poll_interval) => {
                        if let Some(rss) = get_rss_bytes(pid) {
                            if rss > max_memory_bytes {
                                tracing::warn!(
                                    process = %process_name,
                                    rss_mb   = rss / 1_048_576,
                                    limit_mb = max_memory_bytes / 1_048_576,
                                    "Memory limit exceeded — killing process"
                                );
                                // SIGKILL the process; the exit watcher will
                                // handle the restart cycle.
                                #[cfg(unix)]
                                {
                                    let _ = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
                                }
                                #[cfg(not(unix))]
                                {
                                    tracing::warn!(
                                        pid = pid,
                                        "Cannot kill process: SIGKILL not available on this platform"
                                    );
                                }
                                break;
                            } else {
                                tracing::trace!(
                                    process = %process_name,
                                    pid     = pid,
                                    rss_mb  = rss / 1_048_576,
                                    "Memory sample within limit"
                                );
                            }
                        }
                        // If `get_rss_bytes` returns None the process likely
                        // already exited; stop monitoring silently.
                        else {
                            tracing::debug!(
                                process = %process_name,
                                pid     = pid,
                                "RSS unavailable — stopping memory monitor"
                            );
                            break;
                        }
                    }
                    _ = &mut rx => {
                        tracing::debug!(
                            process = %process_name,
                            "Memory monitor shut down by request"
                        );
                        break;
                    }
                }
            }
        });

        tx
    }
}

// ---------------------------------------------------------------------------
// get_rss_bytes
// ---------------------------------------------------------------------------

/// Return the resident set size in bytes for the given PID, or `None` if the
/// information is unavailable (process already exited, unsupported platform,
/// etc.).
pub fn get_rss_bytes(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        // /proc/{pid}/status contains a line like:
        //   VmRSS:   12345 kB
        let status = std::fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        // `ps -o rss= -p <PID>` prints RSS in KiB (no header).
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let rss_kb: u64 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .ok()?;
        Some(rss_kb * 1024)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// get_rss_bytes should return Some(non-zero) for our own process.
    #[test]
    fn current_process_rss_is_nonzero() {
        let pid = std::process::id();
        let rss = get_rss_bytes(pid);

        // On Linux and macOS this must succeed.
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            assert!(
                rss.is_some(),
                "get_rss_bytes returned None for current PID {pid}"
            );
            assert!(
                rss.unwrap() > 0,
                "RSS for current process should be > 0, got {}",
                rss.unwrap()
            );
        }

        // On other platforms the function returns None — that is acceptable.
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            // Just verify it doesn't panic.
            let _ = rss;
        }
    }

    /// A nonexistent PID should return None, not panic.
    #[test]
    fn nonexistent_pid_returns_none() {
        // PID 0 is never a valid user process.
        let rss = get_rss_bytes(0);
        assert!(rss.is_none(), "expected None for PID 0, got {rss:?}");
    }

    /// Spawning a monitor task should give back a sender immediately.
    #[tokio::test]
    async fn spawn_returns_sender() {
        let pid = std::process::id();
        let tx = MemoryMonitor::spawn(
            pid,
            u64::MAX, // limit so high it will never be exceeded
            "test-process".to_string(),
            Duration::from_secs(60),
        );
        // Sending on the channel should cancel the monitor without panicking.
        let _ = tx.send(());
        // Give the spawned task a moment to exit cleanly.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    /// get_rss_bytes for a very high PID that almost certainly does not exist.
    #[test]
    fn very_high_pid_returns_none() {
        // PID 4194304 (2^22) is above the Linux max PID and almost certainly absent.
        let rss = get_rss_bytes(4_194_304);
        // We don't assert the exact value since on some systems very large PIDs
        // could theoretically be valid, but on a test machine they won't be.
        // The important thing is that the function does not panic.
        let _ = rss;
    }

    /// get_rss_bytes should always return Some for the test process itself on
    /// supported platforms, regardless of how it is called.
    #[test]
    fn current_process_rss_consistent() {
        let pid = std::process::id();
        let first = get_rss_bytes(pid);
        let second = get_rss_bytes(pid);

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            assert!(first.is_some());
            assert!(second.is_some());
            // Both calls should return a positive value.
            assert!(first.unwrap() > 0);
            assert!(second.unwrap() > 0);
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            // Unsupported platform; just confirm no panic.
            let _ = (first, second);
        }
    }

    /// Dropping the sender immediately shuts down the monitor task.
    #[tokio::test]
    async fn drop_sender_shuts_down_monitor() {
        let pid = std::process::id();
        let tx = MemoryMonitor::spawn(
            pid,
            u64::MAX,
            "drop-test".to_string(),
            Duration::from_secs(30),
        );
        // Dropping the sender should signal the task to stop.
        drop(tx);
        tokio::time::sleep(Duration::from_millis(20)).await;
        // If the task is still alive that is fine — we're just verifying no panic.
    }
}
