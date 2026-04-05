use std::fs;

use colored::Colorize;

// ---------------------------------------------------------------------------
// Template definitions
// ---------------------------------------------------------------------------

struct Template {
    name: &'static str,
    stack: &'static str,
    command: &'static str,
    port: Option<u16>,
    health: Option<&'static str>,
    cron: bool,
}

const TEMPLATES: &[Template] = &[
    Template {
        name: "nextjs",
        stack: "Next.js",
        command: "npm run start",
        port: Some(3000),
        health: Some("/api/health"),
        cron: false,
    },
    Template {
        name: "react",
        stack: "React (static)",
        command: "npx serve -s build",
        port: Some(3000),
        health: None,
        cron: false,
    },
    Template {
        name: "express",
        stack: "Express.js",
        command: "node server.js",
        port: Some(3000),
        health: Some("/health"),
        cron: false,
    },
    Template {
        name: "fastapi",
        stack: "FastAPI",
        command: "uvicorn main:app --host 0.0.0.0",
        port: Some(8000),
        health: Some("/health"),
        cron: false,
    },
    Template {
        name: "django",
        stack: "Django",
        command: "gunicorn myapp.wsgi --bind 0.0.0.0:8000",
        port: Some(8000),
        health: None,
        cron: false,
    },
    Template {
        name: "rails",
        stack: "Ruby on Rails",
        command: "rails server -b 0.0.0.0",
        port: Some(3000),
        health: None,
        cron: false,
    },
    Template {
        name: "go",
        stack: "Go",
        command: "./main",
        port: Some(8080),
        health: Some("/healthz"),
        cron: false,
    },
    Template {
        name: "rust",
        stack: "Rust",
        command: "./target/release/myapp",
        port: Some(8080),
        health: None,
        cron: false,
    },
    Template {
        name: "python-worker",
        stack: "Python Worker",
        command: "python3 worker.py",
        port: None,
        health: None,
        cron: true,
    },
    Template {
        name: "static-site",
        stack: "Static Site",
        command: "npx serve .",
        port: Some(3000),
        health: None,
        cron: false,
    },
];

// ---------------------------------------------------------------------------
// Public command runners
// ---------------------------------------------------------------------------

/// Print a table of all available templates.
pub fn run_list() -> Result<(), String> {
    println!("{} Available templates:\n", "[mhost]".green().bold());
    println!(
        "  {:<16} {:<16} {:<42} {:<6} {}",
        "Name".bold(),
        "Stack".bold(),
        "Command".bold(),
        "Port".bold(),
        "Health Check".bold(),
    );
    println!("  {}", "-".repeat(96));

    for t in TEMPLATES {
        let port_str = t
            .port
            .map(|p| p.to_string())
            .unwrap_or_else(|| "-".to_string());
        let health_str = t.health.unwrap_or("-");

        println!(
            "  {:<16} {:<16} {:<42} {:<6} {}",
            t.name.cyan(),
            t.stack,
            t.command,
            port_str,
            health_str,
        );
    }
    println!();
    println!(
        "  Use {} to generate an mhost.toml from a template.",
        "mhost template init <name>".yellow()
    );
    Ok(())
}

/// Generate an mhost.toml in the current directory from a named template.
pub fn run_init(name: &str) -> Result<(), String> {
    let tmpl = TEMPLATES.iter().find(|t| t.name == name).ok_or_else(|| {
        format!("Unknown template '{name}'. Run 'mhost template list' to see available templates.")
    })?;

    let toml_content = generate_toml(tmpl);

    let dest = "mhost.toml";
    if std::path::Path::new(dest).exists() {
        return Err(
            "mhost.toml already exists in the current directory. Remove it first or use a different directory.".to_string()
        );
    }

    fs::write(dest, &toml_content).map_err(|e| format!("Failed to write {dest}: {e}"))?;

    println!(
        "{} Created {} from '{}' template.",
        "[mhost]".green().bold(),
        dest.cyan(),
        tmpl.name.cyan()
    );
    println!(
        "  Edit the file, then run {} to start.",
        "mhost start mhost.toml".yellow()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn generate_toml(tmpl: &Template) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "# mhost.toml — generated from the '{}' template",
        tmpl.name
    ));
    lines.push(format!("# Stack: {}", tmpl.stack));
    lines.push(String::new());
    lines.push("[[apps]]".to_string());
    lines.push(format!("name = \"my-{}\"", tmpl.name));
    lines.push(format!("command = \"{}\"", tmpl.command));

    if let Some(port) = tmpl.port {
        lines.push(String::new());
        lines.push("# Port this application listens on.".to_string());
        lines.push(format!("port = {port}"));
    }

    lines.push(String::new());
    lines.push("# Number of instances to run (1 = single, >1 = cluster mode).".to_string());
    lines.push("instances = 1".to_string());

    lines.push(String::new());
    lines.push("# Restart limits.".to_string());
    lines.push("max_restarts = 15".to_string());
    lines.push("min_uptime_ms = 1000".to_string());
    lines.push("restart_delay_ms = 100".to_string());

    if let Some(health) = tmpl.health {
        lines.push(String::new());
        lines.push("# Health check configuration.".to_string());
        lines.push("[apps.health]".to_string());
        if let Some(port) = tmpl.port {
            lines.push(format!("url = \"http://localhost:{port}{health}\""));
        } else {
            lines.push(format!("url = \"http://localhost:8080{health}\""));
        }
        lines.push("interval_secs = 30".to_string());
        lines.push("timeout_secs = 5".to_string());
        lines.push("max_failures = 3".to_string());
    }

    if tmpl.cron {
        lines.push(String::new());
        lines.push("# Cron schedule (runs every 5 minutes by default).".to_string());
        lines.push("cron_restart = \"*/5 * * * *\"".to_string());
    }

    lines.push(String::new());
    lines.push("# Environment variables.".to_string());
    lines.push("[apps.env]".to_string());
    lines.push("NODE_ENV = \"production\"".to_string());

    lines.push(String::new());

    lines.join("\n")
}
