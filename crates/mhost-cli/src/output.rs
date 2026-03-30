use colored::Colorize;
use mhost_core::process::{ProcessInfo, ProcessStatus};

// ---------------------------------------------------------------------------
// Status formatting
// ---------------------------------------------------------------------------

pub fn format_status(status: &ProcessStatus) -> String {
    match status {
        ProcessStatus::Online => "● online".green().bold().to_string(),
        ProcessStatus::Starting => "◐ starting".yellow().to_string(),
        ProcessStatus::Stopping => "◑ stopping".yellow().to_string(),
        ProcessStatus::Stopped => "○ stopped".dimmed().to_string(),
        ProcessStatus::Errored => "✖ errored".red().bold().to_string(),
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

// ---------------------------------------------------------------------------
// Process table — modern box-drawing style
// ---------------------------------------------------------------------------

pub fn print_process_table(processes: &[ProcessInfo]) {
    if processes.is_empty() {
        println!();
        println!("  {}  {}", "○".dimmed(), "No processes running".dimmed());
        println!("  {}  Run: {}", " ".dimmed(), "mhost start <app>".cyan());
        println!();
        return;
    }

    let online = processes
        .iter()
        .filter(|p| p.status == ProcessStatus::Online)
        .count();
    let total = processes.len();

    // Header
    println!();
    println!(
        "  {} {} {}",
        "mhost".bold(),
        "│".dimmed(),
        format!("{online}/{total} online").green()
    );
    println!(
        "  {}",
        "─────┬──────────────────────┬──────────────┬─────────┬────────────┬──────────┬─────────"
            .dimmed()
    );
    println!(
        "  {}  │  {}  │  {}  │  {}  │  {}  │  {}  │  {}",
        " # ".dimmed(),
        "name".bold().white(),
        "status".bold().white(),
        " pid  ".dimmed(),
        " uptime   ".dimmed(),
        "restarts".dimmed(),
        "memory".dimmed(),
    );
    println!(
        "  {}",
        "─────┼──────────────────────┼──────────────┼─────────┼────────────┼──────────┼─────────"
            .dimmed()
    );

    for (i, p) in processes.iter().enumerate() {
        let pid_str = p
            .pid
            .map(|v| format!("{v}"))
            .unwrap_or_else(|| "—".dimmed().to_string());

        let mem_str = p
            .memory_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "—".dimmed().to_string());

        let uptime = p.format_uptime();
        let uptime_str = if uptime == "0s" {
            "—".dimmed().to_string()
        } else {
            uptime
        };

        let restarts = if p.restart_count > 0 {
            format!("{}", p.restart_count).yellow().to_string()
        } else {
            "0".dimmed().to_string()
        };

        // Truncate long names
        let name = if p.config.name.len() > 18 {
            format!("{}…", &p.config.name[..17])
        } else {
            p.config.name.clone()
        };

        println!(
            "  {:<3} {} {:<18}   {} {:<12} {} {:<7} {} {:<10} {} {:<8} {} {:<7}",
            format!("{i}").dimmed(),
            "│".dimmed(),
            name.white(),
            "│".dimmed(),
            format_status(&p.status),
            "│".dimmed(),
            pid_str,
            "│".dimmed(),
            uptime_str,
            "│".dimmed(),
            restarts,
            "│".dimmed(),
            mem_str,
        );
    }

    println!(
        "  {}",
        "─────┴──────────────────────┴──────────────┴─────────┴────────────┴──────────┴─────────"
            .dimmed()
    );
    println!();
}

// ---------------------------------------------------------------------------
// User-facing messages
// ---------------------------------------------------------------------------

pub fn print_success(msg: &str) {
    println!("{} {}", "✔".green().bold(), msg);
}

pub fn print_error(msg: &str) {
    eprintln!("{} {}", "✖".red().bold(), msg);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes_bytes() {
        assert_eq!(format_bytes(512), "512 B");
    }

    #[test]
    fn test_format_bytes_kb() {
        assert_eq!(format_bytes(2048), "2 KB");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn test_format_status_online() {
        let s = format_status(&ProcessStatus::Online);
        assert!(s.contains("online"));
    }
}
