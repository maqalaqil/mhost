use colored::Colorize;

use crate::output::{print_error, print_success};

pub fn run(from: &str) -> Result<(), String> {
    match from {
        "pm2" => migrate_pm2(),
        other => Err(format!("Unknown source: '{other}'. Supported: pm2")),
    }
}

fn migrate_pm2() -> Result<(), String> {
    println!("\n  {} Migrating from PM2\n", "🔄".cyan());

    let pm2_dump = dirs::home_dir()
        .ok_or_else(|| "Cannot determine home directory".to_string())?
        .join(".pm2")
        .join("dump.pm2");
    let pm2_eco = std::path::Path::new("ecosystem.config.js");

    if pm2_dump.exists() {
        println!("  Found PM2 dump at: {}", pm2_dump.display());

        let content = std::fs::read_to_string(&pm2_dump).map_err(|e| e.to_string())?;
        let processes: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap_or_default();

        let mut toml = String::from(
            "# Generated from PM2 dump\n\
             # Review and adjust settings before using\n\n",
        );

        for proc in &processes {
            let name = proc["name"].as_str().unwrap_or("app");
            let script = proc["pm_exec_path"]
                .as_str()
                .or_else(|| proc["pm2_env"]["pm_exec_path"].as_str())
                .unwrap_or("node app.js");
            let instances = proc["pm2_env"]["instances"].as_u64().unwrap_or(1);
            let max_restarts = proc["pm2_env"]["max_restarts"].as_u64().unwrap_or(15);

            toml.push_str(&format!("[process.{}]\n", name));
            toml.push_str(&format!("command = \"{}\"\n", script));
            toml.push_str(&format!("instances = {}\n", instances));
            toml.push_str(&format!("max_restarts = {}\n\n", max_restarts));
        }

        let output = "mhost.toml";
        std::fs::write(output, &toml).map_err(|e| e.to_string())?;
        print_success(&format!(
            "Converted {} processes to {}",
            processes.len(),
            output
        ));
        println!("\n  Generated config:\n");
        println!("{}", toml.dimmed());
    } else if pm2_eco.exists() {
        println!("  Found ecosystem.config.js");
        println!("  Note: JS configs need manual conversion.");
        println!("  Run: pm2 save && mhost migrate --from pm2");
    } else {
        print_error("No PM2 dump found. Run 'pm2 save' first, then try again.");
    }

    Ok(())
}
