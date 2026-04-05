use std::process::Command;

use colored::Colorize;

// ---------------------------------------------------------------------------
// Public command runners
// ---------------------------------------------------------------------------

/// Run a new Docker container managed by mhost.
pub fn run_docker_run(
    image: &str,
    name: &str,
    port: Option<u16>,
    envs: &[String],
) -> Result<(), String> {
    let mut args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--label".to_string(),
        "mhost=true".to_string(),
    ];

    if let Some(p) = port {
        args.push("-p".to_string());
        args.push(format!("{p}:{p}"));
    }

    for env_pair in envs {
        args.push("-e".to_string());
        args.push(env_pair.clone());
    }

    args.push(image.to_string());

    let output = run_docker_cmd(&args)?;
    let container_id = output.trim();
    println!(
        "{} Container '{}' started ({})",
        "[mhost]".green().bold(),
        name.cyan(),
        &container_id[..12.min(container_id.len())]
    );
    Ok(())
}

/// List all mhost-managed containers.
pub fn run_docker_list() -> Result<(), String> {
    let args = [
        "ps",
        "-a",
        "--filter",
        "label=mhost=true",
        "--format",
        "table {{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}",
    ];
    let output = run_docker_cmd(&args)?;
    if output.trim().is_empty() {
        println!(
            "{} No mhost-managed containers found.",
            "[mhost]".yellow().bold()
        );
    } else {
        println!("{} Docker containers:\n", "[mhost]".green().bold());
        println!("{output}");
    }
    Ok(())
}

/// Stop an mhost-managed container by name.
pub fn run_docker_stop(name: &str) -> Result<(), String> {
    run_docker_cmd(&["stop", name])?;
    println!(
        "{} Container '{}' stopped.",
        "[mhost]".green().bold(),
        name.cyan()
    );
    Ok(())
}

/// Restart an mhost-managed container by name.
pub fn run_docker_restart(name: &str) -> Result<(), String> {
    run_docker_cmd(&["restart", name])?;
    println!(
        "{} Container '{}' restarted.",
        "[mhost]".green().bold(),
        name.cyan()
    );
    Ok(())
}

/// Show logs for an mhost-managed container.
pub fn run_docker_logs(name: &str, lines: usize) -> Result<(), String> {
    let tail_arg = lines.to_string();
    let output = run_docker_cmd(&["logs", "--tail", &tail_arg, name])?;
    println!("{} Logs for '{}':\n", "[mhost]".green().bold(), name.cyan());
    println!("{output}");
    Ok(())
}

/// Remove an mhost-managed container.
pub fn run_docker_rm(name: &str) -> Result<(), String> {
    run_docker_cmd(&["rm", "-f", name])?;
    println!(
        "{} Container '{}' removed.",
        "[mhost]".green().bold(),
        name.cyan()
    );
    Ok(())
}

/// Pull a Docker image.
pub fn run_docker_pull(image: &str) -> Result<(), String> {
    println!(
        "{} Pulling image '{}'...",
        "[mhost]".green().bold(),
        image.cyan()
    );
    run_docker_cmd(&["pull", image])?;
    println!(
        "{} Image '{}' pulled successfully.",
        "[mhost]".green().bold(),
        image.cyan()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
fn build_docker_run_args(
    image: &str,
    name: &str,
    port: Option<u16>,
    envs: &[String],
) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--label".to_string(),
        "mhost=true".to_string(),
    ];

    if let Some(p) = port {
        args.push("-p".to_string());
        args.push(format!("{p}:{p}"));
    }

    for env_pair in envs {
        args.push("-e".to_string());
        args.push(env_pair.clone());
    }

    args.push(image.to_string());
    args
}

#[cfg(test)]
fn build_docker_list_args() -> Vec<&'static str> {
    vec![
        "ps",
        "-a",
        "--filter",
        "label=mhost=true",
        "--format",
        "table {{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}",
    ]
}

fn run_docker_cmd<S: AsRef<str>>(args: &[S]) -> Result<String, String> {
    let str_args: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();

    let output = Command::new("docker")
        .args(&str_args)
        .output()
        .map_err(|e| format!("Failed to execute docker: {e}. Is Docker installed?"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker {} failed: {}", str_args.join(" "), stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_run_command_builds_correctly() {
        let args = build_docker_run_args(
            "nginx:latest",
            "my-nginx",
            Some(8080),
            &["FOO=bar".to_string(), "BAZ=qux".to_string()],
        );
        assert_eq!(args[0], "run");
        assert_eq!(args[1], "-d");
        assert_eq!(args[2], "--name");
        assert_eq!(args[3], "my-nginx");
        assert!(args.contains(&"8080:8080".to_string()));
        assert!(args.contains(&"FOO=bar".to_string()));
        assert!(args.contains(&"BAZ=qux".to_string()));
        assert_eq!(args.last().unwrap(), "nginx:latest");
    }

    #[test]
    fn test_docker_run_command_no_port() {
        let args = build_docker_run_args("alpine", "worker", None, &[]);
        assert!(!args.iter().any(|a| a == "-p"));
        assert_eq!(args.last().unwrap(), "alpine");
    }

    #[test]
    fn test_docker_label_filter() {
        let args = build_docker_list_args();
        assert!(args.contains(&"label=mhost=true"));
    }

    #[test]
    fn test_docker_run_args_contain_label() {
        let args = build_docker_run_args("img", "name", None, &[]);
        assert!(args.contains(&"--label".to_string()));
        assert!(args.contains(&"mhost=true".to_string()));
    }
}
