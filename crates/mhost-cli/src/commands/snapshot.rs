use colored::Colorize;
use mhost_core::protocol::methods;
use mhost_core::MhostPaths;
use mhost_ipc::IpcClient;

use crate::output::print_success;

pub async fn create(client: &IpcClient, paths: &MhostPaths, name: &str) -> Result<(), String> {
    let snap_dir = paths.root().join("snapshots");
    std::fs::create_dir_all(&snap_dir).map_err(|e| e.to_string())?;

    // Capture process list
    let resp = client
        .call(methods::PROCESS_LIST, serde_json::json!(null))
        .await
        .map_err(|e| e.to_string())?;
    let processes = resp.result.unwrap_or_default();

    let snapshot = serde_json::json!({
        "name": name,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "processes": processes,
    });

    let path = snap_dir.join(format!("{name}.json"));
    let json = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;

    print_success(&format!("Snapshot '{name}' created"));
    println!("  Path: {}", path.display());
    Ok(())
}

pub fn list(paths: &MhostPaths) -> Result<(), String> {
    let snap_dir = paths.root().join("snapshots");
    if !snap_dir.exists() {
        println!("  No snapshots. Create one: mhost snapshot create <name>");
        return Ok(());
    }

    println!("\n  {} Snapshots\n", "📸".cyan());
    let mut entries: Vec<_> = std::fs::read_dir(&snap_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
        let snap: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        let name = snap["name"].as_str().unwrap_or("?");
        let created = snap["created_at"].as_str().unwrap_or("?");
        let proc_count = snap["processes"]
            .get("processes")
            .and_then(|p| p.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        println!(
            "  {} {} — {} processes ({})",
            "●".cyan(),
            name.white().bold(),
            proc_count,
            created.dimmed()
        );
    }
    println!();
    Ok(())
}

pub async fn restore(client: &IpcClient, paths: &MhostPaths, name: &str) -> Result<(), String> {
    let path = paths.root().join("snapshots").join(format!("{name}.json"));
    if !path.exists() {
        return Err(format!("Snapshot '{name}' not found"));
    }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let snap: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    // Stop all current processes
    let _ = client
        .call(methods::PROCESS_STOP, serde_json::json!({"name": "all"}))
        .await;

    // Restore from snapshot — save the process data as dump.json and resurrect
    let processes = snap.get("processes").cloned().unwrap_or_default();
    let dump_path = paths.dump_file();
    let procs_array = processes.get("processes").cloned().unwrap_or(processes);
    std::fs::write(
        &dump_path,
        serde_json::to_string_pretty(&procs_array).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;

    let _ = client
        .call(methods::PROCESS_RESURRECT, serde_json::json!(null))
        .await;

    print_success(&format!("Snapshot '{name}' restored"));
    Ok(())
}
