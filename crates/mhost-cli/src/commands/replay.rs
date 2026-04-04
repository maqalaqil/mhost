use colored::Colorize;
use mhost_core::MhostPaths;

pub fn run(paths: &MhostPaths, process: &str, time: Option<&str>) -> Result<(), String> {
    println!(
        "\n  {} Incident Replay: {}\n",
        "▶".cyan().bold(),
        process.white().bold()
    );

    if let Some(t) = time {
        println!("  Filtering around: {}\n", t.yellow());
    }

    // Read incidents from brain
    let incidents_file = paths.root().join("brain").join("incidents.json");
    let incidents: Vec<serde_json::Value> = if incidents_file.exists() {
        serde_json::from_str(&std::fs::read_to_string(&incidents_file).unwrap_or_default())
            .unwrap_or_default()
    } else {
        vec![]
    };

    let relevant: Vec<&serde_json::Value> = incidents
        .iter()
        .filter(|i| i["process"].as_str() == Some(process))
        .collect();

    if relevant.is_empty() {
        println!("  No incidents recorded for '{process}'.");
        println!("  Start the agent to begin recording: mhost agent start");
        return Ok(());
    }

    // Show timeline
    println!("  {}", "─".repeat(60).dimmed());
    println!(
        "  {:<22} {:<12} {:<15} {}",
        "Time".dimmed(),
        "Action".dimmed(),
        "Result".dimmed(),
        "Details".dimmed()
    );
    println!("  {}", "─".repeat(60).dimmed());

    for inc in relevant.iter().rev().take(20) {
        let ts = inc["timestamp"].as_str().unwrap_or("?");
        let short_ts = ts.find('T').map(|i| &ts[i + 1..]).unwrap_or(ts);
        let short_ts = short_ts
            .find('.')
            .map(|i| &short_ts[..i])
            .unwrap_or(short_ts);
        let action = inc["action"].as_str().unwrap_or("–");
        let result = inc["result"].as_str().unwrap_or("–");
        let error = inc["error"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(30)
            .collect::<String>();

        let result_colored = match result {
            "success" => result.green().to_string(),
            "failed" => result.red().to_string(),
            _ => result.to_string(),
        };
        println!(
            "  {:<22} {:<12} {:<15} {}",
            short_ts,
            action,
            result_colored,
            error.dimmed()
        );
    }

    // Show recent logs
    println!("\n  {} Recent logs:\n", "📋".dimmed());
    let log_path = paths.process_out_log(process, 0);
    if log_path.exists() {
        if let Ok(lines) = mhost_logs::reader::tail(&log_path, 10) {
            for line in &lines {
                println!("  {}", line.dimmed());
            }
        }
    } else {
        println!("  {}", "(no stdout log found)".dimmed());
    }

    // Show health score
    let health_file = paths.root().join("brain").join("health.json");
    if let Ok(content) = std::fs::read_to_string(&health_file) {
        if let Ok(health) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(score) = health.get(process).and_then(|h| h["score"].as_f64()) {
                let score_str = format!("{:.0}", score);
                let colored_score = if score >= 80.0 {
                    score_str.green()
                } else if score >= 50.0 {
                    score_str.yellow()
                } else {
                    score_str.red()
                };
                println!("\n  Health Score: {}/100", colored_score);
            }
        }
    }

    println!();
    Ok(())
}
