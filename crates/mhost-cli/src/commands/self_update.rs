use crate::output::print_success;

/// Check for a newer mhost release on GitHub and install it if available.
pub fn run() -> Result<(), String> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("maheralaqil")
        .repo_name("mhost")
        .bin_name("mhost")
        .current_version(env!("CARGO_PKG_VERSION"))
        .build()
        .map_err(|e| format!("Failed to configure updater: {e}"))?
        .update()
        .map_err(|e| format!("Update failed: {e}"))?;

    if status.updated() {
        print_success(&format!("Updated to version {}", status.version()));
    } else {
        println!(
            "Already on the latest version ({}).",
            env!("CARGO_PKG_VERSION")
        );
    }

    Ok(())
}
