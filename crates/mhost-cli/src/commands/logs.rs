use std::fs::File;
use std::io::{BufRead, BufReader};

use mhost_core::paths::MhostPaths;

/// Tail log lines for a process.
///
/// - `name`       — process name
/// - `lines`      — how many tail lines to show (0 = all)
/// - `err_stream` — if true read stderr log, otherwise stdout log
/// - `grep`       — optional substring filter
pub fn run(
    paths: &MhostPaths,
    name: &str,
    lines: usize,
    err_stream: bool,
    grep: Option<&str>,
) -> Result<(), String> {
    // Instance 0 is the canonical instance for single-instance processes.
    let log_path = if err_stream {
        paths.process_err_log(name, 0)
    } else {
        paths.process_out_log(name, 0)
    };

    let file = File::open(&log_path)
        .map_err(|e| format!("Cannot open log '{}': {e}", log_path.display()))?;

    let reader = BufReader::new(file);
    let all_lines: Vec<String> = reader
        .lines()
        .filter_map(|l| l.ok())
        .filter(|l| grep.map(|g| l.contains(g)).unwrap_or(true))
        .collect();

    let start = if lines == 0 || all_lines.len() <= lines {
        0
    } else {
        all_lines.len() - lines
    };

    for line in &all_lines[start..] {
        println!("{}", line);
    }

    Ok(())
}

/// Validate that the log path exists without reading it (used in tests).
#[allow(dead_code)]
pub fn log_exists(paths: &MhostPaths, name: &str, err_stream: bool) -> bool {
    let p = if err_stream {
        paths.process_err_log(name, 0)
    } else {
        paths.process_out_log(name, 0)
    };
    p.exists()
}
