use colored::Colorize;
use mhost_core::MhostPaths;

pub fn run(paths: &MhostPaths) -> Result<(), String> {
    println!("\n  {} Process Dependencies\n", "🔗".cyan());

    let db_path = paths.db();
    if !db_path.exists() {
        println!("  No processes registered. Start some first.");
        return Ok(());
    }

    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT DISTINCT name FROM processes ORDER BY name")
        .map_err(|e| e.to_string())?;
    let names: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    if names.is_empty() {
        println!("  No processes found.");
        return Ok(());
    }

    println!("  {}", "─".repeat(40));
    for (i, name) in names.iter().enumerate() {
        let connector = if i == names.len() - 1 {
            "└──"
        } else {
            "├──"
        };
        println!(
            "  {} {} {}",
            connector.dimmed(),
            "●".green(),
            name.white().bold()
        );
    }
    println!("  {}", "─".repeat(40));
    println!("\n  Tip: Define groups in mhost.toml for dependency ordering.");
    println!();
    Ok(())
}
