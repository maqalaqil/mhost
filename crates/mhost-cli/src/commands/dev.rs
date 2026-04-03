use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use colored::Colorize;

/// Run a script in dev mode with polling-based file watching and auto-restart.
pub fn run(
    script: &str,
    watch_dir: Option<&str>,
    extensions: Option<&str>,
    env_file: Option<&str>,
) -> Result<(), String> {
    // Load .env file manually to avoid extra dependencies
    let env_path = env_file.unwrap_or(".env");
    if Path::new(env_path).exists() {
        load_env_file(env_path);
        println!("  {} Loaded {}", "▸".cyan(), env_path.dimmed());
    }

    // Determine interpreter
    let (cmd, args) = crate::commands::start::detect_interpreter(script, vec![]);

    // Resolve script path
    let script_path = std::env::current_dir()
        .ok()
        .map(|d| d.join(script))
        .unwrap_or_else(|| PathBuf::from(script));

    // Watch directory
    let watch_path = watch_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Extensions to watch
    let exts: Vec<String> = extensions
        .unwrap_or("js,ts,mjs,py,rb,rs,go,json,toml,yaml,yml,sh")
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    println!();
    println!(
        "  {} {} {}",
        "⚡".cyan(),
        "mhost dev".bold(),
        script.white()
    );
    println!(
        "  {}  Watching: {}",
        " ".dimmed(),
        watch_path.display().to_string().dimmed()
    );
    println!(
        "  {}  Extensions: {}",
        " ".dimmed(),
        exts.join(", ").dimmed()
    );
    println!("  {}  Press Ctrl+C to stop", " ".dimmed());
    println!("  {}", "─".repeat(60).dimmed());

    let mut child = spawn_dev_process(&cmd, &args, &script_path)?;
    let mut last_modified = get_dir_mtime(&watch_path, &exts);

    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));

        let current = get_dir_mtime(&watch_path, &exts);
        if current > last_modified {
            last_modified = current;
            println!();
            println!("  {} File changed — restarting...", "↻".yellow());

            let _ = child.kill();
            let _ = child.wait();

            child = spawn_dev_process(&cmd, &args, &script_path)?;
        }

        // Check if process exited
        if let Ok(Some(status)) = child.try_wait() {
            println!("  {} Process exited with {}", "✖".red(), status);
            println!("  {}  Waiting for file changes to restart...", " ".dimmed());

            // Wait for a file change to restart
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let current = get_dir_mtime(&watch_path, &exts);
                if current > last_modified {
                    last_modified = current;
                    println!("  {} File changed — restarting...", "↻".yellow());
                    child = spawn_dev_process(&cmd, &args, &script_path)?;
                    break;
                }
            }
        }
    }
}

fn spawn_dev_process(cmd: &str, args: &[String], script_path: &Path) -> Result<Child, String> {
    let mut command = Command::new(cmd);
    command.args(args);
    // Only append script_path if it's not already included in args
    if args.is_empty()
        || !args
            .iter()
            .any(|a| a.contains(script_path.to_string_lossy().as_ref()))
    {
        command.arg(script_path);
    }
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    command
        .spawn()
        .map_err(|e| format!("Failed to start process: {e}"))
}

fn get_dir_mtime(dir: &Path, exts: &[String]) -> std::time::SystemTime {
    let mut latest = std::time::SystemTime::UNIX_EPOCH;
    let paths = collect_watched_files(dir, exts, 3);
    for path in paths {
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if modified > latest {
                    latest = modified;
                }
            }
        }
    }
    latest
}

fn collect_watched_files(dir: &Path, exts: &[String], max_depth: usize) -> Vec<PathBuf> {
    let mut results = Vec::new();
    collect_files_inner(dir, exts, max_depth, 0, &mut results);
    results
}

fn collect_files_inner(
    dir: &Path,
    exts: &[String],
    max_depth: usize,
    depth: usize,
    results: &mut Vec<PathBuf>,
) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // Skip hidden dirs, node_modules, target, __pycache__
        if name.starts_with('.')
            || name == "node_modules"
            || name == "target"
            || name == "__pycache__"
        {
            continue;
        }
        if path.is_dir() {
            collect_files_inner(&path, exts, max_depth, depth + 1, results);
        } else if let Some(ext) = path.extension() {
            if exts.iter().any(|e| e == ext.to_string_lossy().as_ref()) {
                results.push(path);
            }
        }
    }
}

/// Load a .env file and set environment variables without external dependencies.
fn load_env_file(path: &str) {
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            let trimmed = line.trim();
            // Skip blank lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');
                if !key.is_empty() {
                    // Only set if not already in environment
                    if std::env::var(key).is_err() {
                        std::env::set_var(key, value);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_collect_watched_files_empty_dir() {
        let tmp = std::env::temp_dir().join("mhost_dev_test_empty");
        let _ = std::fs::create_dir_all(&tmp);
        let exts = vec!["js".to_string()];
        let files = collect_watched_files(&tmp, &exts, 1);
        // Should return empty or only matching files
        assert!(files
            .iter()
            .all(|f| { f.extension().map(|e| e == "js").unwrap_or(false) }));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_get_dir_mtime_nonexistent() {
        let path = PathBuf::from("/nonexistent/path/that/does/not/exist");
        let exts = vec!["js".to_string()];
        // Should not panic, returns UNIX_EPOCH
        let mtime = get_dir_mtime(&path, &exts);
        assert_eq!(mtime, std::time::SystemTime::UNIX_EPOCH);
    }

    #[test]
    fn test_load_env_file_parses_values() {
        let tmp = std::env::temp_dir().join("mhost_test_env.env");
        std::fs::write(
            &tmp,
            "# comment\nTEST_KEY_MHOST_DEV=hello\nQUOTED=\"world\"\n",
        )
        .unwrap();
        // Clear keys first if they exist
        std::env::remove_var("TEST_KEY_MHOST_DEV");
        std::env::remove_var("QUOTED");

        load_env_file(tmp.to_str().unwrap());

        assert_eq!(std::env::var("TEST_KEY_MHOST_DEV").unwrap(), "hello");
        assert_eq!(std::env::var("QUOTED").unwrap(), "world");

        let _ = std::fs::remove_file(&tmp);
    }
}
