use colored::Colorize;
use mhost_core::process::ProcessInfo;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;
use serde_json::json;

use crate::output::{format_bytes, format_status, print_error};

/// Print detailed information about a single process.
pub async fn run(client: &IpcClient, name: &str) -> Result<(), String> {
    let resp = client
        .call(methods::PROCESS_INFO, json!({ "name": name }))
        .await
        .map_err(|e| format!("IPC error: {e}"))?;

    if let Some(err) = resp.error {
        print_error(&format!("Process '{}' not found: {}", name, err.message));
        return Ok(());
    }

    let result = resp
        .result
        .ok_or("Empty response from daemon")?;

    // Handler returns {"processes": [...]}
    let process_list = if let Some(arr) = result.get("processes") {
        arr.clone()
    } else {
        result
    };
    let infos: Vec<ProcessInfo> = serde_json::from_value(process_list)
        .map_err(|e| format!("Failed to parse process info: {e}"))?;

    for info in &infos {
        print_info(info);
    }
    Ok(())
}

fn print_info(p: &ProcessInfo) {
    println!("{}", "──── Process Info ────────────────────────────".dimmed());
    println!("  {:20} {}", "id:".bold(), p.id);
    println!("  {:20} {}", "name:".bold(), p.config.name);
    println!("  {:20} {}", "status:".bold(), format_status(&p.status));
    println!("  {:20} {}", "command:".bold(), p.config.command);

    if !p.config.args.is_empty() {
        println!("  {:20} {}", "args:".bold(), p.config.args.join(" "));
    }

    if let Some(cwd) = &p.config.cwd {
        println!("  {:20} {}", "cwd:".bold(), cwd);
    }

    println!(
        "  {:20} {}",
        "pid:".bold(),
        p.pid.map(|v| v.to_string()).unwrap_or_else(|| "N/A".to_string())
    );
    println!("  {:20} {}", "instance:".bold(), p.instance);
    println!("  {:20} {}", "instances:".bold(), p.config.instances);
    println!("  {:20} {}", "restarts:".bold(), p.restart_count);
    println!("  {:20} {}", "max restarts:".bold(), p.config.max_restarts);
    println!("  {:20} {}", "uptime:".bold(), p.format_uptime());
    println!("  {:20} {}", "created:".bold(), p.created_at.to_rfc3339());

    if let Some(lr) = p.last_restart {
        println!("  {:20} {}", "last restart:".bold(), lr.to_rfc3339());
    }

    if let Some(mem) = p.memory_bytes {
        println!("  {:20} {}", "memory:".bold(), format_bytes(mem));
    }

    if let Some(cpu) = p.cpu_percent {
        println!("  {:20} {:.1}%", "cpu:".bold(), cpu);
    }

    println!("{}", "──────────────────────────────────────────────".dimmed());
}
