use colored::Colorize;
use std::path::PathBuf;

use crate::output::print_success;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_WORKSPACE: &str = "default";
const CURRENT_WORKSPACE_FILE: &str = "current-workspace";
const WORKSPACES_DIR: &str = "workspaces";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn mhost_root() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".mhost"))
        .ok_or_else(|| "Could not determine home directory".to_string())
}

fn workspaces_dir() -> Result<PathBuf, String> {
    mhost_root().map(|r| r.join(WORKSPACES_DIR))
}

fn current_workspace_file() -> Result<PathBuf, String> {
    mhost_root().map(|r| r.join(CURRENT_WORKSPACE_FILE))
}

fn read_current_workspace() -> Result<String, String> {
    // Environment variable takes priority
    if let Ok(env_ws) = std::env::var("MHOST_WORKSPACE") {
        if !env_ws.is_empty() {
            return Ok(env_ws);
        }
    }

    let file = current_workspace_file()?;
    if file.exists() {
        let name = std::fs::read_to_string(&file)
            .map_err(|e| format!("Failed to read current workspace file: {e}"))?
            .trim()
            .to_string();
        if name.is_empty() {
            Ok(DEFAULT_WORKSPACE.to_string())
        } else {
            Ok(name)
        }
    } else {
        Ok(DEFAULT_WORKSPACE.to_string())
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

pub fn run_list() -> Result<(), String> {
    let ws_dir = workspaces_dir()?;

    println!("\n  {} Workspaces\n", "Workspaces".white().bold());

    let current = read_current_workspace()?;

    // Always show "default"
    let default_marker = if current == DEFAULT_WORKSPACE {
        " (active)".green().to_string()
    } else {
        String::new()
    };
    println!(
        "  {} {}{}",
        "●".cyan(),
        DEFAULT_WORKSPACE.white().bold(),
        default_marker,
    );

    if ws_dir.exists() {
        let mut entries: Vec<String> = std::fs::read_dir(&ws_dir)
            .map_err(|e| format!("Failed to read workspaces directory: {e}"))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        entries.sort();

        for name in &entries {
            let marker = if *name == current {
                " (active)".green().to_string()
            } else {
                String::new()
            };
            println!("  {} {}{}", "●".cyan(), name.white().bold(), marker,);
        }
    }

    println!();
    Ok(())
}

pub fn run_create(name: &str) -> Result<(), String> {
    if name == DEFAULT_WORKSPACE {
        return Err("Cannot create a workspace named 'default' — it already exists".to_string());
    }

    let ws_dir = workspaces_dir()?.join(name);
    if ws_dir.exists() {
        return Err(format!("Workspace '{name}' already exists"));
    }

    let logs_dir = ws_dir.join("logs");
    let pids_dir = ws_dir.join("pids");

    std::fs::create_dir_all(&logs_dir)
        .map_err(|e| format!("Failed to create logs directory: {e}"))?;
    std::fs::create_dir_all(&pids_dir)
        .map_err(|e| format!("Failed to create pids directory: {e}"))?;

    print_success(&format!("Workspace '{name}' created"));
    println!("  Path: {}", ws_dir.display());
    println!(
        "  Switch: {}",
        format!("mhost workspace switch {name}").cyan()
    );
    Ok(())
}

pub fn run_switch(name: &str) -> Result<(), String> {
    if name != DEFAULT_WORKSPACE {
        let ws_dir = workspaces_dir()?.join(name);
        if !ws_dir.exists() {
            return Err(format!(
                "Workspace '{name}' does not exist. Create it first: mhost workspace create {name}"
            ));
        }
    }

    let file = current_workspace_file()?;
    let content = if name == DEFAULT_WORKSPACE {
        String::new()
    } else {
        name.to_string()
    };
    std::fs::write(&file, content)
        .map_err(|e| format!("Failed to write current workspace file: {e}"))?;

    print_success(&format!("Switched to workspace '{name}'"));
    Ok(())
}

pub fn run_current() -> Result<(), String> {
    let current = read_current_workspace()?;
    let source = if std::env::var("MHOST_WORKSPACE").is_ok() {
        " (from MHOST_WORKSPACE env)".dimmed().to_string()
    } else {
        String::new()
    };

    println!(
        "\n  {} Active workspace: {}{}\n",
        "●".cyan(),
        current.white().bold(),
        source,
    );

    if current != DEFAULT_WORKSPACE {
        let ws_dir = workspaces_dir()?.join(&current);
        println!("  Path: {}", ws_dir.display());
        println!();
    }

    Ok(())
}

pub fn run_delete(name: &str) -> Result<(), String> {
    if name == DEFAULT_WORKSPACE {
        return Err("Cannot delete the default workspace".to_string());
    }

    let ws_dir = workspaces_dir()?.join(name);
    if !ws_dir.exists() {
        return Err(format!("Workspace '{name}' does not exist"));
    }

    // If deleting the active workspace, reset to default
    let current = read_current_workspace()?;
    if current == name {
        let file = current_workspace_file()?;
        std::fs::write(&file, "").map_err(|e| format!("Failed to reset current workspace: {e}"))?;
        println!(
            "  {} Active workspace reset to '{}'",
            "!".yellow().bold(),
            DEFAULT_WORKSPACE
        );
    }

    std::fs::remove_dir_all(&ws_dir)
        .map_err(|e| format!("Failed to remove workspace directory: {e}"))?;

    print_success(&format!("Workspace '{name}' deleted"));
    Ok(())
}
