use std::path::PathBuf;

use crate::output::{print_error, print_success};

/// Start the web monitoring dashboard.
pub fn run(port: u16) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let scripts_dir = home.join(".mhost").join("dashboard");
    let script = scripts_dir.join("mhost-dashboard.js");

    // If already installed, use it
    if !script.exists() {
        // Search for the source script
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.to_path_buf()));

        let mut candidates: Vec<PathBuf> = vec![
            // Relative to cwd
            PathBuf::from("examples/mhost-dashboard.js"),
        ];

        // Relative to binary (dev: target/release/mhost → repo root)
        if let Some(ref dir) = exe_dir {
            candidates.push(dir.join("../../examples/mhost-dashboard.js"));
            candidates.push(dir.join("../examples/mhost-dashboard.js"));
            candidates.push(dir.join("examples/mhost-dashboard.js"));
        }

        // Global fallback
        candidates.push(PathBuf::from(
            "/usr/local/lib/mhost/examples/mhost-dashboard.js",
        ));

        match candidates.iter().find(|p| p.exists()) {
            Some(src) => {
                std::fs::create_dir_all(&scripts_dir)
                    .map_err(|e| format!("Cannot create dashboard dir: {e}"))?;
                std::fs::copy(src, &script)
                    .map_err(|e| format!("Cannot copy dashboard script: {e}"))?;
            }
            None => {
                return Err("Dashboard script not found. \
                     Run `mhost dashboard` from the mhost repo directory once to install it, \
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
        .map_err(|e| format!("Failed to launch dashboard: {e}"))?;

    if !status.success() {
        print_error("Dashboard exited with a non-zero status");
    }
    Ok(())
}
