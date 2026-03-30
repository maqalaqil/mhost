use colored::Colorize;
use mhost_core::process::{ProcessInfo, ProcessStatus};

// ---------------------------------------------------------------------------
// Status formatting
// ---------------------------------------------------------------------------

fn status_icon(status: &ProcessStatus) -> &'static str {
    match status {
        ProcessStatus::Online => "●",
        ProcessStatus::Starting => "◐",
        ProcessStatus::Stopping => "◑",
        ProcessStatus::Stopped => "○",
        ProcessStatus::Errored => "✖",
    }
}

fn status_label(status: &ProcessStatus) -> &'static str {
    match status {
        ProcessStatus::Online => "online",
        ProcessStatus::Starting => "starting",
        ProcessStatus::Stopping => "stopping",
        ProcessStatus::Stopped => "stopped",
        ProcessStatus::Errored => "errored",
    }
}

pub fn format_status(status: &ProcessStatus) -> String {
    let icon = status_icon(status);
    let label = status_label(status);
    match status {
        ProcessStatus::Online => format!("{} {}", icon.green(), label.green()),
        ProcessStatus::Starting | ProcessStatus::Stopping => {
            format!("{} {}", icon.yellow(), label.yellow())
        }
        ProcessStatus::Stopped => format!("{} {}", icon.dimmed(), label.dimmed()),
        ProcessStatus::Errored => format!("{} {}", icon.red(), label.red()),
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
// Process table
// ---------------------------------------------------------------------------

pub fn print_process_table(processes: &[ProcessInfo]) {
    if processes.is_empty() {
        println!();
        println!("  {}  {}", "○".dimmed(), "No processes running".dimmed());
        println!("     Run: {}", "mhost start <app>".cyan());
        println!();
        return;
    }

    let online = processes
        .iter()
        .filter(|p| p.status == ProcessStatus::Online)
        .count();
    let total = processes.len();

    // Column widths
    const W_ID: usize = 4;
    const W_NAME: usize = 20;
    const W_STATUS: usize = 12;
    const W_PID: usize = 8;
    const W_UP: usize = 12;
    const W_RST: usize = 4;
    const W_MEM: usize = 8;

    let sep = format!(
        "  {}",
        "─".repeat(W_ID + W_NAME + W_STATUS + W_PID + W_UP + W_RST + W_MEM + 20)
    );

    println!();
    println!(
        "  {} {} {}",
        "mhost".bold(),
        "│".dimmed(),
        format!("{online}/{total} online").green()
    );
    println!("{}", sep.dimmed());

    // Header — use plain padding then colorize
    println!(
        "  {}  {}  {}  {}  {}  {}  {}",
        format!("{:<W_ID$}", "ID").dimmed(),
        format!("{:<W_NAME$}", "Name").bold(),
        format!("{:<W_STATUS$}", "Status").bold(),
        format!("{:<W_PID$}", "PID").dimmed(),
        format!("{:<W_UP$}", "Uptime").dimmed(),
        format!("{:<W_RST$}", "↺").dimmed(),
        "Mem".dimmed(),
    );

    println!("{}", sep.dimmed());

    // Rows
    for (i, p) in processes.iter().enumerate() {
        // Build raw (uncolored) padded strings first
        let id_raw = format!("{i:<W_ID$}");
        let name_raw = if p.config.name.len() > W_NAME {
            format!("{:<W_NAME$}", format!("{}…", &p.config.name[..W_NAME - 1]))
        } else {
            format!("{:<W_NAME$}", p.config.name)
        };
        let pid_raw = format!(
            "{:<W_PID$}",
            p.pid.map(|v| v.to_string()).unwrap_or_else(|| "–".into())
        );
        let uptime_raw = {
            let u = p.format_uptime();
            format!("{:<W_UP$}", if u == "0s" { "–".into() } else { u })
        };
        let rst_raw = format!("{:<W_RST$}", p.restart_count);
        let mem_raw = p
            .memory_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "–".into());

        // Status: pad the raw label, then colorize the whole thing
        let status_raw = format!(
            "{} {:<width$}",
            status_icon(&p.status),
            status_label(&p.status),
            width = W_STATUS - 2
        );
        let status_colored = match p.status {
            ProcessStatus::Online => status_raw.green().to_string(),
            ProcessStatus::Starting | ProcessStatus::Stopping => status_raw.yellow().to_string(),
            ProcessStatus::Stopped => status_raw.dimmed().to_string(),
            ProcessStatus::Errored => status_raw.red().to_string(),
        };

        // Now colorize individual cells (color applied to already-padded strings)
        let rst_colored = if p.restart_count > 0 {
            rst_raw.yellow().to_string()
        } else {
            rst_raw.dimmed().to_string()
        };

        println!(
            "  {}  {}  {}  {}  {}  {}  {}",
            id_raw.dimmed(),
            name_raw,
            status_colored,
            pid_raw,
            uptime_raw,
            rst_colored,
            mem_raw.dimmed(),
        );
    }

    println!("{}", sep.dimmed());
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
