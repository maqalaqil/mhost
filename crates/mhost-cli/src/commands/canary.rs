use colored::Colorize;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

use crate::output::{print_error, print_success};

pub async fn run(client: &IpcClient, app: &str, percent: u32, duration: u64) -> Result<(), String> {
    println!(
        "\n  {} Canary deploy: {} ({}% traffic for {}s)\n",
        "🐤".yellow(),
        app.white().bold(),
        percent,
        duration
    );

    // Step 1: Scale up one extra instance
    println!("  [1/4] Scaling up canary instance...");
    let resp = client
        .call(methods::PROCESS_INFO, serde_json::json!({"name": app}))
        .await
        .map_err(|e| e.to_string())?;
    let current_instances = resp
        .result
        .as_ref()
        .and_then(|r| r.get("processes"))
        .and_then(|p| p.as_array())
        .map(|a| a.len() as u32)
        .unwrap_or(1);

    let _ = client
        .call(
            methods::PROCESS_SCALE,
            serde_json::json!({"name": app, "instances": current_instances + 1}),
        )
        .await;
    print_success("Canary instance started");

    // Step 2: Monitor
    println!("  [2/4] Monitoring for {}s...", duration);
    let check_interval = std::time::Duration::from_secs(10);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(duration);
    let mut errors = 0u32;

    while std::time::Instant::now() < deadline {
        tokio::time::sleep(check_interval).await;
        let remaining = deadline
            .duration_since(std::time::Instant::now())
            .as_secs()
            .min(duration);
        let elapsed = duration.saturating_sub(remaining);
        let resp = client
            .call(methods::PROCESS_LIST, serde_json::json!(null))
            .await
            .map_err(|e| e.to_string())?;
        let result = resp.result.unwrap_or_default();
        let procs = result
            .get("processes")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();
        let errored = procs
            .iter()
            .filter(|p| {
                p["config"]["name"].as_str() == Some(app) && p["status"].as_str() == Some("errored")
            })
            .count();
        if errored > 0 {
            errors += 1;
        }
        let error_display = if errors > 0 {
            format!("{errors}").red().to_string()
        } else {
            "0".green().to_string()
        };
        print!(
            "\r  ⏱  {}s / {}s — errors: {}",
            elapsed, duration, error_display
        );
    }
    println!();

    // Step 3: Decide
    if errors > 2 {
        println!("  [3/4] {} Too many errors — rolling back", "✖".red());
        let _ = client
            .call(
                methods::PROCESS_SCALE,
                serde_json::json!({"name": app, "instances": current_instances}),
            )
            .await;
        print_error("Canary failed — rolled back to original");
    } else {
        println!("  [3/4] {} Canary healthy — promoting", "✔".green());
        print_success(&format!(
            "Canary promoted. {} now has {} instances",
            app,
            current_instances + 1
        ));
    }

    println!("  [4/4] Done\n");
    Ok(())
}
