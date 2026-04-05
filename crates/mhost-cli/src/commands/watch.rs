use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use colored::Colorize;

use crate::output;

// ---------------------------------------------------------------------------
// Config file discovery
// ---------------------------------------------------------------------------

const DEFAULT_CONFIG_NAMES: &[&str] = &["mhost.toml", "mhost.yaml", "mhost.json"];
const POLL_INTERVAL: Duration = Duration::from_secs(2);

fn discover_config() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    for name in DEFAULT_CONFIG_NAMES {
        let candidate = cwd.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn file_mtime(path: &Path) -> Result<SystemTime, String> {
    let metadata =
        std::fs::metadata(path).map_err(|e| format!("Cannot stat '{}': {e}", path.display()))?;
    metadata
        .modified()
        .map_err(|e| format!("Cannot read mtime for '{}': {e}", path.display()))
}

fn reload_config(config_path: &Path) {
    let path_str = config_path.to_string_lossy();
    println!("  {} Running: mhost start {}", "→".blue(), path_str.cyan());

    match Command::new("mhost").arg("start").arg(&*path_str).output() {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);

            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            if result.status.success() {
                output::print_success("Config reloaded successfully");
            } else {
                output::print_error(&format!(
                    "Reload exited with status {}",
                    result.status.code().unwrap_or(-1)
                ));
            }
        }
        Err(e) => {
            output::print_error(&format!("Failed to execute mhost: {e}"));
        }
    }
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

pub fn run(config_file: Option<&str>) -> Result<(), String> {
    let config_path = match config_file {
        Some(f) => {
            let p = PathBuf::from(f);
            if !p.exists() {
                return Err(format!("Config file not found: '{}'", p.display()));
            }
            p
        }
        None => discover_config().ok_or_else(|| {
            format!(
                "No config file found. Looked for: {}",
                DEFAULT_CONFIG_NAMES.join(", ")
            )
        })?,
    };

    println!(
        "\n  {} Watching {} for changes (poll every 2s)",
        "👁".bold(),
        config_path.display().to_string().cyan()
    );
    println!("  {}", "Press Ctrl+C to stop.\n".dimmed());

    let mut last_mtime = file_mtime(&config_path)?;

    loop {
        std::thread::sleep(POLL_INTERVAL);

        match file_mtime(&config_path) {
            Ok(current_mtime) => {
                if current_mtime != last_mtime {
                    println!("\n  {} Config changed, reloading...", "!".yellow().bold());
                    last_mtime = current_mtime;
                    reload_config(&config_path);
                }
            }
            Err(e) => {
                output::print_error(&format!("Watch error: {e}"));
            }
        }
    }
}
