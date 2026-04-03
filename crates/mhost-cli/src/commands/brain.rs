use colored::Colorize;
use mhost_core::paths::MhostPaths;

// ─── Status ──────────────────────────────────────────────────────────────────

/// Print fleet health scores from the brain's health.json.
pub fn run_status(paths: &MhostPaths) -> Result<(), String> {
    let health_file = paths.root().join("brain").join("health.json");

    if !health_file.exists() {
        println!("  Brain has no data yet. Start the agent: mhost agent start");
        return Ok(());
    }

    let content = std::fs::read_to_string(&health_file).map_err(|e| e.to_string())?;
    let health: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    println!();
    println!("  {} {}", "mhost Brain".bold(), "— Fleet Health".dimmed());
    println!("  {}", "─".repeat(50));

    if let Some(obj) = health.as_object() {
        if obj.is_empty() {
            println!("  No health data recorded yet.");
        } else {
            for (name, data) in obj {
                let score = data["score"].as_f64().unwrap_or(0.0) as u32;
                let bar = health_bar(score);
                let color_score = if score >= 80 {
                    format!("{score}").green().to_string()
                } else if score >= 50 {
                    format!("{score}").yellow().to_string()
                } else {
                    format!("{score}").red().to_string()
                };
                println!("  {name:<20} {bar} {color_score}/100");
            }
        }
    }

    println!();
    Ok(())
}

// ─── History ─────────────────────────────────────────────────────────────────

/// Print the 15 most recent brain incidents.
pub fn run_history(paths: &MhostPaths) -> Result<(), String> {
    let incidents_file = paths.root().join("brain").join("incidents.json");

    if !incidents_file.exists() {
        println!("  No incidents recorded yet.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&incidents_file).map_err(|e| e.to_string())?;
    let incidents: Vec<serde_json::Value> =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let total = incidents.len();
    let recent = &incidents[total.saturating_sub(15)..];

    println!();
    println!(
        "  {} {}",
        "Brain History".bold(),
        format!("({total} total incidents)").dimmed()
    );
    println!("  {}", "─".repeat(70));
    println!(
        "  {:<5} {:<15} {:<12} {:<15} Error",
        "#".dimmed(),
        "Process".bold(),
        "Action",
        "Result"
    );
    println!("  {}", "─".repeat(70));

    for inc in recent.iter().rev() {
        let id = inc["id"].as_u64().unwrap_or(0);
        let process = inc["process"].as_str().unwrap_or("?");
        let action = inc["action"].as_str().unwrap_or("–");
        let result = inc["result"].as_str().unwrap_or("–");
        let error: String = inc["error"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(30)
            .collect();

        let result_colored = match result {
            "success" => "success".green().to_string(),
            "failed" => "failed".red().to_string(),
            other => other.to_string(),
        };

        println!(
            "  {:<5} {:<15} {:<12} {:<15} {}",
            id,
            process,
            action,
            result_colored,
            error.dimmed()
        );
    }

    println!();
    Ok(())
}

// ─── Playbooks ───────────────────────────────────────────────────────────────

/// List all healing playbooks (built-in and auto-learned).
pub fn run_playbooks(paths: &MhostPaths) -> Result<(), String> {
    let playbooks_file = paths.root().join("brain").join("playbooks.json");

    if !playbooks_file.exists() {
        println!("  No playbooks yet. Start the agent to auto-learn.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&playbooks_file).map_err(|e| e.to_string())?;
    let playbooks: Vec<serde_json::Value> =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;

    println!();
    println!(
        "  {} {}",
        "Healing Playbooks".bold(),
        format!("({} total)", playbooks.len()).dimmed()
    );
    println!("  {}", "─".repeat(70));

    for pb in &playbooks {
        let name = pb["name"].as_str().unwrap_or("?");
        let trigger = pb["trigger"].as_str().unwrap_or("?");
        let action = pb["action"].as_str().unwrap_or("?");
        let desc = pb["description"].as_str().unwrap_or("");
        let learned = pb["learned"].as_bool().unwrap_or(false);
        let tag = if learned {
            " (auto-learned)".cyan().to_string()
        } else {
            String::new()
        };

        println!("  {} {}{}", "▸".cyan(), name.bold(), tag);
        println!("    Trigger: {}", trigger.yellow());
        println!("    Action:  {}", action.green());
        println!("    {}", desc.dimmed());
        println!();
    }

    Ok(())
}

// ─── Explain ─────────────────────────────────────────────────────────────────

/// Show health score and incident summary for a specific process.
pub fn run_explain(paths: &MhostPaths, process: &str) -> Result<(), String> {
    let health_file = paths.root().join("brain").join("health.json");
    let incidents_file = paths.root().join("brain").join("incidents.json");

    println!();
    println!("  {} {}", "Brain Analysis:".bold(), process.cyan());
    println!("  {}", "─".repeat(50));

    // Health score
    if let Ok(content) = std::fs::read_to_string(&health_file) {
        if let Ok(health) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(data) = health.get(process) {
                let score = data["score"].as_f64().unwrap_or(0.0) as u32;
                println!("  Health: {} {}/100", health_bar(score), score);
            } else {
                println!("  Health: no data yet");
            }
        }
    } else {
        println!("  Health: no data yet");
    }

    // Incident summary
    if let Ok(content) = std::fs::read_to_string(&incidents_file) {
        if let Ok(incidents) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
            let relevant: Vec<_> = incidents
                .iter()
                .filter(|i| i["process"].as_str() == Some(process))
                .collect();

            let crash_count = relevant
                .iter()
                .filter(|i| i["status"].as_str() == Some("errored"))
                .count();
            let restart_count = relevant
                .iter()
                .filter(|i| i["action"].as_str() == Some("restart"))
                .count();

            println!("  Total incidents: {}", relevant.len());
            println!(
                "  Crashes: {}",
                if crash_count > 3 {
                    format!("{crash_count}").red().to_string()
                } else {
                    crash_count.to_string()
                }
            );
            println!("  Auto-restarts: {restart_count}");

            if let Some(last) = relevant.last() {
                let ts = last["timestamp"].as_str().unwrap_or("?");
                let err: String = last["error"]
                    .as_str()
                    .unwrap_or("?")
                    .chars()
                    .take(60)
                    .collect();
                println!("  Last incident: {ts} — {err}");
            }
        }
    } else {
        println!("  No incidents recorded yet.");
    }

    println!();
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn health_bar(score: u32) -> String {
    let filled = (score / 10) as usize;
    let empty = 10usize.saturating_sub(filled);
    let bar_filled = "█".repeat(filled);
    let bar_empty = "░".repeat(empty);

    if score >= 80 {
        format!("{}{}", bar_filled.green(), bar_empty.dimmed())
    } else if score >= 50 {
        format!("{}{}", bar_filled.yellow(), bar_empty.dimmed())
    } else {
        format!("{}{}", bar_filled.red(), bar_empty.dimmed())
    }
}
