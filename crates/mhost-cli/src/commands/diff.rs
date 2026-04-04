use colored::Colorize;
use mhost_core::MhostPaths;

pub fn run(paths: &MhostPaths, env_a: &str, env_b: &str) -> Result<(), String> {
    println!(
        "\n  {} Comparing: {} vs {}\n",
        "🔍".cyan(),
        env_a.white().bold(),
        env_b.white().bold()
    );

    // Read fleet config
    let fleet_path = paths.fleet_config();
    if !fleet_path.exists() {
        println!("  No fleet configured. Add servers with: mhost cloud add");
        println!("  Or compare local configs: mhost diff config1.toml config2.toml");
        return Ok(());
    }

    let content = std::fs::read_to_string(&fleet_path).map_err(|e| e.to_string())?;
    let fleet: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let servers = fleet.get("servers").and_then(|s| s.as_object());

    if let Some(servers) = servers {
        let a_exists = servers.contains_key(env_a);
        let b_exists = servers.contains_key(env_b);

        println!(
            "  {:<20} {:<15} {}",
            "Property".bold(),
            env_a.cyan(),
            env_b.cyan()
        );
        println!("  {}", "─".repeat(55));

        if a_exists && b_exists {
            let a = &servers[env_a];
            let b = &servers[env_b];
            for key in ["host", "port", "user", "region", "provider"] {
                let va = a.get(key).and_then(|v| v.as_str()).unwrap_or("–");
                let vb = b.get(key).and_then(|v| v.as_str()).unwrap_or("–");
                let marker = if va != vb {
                    "≠".yellow().to_string()
                } else {
                    "=".dimmed().to_string()
                };
                println!("  {key:<20} {va:<15} {marker} {vb}");
            }
        } else {
            if !a_exists {
                println!("  Server '{env_a}' not found in fleet");
            }
            if !b_exists {
                println!("  Server '{env_b}' not found in fleet");
            }
        }
    }
    println!();
    Ok(())
}
