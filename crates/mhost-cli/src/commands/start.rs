use std::path::Path;

use mhost_config::EcosystemConfig;
use mhost_core::process::ProcessConfig;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

use crate::output::{print_error, print_success};

/// Start a process or ecosystem config.
///
/// `target` may be:
/// - A path ending in `.toml`, `.yaml`, `.yml`, or `.json` — loaded as an
///   ecosystem config and every app in it is started.
/// - Any other string — treated as a bare command and started under `name`
///   (or `target` itself when no name is given).
pub async fn run(client: &IpcClient, target: &str, name: Option<&str>) -> Result<(), String> {
    let configs = build_configs(target, name)?;

    for cfg in &configs {
        let params = serde_json::to_value(cfg).map_err(|e| format!("Serialize error: {e}"))?;
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
    let caller_cwd = std::env::current_dir().ok();

    if is_config_file(target) {
        // Resolve config file path relative to caller's CWD
        let config_path = resolve_path(target, caller_cwd.as_deref());
        let eco = EcosystemConfig::from_file(&config_path)
            .map_err(|e| format!("Failed to parse ecosystem config '{target}': {e}"))?;
        let mut configs = eco.to_process_configs();
        // Set CWD on configs that don't have one — relative to config file's parent
        let config_dir = config_path
            .parent()
            .map(|p| p.to_string_lossy().to_string());
        for cfg in &mut configs {
            if cfg.cwd.is_none() {
                cfg.cwd = config_dir.clone();
            } else if let Some(ref cwd) = cfg.cwd {
                // Resolve relative CWD against config file location
                let cwd_path = Path::new(cwd);
                if cwd_path.is_relative() {
                    if let Some(ref dir) = config_dir {
                        let resolved = Path::new(dir).join(cwd_path);
                        cfg.cwd = Some(resolved.to_string_lossy().to_string());
                    }
                }
            }
        }
        Ok(configs)
    } else {
        // Treat target as command, split on whitespace for simplicity.
        let mut parts = target.split_whitespace();
        let first = parts.next().ok_or("Empty command")?.to_string();
        let rest_args: Vec<String> = parts.map(String::from).collect();

        // Auto-detect interpreter from file extension
        let (command, mut args) = detect_interpreter(&first, rest_args);

        // Resolve script path to absolute so the daemon can find it
        if !args.is_empty() {
            let script = &args[0];
            let resolved = resolve_path(script, caller_cwd.as_deref());
            args[0] = resolved.to_string_lossy().to_string();
        }

        let cfg_name = name.map(String::from).unwrap_or_else(|| {
            Path::new(&first)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&first)
                .to_string()
        });

        Ok(vec![ProcessConfig {
            name: cfg_name,
            command,
            args,
            cwd: caller_cwd.map(|p| p.to_string_lossy().to_string()),
            ..Default::default()
        }])
    }
}

/// Resolve a potentially relative path against a base directory.
fn resolve_path(path: &str, base: Option<&Path>) -> std::path::PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else if let Some(base) = base {
        let resolved = base.join(p);
        // Try to canonicalize, fall back to joined path
        resolved.canonicalize().unwrap_or(resolved)
    } else {
        p.to_path_buf()
    }
}

/// Auto-detect the interpreter based on file extension.
/// e.g. "server.js" -> ("node", ["server.js"])
///      "worker.py" -> ("python3", ["worker.py"])
///      "app.ts"    -> ("npx", ["tsx", "app.ts"])
///      "node server.js" -> ("node", ["server.js"]) (already has interpreter)
fn detect_interpreter(first: &str, rest: Vec<String>) -> (String, Vec<String>) {
    let lower = first.to_lowercase();

    // If the first word is already an interpreter or binary, use as-is
    if !lower.contains('.') || is_known_binary(&lower) {
        return (first.to_string(), rest);
    }

    let ext = Path::new(first)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "js" | "mjs" | "cjs" => {
            let mut args = vec![first.to_string()];
            args.extend(rest);
            ("node".to_string(), args)
        }
        "ts" | "mts" => {
            let mut args = vec!["tsx".to_string(), first.to_string()];
            args.extend(rest);
            ("npx".to_string(), args)
        }
        "py" => {
            let mut args = vec![first.to_string()];
            args.extend(rest);
            ("python3".to_string(), args)
        }
        "rb" => {
            let mut args = vec![first.to_string()];
            args.extend(rest);
            ("ruby".to_string(), args)
        }
        "sh" | "bash" => {
            let mut args = vec![first.to_string()];
            args.extend(rest);
            ("sh".to_string(), args)
        }
        "php" => {
            let mut args = vec![first.to_string()];
            args.extend(rest);
            ("php".to_string(), args)
        }
        _ => (first.to_string(), rest),
    }
}

fn is_known_binary(name: &str) -> bool {
    matches!(
        name,
        "node"
            | "python"
            | "python3"
            | "ruby"
            | "php"
            | "sh"
            | "bash"
            | "npx"
            | "deno"
            | "bun"
            | "go"
            | "cargo"
            | "java"
    )
}

fn is_config_file(target: &str) -> bool {
    let lower = target.to_lowercase();
    lower.ends_with(".toml")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
        || (lower.ends_with(".json") && (lower.contains("mhost") || lower.contains("ecosystem")))
}
