use colored::Colorize;
use mhost_core::process::{ProcessInfo, ProcessStatus};

// ---------------------------------------------------------------------------
// Status formatting
// ---------------------------------------------------------------------------

pub fn format_status(status: &ProcessStatus) -> String {
    let s = status.to_string();
    match status {
        ProcessStatus::Online => s.green().to_string(),
        ProcessStatus::Starting | ProcessStatus::Stopping => s.yellow().to_string(),
        ProcessStatus::Stopped | ProcessStatus::Errored => s.red().to_string(),
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

// ---------------------------------------------------------------------------
// Process table
// ---------------------------------------------------------------------------

pub fn print_process_table(processes: &[ProcessInfo]) {
    if processes.is_empty() {
        println!("{}", "No processes registered.".dimmed());
        return;
    }

    // Header
    println!(
        "{:<4} {:<20} {:<12} {:<8} {:<6} {:<12} {:<9} {:<10}",
        "id".bold(),
        "name".bold(),
        "status".bold(),
        "pid".bold(),
        "inst".bold(),
        "uptime".bold(),
        "restarts".bold(),
        "memory".bold(),
    );
    println!("{}", "─".repeat(85).dimmed());

    for p in processes {
        let pid_str = p.pid.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());
        let mem_str = p
            .memory_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "-".to_string());

        // truncate long names
        let name = if p.config.name.len() > 20 {
            format!("{}…", &p.config.name[..19])
        } else {
            p.config.name.clone()
        };

        println!(
            "{:<4} {:<20} {:<24} {:<8} {:<6} {:<12} {:<9} {:<10}",
            &p.id[..4.min(p.id.len())],
            name,
            format_status(&p.status),
            pid_str,
            p.instance,
            p.format_uptime(),
            p.restart_count,
            mem_str,
        );
    }
}

// ---------------------------------------------------------------------------
// User-facing messages
// ---------------------------------------------------------------------------

pub fn print_success(msg: &str) {
    println!("{} {}", "✔".green(), msg);
}

pub fn print_error(msg: &str) {
    eprintln!("{} {}", "✖".red(), msg);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes_bytes() {
        assert_eq!(format_bytes(512), "512B");
    }

    #[test]
    fn test_format_bytes_kb() {
        assert_eq!(format_bytes(2048), "2.0KB");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0MB");
    }

    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.0GB");
    }

    #[test]
    fn test_format_status_online() {
        // Just verify it returns a non-empty string (color codes will be present)
        let s = format_status(&ProcessStatus::Online);
        assert!(s.contains("online"));
    }
}
