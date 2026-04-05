use colored::Colorize;
use std::io::Write;

use crate::output::{print_error, print_success};

// ---------------------------------------------------------------------------
// HTML generation
// ---------------------------------------------------------------------------

fn generate_html() -> String {
    // In a real implementation this would call `mhost list` and `mhost brain status`
    // to populate live data.  For now we generate a static skeleton that can be
    // populated via the daemon later.
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>mhost Status</title>
<style>
  *,*::before,*::after{{box-sizing:border-box}}
  body{{
    margin:0;padding:0;
    font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;
    background:#0a0a0f;color:#e2e8f0;
  }}
  .container{{max-width:800px;margin:0 auto;padding:2rem 1rem}}
  h1{{color:#8b5cf6;font-size:1.8rem;margin-bottom:.25rem}}
  .subtitle{{color:#64748b;font-size:.9rem;margin-bottom:2rem}}
  .fleet-health{{
    background:#111118;border:1px solid #1e1e2e;border-radius:8px;
    padding:1.5rem;margin-bottom:2rem;
  }}
  .fleet-health h2{{margin:0 0 .75rem;font-size:1.1rem;color:#8b5cf6}}
  .health-bar-bg{{
    height:12px;border-radius:6px;background:#1e1e2e;overflow:hidden;
  }}
  .health-bar-fg{{
    height:100%;border-radius:6px;background:linear-gradient(90deg,#8b5cf6,#a78bfa);
    transition:width .3s;
  }}
  .health-pct{{font-size:2rem;font-weight:700;color:#a78bfa;margin-top:.5rem}}
  .process-card{{
    background:#111118;border:1px solid #1e1e2e;border-radius:8px;
    padding:1rem 1.25rem;margin-bottom:.75rem;
    display:flex;align-items:center;gap:1rem;
  }}
  .dot{{width:10px;height:10px;border-radius:50%;flex-shrink:0}}
  .dot.green{{background:#22c55e}}
  .dot.red{{background:#ef4444}}
  .dot.yellow{{background:#eab308}}
  .process-name{{font-weight:600;flex:1}}
  .process-meta{{color:#64748b;font-size:.85rem}}
  .score-bar-bg{{
    width:80px;height:6px;border-radius:3px;background:#1e1e2e;overflow:hidden;
  }}
  .score-bar-fg{{
    height:100%;border-radius:3px;background:#8b5cf6;
  }}
  .section-title{{
    color:#8b5cf6;font-size:1rem;font-weight:600;
    margin:2rem 0 1rem;
  }}
  .incident{{
    background:#111118;border:1px solid #1e1e2e;border-radius:8px;
    padding:.75rem 1rem;margin-bottom:.5rem;
    color:#94a3b8;font-size:.85rem;
  }}
  .incident .time{{color:#64748b}}
  .empty{{color:#475569;font-style:italic;padding:1rem 0}}
  footer{{
    text-align:center;color:#334155;font-size:.8rem;
    padding:3rem 0 2rem;
  }}
  footer a{{color:#8b5cf6;text-decoration:none}}
</style>
</head>
<body>
<div class="container">
  <h1>mhost Status</h1>
  <p class="subtitle">Generated at {timestamp}</p>

  <div class="fleet-health">
    <h2>Fleet Health</h2>
    <div class="health-bar-bg"><div class="health-bar-fg" style="width:100%"></div></div>
    <div class="health-pct">100%</div>
  </div>

  <div class="section-title">Processes</div>
  <p class="empty">No process data available — start some processes and regenerate.</p>

  <div class="section-title">Recent Incidents</div>
  <p class="empty">No incidents recorded.</p>

  <footer>Powered by <a href="https://github.com/maqalaqil/mhost">mhost</a></footer>
</div>
</body>
</html>"##,
        timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    )
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

pub fn run_generate() -> Result<(), String> {
    let html = generate_html();
    println!("{html}");
    Ok(())
}

pub fn run_serve(port: u16) -> Result<(), String> {
    let html = generate_html();
    let addr = format!("0.0.0.0:{port}");
    let listener =
        std::net::TcpListener::bind(&addr).map_err(|e| format!("Failed to bind to {addr}: {e}"))?;

    print_success(&format!("Status page serving at http://localhost:{port}"));
    println!("  {} Press Ctrl+C to stop\n", "▸".dimmed());

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html,
    );
    let response_bytes = response.into_bytes();

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let _ = stream.write_all(&response_bytes);
                let _ = stream.flush();
            }
            Err(e) => {
                print_error(&format!("Connection error: {e}"));
            }
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
    fn test_status_html_contains_mhost() {
        let html = generate_html();
        assert!(html.contains("mhost"), "HTML should contain 'mhost'");
        assert!(html.contains("mhost Status"), "HTML should contain title");
    }

    #[test]
    fn test_status_html_has_styles() {
        let html = generate_html();
        assert!(html.contains("<style>"), "HTML should have a style tag");
        assert!(html.contains("</style>"), "HTML should close the style tag");
    }

    #[test]
    fn test_status_html_is_valid_structure() {
        let html = generate_html();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<html"));
        assert!(html.contains("</html>"));
        assert!(html.contains("<body>"));
        assert!(html.contains("</body>"));
    }

    #[test]
    fn test_status_html_has_fleet_health() {
        let html = generate_html();
        assert!(html.contains("Fleet Health"));
        assert!(html.contains("100%"));
    }
}
