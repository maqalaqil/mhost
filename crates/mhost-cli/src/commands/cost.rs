use colored::Colorize;
use mhost_core::process::ProcessInfo;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

pub async fn run(client: &IpcClient) -> Result<(), String> {
    let resp = client
        .call(methods::PROCESS_LIST, serde_json::json!(null))
        .await
        .map_err(|e| e.to_string())?;

    let result = resp.result.unwrap_or_default();
    let list = if let Some(arr) = result.get("processes") {
        arr.clone()
    } else {
        result
    };
    let processes: Vec<ProcessInfo> = serde_json::from_value(list).unwrap_or_default();

    if processes.is_empty() {
        println!("  No processes running.");
        return Ok(());
    }

    println!("\n  {} Resource Cost Estimate\n", "💰".cyan());
    println!("  {}", "─".repeat(60));
    println!(
        "  {:<20} {:<10} {:<12} {:<10}",
        "Process".bold(),
        "Memory".dimmed(),
        "Est. Type".dimmed(),
        "$/month".dimmed()
    );
    println!("  {}", "─".repeat(60));

    let mut total_cost = 0.0_f64;
    for p in &processes {
        let mem_mb = p.memory_bytes.unwrap_or(0) as f64 / 1_048_576.0;
        let (instance_type, monthly_cost) = estimate_instance(mem_mb);
        total_cost += monthly_cost;
        println!(
            "  {:<20} {:<10} {:<12} ${:.2}",
            p.config.name,
            format!("{:.0}MB", mem_mb),
            instance_type,
            monthly_cost
        );
    }

    println!("  {}", "─".repeat(60));
    println!(
        "  {:<20} {:<10} {:<12} {}",
        "Total".bold(),
        "",
        "",
        format!("${total_cost:.2}/mo").green().bold()
    );
    println!();
    Ok(())
}

fn estimate_instance(mem_mb: f64) -> (&'static str, f64) {
    if mem_mb <= 512.0 {
        ("t3.micro", 8.35)
    } else if mem_mb <= 1024.0 {
        ("t3.small", 16.70)
    } else if mem_mb <= 2048.0 {
        ("t3.medium", 33.41)
    } else if mem_mb <= 4096.0 {
        ("t3.large", 66.82)
    } else {
        ("t3.xlarge", 133.63)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_micro_instance() {
        let (t, c) = estimate_instance(256.0);
        assert_eq!(t, "t3.micro");
        assert!(c < 10.0);
    }

    #[test]
    fn test_small_instance() {
        let (t, _) = estimate_instance(800.0);
        assert_eq!(t, "t3.small");
    }

    #[test]
    fn test_large_instance() {
        let (t, _) = estimate_instance(3000.0);
        assert_eq!(t, "t3.large");
    }
}
