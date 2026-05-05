use colored::Colorize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub async fn run(url: &str, duration_secs: u64, concurrency: u32) -> Result<(), String> {
    println!("\n  {} Benchmarking: {}", "⚡".cyan(), url.white().bold());
    println!("  Duration: {duration_secs}s | Concurrency: {concurrency}\n");

    let total_requests = Arc::new(AtomicU64::new(0));
    let total_errors = Arc::new(AtomicU64::new(0));
    let total_latency_us = Arc::new(AtomicU64::new(0));
    let min_latency_us = Arc::new(AtomicU64::new(u64::MAX));
    let max_latency_us = Arc::new(AtomicU64::new(0));

    let deadline = Instant::now() + Duration::from_secs(duration_secs);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let mut handles = vec![];
    for _ in 0..concurrency {
        let client = client.clone();
        let url = url.to_string();
        let reqs = Arc::clone(&total_requests);
        let errs = Arc::clone(&total_errors);
        let lat = Arc::clone(&total_latency_us);
        let min_l = Arc::clone(&min_latency_us);
        let max_l = Arc::clone(&max_latency_us);

        handles.push(tokio::spawn(async move {
            while Instant::now() < deadline {
                let start = Instant::now();
                let result = client.get(&url).send().await;
                let elapsed = start.elapsed().as_micros() as u64;

                reqs.fetch_add(1, Ordering::Relaxed);
                lat.fetch_add(elapsed, Ordering::Relaxed);
                min_l.fetch_min(elapsed, Ordering::Relaxed);
                max_l.fetch_max(elapsed, Ordering::Relaxed);

                match result {
                    Ok(resp) if !resp.status().is_success() => {
                        errs.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        errs.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    let reqs = total_requests.load(Ordering::Relaxed);
    let errs = total_errors.load(Ordering::Relaxed);
    let total_lat = total_latency_us.load(Ordering::Relaxed);
    let min_lat = min_latency_us.load(Ordering::Relaxed);
    let max_lat = max_latency_us.load(Ordering::Relaxed);

    let avg_lat = total_lat.checked_div(reqs).unwrap_or(0);
    let rps = reqs.checked_div(duration_secs).unwrap_or(0);
    let error_pct = if reqs > 0 {
        (errs as f64 / reqs as f64) * 100.0
    } else {
        0.0
    };

    println!("  {}", "─".repeat(50));
    println!(
        "  {} {}",
        "Requests/sec:".bold(),
        format!("{rps}").green().bold()
    );
    println!("  {} {}", "Total requests:".bold(), reqs);
    println!(
        "  {} {} ({:.1}%)",
        "Errors:".bold(),
        if errs > 0 {
            format!("{errs}").red().to_string()
        } else {
            "0".to_string()
        },
        error_pct
    );
    println!("  {} {}", "Avg latency:".bold(), format_us(avg_lat));
    println!("  {} {}", "Min latency:".bold(), format_us(min_lat));
    println!("  {} {}", "Max latency:".bold(), format_us(max_lat));
    println!("  {}", "─".repeat(50));
    println!();
    Ok(())
}

fn format_us(us: u64) -> String {
    if us >= 1_000_000 {
        format!("{:.1}s", us as f64 / 1_000_000.0)
    } else if us >= 1_000 {
        format!("{:.1}ms", us as f64 / 1_000.0)
    } else {
        format!("{us}μs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_us_microseconds() {
        assert_eq!(format_us(500), "500μs");
    }

    #[test]
    fn test_format_us_milliseconds() {
        assert_eq!(format_us(5_000), "5.0ms");
    }

    #[test]
    fn test_format_us_seconds() {
        assert_eq!(format_us(2_500_000), "2.5s");
    }
}
