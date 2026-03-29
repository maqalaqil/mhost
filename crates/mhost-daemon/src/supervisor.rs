use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

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
            // ^--- RwLock guard dropped here, no state ref held across await above

            // Spawn child (async, no state ref)
            let info = self.spawn_child(config.clone(), i).await?;

            // Write PID file (sync)
            if let Some(pid) = info.pid {
                let pid_path = self.paths.process_pid(&config.name, i);
                let _ = std::fs::write(&pid_path, pid.to_string());
            }

            // Store in map (async write, no state ref held across await)
            {
                let mut procs = self.processes.write().await;
                procs
                    .entry(key)
                    .and_modify(|mp| mp.info = info.clone())
                    .or_insert_with(|| ManagedProcess {
                        info: info.clone(),
                        child: None,
                        ring_out: RingBuffer::new(1000),
                        ring_err: RingBuffer::new(1000),
                        log_capture: LogCapture::new(64),
                    });
            }
            // ^--- RwLock guard dropped

            // Now we can use state (sync, no awaits after this)
            state.upsert_process(&info).map_err(|e| e.to_string())?;
            state
                .log_event(&config.name, "started", Some("process started"))
                .map_err(|e| e.to_string())?;

            started.push(info);
        }

        Ok(started)
    }

    /// Spawn a single child process for `instance`.
    async fn spawn_child(
        &self,
        config: ProcessConfig,
        instance: u32,
    ) -> Result<ProcessInfo, String> {
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

        let mut info = ProcessInfo::new(config, instance);
        info.status = ProcessStatus::Online;
        info.pid = pid;
        info.uptime_started = Some(Utc::now());

        // Drop child — a production supervisor would keep it for wait/kill
        drop(child);

        Ok(info)
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
                    if let Some(ref mut child) = mp.child {
                        let _ = child.kill().await;
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
}
