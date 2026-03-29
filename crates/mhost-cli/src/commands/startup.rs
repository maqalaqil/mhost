#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::env;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::fs;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::path::PathBuf;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use crate::output::print_error;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::output::print_success;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Register mhost as a startup service.
pub fn run_startup() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return install_launchd();

    #[cfg(target_os = "linux")]
    return install_systemd();

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        print_error("Startup is not supported on this platform.");
        Ok(())
    }
}

/// Unregister the startup service.
pub fn run_unstartup() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return uninstall_launchd();

    #[cfg(target_os = "linux")]
    return uninstall_systemd();

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        print_error("Startup is not supported on this platform.");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// macOS — launchd
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn plist_path() -> PathBuf {
    dirs::home_dir()
        .expect("home dir")
        .join("Library/LaunchAgents/io.mhost.daemon.plist")
}

#[cfg(target_os = "macos")]
fn mhost_bin() -> String {
    env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "mhost".to_string())
}

#[cfg(target_os = "macos")]
fn install_launchd() -> Result<(), String> {
    let bin = mhost_bin();
    let plist = plist_path();

    if let Some(parent) = plist.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create LaunchAgents directory: {e}"))?;
    }

    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>io.mhost.daemon</string>
  <key>ProgramArguments</key>
  <array>
    <string>{bin}</string>
    <string>resurrect</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
  <key>StandardOutPath</key>
  <string>{home}/.mhost/logs/launchd-out.log</string>
  <key>StandardErrorPath</key>
  <string>{home}/.mhost/logs/launchd-err.log</string>
</dict>
</plist>
"#,
        bin = bin,
        home = dirs::home_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    );

    fs::write(&plist, content)
        .map_err(|e| format!("Failed to write plist to {}: {e}", plist.display()))?;

    print_success(&format!("Startup plist written to {}", plist.display()));
    println!("  Run: launchctl load {}", plist.display());
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_launchd() -> Result<(), String> {
    let plist = plist_path();
    if plist.exists() {
        fs::remove_file(&plist).map_err(|e| format!("Failed to remove plist: {e}"))?;
        print_success("Startup plist removed.");
    } else {
        println!("No startup plist found at {}", plist.display());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Linux — systemd user service
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn unit_path() -> PathBuf {
    dirs::home_dir()
        .expect("home dir")
        .join(".config/systemd/user/mhost.service")
}

#[cfg(target_os = "linux")]
fn install_systemd() -> Result<(), String> {
    let bin = env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "mhost".to_string());
    let unit = unit_path();

    if let Some(parent) = unit.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create systemd user directory: {e}"))?;
    }

    let content = format!(
        "[Unit]\nDescription=mhost process manager\nAfter=default.target\n\n\
         [Service]\nType=forking\nExecStart={bin} resurrect\nRestart=no\n\n\
         [Install]\nWantedBy=default.target\n",
        bin = bin
    );

    fs::write(&unit, content)
        .map_err(|e| format!("Failed to write unit to {}: {e}", unit.display()))?;

    print_success(&format!("Systemd unit written to {}", unit.display()));
    println!("  Run: systemctl --user enable --now mhost");
    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_systemd() -> Result<(), String> {
    let unit = unit_path();
    if unit.exists() {
        fs::remove_file(&unit).map_err(|e| format!("Failed to remove unit: {e}"))?;
        print_success("Systemd unit removed.");
    } else {
        println!("No systemd unit found at {}", unit.display());
    }
    Ok(())
}
