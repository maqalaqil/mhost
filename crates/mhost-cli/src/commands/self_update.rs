use colored::Colorize;

use crate::output::print_success;

/// Check for a newer mhost release on GitHub and install it if available.
///
/// Detects when the binary is managed by an external package manager
/// (npm wrapper, Homebrew, Cargo) and prints the right command instead
/// of trying to overwrite a binary it doesn't own.
pub fn run() -> Result<(), String> {
    let current = env!("CARGO_PKG_VERSION");

    // If the running binary lives inside an npm package, defer to npm.
    if let Ok(exe) = std::env::current_exe() {
        let exe_str = exe.to_string_lossy().to_lowercase();
        if exe_str.contains("/node_modules/") || exe_str.contains("\\node_modules\\") {
            println!();
            println!("  {} mhost was installed via npm.", "ℹ".cyan());
            println!(
                "    Run: {}",
                "npm install -g @maqalaqil93/mhost@latest".cyan()
            );
            println!();
            return Ok(());
        }
        if exe_str.contains("/homebrew/") || exe_str.contains("/.linuxbrew/") {
            println!();
            println!("  {} mhost was installed via Homebrew.", "ℹ".cyan());
            println!("    Run: {}", "brew upgrade mhost".cyan());
            println!();
            return Ok(());
        }
        if exe_str.contains("/.cargo/bin/") {
            println!();
            println!("  {} mhost was installed via Cargo.", "ℹ".cyan());
            println!("    Run: {}", "cargo install mhost-cli --force".cyan());
            println!();
            return Ok(());
        }
    }

    println!("  Checking for updates… (current: {current})");

    let status = self_update::backends::github::Update::configure()
        .repo_owner("maqalaqil")
        .repo_name("mhost")
        .bin_name("mhost")
        .show_download_progress(true)
        .current_version(current)
        .build()
        .map_err(|e| format!("Failed to configure updater: {e}"))?
        .update()
        .map_err(|e| format!("Update failed: {e}"))?;

    if status.updated() {
        print_success(&format!("Updated to version {}", status.version()));
    } else {
        println!("  Already on the latest version ({current}).");
    }

    Ok(())
}
