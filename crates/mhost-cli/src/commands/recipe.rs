use colored::Colorize;
use std::path::Path;

use crate::output::{print_error, print_success};

pub fn run(file: &str) -> Result<(), String> {
    let path = Path::new(file);
    if !path.exists() {
        return Err(format!("Recipe file not found: {file}"));
    }

    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;

    // Parse as simple list of commands (one per line, # for comments)
    let commands: Vec<&str> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    println!(
        "\n  {} Running recipe: {} ({} steps)\n",
        "📋".cyan(),
        file.white().bold(),
        commands.len()
    );

    for (i, cmd) in commands.iter().enumerate() {
        println!("  [{}/{}] {}", i + 1, commands.len(), cmd.cyan());

        let full_cmd = if cmd.starts_with("mhost ") {
            cmd.to_string()
        } else {
            format!("mhost {cmd}")
        };
        let output = std::process::Command::new("sh")
            .args(["-c", &full_cmd])
            .output()
            .map_err(|e| format!("Failed to run '{cmd}': {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.trim().is_empty() {
            for line in stdout.lines() {
                println!("        {}", line.dimmed());
            }
        }

        if !output.status.success() {
            print_error(&format!("Step {} failed: {}", i + 1, stderr.trim()));
            return Err(format!("Recipe aborted at step {}", i + 1));
        }
        print_success(&format!("Step {} complete", i + 1));
    }

    println!(
        "\n  {} Recipe completed successfully!\n",
        "✔".green().bold()
    );
    Ok(())
}
