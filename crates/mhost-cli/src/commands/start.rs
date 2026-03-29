use std::path::Path;

use mhost_config::EcosystemConfig;
use mhost_core::process::ProcessConfig;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{print_error, print_success};

/// Start a process or ecosystem config.
///
/// `target` may be:
/// - A path ending in `.toml`, `.yaml`, `.yml`, or `.json` — loaded as an
///   ecosystem config and every app in it is started.
/// - Any other string — treated as a bare command and started under `name`
///   (or `target` itself when no name is given).
pub async fn run(
    client: &IpcClient,
    target: &str,
    name: Option<&str>,
) -> Result<(), String> {
    let configs = build_configs(target, name)?;

    for cfg in &configs {
        let params = json!({ "config": cfg });
        let resp = client
            .call(methods::PROCESS_START, params)
            .await
            .map_err(|e| format!("IPC error: {e}"))?;

        if let Some(err) = resp.error {
            print_error(&format!("Failed to start '{}': {}", cfg.name, err.message));
        } else {
            print_success(&format!("Started '{}'", cfg.name));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn build_configs(target: &str, name: Option<&str>) -> Result<Vec<ProcessConfig>, String> {
    if is_config_file(target) {
        let eco = EcosystemConfig::from_file(Path::new(target))
            .map_err(|e| format!("Failed to parse ecosystem config '{}': {e}", target))?;
        Ok(eco.to_process_configs())
    } else {
        // Treat target as command, split on whitespace for simplicity.
        let mut parts = target.split_whitespace();
        let command = parts
            .next()
            .ok_or("Empty command")?
            .to_string();
        let args: Vec<String> = parts.map(String::from).collect();
        let cfg_name = name.unwrap_or(target).to_string();

        Ok(vec![ProcessConfig {
            name: cfg_name,
            command,
            args,
            ..Default::default()
        }])
    }
}

fn is_config_file(target: &str) -> bool {
    let lower = target.to_lowercase();
    lower.ends_with(".toml")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
        || lower.ends_with(".json")
}
