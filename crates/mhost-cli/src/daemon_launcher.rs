use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use mhost_core::paths::MhostPaths;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Ensure the daemon is running. If the socket is absent or the PID is dead,
/// spawn a fresh daemon process.
pub fn ensure_daemon_running(paths: &MhostPaths) -> Result<(), String> {
    let socket = paths.socket();

    if socket.exists() {
        // Check the PID to confirm the daemon is truly alive.
        if let Some(pid) = read_pid(paths) {
            if is_process_alive(pid) {
                return Ok(()); // already running
            }
            // Stale socket — remove it before respawning.
            let _ = fs::remove_file(&socket);
        } else {
            // Socket exists but no PID file — optimistically assume alive.
            return Ok(());
        }
    }

    spawn_daemon(paths)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn read_pid(paths: &MhostPaths) -> Option<u32> {
    let pid_path = paths.pid_file();
    let text = fs::read_to_string(pid_path).ok()?;
    text.trim().parse::<u32>().ok()
}

fn spawn_daemon(paths: &MhostPaths) -> Result<(), String> {
    let daemon_bin = find_daemon_binary()?;
    paths
        .ensure_dirs()
        .map_err(|e| format!("Failed to create mhost dirs: {e}"))?;

    let log_path = paths.daemon_log();
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Cannot open daemon log {}: {e}", log_path.display()))?;

    std::process::Command::new(&daemon_bin)
        .stdout(log_file.try_clone().map_err(|e| e.to_string())?)
        .stderr(log_file)
        .spawn()
        .map_err(|e| format!("Failed to spawn daemon {}: {e}", daemon_bin.display()))?;

    // Wait up to 2.5 s for the socket to appear.
    let socket = paths.socket();
    let deadline = Instant::now() + Duration::from_millis(2500);
    while Instant::now() < deadline {
        if socket.exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    Err("Daemon did not start within 2.5 seconds".to_string())
}

/// Find the `mhostd` binary — search multiple locations.
fn find_daemon_binary() -> Result<PathBuf, String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()));

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(ref dir) = exe_dir {
        // Same directory as mhost binary (brew, cargo, curl installs)
        candidates.push(dir.join("mhostd"));
        // npm: vendor/ directory
        candidates.push(dir.join("../vendor/mhostd"));
        // Dev: target/release/ or target/debug/
        candidates.push(dir.join("mhostd"));
    }

    // System PATH
    candidates.push(PathBuf::from("/usr/local/bin/mhostd"));
    candidates.push(PathBuf::from("/usr/bin/mhostd"));

    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }

    // Last resort: hope it's in PATH
    Ok(PathBuf::from("mhostd"))
}

// ---------------------------------------------------------------------------
// Platform-specific process liveness check
// ---------------------------------------------------------------------------

#[cfg(unix)]
pub fn is_process_alive(pid: u32) -> bool {
    // SAFETY: kill(pid, 0) only checks existence — it never sends a signal.
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0
}

#[cfg(not(unix))]
pub fn is_process_alive(_pid: u32) -> bool {
    // Fallback for non-Unix: conservatively assume alive.
    true
}
