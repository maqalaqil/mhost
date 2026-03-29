use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use cron::Schedule;
use tokio::sync::{oneshot, RwLock};

use mhost_core::paths::MhostPaths;
use mhost_core::process::ProcessConfig;

use crate::state::StateStore;
use crate::supervisor::ManagedProcess;

// ---------------------------------------------------------------------------
// CronScheduler
// ---------------------------------------------------------------------------

/// Spawns a background task that triggers a process restart on a cron schedule.
pub struct CronScheduler;

impl CronScheduler {
    /// Spawn a task that restarts a process on a cron schedule.
    ///
    /// Returns `Some(shutdown_tx)` when the cron expression is valid, or
    /// `None` when it cannot be parsed (an error is logged in that case).
    ///
    /// Dropping or sending on `shutdown_tx` cancels the scheduler loop.
    pub fn spawn(
        cron_expr: String,
        config: ProcessConfig,
        processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
        state: Arc<tokio::sync::Mutex<StateStore>>,
        paths: MhostPaths,
    ) -> Option<oneshot::Sender<()>> {
        let schedule = match Schedule::from_str(&cron_expr) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(
                    cron = %cron_expr,
                    process = %config.name,
                    error = %e,
                    "Invalid cron expression — cron scheduler not started"
                );
                return None;
            }
        };

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let process_name = config.name.clone();

        tokio::spawn(async move {
            loop {
                // Find the next scheduled tick.
                let next_opt = schedule.upcoming(chrono::Utc).next();
                let Some(next_time) = next_opt else {
                    tracing::debug!(
                        process = %process_name,
                        "Cron schedule has no upcoming events — exiting scheduler"
                    );
                    break;
                };

                let now = chrono::Utc::now();
                let delay = (next_time - now)
                    .to_std()
                    .unwrap_or(std::time::Duration::ZERO);

                tokio::select! {
                    _ = tokio::time::sleep(delay) => {
                        tracing::info!(
                            process = %process_name,
                            "Cron-triggered restart"
                        );

                        // Perform the restart by killing the process; the exit
                        // watcher will handle respawn automatically.
                        kill_process_instances(&processes, &process_name).await;

                        // On a cron restart we also want a fresh watcher cycle,
                        // so we re-spawn via start_process_arc using the config.
                        restart_via_supervisor(
                            config.clone(),
                            Arc::clone(&processes),
                            Arc::clone(&state),
                            paths.clone(),
                        )
                        .await;
                    }
                    _ = &mut shutdown_rx => {
                        tracing::debug!(
                            process = %process_name,
                            "Cron scheduler shutting down"
                        );
                        break;
                    }
                }
            }
        });

        Some(shutdown_tx)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// SIGKILL every running instance of `name` so the exit watcher can take over.
async fn kill_process_instances(
    processes: &Arc<RwLock<HashMap<String, ManagedProcess>>>,
    name: &str,
) {
    let procs = processes.read().await;
    for (key, mp) in procs.iter() {
        if !key.starts_with(&format!("{}:", name)) {
            continue;
        }
        if let Some(pid) = mp.info.pid {
            #[cfg(unix)]
            {
                let _ = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
            }
            #[cfg(not(unix))]
            {
                tracing::warn!(pid = pid, "kill not supported on this platform");
            }
        }
    }
}

/// Stop all instances of `config.name` then start them fresh.
///
/// This mirrors what the supervisor's `restart_process` does but accepts an
/// `Arc<Mutex<StateStore>>` so the fresh exit watcher can persist state.
async fn restart_via_supervisor(
    config: ProcessConfig,
    processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
    state: Arc<tokio::sync::Mutex<StateStore>>,
    paths: MhostPaths,
) {
    use mhost_core::process::{ProcessInfo, ProcessStatus};

    let name = config.name.clone();

    // --- stop ---
    let keys: Vec<String> = {
        let procs = processes.read().await;
        procs
            .keys()
            .filter(|k| k.starts_with(&format!("{}:", name)))
            .cloned()
            .collect()
    };

    {
        let mut procs = processes.write().await;
        for key in &keys {
            if let Some(mp) = procs.get_mut(key) {
                if let Some(handle) = mp.watcher.take() {
                    handle.abort();
                }
                // Also abort cron / memory shutdown senders so they don't fire
                // concurrently during this restart cycle.
                let _ = mp.cron_shutdown.take();
                let _ = mp.memory_shutdown.take();

                mp.info = ProcessInfo {
                    status: ProcessStatus::Stopping,
                    ..mp.info.clone()
                };

                if let Some(ref mut child) = mp.child {
                    #[cfg(unix)]
                    {
                        use nix::sys::signal::{kill as nix_kill, Signal};
                        use nix::unistd::Pid;
                        if let Some(pid) = child.id() {
                            let _ = nix_kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                            let grace = tokio::time::Duration::from_millis(mp.info.config.grace_period_ms);
                            let _ = tokio::time::timeout(grace, child.wait()).await;
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = child.kill().await;
                    }
                }

                mp.info = ProcessInfo {
                    status: ProcessStatus::Stopped,
                    pid: None,
                    uptime_started: None,
                    ..mp.info.clone()
                };
            }
        }
    }

    // --- start ---
    for i in 0..config.instances {
        let key = crate::supervisor::Supervisor::process_key(&config.name, i);

        match crate::supervisor::spawn_child_static(&config, i, &paths).await {
            Ok((new_info, new_child)) => {
                if let Some(pid) = new_info.pid {
                    let pid_path = paths.process_pid(&config.name, i);
                    let _ = std::fs::write(&pid_path, pid.to_string());
                }

                let watcher_handle = {
                    let mut procs = processes.write().await;
                    let entry = procs
                        .entry(key.clone())
                        .and_modify(|mp| {
                            mp.info = new_info.clone();
                            mp.child = None;
                        })
                        .or_insert_with(|| ManagedProcess {
                            info: new_info.clone(),
                            child: None,
                            ring_out: mhost_logs::RingBuffer::new(1000),
                            ring_err: mhost_logs::RingBuffer::new(1000),
                            log_capture: mhost_logs::LogCapture::new(64),
                            watcher: None,
                            cron_shutdown: None,
                            memory_shutdown: None,
                        });
                    entry.child = Some(new_child);

                    crate::watcher::ExitWatcher::spawn(
                        Arc::clone(&processes),
                        key.clone(),
                        config.clone(),
                        Arc::clone(&state),
                        paths.clone(),
                    )
                };

                {
                    let mut procs = processes.write().await;
                    if let Some(mp) = procs.get_mut(&key) {
                        mp.watcher = Some(watcher_handle);
                    }
                }

                let state_guard = state.lock().await;
                let _ = state_guard.upsert_process(&new_info);
                let _ = state_guard.log_event(&name, "restarted", Some("cron-triggered restart"));

                tracing::info!(process = %name, instance = i, "process restarted by cron");
            }
            Err(e) => {
                tracing::error!(process = %name, instance = i, error = %e, "cron restart: spawn failed");
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
    use crate::state::StateStore;

    fn make_state() -> Arc<tokio::sync::Mutex<StateStore>> {
        Arc::new(tokio::sync::Mutex::new(StateStore::in_memory().unwrap()))
    }

    fn make_processes() -> Arc<RwLock<HashMap<String, ManagedProcess>>> {
        Arc::new(RwLock::new(HashMap::new()))
    }

    fn make_paths() -> MhostPaths {
        let tmp = tempfile::tempdir().unwrap();
        let paths = MhostPaths::with_root(tmp.path().to_path_buf());
        // intentionally leak the TempDir so the path stays alive for the test
        std::mem::forget(tmp);
        paths
    }

    fn make_config(name: &str) -> ProcessConfig {
        ProcessConfig {
            name: name.to_string(),
            command: "sleep".to_string(),
            args: vec!["10".to_string()],
            ..Default::default()
        }
    }

    /// A valid cron expression should produce a Some(sender).
    #[tokio::test]
    async fn valid_cron_returns_sender() {
        let sender = CronScheduler::spawn(
            "0 * * * * *".to_string(), // every minute
            make_config("test-valid"),
            make_processes(),
            make_state(),
            make_paths(),
        );
        assert!(sender.is_some(), "valid cron should yield a shutdown sender");
        // Dropping sender shuts down the scheduler task.
    }

    /// An invalid cron expression should return None.
    #[tokio::test]
    async fn invalid_cron_returns_none() {
        let sender = CronScheduler::spawn(
            "not-a-cron".to_string(),
            make_config("test-invalid"),
            make_processes(),
            make_state(),
            make_paths(),
        );
        assert!(sender.is_none(), "invalid cron should yield None");
    }
}
