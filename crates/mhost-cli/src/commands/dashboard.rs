use crate::embedded;
use crate::output::{print_error, print_success};

/// Start the web monitoring dashboard.
pub fn run(port: u16) -> Result<(), String> {
    let script = embedded::ensure_dashboard()?;

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
