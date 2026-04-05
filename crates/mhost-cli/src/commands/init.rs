use std::path::Path;

use crate::output::{print_error, print_success};

// ---------------------------------------------------------------------------
// Detection result (immutable value object)
// ---------------------------------------------------------------------------

struct DetectedStack {
    name: String,
    command: String,
    args: Vec<String>,
    port: Option<u16>,
    description: String,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Scan the current directory and generate an `mhost.toml` ecosystem config.
pub fn run() -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| format!("Cannot read current directory: {e}"))?;
    let output_path = cwd.join("mhost.toml");

    if output_path.exists() {
        print_error("mhost.toml already exists in this directory. Remove it first or use a different directory.");
        return Ok(());
    }

    let detections = detect_stacks(&cwd);

    if detections.is_empty() {
        println!("No recognised project files found in {}", cwd.display());
        println!("Supported: package.json, requirements.txt, pyproject.toml, Cargo.toml, go.mod, Gemfile, Procfile, docker-compose.yml");
        return Ok(());
    }

    let toml_content = generate_toml(&detections);

    std::fs::write(&output_path, &toml_content)
        .map_err(|e| format!("Failed to write mhost.toml: {e}"))?;

    println!();
    println!("Detected stacks:");
    for d in &detections {
        println!("  - {} ({})", d.name, d.description);
    }
    println!();
    print_success(&format!("Generated {}", output_path.display()));

    Ok(())
}

// ---------------------------------------------------------------------------
// Stack detection
// ---------------------------------------------------------------------------

fn detect_stacks(dir: &Path) -> Vec<DetectedStack> {
    let mut results = Vec::new();

    // 1. Procfile — parse directly, takes priority
    let procfile = dir.join("Procfile");
    if procfile.exists() {
        if let Ok(content) = std::fs::read_to_string(&procfile) {
            for line in content.lines() {
                if let Some(det) = parse_procfile_line(line) {
                    results.push(det);
                }
            }
            if !results.is_empty() {
                return results;
            }
        }
    }

    // 2. docker-compose.yml
    let dc = dir.join("docker-compose.yml");
    let dc_yaml = dir.join("docker-compose.yaml");
    if dc.exists() || dc_yaml.exists() {
        results.push(DetectedStack {
            name: "docker".to_string(),
            command: "docker".to_string(),
            args: vec!["compose".to_string(), "up".to_string()],
            port: None,
            description: "Docker Compose project".to_string(),
        });
    }

    // 3. package.json (Node.js)
    let pkg_json = dir.join("package.json");
    if pkg_json.exists() {
        if let Some(det) = detect_node(&pkg_json) {
            results.push(det);
        }
    }

    // 4. requirements.txt or pyproject.toml (Python)
    let req_txt = dir.join("requirements.txt");
    let pyproject = dir.join("pyproject.toml");
    if req_txt.exists() || pyproject.exists() {
        results.push(DetectedStack {
            name: "app".to_string(),
            command: "python3".to_string(),
            args: vec!["app.py".to_string()],
            port: Some(8000),
            description: "Python project".to_string(),
        });
    }

    // 5. Cargo.toml (Rust)
    let cargo_toml = dir.join("Cargo.toml");
    if cargo_toml.exists() {
        let name = detect_cargo_name(&cargo_toml);
        results.push(DetectedStack {
            name: name.clone(),
            command: "cargo".to_string(),
            args: vec!["run".to_string()],
            port: None,
            description: format!("Rust project ({name})"),
        });
    }

    // 6. go.mod (Go)
    let go_mod = dir.join("go.mod");
    if go_mod.exists() {
        results.push(DetectedStack {
            name: "app".to_string(),
            command: "go".to_string(),
            args: vec!["run".to_string(), ".".to_string()],
            port: None,
            description: "Go project".to_string(),
        });
    }

    // 7. Gemfile (Ruby)
    let gemfile = dir.join("Gemfile");
    if gemfile.exists() {
        let has_rails = std::fs::read_to_string(&gemfile)
            .map(|c| c.contains("rails"))
            .unwrap_or(false);
        if has_rails {
            results.push(DetectedStack {
                name: "web".to_string(),
                command: "bundle".to_string(),
                args: vec![
                    "exec".to_string(),
                    "rails".to_string(),
                    "server".to_string(),
                ],
                port: Some(3000),
                description: "Ruby on Rails".to_string(),
            });
        } else {
            results.push(DetectedStack {
                name: "app".to_string(),
                command: "bundle".to_string(),
                args: vec!["exec".to_string(), "ruby".to_string(), "app.rb".to_string()],
                port: None,
                description: "Ruby project".to_string(),
            });
        }
    }

    results
}

fn detect_node(pkg_path: &Path) -> Option<DetectedStack> {
    let content = std::fs::read_to_string(pkg_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

    let scripts = parsed.get("scripts")?;
    let start_script = scripts.get("start").and_then(|v| v.as_str());

    let (command, args, desc) = match start_script {
        Some(script) => {
            let parts: Vec<&str> = script.split_whitespace().collect();
            if parts.is_empty() {
                return None;
            }
            let cmd = parts[0].to_string();
            let rest: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
            (cmd, rest, format!("Node.js (scripts.start: {script})"))
        }
        None => (
            "node".to_string(),
            vec!["index.js".to_string()],
            "Node.js (default)".to_string(),
        ),
    };

    // Try to detect port from start script or common env
    let port = start_script
        .and_then(|s| extract_port_from_string(s))
        .or(Some(3000));

    Some(DetectedStack {
        name: "app".to_string(),
        command,
        args,
        port,
        description: desc,
    })
}

fn detect_cargo_name(cargo_path: &Path) -> String {
    std::fs::read_to_string(cargo_path)
        .ok()
        .and_then(|content| {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("name") {
                    if let Some(val) = trimmed.split('=').nth(1) {
                        let cleaned = val.trim().trim_matches('"').trim_matches('\'');
                        if !cleaned.is_empty() {
                            return Some(cleaned.to_string());
                        }
                    }
                }
            }
            None
        })
        .unwrap_or_else(|| "app".to_string())
}

fn parse_procfile_line(line: &str) -> Option<DetectedStack> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (name, rest) = trimmed.split_once(':')?;
    let cmd_str = rest.trim();
    let parts: Vec<&str> = cmd_str.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let command = parts[0].to_string();
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
    let port = extract_port_from_string(cmd_str);

    Some(DetectedStack {
        name: name.trim().to_string(),
        command,
        args,
        port,
        description: "Procfile entry".to_string(),
    })
}

fn extract_port_from_string(s: &str) -> Option<u16> {
    // Look for common port patterns: --port 3000, -p 8080, PORT=3000, :3000
    let patterns = ["--port ", "-p "];
    for pat in patterns {
        if let Some(idx) = s.find(pat) {
            let after = &s[idx + pat.len()..];
            if let Some(num_str) = after.split_whitespace().next() {
                if let Ok(port) = num_str.parse::<u16>() {
                    return Some(port);
                }
            }
        }
    }
    // PORT=NNNN
    if let Some(idx) = s.find("PORT=") {
        let after = &s[idx + 5..];
        if let Some(num_str) = after.split(|c: char| !c.is_ascii_digit()).next() {
            if let Ok(port) = num_str.parse::<u16>() {
                return Some(port);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// TOML generation
// ---------------------------------------------------------------------------

fn generate_toml(stacks: &[DetectedStack]) -> String {
    let mut out = String::from("# mhost ecosystem config — auto-generated by `mhost init`\n\n");

    for stack in stacks {
        out.push_str(&format!("[process.{}]\n", stack.name));
        out.push_str(&format!("command = \"{}\"\n", stack.command));

        if !stack.args.is_empty() {
            let quoted: Vec<String> = stack.args.iter().map(|a| format!("\"{a}\"")).collect();
            out.push_str(&format!("args = [{}]\n", quoted.join(", ")));
        }

        out.push_str("instances = 1\n");
        out.push_str("max_restarts = 10\n");
        out.push_str("restart_delay_ms = 1000\n");

        if let Some(port) = stack.port {
            out.push('\n');
            out.push_str(&format!("[process.{}.health]\n", stack.name));
            out.push_str(&format!(
                "kind = \"http\"\nurl = \"http://localhost:{port}/\"\ninterval_secs = 30\ntimeout_secs = 5\n"
            ));
        }

        out.push('\n');
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_port_flag() {
        assert_eq!(
            extract_port_from_string("node server.js --port 4000"),
            Some(4000)
        );
    }

    #[test]
    fn test_extract_port_env() {
        assert_eq!(
            extract_port_from_string("PORT=8080 node app.js"),
            Some(8080)
        );
    }

    #[test]
    fn test_extract_port_none() {
        assert_eq!(extract_port_from_string("node app.js"), None);
    }

    #[test]
    fn test_parse_procfile_line_valid() {
        let det = parse_procfile_line("web: node server.js --port 3000").unwrap();
        assert_eq!(det.name, "web");
        assert_eq!(det.command, "node");
        assert_eq!(det.port, Some(3000));
    }

    #[test]
    fn test_parse_procfile_line_comment() {
        assert!(parse_procfile_line("# comment").is_none());
    }

    #[test]
    fn test_parse_procfile_line_empty() {
        assert!(parse_procfile_line("").is_none());
    }

    #[test]
    fn test_generate_toml_basic() {
        let stacks = vec![DetectedStack {
            name: "api".to_string(),
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            port: Some(3000),
            description: "test".to_string(),
        }];
        let toml = generate_toml(&stacks);
        assert!(toml.contains("[process.api]"));
        assert!(toml.contains("command = \"node\""));
        assert!(toml.contains("url = \"http://localhost:3000/\""));
    }

    #[test]
    fn test_generate_toml_no_port_no_health() {
        let stacks = vec![DetectedStack {
            name: "worker".to_string(),
            command: "cargo".to_string(),
            args: vec!["run".to_string()],
            port: None,
            description: "test".to_string(),
        }];
        let toml = generate_toml(&stacks);
        assert!(toml.contains("[process.worker]"));
        assert!(!toml.contains("[process.worker.health]"));
    }
}
