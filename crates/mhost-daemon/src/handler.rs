use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::Mutex;

use mhost_core::process::ProcessConfig;
use mhost_core::protocol::{error_codes, methods, RpcError, RpcRequest, RpcResponse};

use crate::state::StateStore;
use crate::supervisor::Supervisor;

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub struct Handler {
    pub supervisor: Arc<Supervisor>,
    pub state: Arc<Mutex<StateStore>>,
}

impl Handler {
    pub fn new(supervisor: Arc<Supervisor>, state: Arc<Mutex<StateStore>>) -> Self {
        Self { supervisor, state }
    }

    /// Dispatch an RPC request and return a response.
    ///
    /// Returns `(RpcResponse, bool)` where the bool is `true` when the daemon
    /// should shut down after sending the response (`daemon.kill`).
    pub async fn dispatch(&self, req: RpcRequest) -> (RpcResponse, bool) {
        let id = req.id;
        match req.method.as_str() {
            // ----------------------------------------------------------------
            // daemon.ping
            // ----------------------------------------------------------------
            methods::DAEMON_PING => {
                let resp = RpcResponse::success(
                    id,
                    json!({
                        "pong": true,
                        "version": env!("CARGO_PKG_VERSION"),
                    }),
                );
                (resp, false)
            }

            // ----------------------------------------------------------------
            // daemon.version
            // ----------------------------------------------------------------
            methods::DAEMON_VERSION => {
                let resp = RpcResponse::success(
                    id,
                    json!({
                        "version": env!("CARGO_PKG_VERSION"),
                        "target": std::env::consts::ARCH,
                        "os": std::env::consts::OS,
                    }),
                );
                (resp, false)
            }

            // ----------------------------------------------------------------
            // process.start
            // ----------------------------------------------------------------
            methods::PROCESS_START => {
                let config: ProcessConfig = match serde_json::from_value(req.params.clone()) {
                    Ok(c) => c,
                    Err(e) => {
                        return (
                            RpcResponse::error(
                                id,
                                RpcError::new(error_codes::INVALID_CONFIG, e.to_string()),
                            ),
                            false,
                        );
                    }
                };

                // Lock, call start (which internally locks state only briefly), unlock.
                let state_guard = self.state.lock().await;
                let result = self.supervisor.start_process(config, &state_guard).await;
                drop(state_guard);

                match result {
                    Ok(infos) => {
                        let list: Vec<Value> = infos
                            .iter()
                            .map(|i| serde_json::to_value(i).unwrap_or(Value::Null))
                            .collect();
                        (RpcResponse::success(id, json!({ "processes": list })), false)
                    }
                    Err(e) => (
                        RpcResponse::error(id, RpcError::new(error_codes::SPAWN_FAILED, e)),
                        false,
                    ),
                }
            }

            // ----------------------------------------------------------------
            // process.stop
            // ----------------------------------------------------------------
            methods::PROCESS_STOP => {
                let name = match string_param(&req.params, "name") {
                    Ok(n) => n,
                    Err(e) => return (e, false),
                };

                let state_guard = self.state.lock().await;
                let result = if name == "all" {
                    self.supervisor.stop_all(&state_guard).await;
                    Ok(())
                } else {
                    self.supervisor.stop_process(&name, &state_guard).await
                };
                drop(state_guard);

                match result {
                    Ok(_) => (
                        RpcResponse::success(id, json!({ "stopped": name })),
                        false,
                    ),
                    Err(e) => (
                        RpcResponse::error(id, RpcError::new(error_codes::PROCESS_NOT_FOUND, e)),
                        false,
                    ),
                }
            }

            // ----------------------------------------------------------------
            // process.restart
            // ----------------------------------------------------------------
            methods::PROCESS_RESTART => {
                let name = match string_param(&req.params, "name") {
                    Ok(n) => n,
                    Err(e) => return (e, false),
                };

                let state_guard = self.state.lock().await;
                let result = self.supervisor.restart_process(&name, &state_guard).await;
                drop(state_guard);

                match result {
                    Ok(_) => (
                        RpcResponse::success(id, json!({ "restarted": name })),
                        false,
                    ),
                    Err(e) => (
                        RpcResponse::error(id, RpcError::new(error_codes::PROCESS_NOT_FOUND, e)),
                        false,
                    ),
                }
            }

            // ----------------------------------------------------------------
            // process.delete
            // ----------------------------------------------------------------
            methods::PROCESS_DELETE => {
                let name = match string_param(&req.params, "name") {
                    Ok(n) => n,
                    Err(e) => return (e, false),
                };

                let state_guard = self.state.lock().await;
                let result = self.supervisor.delete_process(&name, &state_guard).await;
                drop(state_guard);

                match result {
                    Ok(_) => (
                        RpcResponse::success(id, json!({ "deleted": name })),
                        false,
                    ),
                    Err(e) => (
                        RpcResponse::error(id, RpcError::new(error_codes::PROCESS_NOT_FOUND, e)),
                        false,
                    ),
                }
            }

            // ----------------------------------------------------------------
            // process.list
            // ----------------------------------------------------------------
            methods::PROCESS_LIST => {
                let infos = self.supervisor.list_processes().await;
                let list: Vec<Value> = infos
                    .iter()
                    .map(|i| serde_json::to_value(i).unwrap_or(Value::Null))
                    .collect();
                (RpcResponse::success(id, json!({ "processes": list })), false)
            }

            // ----------------------------------------------------------------
            // process.info
            // ----------------------------------------------------------------
            methods::PROCESS_INFO => {
                let name = match string_param(&req.params, "name") {
                    Ok(n) => n,
                    Err(e) => return (e, false),
                };

                let infos = self.supervisor.get_process(&name).await;
                if infos.is_empty() {
                    return (
                        RpcResponse::error(
                            id,
                            RpcError::new(
                                error_codes::PROCESS_NOT_FOUND,
                                format!("Process '{}' not found", name),
                            ),
                        ),
                        false,
                    );
                }
                let list: Vec<Value> = infos
                    .iter()
                    .map(|i| serde_json::to_value(i).unwrap_or(Value::Null))
                    .collect();
                (RpcResponse::success(id, json!({ "processes": list })), false)
            }

            // ----------------------------------------------------------------
            // process.scale
            // ----------------------------------------------------------------
            methods::PROCESS_SCALE => {
                let name = match string_param(&req.params, "name") {
                    Ok(n) => n,
                    Err(e) => return (e, false),
                };
                let instances = match req.params.get("instances").and_then(Value::as_u64) {
                    Some(n) => n as u32,
                    None => {
                        return (
                            RpcResponse::error(
                                id,
                                RpcError::new(
                                    error_codes::INVALID_CONFIG,
                                    "Missing 'instances' parameter",
                                ),
                            ),
                            false,
                        );
                    }
                };

                let state_guard = self.state.lock().await;
                let result = self
                    .supervisor
                    .scale_process(&name, instances, &state_guard)
                    .await;
                drop(state_guard);

                match result {
                    Ok(infos) => {
                        let list: Vec<Value> = infos
                            .iter()
                            .map(|i| serde_json::to_value(i).unwrap_or(Value::Null))
                            .collect();
                        (RpcResponse::success(id, json!({ "processes": list })), false)
                    }
                    Err(e) => (
                        RpcResponse::error(id, RpcError::new(error_codes::PROCESS_NOT_FOUND, e)),
                        false,
                    ),
                }
            }

            // ----------------------------------------------------------------
            // process.save
            // ----------------------------------------------------------------
            methods::PROCESS_SAVE => {
                let infos = self.supervisor.list_processes().await;
                let configs: Vec<&ProcessConfig> = infos.iter().map(|i| &i.config).collect();
                let dump_path = std::path::PathBuf::from("dump.json");
                match serde_json::to_string_pretty(&configs) {
                    Ok(json_str) => match std::fs::write(&dump_path, json_str) {
                        Ok(_) => (
                            RpcResponse::success(
                                id,
                                json!({ "saved": dump_path.display().to_string() }),
                            ),
                            false,
                        ),
                        Err(e) => (
                            RpcResponse::error(
                                id,
                                RpcError::new(error_codes::INTERNAL_ERROR, e.to_string()),
                            ),
                            false,
                        ),
                    },
                    Err(e) => (
                        RpcResponse::error(
                            id,
                            RpcError::new(error_codes::INTERNAL_ERROR, e.to_string()),
                        ),
                        false,
                    ),
                }
            }

            // ----------------------------------------------------------------
            // process.resurrect
            // ----------------------------------------------------------------
            methods::PROCESS_RESURRECT => {
                let dump_path = std::path::PathBuf::from("dump.json");
                let contents = match std::fs::read_to_string(&dump_path) {
                    Ok(c) => c,
                    Err(e) => {
                        return (
                            RpcResponse::error(
                                id,
                                RpcError::new(error_codes::INTERNAL_ERROR, e.to_string()),
                            ),
                            false,
                        );
                    }
                };

                let configs: Vec<ProcessConfig> = match serde_json::from_str(&contents) {
                    Ok(c) => c,
                    Err(e) => {
                        return (
                            RpcResponse::error(
                                id,
                                RpcError::new(error_codes::INVALID_CONFIG, e.to_string()),
                            ),
                            false,
                        );
                    }
                };

                let state_guard = self.state.lock().await;
                let mut all_infos: Vec<Value> = Vec::new();
                for config in configs {
                    match self.supervisor.start_process(config, &state_guard).await {
                        Ok(infos) => {
                            for info in infos {
                                all_infos
                                    .push(serde_json::to_value(&info).unwrap_or(Value::Null));
                            }
                        }
                        Err(e) => {
                            tracing::warn!("resurrect: failed to start process: {}", e);
                        }
                    }
                }
                drop(state_guard);

                (
                    RpcResponse::success(id, json!({ "resurrected": all_infos })),
                    false,
                )
            }

            // ----------------------------------------------------------------
            // health.status
            // ----------------------------------------------------------------
            methods::HEALTH_STATUS => {
                let name = match string_param(&req.params, "name") {
                    Ok(n) => n,
                    Err(e) => return (e, false),
                };

                let infos = self.supervisor.get_process(&name).await;
                if infos.is_empty() {
                    return (
                        RpcResponse::error(
                            id,
                            RpcError::new(
                                error_codes::PROCESS_NOT_FOUND,
                                format!("Process '{}' not found", name),
                            ),
                        ),
                        false,
                    );
                }
                let list: Vec<Value> = infos
                    .iter()
                    .map(|i| {
                        json!({
                            "id": i.id,
                            "name": i.config.name,
                            "instance": i.instance,
                            "health_status": serde_json::to_value(&i.health_status)
                                .unwrap_or(Value::Null),
                        })
                    })
                    .collect();
                (RpcResponse::success(id, json!({ "health": list })), false)
            }

            // ----------------------------------------------------------------
            // group.start
            // ----------------------------------------------------------------
            methods::GROUP_START => {
                let group = match string_param(&req.params, "group") {
                    Ok(g) => g,
                    Err(e) => return (e, false),
                };
                (
                    RpcResponse::success(id, json!({ "started_group": group })),
                    false,
                )
            }

            // ----------------------------------------------------------------
            // group.stop
            // ----------------------------------------------------------------
            methods::GROUP_STOP => {
                let group = match string_param(&req.params, "group") {
                    Ok(g) => g,
                    Err(e) => return (e, false),
                };
                (
                    RpcResponse::success(id, json!({ "stopped_group": group })),
                    false,
                )
            }

            // ----------------------------------------------------------------
            // group.list
            // ----------------------------------------------------------------
            methods::GROUP_LIST => {
                (RpcResponse::success(id, json!({ "groups": [] })), false)
            }

            // ----------------------------------------------------------------
            // process.cluster  (alias for process.scale)
            // ----------------------------------------------------------------
            methods::PROCESS_CLUSTER => {
                let name = match string_param(&req.params, "name") {
                    Ok(n) => n,
                    Err(e) => return (e, false),
                };
                let instances = match req.params.get("instances").and_then(Value::as_u64) {
                    Some(n) => n as u32,
                    None => {
                        return (
                            RpcResponse::error(
                                id,
                                RpcError::new(
                                    error_codes::INVALID_CONFIG,
                                    "Missing 'instances' parameter",
                                ),
                            ),
                            false,
                        );
                    }
                };

                let state_guard = self.state.lock().await;
                let result = self
                    .supervisor
                    .scale_process(&name, instances, &state_guard)
                    .await;
                drop(state_guard);

                match result {
                    Ok(infos) => {
                        let list: Vec<Value> = infos
                            .iter()
                            .map(|i| serde_json::to_value(i).unwrap_or(Value::Null))
                            .collect();
                        (RpcResponse::success(id, json!({ "processes": list })), false)
                    }
                    Err(e) => (
                        RpcResponse::error(id, RpcError::new(error_codes::PROCESS_NOT_FOUND, e)),
                        false,
                    ),
                }
            }

            // ----------------------------------------------------------------
            // daemon.kill
            // ----------------------------------------------------------------
            methods::DAEMON_KILL => {
                let resp = RpcResponse::success(id, json!({ "shutting_down": true }));
                (resp, true)
            }

            // ----------------------------------------------------------------
            // Unknown method
            // ----------------------------------------------------------------
            _ => {
                let resp = RpcResponse::error(
                    id,
                    RpcError::new(-32601, format!("Method not found: {}", req.method)),
                );
                (resp, false)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Parameter helpers
// ---------------------------------------------------------------------------

fn string_param(params: &Value, key: &str) -> Result<String, RpcResponse> {
    match params.get(key).and_then(Value::as_str) {
        Some(s) => Ok(s.to_string()),
        None => Err(RpcResponse::error(
            0,
            RpcError::new(
                error_codes::INVALID_CONFIG,
                format!("Missing or invalid '{}' parameter", key),
            ),
        )),
    }
}
