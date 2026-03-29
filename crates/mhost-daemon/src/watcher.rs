use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::sync::RwLock;

use mhost_core::process::{ProcessConfig, ProcessInfo, ProcessStatus};
use mhost_core::MhostPaths;

use crate::state::StateStore;
use crate::supervisor::{backoff_delay, ManagedProcess};

// ---------------------------------------------------------------------------
// ExitWatcher
// ---------------------------------------------------------------------------

/// Watches a child process for exit and triggers auto-restart with exponential
/// backoff when the exit was unintentional.
pub struct ExitWatcher;

impl ExitWatcher {
    /// Spawn a tokio task that waits for the child to exit, then handles
    /// restart logic.
    ///
    /// The task:
    /// 1. Takes the `Child` out of the managed process map.
    /// 2. Waits for the child to exit.
    /// 3. If the process was intentionally stopped (status is `Stopping` or
    ///    `Stopped`), returns without restarting.
    /// 4. If the process ran for less than `min_uptime_ms`, increments the
    ///    restart counter (crash loop detection).
    /// 5. If `restart_count >= max_restarts`, marks the process as `Errored`.
    /// 6. Otherwise, sleeps for the back-off delay, then respawns the child by
    ///    calling `crate::supervisor::spawn_child_static`.
    pub fn spawn(
        processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
        key: String,
        config: ProcessConfig,
        state: Arc<tokio::sync::Mutex<StateStore>>,
        paths: MhostPaths,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            // -----------------------------------------------------------------
            // Phase 1: take the child out of the map and wait for it to exit.
            // -----------------------------------------------------------------
            let child_taken = {
                let mut procs = processes.write().await;
                procs
                    .get_mut(&key)
                    .and_then(|mp| mp.child.take())
            };

            let Some(mut child) = child_taken else {
                // No child to watch (already taken or never set). Nothing to do.
                return;
            };

            let start_time = Instant::now();
            let exit_status = child.wait().await;

            let exit_code: Option<i32> = match &exit_status {
                Ok(status) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::ExitStatusExt;
                        if let Some(sig) = status.signal() {
                            tracing::debug!(key = %key, signal = sig, "child killed by signal");
                        }
                    }
                    status.code()
                }
                Err(e) => {
                    tracing::error!(key = %key, error = %e, "error waiting for child exit");
                    None
                }
            };

            tracing::info!(key = %key, exit_code = ?exit_code, "child exited");

            // -----------------------------------------------------------------
            // Phase 2: check if the stop was intentional.
            // -----------------------------------------------------------------
            {
                let procs = processes.read().await;
                if let Some(mp) = procs.get(&key) {
                    let status = &mp.info.status;
                    if matches!(status, ProcessStatus::Stopping | ProcessStatus::Stopped) {
                        tracing::debug!(key = %key, "intentional stop — no restart");
                        return;
                    }
                } else {
                    // Process was removed from the map (deleted); nothing to do.
                    return;
                }
            }

            // -----------------------------------------------------------------
            // Phase 3: determine whether to bump the crash counter.
            // -----------------------------------------------------------------
            let uptime_ms = start_time.elapsed().as_millis() as u64;
            let is_crash = uptime_ms < config.min_uptime_ms;

            // Read current restart_count, then decide.
            let (current_restart_count, current_pid) = {
                let procs = processes.read().await;
                match procs.get(&key) {
                    Some(mp) => (mp.info.restart_count, mp.info.pid),
                    None => return,
                }
            };

            let new_restart_count = if is_crash {
                current_restart_count + 1
            } else {
                // Process ran long enough — reset crash counter.
                0
            };

            // Update the info with the crash counter and exit code.
            {
                let mut procs = processes.write().await;
                if let Some(mp) = procs.get_mut(&key) {
                    mp.info = ProcessInfo {
                        restart_count: new_restart_count,
                        exit_code,
                        pid: None,
                        uptime_started: None,
                        ..mp.info.clone()
                    };
                } else {
                    return;
                }
            }

            // -----------------------------------------------------------------
            // Phase 4: circuit breaker — too many crashes?
            // -----------------------------------------------------------------
            if new_restart_count >= config.max_restarts {
                tracing::error!(
                    key = %key,
                    restart_count = new_restart_count,
                    max_restarts = config.max_restarts,
                    "max restarts reached — marking as errored"
                );
                {
                    let mut procs = processes.write().await;
                    if let Some(mp) = procs.get_mut(&key) {
                        mp.info = ProcessInfo {
                            status: ProcessStatus::Errored,
                            ..mp.info.clone()
                        };
                        let info_snapshot = mp.info.clone();
                        // Persist outside of the lock in a fire-and-forget fashion.
                        drop(procs);
                        let state_guard = state.lock().await;
                        let _ = state_guard.upsert_process(&info_snapshot);
                        let _ = state_guard.log_event(
                            &config.name,
                            "errored",
                            Some("max restarts reached"),
                        );
                    }
                }
                return;
            }

            // -----------------------------------------------------------------
            // Phase 5: sleep with exponential back-off, then respawn.
            // -----------------------------------------------------------------
            let delay_ms =
                backoff_delay(new_restart_count, config.restart_delay_ms, 30_000);
            tracing::info!(
                key = %key,
                attempt = new_restart_count,
                delay_ms = delay_ms,
                "scheduling restart after back-off"
            );
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;

            // Mark as Starting before we respawn.
            {
                let mut procs = processes.write().await;
                if let Some(mp) = procs.get_mut(&key) {
                    mp.info = ProcessInfo {
                        status: ProcessStatus::Starting,
                        ..mp.info.clone()
                    };
                } else {
                    return;
                }
            }

            // Determine the instance index from the key ("name:instance").
            let instance: u32 = key
                .rsplit(':')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            // Respawn the child.
            match crate::supervisor::spawn_child_static(&config, instance, &paths).await {
                Ok((new_info, new_child)) => {
                    // Write PID file.
                    if let Some(pid) = new_info.pid {
                        let pid_path = paths.process_pid(&config.name, instance);
                        let _ = std::fs::write(&pid_path, pid.to_string());
                    }

                    // Build updated info preserving restart_count from this session.
                    let updated_info = ProcessInfo {
                        restart_count: new_restart_count,
                        ..new_info.clone()
                    };

                    // Store child and updated info; spawn a new watcher.
                    let watcher_handle = {
                        let mut procs = processes.write().await;
                        if let Some(mp) = procs.get_mut(&key) {
                            mp.child = Some(new_child);
                            mp.info = updated_info.clone();
                        } else {
                            return;
                        }

                        // Spawn fresh watcher for the new child.
                        ExitWatcher::spawn(
                            Arc::clone(&processes),
                            key.clone(),
                            config.clone(),
                            Arc::clone(&state),
                            paths.clone(),
                        )
                    };

                    // Store the new watcher handle.
                    {
                        let mut procs = processes.write().await;
                        if let Some(mp) = procs.get_mut(&key) {
                            mp.watcher = Some(watcher_handle);
                        }
                    }

                    // Persist restart.
                    let state_guard = state.lock().await;
                    let _ = state_guard.upsert_process(&updated_info);
                    let _ = state_guard.log_event(
                        &config.name,
                        "restarted",
                        Some("auto-restart after crash"),
                    );

                    tracing::info!(
                        key = %key,
                        pid = ?updated_info.pid,
                        restart_count = new_restart_count,
                        "process restarted"
                    );
                }
                Err(e) => {
                    tracing::error!(key = %key, error = %e, "failed to respawn process");
                    let mut procs = processes.write().await;
                    if let Some(mp) = procs.get_mut(&key) {
                        mp.info = ProcessInfo {
                            status: ProcessStatus::Errored,
                            ..mp.info.clone()
                        };
                        let info_snapshot = mp.info.clone();
                        drop(procs);
                        let state_guard = state.lock().await;
                        let _ = state_guard.upsert_process(&info_snapshot);
                        let _ = state_guard.log_event(
                            &config.name,
                            "errored",
                            Some(&format!("respawn failed: {}", e)),
                        );
                    }
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::backoff_delay;

    #[test]
    fn backoff_never_exceeds_cap() {
        for attempt in 0..=30 {
            let delay = backoff_delay(attempt, 100, 30_000);
            assert!(
                delay <= 30_000,
                "attempt {} produced delay {} > 30_000",
                attempt,
                delay
            );
        }
    }

    #[test]
    fn backoff_doubles_until_cap() {
        assert_eq!(backoff_delay(0, 100, 30_000), 100);
        assert_eq!(backoff_delay(1, 100, 30_000), 200);
        assert_eq!(backoff_delay(2, 100, 30_000), 400);
        assert_eq!(backoff_delay(3, 100, 30_000), 800);
    }
}
