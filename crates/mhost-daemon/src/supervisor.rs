use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

use mhost_core::group::GroupConfig;
use mhost_core::paths::MhostPaths;
use mhost_core::process::{ProcessConfig, ProcessInfo, ProcessStatus};
use mhost_logs::{LogCapture, RingBuffer};

use crate::state::StateStore;

// ---------------------------------------------------------------------------
// ManagedProcess
// ---------------------------------------------------------------------------

pub struct ManagedProcess {
    pub info: ProcessInfo,
    pub child: Option<Child>,
    pub ring_out: RingBuffer,
    pub ring_err: RingBuffer,
    pub log_capture: LogCapture,
    /// Handle to the exit-watcher task. Aborted when the process is stopped.
    pub watcher: Option<tokio::task::JoinHandle<()>>,
    /// Shutdown channel for the cron-restart scheduler (Task 10).
    pub cron_shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    /// Shutdown channel for the memory-limit monitor (Task 11).
    pub memory_shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

// ---------------------------------------------------------------------------
// Supervisor
// ---------------------------------------------------------------------------

pub struct Supervisor {
    processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
    paths: MhostPaths,
}

impl Supervisor {
    pub fn new(paths: MhostPaths) -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            paths,
        }
    }

    /// Canonical key for a process instance: "name:instance".
    pub fn process_key(name: &str, instance: u32) -> String {
        format!("{}:{}", name, instance)
    }

    // -----------------------------------------------------------------------
    // Start
    // -----------------------------------------------------------------------

    /// Start all instances of a process described by `config`.
    /// Returns the list of `ProcessInfo` for every started instance.
    pub async fn start_process(
        &self,
        config: ProcessConfig,
        state: &StateStore,
    ) -> Result<Vec<ProcessInfo>, String> {
        let instances = config.instances;
        let mut started: Vec<ProcessInfo> = Vec::new();

        for i in 0..instances {
            let key = Self::process_key(&config.name, i);

            // Check not already running (async read, no state ref held across await)
            {
                let procs = self.processes.read().await;
                if let Some(mp) = procs.get(&key) {
                    if mp.info.status == ProcessStatus::Online
                        || mp.info.status == ProcessStatus::Starting
                    {
                        return Err(format!(
                            "Process '{}' instance {} is already running",
                            config.name, i
                        ));
                    }
                }
            }
            // ^--- RwLock guard dropped here

            // Spawn child (async, no state ref)
            let (info, child) = spawn_child_static(&config, i, &self.paths).await?;

            // Write PID file (sync)
            if let Some(pid) = info.pid {
                let pid_path = self.paths.process_pid(&config.name, i);
                let _ = std::fs::write(&pid_path, pid.to_string());
            }

            // Store in map (async write) and spawn the exit watcher.
            let watcher_handle = {
                let mut procs = self.processes.write().await;
                let entry = procs
                    .entry(key.clone())
                    .and_modify(|mp| {
                        mp.info = info.clone();
                        mp.child = None; // will be replaced below
                    })
                    .or_insert_with(|| ManagedProcess {
                        info: info.clone(),
                        child: None,
                        ring_out: RingBuffer::new(1000),
                        ring_err: RingBuffer::new(1000),
                        log_capture: LogCapture::new(64),
                        watcher: None,
                        cron_shutdown: None,
                        memory_shutdown: None,
                    });
                entry.child = Some(child);

                // Spawn watcher while still holding the write lock so the child
                // cannot exit and be missed between insert and watcher start.
                crate::watcher::ExitWatcher::spawn(
                    Arc::clone(&self.processes),
                    key.clone(),
                    config.clone(),
                    // The state is not Arc here — wrap a fresh in-memory store
                    // placeholder; the real state is only needed for restart
                    // persistence which we wire through the Arc in main.rs.
                    // For now we pass a dummy; the caller should use the Arc
                    // version from main.rs via start_process_arc.
                    Arc::new(tokio::sync::Mutex::new(
                        StateStore::in_memory().map_err(|e| e.to_string())?,
                    )),
                    self.paths.clone(),
                )
                // Note: cron_shutdown / memory_shutdown are NOT set here
                // because start_process (the non-Arc variant) is used without
                // a real state store.  Use start_process_arc for full wiring.
            };

            // Attach the watcher handle.
            {
                let mut procs = self.processes.write().await;
                if let Some(mp) = procs.get_mut(&key) {
                    mp.watcher = Some(watcher_handle);
                }
            }

            // Now we can use state (sync, no awaits after this)
            state.upsert_process(&info).map_err(|e| e.to_string())?;
            state
                .log_event(&config.name, "started", Some("process started"))
                .map_err(|e| e.to_string())?;

            started.push(info);
        }

        Ok(started)
    }

    /// Like `start_process` but takes an `Arc<Mutex<StateStore>>` so the exit
    /// watcher can persist state on auto-restart.
    pub async fn start_process_arc(
        &self,
        config: ProcessConfig,
        state: Arc<tokio::sync::Mutex<StateStore>>,
    ) -> Result<Vec<ProcessInfo>, String> {
        let instances = config.instances;
        let mut started: Vec<ProcessInfo> = Vec::new();

        for i in 0..instances {
            let key = Self::process_key(&config.name, i);

            // Check not already running.
            {
                let procs = self.processes.read().await;
                if let Some(mp) = procs.get(&key) {
                    if mp.info.status == ProcessStatus::Online
                        || mp.info.status == ProcessStatus::Starting
                    {
                        return Err(format!(
                            "Process '{}' instance {} is already running",
                            config.name, i
                        ));
                    }
                }
            }

            let (info, child) = spawn_child_static(&config, i, &self.paths).await?;

            if let Some(pid) = info.pid {
                let pid_path = self.paths.process_pid(&config.name, i);
                let _ = std::fs::write(&pid_path, pid.to_string());
            }

            let watcher_handle = {
                let mut procs = self.processes.write().await;
                let entry = procs
                    .entry(key.clone())
                    .and_modify(|mp| {
                        mp.info = info.clone();
                        mp.child = None;
                    })
                    .or_insert_with(|| ManagedProcess {
                        info: info.clone(),
                        child: None,
                        ring_out: RingBuffer::new(1000),
                        ring_err: RingBuffer::new(1000),
                        log_capture: LogCapture::new(64),
                        watcher: None,
                        cron_shutdown: None,
                        memory_shutdown: None,
                    });
                entry.child = Some(child);

                crate::watcher::ExitWatcher::spawn(
                    Arc::clone(&self.processes),
                    key.clone(),
                    config.clone(),
                    Arc::clone(&state),
                    self.paths.clone(),
                )
            };

            // --- attach watcher -------------------------------------------------
            {
                let mut procs = self.processes.write().await;
                if let Some(mp) = procs.get_mut(&key) {
                    mp.watcher = Some(watcher_handle);
                }
            }

            // --- cron scheduler (Task 10) ----------------------------------------
            if let Some(ref cron_expr) = config.cron_restart {
                let cron_tx = crate::cron_scheduler::CronScheduler::spawn(
                    cron_expr.clone(),
                    config.clone(),
                    Arc::clone(&self.processes),
                    Arc::clone(&state),
                    self.paths.clone(),
                );
                let mut procs = self.processes.write().await;
                if let Some(mp) = procs.get_mut(&key) {
                    mp.cron_shutdown = cron_tx;
                }
            }

            // --- memory monitor (Task 11) ----------------------------------------
            if let (Some(max_mb), Some(pid)) = (config.max_memory_mb, info.pid) {
                let mem_tx = crate::memory_monitor::MemoryMonitor::spawn(
                    pid,
                    max_mb * 1_048_576,
                    config.name.clone(),
                    std::time::Duration::from_secs(5),
                );
                let mut procs = self.processes.write().await;
                if let Some(mp) = procs.get_mut(&key) {
                    mp.memory_shutdown = Some(mem_tx);
                }
            }

            let state_guard = state.lock().await;
            state_guard.upsert_process(&info).map_err(|e| e.to_string())?;
            state_guard
                .log_event(&config.name, "started", Some("process started"))
                .map_err(|e| e.to_string())?;

            started.push(info);
        }

        Ok(started)
    }

    // -----------------------------------------------------------------------
    // Stop
    // -----------------------------------------------------------------------

    /// Stop all instances of process `name`.
    pub async fn stop_process(&self, name: &str, state: &StateStore) -> Result<(), String> {
        // Collect keys first (async read)
        let keys: Vec<String> = {
            let procs = self.processes.read().await;
            procs
                .keys()
                .filter(|k| k.starts_with(&format!("{}:", name)))
                .cloned()
                .collect()
        };

        if keys.is_empty() {
            return Err(format!("Process '{}' not found", name));
        }

        // Build list of updated infos (async write)
        let mut updated_infos: Vec<ProcessInfo> = Vec::new();
        {
            let mut procs = self.processes.write().await;
            for key in &keys {
                if let Some(mp) = procs.get_mut(key) {
                    // Cancel cron scheduler and memory monitor before stopping.
                    if let Some(tx) = mp.cron_shutdown.take() {
                        let _ = tx.send(());
                    }
                    if let Some(tx) = mp.memory_shutdown.take() {
                        let _ = tx.send(());
                    }

                    // Abort the watcher first so it does not try to restart
                    // the process after we kill it.
                    if let Some(handle) = mp.watcher.take() {
                        handle.abort();
                    }

                    // Mark as Stopping so the watcher (if it already took the
                    // child) can see we intended the stop.
                    mp.info = ProcessInfo {
                        status: ProcessStatus::Stopping,
                        ..mp.info.clone()
                    };

                    // Graceful shutdown: SIGTERM → grace period → SIGKILL.
                    if let Some(ref mut child) = mp.child {
                        let grace_ms = mp.info.config.grace_period_ms;
                        graceful_kill(child, grace_ms).await;
                    }

                    let updated = ProcessInfo {
                        status: ProcessStatus::Stopped,
                        pid: None,
                        uptime_started: None,
                        ..mp.info.clone()
                    };

                    let pid_path = self
                        .paths
                        .process_pid(&mp.info.config.name, mp.info.instance);
                    let _ = std::fs::remove_file(&pid_path);

                    updated_infos.push(updated.clone());
                    mp.info = updated;
                }
            }
        }
        // ^--- RwLock guard dropped

        // Sync DB operations — no awaits below
        for updated in &updated_infos {
            let _ = state.upsert_process(updated);
        }
        let _ = state.log_event(name, "stopped", Some("process stopped"));

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Restart
    // -----------------------------------------------------------------------

    /// Restart all instances of process `name`.
    pub async fn restart_process(&self, name: &str, state: &StateStore) -> Result<(), String> {
        // Get config (async read)
        let config = {
            let procs = self.processes.read().await;
            procs
                .iter()
                .find(|(k, _)| k.starts_with(&format!("{}:", name)))
                .map(|(_, mp)| mp.info.config.clone())
                .ok_or_else(|| format!("Process '{}' not found", name))?
        };

        self.stop_process(name, state).await?;
        self.start_process(config, state).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Delete
    // -----------------------------------------------------------------------

    /// Stop a process and remove it from the supervisor map and database.
    pub async fn delete_process(&self, name: &str, state: &StateStore) -> Result<(), String> {
        // Stop (ignore not-found — already stopped is fine)
        let _ = self.stop_process(name, state).await;

        // Remove from map (async write)
        {
            let mut procs = self.processes.write().await;
            procs.retain(|k, _| !k.starts_with(&format!("{}:", name)));
        }

        // Sync DB
        state.delete_process(name).map_err(|e| e.to_string())?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Scale
    // -----------------------------------------------------------------------

    /// Stop a process, adjust its instance count, and restart it.
    pub async fn scale_process(
        &self,
        name: &str,
        target: u32,
        state: &StateStore,
    ) -> Result<Vec<ProcessInfo>, String> {
        let config = {
            let procs = self.processes.read().await;
            procs
                .iter()
                .find(|(k, _)| k.starts_with(&format!("{}:", name)))
                .map(|(_, mp)| mp.info.config.clone())
                .ok_or_else(|| format!("Process '{}' not found", name))?
        };

        self.stop_process(name, state).await?;

        {
            let mut procs = self.processes.write().await;
            procs.retain(|k, _| !k.starts_with(&format!("{}:", name)));
        }

        let new_config = ProcessConfig {
            instances: target,
            ..config
        };

        self.start_process(new_config, state).await
    }

    // -----------------------------------------------------------------------
    // Stop all
    // -----------------------------------------------------------------------

    /// Stop every managed process.
    pub async fn stop_all(&self, state: &StateStore) {
        let names: Vec<String> = {
            let procs = self.processes.read().await;
            procs
                .values()
                .map(|mp| mp.info.config.name.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect()
        };

        for name in names {
            let _ = self.stop_process(&name, state).await;
        }
    }

    // -----------------------------------------------------------------------
    // Group start / stop (Task 8)
    // -----------------------------------------------------------------------

    /// Start all processes for `group_name` (and its transitive dependencies)
    /// in dependency order.
    pub async fn start_group(
        &self,
        group_name: &str,
        groups: &HashMap<String, GroupConfig>,
        all_configs: &HashMap<String, ProcessConfig>,
        state: &StateStore,
    ) -> Result<Vec<ProcessInfo>, String> {
        let ordered = mhost_core::group::ordered_processes_for_group(group_name, groups)
            .map_err(|e| e.to_string())?;

        let mut results: Vec<ProcessInfo> = Vec::new();
        for proc_name in &ordered {
            if let Some(config) = all_configs.get(proc_name) {
                match self.start_process(config.clone(), state).await {
                    Ok(infos) => results.extend(infos),
                    Err(e) => {
                        tracing::error!(process = %proc_name, error = %e, "failed to start process in group")
                    }
                }
            }
        }
        Ok(results)
    }

    /// Stop all processes for `group_name` in reverse dependency order.
    pub async fn stop_group(
        &self,
        group_name: &str,
        groups: &HashMap<String, GroupConfig>,
        state: &StateStore,
    ) -> Result<(), String> {
        let ordered = mhost_core::group::ordered_processes_for_group(group_name, groups)
            .map_err(|e| e.to_string())?;

        // Stop in REVERSE order so dependents shut down before dependencies.
        for proc_name in ordered.iter().rev() {
            let _ = self.stop_process(proc_name, state).await;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // List / get
    // -----------------------------------------------------------------------

    /// Return the `ProcessInfo` for every managed process.
    pub async fn list_processes(&self) -> Vec<ProcessInfo> {
        let procs = self.processes.read().await;
        procs.values().map(|mp| mp.info.clone()).collect()
    }

    /// Return all `ProcessInfo` records whose name matches `name`.
    pub async fn get_process(&self, name: &str) -> Vec<ProcessInfo> {
        let procs = self.processes.read().await;
        procs
            .iter()
            .filter(|(k, _)| k.starts_with(&format!("{}:", name)))
            .map(|(_, mp)| mp.info.clone())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Logs
    // -----------------------------------------------------------------------

    /// Return the last `n` stdout lines for all instances of `name`.
    pub async fn get_log_lines(&self, name: &str, n: usize) -> Vec<String> {
        let procs = self.processes.read().await;
        let mut lines = Vec::new();
        for (key, mp) in procs.iter() {
            if key.starts_with(&format!("{}:", name)) {
                for line in mp.ring_out.last_n(n) {
                    lines.push(line.to_string());
                }
            }
        }
        lines
    }
}

// ---------------------------------------------------------------------------
// spawn_child_static (public for watcher.rs)
// ---------------------------------------------------------------------------

/// Spawn a single child process for `instance`.  Returns the `ProcessInfo`
/// (with status set to `Starting` when a health check is configured, or
/// `Online` otherwise) and the raw `Child` handle.
pub async fn spawn_child_static(
    config: &ProcessConfig,
    instance: u32,
    paths: &MhostPaths,
) -> Result<(ProcessInfo, Child), String> {
    let (program, mut cmd_args): (String, Vec<String>) =
        if let Some(ref interp) = config.interpreter {
            (interp.clone(), vec![config.command.clone()])
        } else {
            let mut parts = config.command.split_whitespace();
            let prog = parts
                .next()
                .ok_or_else(|| "Empty command".to_string())?
                .to_string();
            let rest: Vec<String> = parts.map(|s| s.to_string()).collect();
            (prog, rest)
        };

    cmd_args.extend_from_slice(&config.args);

    let mut cmd = Command::new(&program);
    cmd.args(&cmd_args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if let Some(ref cwd) = config.cwd {
        cmd.current_dir(cwd);
    }

    for (k, v) in &config.env {
        cmd.env(k, v);
    }

    let child = cmd.spawn().map_err(|e| format!("spawn failed: {}", e))?;
    let pid = child.id();

    // Task 7: set to Starting when a health check is configured; the health
    // runner transitions to Online once the check passes.  If no health config
    // is present, set directly to Online.
    let initial_status = if config.health_config.is_some() {
        ProcessStatus::Starting
    } else {
        ProcessStatus::Online
    };

    let mut info = ProcessInfo::new(config.clone(), instance);
    info.status = initial_status;
    info.pid = pid;
    info.uptime_started = Some(Utc::now());

    // Suppress unused-variable warning for `paths` on platforms where we
    // don't use it inside this function.
    let _ = paths;

    Ok((info, child))
}

// ---------------------------------------------------------------------------
// Graceful kill helper (Task 7)
// ---------------------------------------------------------------------------

/// Send SIGTERM, wait up to `grace_period_ms`, then SIGKILL if still alive.
async fn graceful_kill(child: &mut Child, grace_period_ms: u64) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill as nix_kill, Signal};
        use nix::unistd::Pid;

        if let Some(pid) = child.id() {
            let _ = nix_kill(Pid::from_raw(pid as i32), Signal::SIGTERM);

            let grace = tokio::time::Duration::from_millis(grace_period_ms);
            match tokio::time::timeout(grace, child.wait()).await {
                Ok(_) => return, // exited cleanly within the grace period
                Err(_) => {
                    tracing::debug!(pid = pid, "grace period expired, sending SIGKILL");
                    let _ = child.kill().await;
                }
            }
        } else {
            let _ = child.kill().await;
        }
    }

    #[cfg(not(unix))]
    {
        let _ = grace_period_ms; // suppress unused warning on Windows
        let _ = child.kill().await;
    }
}

// ---------------------------------------------------------------------------
// Backoff helper
// ---------------------------------------------------------------------------

/// Exponential back-off: `base_ms * 2^attempt`, capped at `max_ms`.
pub fn backoff_delay(attempt: u32, base_ms: u64, max_ms: u64) -> u64 {
    let shift = attempt.min(62);
    let delay = base_ms.saturating_mul(1u64 << shift);
    delay.min(max_ms)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_delay_exponential() {
        assert_eq!(backoff_delay(0, 100, 10_000), 100);
        assert_eq!(backoff_delay(1, 100, 10_000), 200);
        assert_eq!(backoff_delay(2, 100, 10_000), 400);
        assert_eq!(backoff_delay(3, 100, 10_000), 800);
    }

    #[test]
    fn backoff_delay_capped() {
        assert_eq!(backoff_delay(10, 100, 5_000), 5_000);
        assert_eq!(backoff_delay(20, 100, 5_000), 5_000);
    }

    #[test]
    fn process_key_format() {
        assert_eq!(Supervisor::process_key("api", 0), "api:0");
        assert_eq!(Supervisor::process_key("worker", 3), "worker:3");
    }

    #[tokio::test]
    async fn start_and_list() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = MhostPaths::with_root(tmp.path().to_path_buf());
        paths.ensure_dirs().unwrap();

        let state = StateStore::open(&paths.db()).unwrap();
        let supervisor = Supervisor::new(paths);

        let config = ProcessConfig {
            name: "sleeper".to_string(),
            command: "sleep".to_string(),
            args: vec!["10".to_string()],
            instances: 2,
            ..Default::default()
        };

        let infos = supervisor.start_process(config, &state).await.unwrap();
        assert_eq!(infos.len(), 2);
        // No health config → status is Online immediately.
        assert!(infos.iter().all(|i| i.status == ProcessStatus::Online));

        let listed = supervisor.list_processes().await;
        assert_eq!(listed.len(), 2);
    }

    #[tokio::test]
    async fn stop_nonexistent() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = MhostPaths::with_root(tmp.path().to_path_buf());
        paths.ensure_dirs().unwrap();

        let state = StateStore::open(&paths.db()).unwrap();
        let supervisor = Supervisor::new(paths);

        let result = supervisor.stop_process("ghost", &state).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn start_group_respects_order() {
        use mhost_core::group::GroupConfig;

        let tmp = tempfile::tempdir().unwrap();
        let paths = MhostPaths::with_root(tmp.path().to_path_buf());
        paths.ensure_dirs().unwrap();

        let state = StateStore::open(&paths.db()).unwrap();
        let supervisor = Supervisor::new(paths);

        let mut groups: HashMap<String, GroupConfig> = HashMap::new();
        groups.insert(
            "infra".to_string(),
            GroupConfig {
                depends_on: vec![],
                processes: vec!["db".to_string()],
            },
        );
        groups.insert(
            "app".to_string(),
            GroupConfig {
                depends_on: vec!["infra".to_string()],
                processes: vec!["api".to_string()],
            },
        );

        let mut all_configs: HashMap<String, ProcessConfig> = HashMap::new();
        all_configs.insert(
            "db".to_string(),
            ProcessConfig {
                name: "db".to_string(),
                command: "sleep".to_string(),
                args: vec!["30".to_string()],
                instances: 1,
                ..Default::default()
            },
        );
        all_configs.insert(
            "api".to_string(),
            ProcessConfig {
                name: "api".to_string(),
                command: "sleep".to_string(),
                args: vec!["30".to_string()],
                instances: 1,
                ..Default::default()
            },
        );

        let infos = supervisor
            .start_group("app", &groups, &all_configs, &state)
            .await
            .unwrap();

        assert_eq!(infos.len(), 2);
        let names: Vec<&str> = infos.iter().map(|i| i.config.name.as_str()).collect();
        // "db" (dependency) must be started before "api"
        let db_pos = names.iter().position(|&n| n == "db").unwrap();
        let api_pos = names.iter().position(|&n| n == "api").unwrap();
        assert!(db_pos < api_pos, "db must start before api");
    }

    #[tokio::test]
    async fn starting_status_when_health_config_present() {
        use mhost_core::health::{HealthCheckKind, HealthConfig};

        let tmp = tempfile::tempdir().unwrap();
        let paths = MhostPaths::with_root(tmp.path().to_path_buf());

        let config = ProcessConfig {
            name: "with-health".to_string(),
            command: "sleep".to_string(),
            args: vec!["30".to_string()],
            health_config: Some(HealthConfig {
                kind: HealthCheckKind::Http {
                    url: "http://localhost:3000/health".to_string(),
                    expected_status: 200,
                },
                interval_ms: 1000,
                timeout_ms: 500,
                retries: 3,
            }),
            ..Default::default()
        };

        let (info, child) = spawn_child_static(&config, 0, &paths).await.unwrap();
        // Status must be Starting (not Online) when health config is present.
        assert_eq!(info.status, ProcessStatus::Starting);
        drop(child);
    }

    #[tokio::test]
    async fn online_status_without_health_config() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = MhostPaths::with_root(tmp.path().to_path_buf());

        let config = ProcessConfig {
            name: "no-health".to_string(),
            command: "sleep".to_string(),
            args: vec!["30".to_string()],
            ..Default::default()
        };

        let (info, child) = spawn_child_static(&config, 0, &paths).await.unwrap();
        assert_eq!(info.status, ProcessStatus::Online);
        drop(child);
    }
}
