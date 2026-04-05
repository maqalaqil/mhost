use std::fs;
use std::path::{Path, PathBuf};

use colored::Colorize;
use serde::{Deserialize, Serialize};

use crate::output;

// ---------------------------------------------------------------------------
// Plugin manifest
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub hooks: PluginHooks,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginHooks {
    pub on_start: Option<String>,
    pub on_stop: Option<String>,
    pub on_crash: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn plugins_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Cannot determine home directory")
        .join(".mhost")
        .join("plugins")
}

fn read_manifest(plugin_dir: &Path) -> Result<PluginManifest, String> {
    let manifest_path = plugin_dir.join("plugin.json");
    if !manifest_path.exists() {
        return Err(format!(
            "No plugin.json found in '{}'",
            plugin_dir.display()
        ));
    }

    let content =
        fs::read_to_string(&manifest_path).map_err(|e| format!("Cannot read plugin.json: {e}"))?;

    serde_json::from_str::<PluginManifest>(&content)
        .map_err(|e| format!("Invalid plugin.json: {e}"))
}

fn format_hooks(hooks: &PluginHooks) -> String {
    let mut parts = Vec::new();
    if hooks.on_start.is_some() {
        parts.push("on_start");
    }
    if hooks.on_stop.is_some() {
        parts.push("on_stop");
    }
    if hooks.on_crash.is_some() {
        parts.push("on_crash");
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

pub fn run_list() -> Result<(), String> {
    let dir = plugins_dir();

    if !dir.exists() {
        println!("No plugins installed.");
        println!("  Plugin directory: {}", dir.display().to_string().dimmed());
        return Ok(());
    }

    let entries: Vec<_> = fs::read_dir(&dir)
        .map_err(|e| format!("Cannot read plugins directory: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    if entries.is_empty() {
        println!("No plugins installed.");
        return Ok(());
    }

    println!(
        "\n  {:<20} {:<10} {:<30} {}",
        "Name".bold(),
        "Version".bold(),
        "Description".bold(),
        "Hooks".bold(),
    );
    println!("  {}", "─".repeat(75).dimmed());

    for entry in &entries {
        match read_manifest(&entry.path()) {
            Ok(manifest) => {
                println!(
                    "  {:<20} {:<10} {:<30} {}",
                    manifest.name.cyan(),
                    manifest.version,
                    manifest.description,
                    format_hooks(&manifest.hooks).dimmed(),
                );
            }
            Err(err) => {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                println!(
                    "  {:<20} {:<10} {}",
                    dir_name.yellow(),
                    "?",
                    format!("(error: {err})").red(),
                );
            }
        }
    }

    println!();
    Ok(())
}

pub fn run_install(source_path: &str) -> Result<(), String> {
    let source = Path::new(source_path);

    if !source.is_dir() {
        return Err(format!("'{}' is not a directory", source.display()));
    }

    let manifest = read_manifest(source)?;
    let dest = plugins_dir().join(&manifest.name);

    if dest.exists() {
        return Err(format!(
            "Plugin '{}' is already installed. Remove it first with: mhost plugin remove {}",
            manifest.name, manifest.name
        ));
    }

    fs::create_dir_all(plugins_dir())
        .map_err(|e| format!("Cannot create plugins directory: {e}"))?;

    copy_dir_recursive(source, &dest)
        .map_err(|e| format!("Failed to copy plugin directory: {e}"))?;

    output::print_success(&format!(
        "Installed plugin '{}' v{}",
        manifest.name, manifest.version
    ));
    Ok(())
}

pub fn run_remove(name: &str) -> Result<(), String> {
    let dir = plugins_dir().join(name);

    if !dir.exists() {
        return Err(format!("Plugin '{name}' is not installed"));
    }

    fs::remove_dir_all(&dir).map_err(|e| format!("Failed to remove plugin directory: {e}"))?;

    output::print_success(&format!("Removed plugin '{name}'"));
    Ok(())
}

pub fn run_info(name: &str) -> Result<(), String> {
    let dir = plugins_dir().join(name);

    if !dir.exists() {
        return Err(format!("Plugin '{name}' is not installed"));
    }

    let manifest = read_manifest(&dir)?;

    println!();
    println!("  {} {}", "Name:".bold(), manifest.name.cyan());
    println!("  {} {}", "Version:".bold(), manifest.version);
    println!("  {} {}", "Description:".bold(), manifest.description);
    println!(
        "  {} {}",
        "Path:".bold(),
        dir.display().to_string().dimmed()
    );
    println!("  {}", "Hooks:".bold());

    if let Some(ref h) = manifest.hooks.on_start {
        println!("    {} {}", "on_start:".dimmed(), h);
    }
    if let Some(ref h) = manifest.hooks.on_stop {
        println!("    {} {}", "on_stop:".dimmed(), h);
    }
    if let Some(ref h) = manifest.hooks.on_crash {
        println!("    {} {}", "on_crash:".dimmed(), h);
    }
    if manifest.hooks.on_start.is_none()
        && manifest.hooks.on_stop.is_none()
        && manifest.hooks.on_crash.is_none()
    {
        println!("    {}", "none".dimmed());
    }

    println!();
    Ok(())
}

// ---------------------------------------------------------------------------
// Utility: recursive directory copy
// ---------------------------------------------------------------------------

#[cfg(test)]
fn plugins_dir_from(home: &Path) -> PathBuf {
    home.join(".mhost").join("plugins")
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_dir_path() {
        let home = PathBuf::from("/home/testuser");
        let dir = plugins_dir_from(&home);
        assert_eq!(dir, PathBuf::from("/home/testuser/.mhost/plugins"));
    }

    #[test]
    fn test_plugin_manifest_parse() {
        let json = r#"{
            "name": "my-plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "hooks": {
                "on_start": "echo starting",
                "on_stop": null,
                "on_crash": "echo crashed"
            }
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert!(manifest.hooks.on_start.is_some());
        assert!(manifest.hooks.on_stop.is_none());
        assert!(manifest.hooks.on_crash.is_some());
    }

    #[test]
    fn test_plugin_manifest_parse_no_hooks() {
        let json = r#"{
            "name": "simple-plugin",
            "version": "0.1.0",
            "description": "No hooks"
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "simple-plugin");
        assert!(manifest.hooks.on_start.is_none());
        assert!(manifest.hooks.on_stop.is_none());
        assert!(manifest.hooks.on_crash.is_none());
    }

    #[test]
    fn test_format_hooks_all() {
        let hooks = PluginHooks {
            on_start: Some("start.sh".to_string()),
            on_stop: Some("stop.sh".to_string()),
            on_crash: Some("crash.sh".to_string()),
        };
        let result = format_hooks(&hooks);
        assert!(result.contains("on_start"));
        assert!(result.contains("on_stop"));
        assert!(result.contains("on_crash"));
    }

    #[test]
    fn test_format_hooks_none() {
        let hooks = PluginHooks::default();
        assert_eq!(format_hooks(&hooks), "none");
    }
}
