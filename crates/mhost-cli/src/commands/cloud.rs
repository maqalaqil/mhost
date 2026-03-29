use mhost_cloud::{Fleet, FleetConfig, RemoteHost, ServerConfig, SshExecutor};
use mhost_core::paths::MhostPaths;

use crate::output::{print_error, print_success};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_fleet(paths: &MhostPaths) -> FleetConfig {
    FleetConfig::load(&paths.fleet_config()).unwrap_or_default()
}

fn save_fleet(paths: &MhostPaths, fleet: &FleetConfig) -> Result<(), String> {
    // Ensure the directory exists before writing.
    if let Some(parent) = paths.fleet_config().parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {e}"))?;
    }
    fleet.save(&paths.fleet_config())
}

// ---------------------------------------------------------------------------
// Add
// ---------------------------------------------------------------------------

pub fn run_add(
    paths: &MhostPaths,
    name: &str,
    host: &str,
    user: Option<&str>,
    key: Option<&str>,
    port: Option<u16>,
) -> Result<(), String> {
    let config = ServerConfig {
        host: host.into(),
        user: user.unwrap_or("root").into(),
        port: port.unwrap_or(22),
        key_path: key.map(String::from),
        ..Default::default()
    };
    let mut fleet = load_fleet(paths);
    fleet.add_server(name, config);
    save_fleet(paths, &fleet)?;
    print_success(&format!("Server '{name}' added ({host})"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Remove
// ---------------------------------------------------------------------------

pub fn run_remove(paths: &MhostPaths, name: &str) -> Result<(), String> {
    let mut fleet = load_fleet(paths);
    if fleet.remove_server(name) {
        save_fleet(paths, &fleet)?;
        print_success(&format!("Server '{name}' removed"));
    } else {
        print_error(&format!("Server '{name}' not found"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

pub fn run_list(paths: &MhostPaths) -> Result<(), String> {
    let fleet = load_fleet(paths);
    if fleet.servers.is_empty() {
        println!("  No servers configured. Run: mhost cloud add <name> --host <ip>");
        return Ok(());
    }
    println!(
        "\n  {:<15} {:<20} {:<8} {:<10} Tags",
        "Name", "Host", "Port", "User"
    );
    println!("  {}", "-".repeat(65));
    for (name, s) in &fleet.servers {
        let tags = s.tags.join(", ");
        println!(
            "  {:<15} {:<20} {:<8} {:<10} {}",
            name, s.host, s.port, s.user, tags
        );
    }
    println!();
    Ok(())
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

pub async fn run_status(paths: &MhostPaths) -> Result<(), String> {
    let fleet_config = load_fleet(paths);
    let fleet = Fleet::new(fleet_config);
    println!("  Checking {} servers...\n", fleet.config.servers.len());
    let statuses = fleet.status_all().await;
    println!(
        "  {:<15} {:<10} {:<8} {:<12} {:<10}",
        "Server", "Status", "mhost", "Processes", "CPU"
    );
    println!("  {}", "-".repeat(60));
    for s in &statuses {
        let status = if s.online { "up" } else { "down" };
        let mhost = if s.mhost_installed { "yes" } else { "no" };
        let cpu = s
            .cpu
            .map(|c| format!("{c:.1}%"))
            .unwrap_or_else(|| "-".into());
        println!(
            "  {:<15} {:<10} {:<8} {:<12} {:<10}",
            s.name, status, mhost, s.process_count, cpu
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Deploy
// ---------------------------------------------------------------------------

pub async fn run_deploy(paths: &MhostPaths, server: &str, config_file: &str) -> Result<(), String> {
    let fleet = load_fleet(paths);
    let sc = fleet
        .get_server(server)
        .ok_or_else(|| format!("Server '{server}' not found"))?;
    let host = RemoteHost::new(server, sc);
    println!("  Deploying to '{server}'...");
    let result = host
        .deploy_config(std::path::Path::new(config_file))
        .await?;
    print_success(&format!("Deployed to '{server}'\n{result}"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Exec
// ---------------------------------------------------------------------------

pub async fn run_exec(paths: &MhostPaths, server: &str, command: &str) -> Result<(), String> {
    let fleet = load_fleet(paths);
    let sc = fleet
        .get_server(server)
        .ok_or_else(|| format!("Server '{server}' not found"))?;
    let ssh = SshExecutor::from_server_config(sc);
    let output = ssh.exec(command).await?;
    print!("{}", output.stdout);
    if !output.stderr.is_empty() {
        eprint!("{}", output.stderr);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

pub async fn run_logs(paths: &MhostPaths, server: &str, app: &str) -> Result<(), String> {
    let fleet = load_fleet(paths);
    let sc = fleet
        .get_server(server)
        .ok_or_else(|| format!("Server '{server}' not found"))?;
    let host = RemoteHost::new(server, sc);
    let logs = host.stream_logs(app).await?;
    println!("{logs}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Restart
// ---------------------------------------------------------------------------

pub async fn run_restart(paths: &MhostPaths, server: &str, app: &str) -> Result<(), String> {
    if server == "all" {
        let fleet_config = load_fleet(paths);
        let fleet = Fleet::new(fleet_config);
        let results = fleet.exec_all(&["restart", app]).await;
        for (name, r) in results {
            match r {
                Ok(_) => print_success(&format!("{name}: restarted '{app}'")),
                Err(e) => print_error(&format!("{name}: {e}")),
            }
        }
    } else {
        let fleet = load_fleet(paths);
        let sc = fleet
            .get_server(server)
            .ok_or_else(|| format!("Server '{server}' not found"))?;
        let host = RemoteHost::new(server, sc);
        host.restart(app).await?;
        print_success(&format!("Restarted '{app}' on '{server}'"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Scale
// ---------------------------------------------------------------------------

pub async fn run_scale(paths: &MhostPaths, server: &str, app: &str, n: u32) -> Result<(), String> {
    let fleet = load_fleet(paths);
    let sc = fleet
        .get_server(server)
        .ok_or_else(|| format!("Server '{server}' not found"))?;
    let host = RemoteHost::new(server, sc);
    host.scale(app, n).await?;
    print_success(&format!("Scaled '{app}' to {n} on '{server}'"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------

pub async fn run_sync(paths: &MhostPaths, config_file: &str) -> Result<(), String> {
    let fleet_config = load_fleet(paths);
    let fleet = Fleet::new(fleet_config);
    println!(
        "  Syncing config to {} servers...",
        fleet.config.servers.len()
    );
    let results = fleet.sync_config(std::path::Path::new(config_file)).await;
    for (name, r) in results {
        match r {
            Ok(_) => print_success(&format!("{name}: synced")),
            Err(e) => print_error(&format!("{name}: {e}")),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SSH
// ---------------------------------------------------------------------------

pub fn run_ssh(paths: &MhostPaths, server: &str) -> Result<(), String> {
    let fleet = load_fleet(paths);
    let sc = fleet
        .get_server(server)
        .ok_or_else(|| format!("Server '{server}' not found"))?;
    let mut args = vec!["-p".to_string(), sc.port.to_string()];
    if let Some(ref key) = sc.key_path {
        args.extend(["-i".into(), key.clone()]);
    }
    args.push(format!("{}@{}", sc.user, sc.host));
    let status = std::process::Command::new("ssh")
        .args(&args)
        .status()
        .map_err(|e| format!("SSH failed: {e}"))?;
    if !status.success() {
        return Err("SSH session ended with error".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Install
// ---------------------------------------------------------------------------

pub async fn run_install(paths: &MhostPaths, server: &str) -> Result<(), String> {
    let fleet = load_fleet(paths);
    let sc = fleet
        .get_server(server)
        .ok_or_else(|| format!("Server '{server}' not found"))?;
    let ssh = SshExecutor::from_server_config(sc);
    println!("  Installing mhost on '{server}'...");
    let version = mhost_cloud::install::RemoteInstaller::install(&ssh).await?;
    print_success(&format!("Installed {version} on '{server}'"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

pub async fn run_update(paths: &MhostPaths, target: &str) -> Result<(), String> {
    if target == "all" {
        let fleet_config = load_fleet(paths);
        let fleet = Fleet::new(fleet_config);
        let results = fleet.update_all().await;
        for (name, r) in results {
            match r {
                Ok(v) => print_success(&format!("{name}: updated to {v}")),
                Err(e) => print_error(&format!("{name}: {e}")),
            }
        }
    } else {
        run_install(paths, target).await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

pub async fn run_import(
    paths: &MhostPaths,
    provider_name: &str,
    region: Option<&str>,
    tag: Option<&str>,
) -> Result<(), String> {
    use mhost_cloud::provider::ImportFilters;

    let provider = mhost_cloud::providers::create_provider(provider_name)?;
    let filters = ImportFilters {
        region: region.map(String::from),
        tags: tag
            .map(|t| {
                let parts: Vec<&str> = t.splitn(2, '=').collect();
                if parts.len() == 2 {
                    vec![(parts[0].into(), parts[1].into())]
                } else {
                    vec![("".into(), t.into())]
                }
            })
            .unwrap_or_default(),
    };

    println!("  Importing from {provider_name}...");
    let instances = provider.list_instances(&filters).await?;

    if instances.is_empty() {
        println!("  No instances found.");
        return Ok(());
    }

    let mut fleet = load_fleet(paths);
    for inst in &instances {
        fleet.add_server(&inst.name, inst.to_server_config());
        println!("  + {} ({})", inst.name, inst.host);
    }
    save_fleet(paths, &fleet)?;
    print_success(&format!(
        "Imported {} servers from {}",
        instances.len(),
        provider_name
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// AI: Setup infra
// ---------------------------------------------------------------------------

pub async fn run_ai_setup(paths: &MhostPaths, description: &str) -> Result<(), String> {
    let ai_config = mhost_ai::AiConfig::load(&paths.ai_config())
        .ok_or("AI not configured. Run: mhost ai setup")?;
    let provider = ai_config.create_provider()?;
    println!("  Planning infrastructure...\n");
    let plan = mhost_cloud::ai_cloud::ai_setup_infra(provider.as_ref(), description).await?;
    println!("{plan}");
    Ok(())
}

// ---------------------------------------------------------------------------
// AI: Diagnose remote
// ---------------------------------------------------------------------------

pub async fn run_ai_diagnose(paths: &MhostPaths, server: &str) -> Result<(), String> {
    let fleet = load_fleet(paths);
    let sc = fleet
        .get_server(server)
        .ok_or_else(|| format!("Server '{server}' not found"))?;
    let ai_config = mhost_ai::AiConfig::load(&paths.ai_config())
        .ok_or("AI not configured. Run: mhost ai setup")?;
    let llm = ai_config.create_provider()?;
    let host = RemoteHost::new(server, sc);
    println!("  Diagnosing '{server}'...\n");
    let result = mhost_cloud::ai_cloud::ai_diagnose_remote(llm.as_ref(), &host).await?;
    println!("{result}");
    Ok(())
}

// ---------------------------------------------------------------------------
// AI: Migrate
// ---------------------------------------------------------------------------

pub async fn run_ai_migrate(paths: &MhostPaths, from: &str, to: &str) -> Result<(), String> {
    let fleet = load_fleet(paths);
    let from_sc = fleet
        .get_server(from)
        .ok_or_else(|| format!("Server '{from}' not found"))?;
    let to_sc = fleet
        .get_server(to)
        .ok_or_else(|| format!("Server '{to}' not found"))?;
    let ai_config = mhost_ai::AiConfig::load(&paths.ai_config())
        .ok_or("AI not configured. Run: mhost ai setup")?;
    let llm = ai_config.create_provider()?;
    let from_host = RemoteHost::new(from, from_sc);
    let to_host = RemoteHost::new(to, to_sc);
    println!("  Planning migration {from} -> {to}...\n");
    let plan = mhost_cloud::ai_cloud::ai_migrate(llm.as_ref(), &from_host, &to_host).await?;
    println!("{plan}");
    Ok(())
}
