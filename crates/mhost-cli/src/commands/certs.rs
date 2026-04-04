use colored::Colorize;

pub async fn run(check_urls: Option<Vec<String>>) -> Result<(), String> {
    let urls = check_urls.unwrap_or_else(|| vec!["https://localhost:443".into()]);

    println!("\n  {} SSL Certificate Check\n", "🔒".cyan());
    println!(
        "  {:<30} {:<15} {}",
        "Host".bold(),
        "Expires In".bold(),
        "Status".bold()
    );
    println!("  {}", "─".repeat(60));

    for url in &urls {
        let host = url
            .replace("https://", "")
            .split('/')
            .next()
            .unwrap_or(url)
            .to_string();

        // Use openssl s_client to check cert
        let output = tokio::process::Command::new("sh")
            .args([
                "-c",
                &format!(
                    "echo | openssl s_client -connect {host}:443 -servername {host} 2>/dev/null \
                     | openssl x509 -noout -enddate 2>/dev/null"
                ),
            ])
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let expiry = stdout.trim().replace("notAfter=", "");
                let expiry_short: String = expiry.chars().take(15).collect();
                if expiry.is_empty() {
                    println!(
                        "  {:<30} {:<15} {}",
                        host,
                        expiry_short,
                        "? Unknown".dimmed()
                    );
                } else {
                    println!("  {:<30} {:<15} {}", host, expiry_short, "✔ Valid".green());
                }
            }
            _ => {
                println!("  {:<30} {:<15} {}", host, "–", "✖ Cannot connect".red());
            }
        }
    }
    println!();
    Ok(())
}
