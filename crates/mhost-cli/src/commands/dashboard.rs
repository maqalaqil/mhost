use std::path::PathBuf;

use crate::output::{print_error, print_success};

/// Start the web monitoring dashboard.
pub fn run(port: u16) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let scripts_dir = home.join(".mhost").join("dashboard");
    let script = scripts_dir.join("mhost-dashboard.js");

    if !script.exists() {
        // Attempt to copy from known candidate locations
        let exe_adjacent = std::env::current_exe()
            .ok()
            .and_then(|e| {
                e.parent()
                    .map(|p| p.join("../../examples/mhost-dashboard.js"))
            })
            .unwrap_or_else(|| PathBuf::from("examples/mhost-dashboard.js"));

        let candidates: &[PathBuf] = &[PathBuf::from("examples/mhost-dashboard.js"), exe_adjacent];

        match candidates.iter().find(|p| p.exists()) {
            Some(src) => {
                std::fs::create_dir_all(&scripts_dir).map_err(|e| {
                    format!(
                        "Cannot create dashboard dir '{}': {e}",
                        scripts_dir.display()
                    )
                })?;
                std::fs::copy(src, &script).map_err(|e| {
                    format!("Cannot copy dashboard script from '{}': {e}", src.display())
                })?;
            }
            None => {
                return Err("Dashboard script not found. \
                     Run from the mhost repo directory first to install, \
                     or copy examples/mhost-dashboard.js to ~/.mhost/dashboard/"
                    .into());
            }
        }
    }

    print_success(&format!("Starting dashboard on http://localhost:{port}"));

    let status = std::process::Command::new("node")
        .arg(&script)
        .env("PORT", port.to_string())
        .status()
        .map_err(|e| format!("Failed to launch Node.js dashboard: {e}"))?;

    if !status.success() {
        print_error("Dashboard exited with a non-zero status");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_dashboard_module_compiles() {
        // Ensures the module compiles and public API is accessible.
    }
}
