use colored::Colorize;
use mhost_core::MhostPaths;

pub fn run(paths: &MhostPaths, app: &str, target: f64) -> Result<(), String> {
    println!("\n  {} SLA Report: {}\n", "📊".cyan(), app.white().bold());

    let incidents_file = paths.root().join("brain").join("incidents.json");
    let incidents: Vec<serde_json::Value> = if incidents_file.exists() {
        serde_json::from_str(&std::fs::read_to_string(&incidents_file).unwrap_or_default())
            .unwrap_or_default()
    } else {
        vec![]
    };

    let relevant: Vec<&serde_json::Value> = incidents
        .iter()
        .filter(|i| i["process"].as_str() == Some(app))
        .collect();

    let total_incidents = relevant.len();
    let crashes = relevant
        .iter()
        .filter(|i| i["status"].as_str() == Some("errored"))
        .count();

    // Estimate downtime: assume 30s per crash incident (rough heuristic)
    let downtime_secs = (crashes as u64) * 30;
    let month_secs: u64 = 30 * 24 * 3600;
    let uptime_pct = if month_secs > 0 {
        ((month_secs - downtime_secs.min(month_secs)) as f64 / month_secs as f64) * 100.0
    } else {
        100.0
    };

    let met_sla = uptime_pct >= target;

    println!("  {}", "─".repeat(50));
    println!(
        "  {:<20} {}",
        "Target SLA:".bold(),
        format!("{target:.1}%").cyan()
    );
    println!(
        "  {:<20} {}",
        "Current uptime:".bold(),
        if met_sla {
            format!("{uptime_pct:.3}%").green().to_string()
        } else {
            format!("{uptime_pct:.3}%").red().to_string()
        }
    );
    println!(
        "  {:<20} {}",
        "Status:".bold(),
        if met_sla {
            "✔ SLA Met".green().to_string()
        } else {
            format!("✖ SLA Missed by {:.3}%", target - uptime_pct)
                .red()
                .to_string()
        }
    );
    println!("  {:<20} {}", "Incidents:".bold(), total_incidents);
    println!("  {:<20} {}", "Crashes:".bold(), crashes);
    println!(
        "  {:<20} {}",
        "Est. downtime:".bold(),
        format_duration(downtime_secs)
    );
    println!("  {}", "─".repeat(50));
    println!();
    Ok(())
}

fn format_duration(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}
