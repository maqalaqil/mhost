use colored::Colorize;

pub fn run() -> Result<(), String> {
    println!("\n  {} mhost Playground\n", "🎮".cyan());
    println!("  Interactive tutorial — try mhost without installing.\n");
    println!(
        "  Visit: {}\n",
        "https://mhostai.com/playground".cyan().bold()
    );
    println!("  Or run locally:");
    println!(
        "    {} Start demo processes",
        "mhost start examples/mhost.toml".cyan()
    );
    println!("    {} Watch them run", "mhost monit".cyan());
    println!(
        "    {} Crash one and watch the brain heal it",
        "mhost delete api && mhost agent start".cyan()
    );
    println!();
    Ok(())
}
