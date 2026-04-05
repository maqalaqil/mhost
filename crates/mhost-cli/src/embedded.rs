//! Embedded JS scripts — compiled into the binary so they're always available
//! regardless of installation method (npm, brew, cargo, curl).

use std::path::PathBuf;

pub const AGENT_JS: &str = include_str!("../../../examples/mhost-agent.js");
pub const BRAIN_JS: &str = include_str!("../../../examples/mhost-brain.js");
pub const DASHBOARD_JS: &str = include_str!("../../../examples/mhost-dashboard.js");
pub const NOTIFIER_JS: &str = include_str!("../../../examples/mhost-telegram-notifier.js");

/// Ensure a script is installed at `~/.mhost/<subdir>/<filename>`.
/// Always overwrites with the embedded version to stay in sync with the binary.
pub fn ensure_script(subdir: &str, filename: &str, content: &str) -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".mhost").join(subdir);
    let path = dir.join(filename);

    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    std::fs::write(&path, content)
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;

    Ok(path)
}

/// Install the agent + brain scripts. Returns agent script path.
pub fn ensure_agent() -> Result<PathBuf, String> {
    let _ = ensure_script("agent-scripts", "mhost-brain.js", BRAIN_JS)?;
    ensure_script("agent-scripts", "mhost-agent.js", AGENT_JS)
}

/// Install the dashboard script. Returns path.
pub fn ensure_dashboard() -> Result<PathBuf, String> {
    ensure_script("dashboard", "mhost-dashboard.js", DASHBOARD_JS)
}

/// Install the notifier script. Returns path.
pub fn ensure_notifier() -> Result<PathBuf, String> {
    ensure_script("notifier", "mhost-telegram-notifier.js", NOTIFIER_JS)
}
