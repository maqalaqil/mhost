#![cfg(feature = "api")]

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use mhost_core::paths::MhostPaths;
use mhost_core::process::ProcessConfig;

use crate::state::StateStore;
use crate::supervisor::Supervisor;

/// Bridges the daemon's [`Supervisor`] to the [`mhost_api::server::SupervisorApi`]
/// trait so that the HTTP/WebSocket API can interact with managed processes.
pub struct SupervisorApiAdapter {
    supervisor: Arc<Supervisor>,
    state: Arc<Mutex<StateStore>>,
    paths: MhostPaths,
}

impl SupervisorApiAdapter {
    pub fn new(
        supervisor: Arc<Supervisor>,
        state: Arc<Mutex<StateStore>>,
        paths: MhostPaths,
    ) -> Self {
        Self {
            supervisor,
            state,
            paths,
        }
    }
}

#[async_trait]
impl mhost_api::server::SupervisorApi for SupervisorApiAdapter {
    async fn list_processes(&self) -> Vec<serde_json::Value> {
        let infos = self.supervisor.list_processes().await;
        infos
            .into_iter()
            .filter_map(|info| serde_json::to_value(&info).ok())
            .collect()
    }

    async fn get_process(&self, name: &str) -> Option<serde_json::Value> {
        let infos = self.supervisor.get_process(name).await;
        if infos.is_empty() {
            None
        } else {
            serde_json::to_value(&infos).ok()
        }
    }

    async fn start_process(&self, config: serde_json::Value) -> Result<serde_json::Value, String> {
        let process_config: ProcessConfig =
            serde_json::from_value(config).map_err(|e| format!("invalid config: {e}"))?;

        let infos = self
            .supervisor
            .start_process_arc(process_config, Arc::clone(&self.state))
            .await?;

        serde_json::to_value(&infos).map_err(|e| format!("serialization error: {e}"))
    }

    async fn stop_process(&self, name: &str) -> Result<(), String> {
        let state_guard = self.state.lock().await;
        self.supervisor.stop_process(name, &state_guard).await
    }

    async fn restart_process(&self, name: &str) -> Result<(), String> {
        let state_guard = self.state.lock().await;
        self.supervisor.restart_process(name, &state_guard).await
    }

    async fn reload_process(&self, _name: &str) -> Result<(), String> {
        // Reload is treated as a restart for now.
        Err("reload not yet implemented; use restart".to_string())
    }

    async fn delete_process(&self, name: &str) -> Result<(), String> {
        let state_guard = self.state.lock().await;
        self.supervisor.delete_process(name, &state_guard).await
    }

    async fn scale_process(&self, name: &str, instances: u32) -> Result<(), String> {
        let state_guard = self.state.lock().await;
        self.supervisor
            .scale_process(name, instances, &state_guard)
            .await?;
        Ok(())
    }

    async fn stop_all(&self) -> Result<(), String> {
        let state_guard = self.state.lock().await;
        self.supervisor.stop_all(&state_guard).await;
        Ok(())
    }

    async fn restart_all(&self) -> Result<(), String> {
        // Collect all unique process names, then restart each.
        let infos = self.supervisor.list_processes().await;
        let names: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            infos
                .into_iter()
                .filter_map(|i| {
                    let name = i.config.name.clone();
                    if seen.insert(name.clone()) {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect()
        };

        let state_guard = self.state.lock().await;
        for name in &names {
            self.supervisor.restart_process(name, &state_guard).await?;
        }
        Ok(())
    }

    async fn save(&self) -> Result<(), String> {
        // Persist current process state to the database (already auto-persisted).
        Ok(())
    }

    async fn resurrect(&self) -> Result<serde_json::Value, String> {
        // Stub: no resurrection logic wired yet.
        Ok(serde_json::json!({ "resurrected": [] }))
    }

    async fn health_status(&self, name: &str) -> Result<serde_json::Value, String> {
        let infos = self.supervisor.get_process(name).await;
        if infos.is_empty() {
            return Err(format!("process '{name}' not found"));
        }
        let statuses: Vec<serde_json::Value> = infos
            .iter()
            .map(|i| {
                serde_json::json!({
                    "name": i.config.name,
                    "instance": i.instance,
                    "status": format!("{:?}", i.status),
                })
            })
            .collect();
        Ok(serde_json::json!({ "health": statuses }))
    }

    async fn metrics(&self, name: &str) -> Result<serde_json::Value, String> {
        let infos = self.supervisor.get_process(name).await;
        if infos.is_empty() {
            return Err(format!("process '{name}' not found"));
        }
        let items: Vec<serde_json::Value> = infos
            .iter()
            .map(|i| {
                serde_json::json!({
                    "name": i.config.name,
                    "instance": i.instance,
                    "pid": i.pid,
                    "status": format!("{:?}", i.status),
                    "restarts": i.restart_count,
                    "uptime_started": i.uptime_started,
                })
            })
            .collect();
        Ok(serde_json::json!({ "metrics": items }))
    }

    async fn all_metrics(&self) -> Result<serde_json::Value, String> {
        let infos = self.supervisor.list_processes().await;
        let items: Vec<serde_json::Value> = infos
            .iter()
            .map(|i| {
                serde_json::json!({
                    "name": i.config.name,
                    "instance": i.instance,
                    "pid": i.pid,
                    "status": format!("{:?}", i.status),
                    "restarts": i.restart_count,
                    "uptime_started": i.uptime_started,
                })
            })
            .collect();
        Ok(serde_json::json!({ "metrics": items }))
    }

    async fn get_logs(&self, name: &str, lines: usize, err: bool) -> Result<Vec<String>, String> {
        // Read from log files on disk.
        let log_path = if err {
            self.paths.process_err_log(name, 0)
        } else {
            self.paths.process_out_log(name, 0)
        };

        let content = std::fs::read_to_string(&log_path)
            .map_err(|e| format!("failed to read log file {}: {e}", log_path.display()))?;

        let all_lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let start = all_lines.len().saturating_sub(lines);
        Ok(all_lines[start..].to_vec())
    }

    async fn search_logs(
        &self,
        name: &str,
        query: &str,
        _since: Option<&str>,
    ) -> Result<Vec<String>, String> {
        // Simple grep-like search over stdout log.
        let log_path = self.paths.process_out_log(name, 0);
        let content = std::fs::read_to_string(&log_path)
            .map_err(|e| format!("failed to read log file {}: {e}", log_path.display()))?;

        let matching: Vec<String> = content
            .lines()
            .filter(|line| line.contains(query))
            .map(|l| l.to_string())
            .collect();
        Ok(matching)
    }

    fn version_info(&self) -> serde_json::Value {
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        })
    }
}
