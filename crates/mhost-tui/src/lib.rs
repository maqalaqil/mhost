pub mod app;
pub mod input;
pub mod tabs;

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event},
    terminal,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Tabs, Wrap};

use mhost_core::paths::MhostPaths;
use mhost_core::process::ProcessInfo;
use mhost_core::protocol::methods;
use mhost_ipc::IpcClient;

use app::App;
use input::{handle_key, Action};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Start the TUI event loop.  Runs until the user presses `q` / `Ctrl-C`.
pub async fn run_tui(client: &IpcClient) -> Result<(), Box<dyn std::error::Error>> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_secs(1);
    let paths = MhostPaths::new();

    // Initial data fetch before first draw.
    fetch_processes(client, &mut app).await;
    fetch_logs(&paths, &mut app);

    while app.running {
        // Expire transient status messages older than 3 s.
        app.expire_status();

        terminal.draw(|f| draw(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_default();

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match handle_key(&mut app, key) {
                    Action::Quit => app.running = false,
                    Action::Restart => {
                        if let Some(name) = app.selected_process_name().map(String::from) {
                            send_action(client, methods::PROCESS_RESTART, &name, &mut app).await;
                        }
                    }
                    Action::Stop => {
                        if let Some(name) = app.selected_process_name().map(String::from) {
                            send_action(client, methods::PROCESS_STOP, &name, &mut app).await;
                        }
                    }
                    Action::Delete => {
                        if let Some(name) = app.selected_process_name().map(String::from) {
                            send_action(client, methods::PROCESS_DELETE, &name, &mut app).await;
                        }
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            fetch_processes(client, &mut app).await;
            fetch_logs(&paths, &mut app);
            app.record_metrics();
            app.tick_count = app.tick_count.wrapping_add(1);
            last_tick = std::time::Instant::now();
        }
    }

    terminal::disable_raw_mode()?;
    crossterm::execute!(io::stdout(), terminal::LeaveAlternateScreen)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn draw(f: &mut Frame, app: &App) {
    // Full frame: tab bar (3 lines) + content + footer (1 line).
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(0),    // content
            Constraint::Length(1), // footer
        ])
        .split(f.area());

    draw_tab_bar(f, root[0], app);

    match app.current_tab {
        app::Tab::Processes => draw_processes_tab(f, root[1], app),
        app::Tab::Logs => tabs::logs::render(f, root[1], app),
        app::Tab::Metrics => tabs::metrics::render(f, root[1], app),
        app::Tab::Proxy => tabs::proxy::render(f, root[1], app),
    }

    draw_footer(f, root[2], app);
}

// ---------------------------------------------------------------------------
// Tab bar
// ---------------------------------------------------------------------------

fn draw_tab_bar(f: &mut Frame, area: Rect, app: &App) {
    let tab_titles: Vec<Line> = vec![" Processes ", " Logs ", " Metrics ", " Proxy "]
        .into_iter()
        .map(Line::from)
        .collect();

    let selected_tab = match app.current_tab {
        app::Tab::Processes => 0,
        app::Tab::Logs => 1,
        app::Tab::Metrics => 2,
        app::Tab::Proxy => 3,
    };

    let tabs_widget = Tabs::new(tab_titles)
        .block(Block::default().borders(Borders::ALL).title(" mhost "))
        .select(selected_tab)
        .highlight_style(Style::default().fg(Color::Cyan).bold());

    f.render_widget(tabs_widget, area);
}

// ---------------------------------------------------------------------------
// Processes tab — split layout
// ---------------------------------------------------------------------------

fn draw_processes_tab(f: &mut Frame, area: Rect, app: &App) {
    // Vertical split: top list, middle row (details + sparklines), bottom logs.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(38), // process table
            Constraint::Percentage(32), // detail panel + sparklines
            Constraint::Percentage(30), // live log tail
        ])
        .split(area);

    // -- Process table --------------------------------------------------------
    tabs::processes::render(f, rows[0], app);

    // -- Details + sparklines (horizontal split) -------------------------------
    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);

    draw_detail_panel(f, middle[0], app);
    draw_sparklines(f, middle[1], app);

    // -- Live log tail --------------------------------------------------------
    tabs::logs::render_mini(f, rows[2], app, 8);
}

// ---------------------------------------------------------------------------
// Detail panel (left half of middle row)
// ---------------------------------------------------------------------------

fn draw_detail_panel(f: &mut Frame, area: Rect, app: &App) {
    let process = app
        .sorted_processes()
        .get(app.selected_process)
        .copied()
        .or_else(|| app.processes.first());

    // Pre-compute all strings that will be borrowed inside Line::from.
    struct DetailStrings {
        pid: String,
        status: String,
        status_color: Color,
        uptime: String,
        restarts: String,
        memory: String,
        cpu: String,
        command: String,
        cwd: String,
        health: String,
        health_color: Color,
        name: String,
    }

    let detail = process.map(|p| {
        let health_str = match p.health_status {
            mhost_core::health::HealthStatus::Healthy => "● healthy".to_string(),
            mhost_core::health::HealthStatus::Unhealthy => "● unhealthy".to_string(),
            mhost_core::health::HealthStatus::Unknown => "● unknown".to_string(),
            mhost_core::health::HealthStatus::Disabled => "  disabled".to_string(),
        };
        let health_color = match p.health_status {
            mhost_core::health::HealthStatus::Healthy => Color::Green,
            mhost_core::health::HealthStatus::Unhealthy => Color::Red,
            _ => Color::DarkGray,
        };
        let status_color = match p.status {
            mhost_core::process::ProcessStatus::Online => Color::Green,
            mhost_core::process::ProcessStatus::Starting
            | mhost_core::process::ProcessStatus::Stopping => Color::Yellow,
            _ => Color::Red,
        };
        let cmd = if p.config.args.is_empty() {
            p.config.command.clone()
        } else {
            format!("{} {}", p.config.command, p.config.args.join(" "))
        };
        DetailStrings {
            pid: p.pid.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            status: format!("● {}", p.status),
            status_color,
            uptime: p.format_uptime(),
            restarts: p.restart_count.to_string(),
            memory: p
                .memory_bytes
                .map(|v| format!("{:.1} MB", v as f64 / 1_048_576.0))
                .unwrap_or_else(|| "-".into()),
            cpu: p
                .cpu_percent
                .map(|v| format!("{:.1}%", v))
                .unwrap_or_else(|| "-".into()),
            command: cmd,
            cwd: p.config.cwd.clone().unwrap_or_else(|| "-".into()),
            health: health_str,
            health_color,
            name: p.config.name.clone(),
        }
    });

    let title = detail
        .as_ref()
        .map(|d| format!(" Details: {} ", d.name))
        .unwrap_or_else(|| " Details ".into());

    let content: Vec<Line> = match &detail {
        None => vec![Line::from("No process selected.")],
        Some(d) => vec![
            kv_line("PID:", &d.pid),
            kv_colored_line("Status:", &d.status, d.status_color),
            kv_line("Uptime:", &d.uptime),
            kv_line("Restarts:", &d.restarts),
            kv_line("Memory:", &d.memory),
            kv_line("CPU:", &d.cpu),
            kv_line("Command:", &d.command),
            kv_line("CWD:", &d.cwd),
            kv_colored_line("Health:", &d.health, d.health_color),
        ],
    };

    let paragraph = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Sparklines panel (right half of middle row)
// ---------------------------------------------------------------------------

fn draw_sparklines(f: &mut Frame, area: Rect, app: &App) {
    use crate::tabs::metrics::sparkline_str;

    let process = app
        .sorted_processes()
        .get(app.selected_process)
        .copied()
        .or_else(|| app.processes.first());

    let name = process.map(|p| p.config.name.as_str()).unwrap_or("—");

    // Split into CPU (top) and Memory (bottom).
    let halves = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // CPU
    {
        let data = app
            .cpu_history
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let inner_width = (halves[0].width as usize).saturating_sub(4);
        let spark = sparkline_str(data, inner_width);
        let last_cpu = process.and_then(|p| p.cpu_percent).unwrap_or(0.0);
        let text = format!("{} {:>5.1}%", spark, last_cpu);
        let para = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" CPU "))
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(para, halves[0]);
    }

    // Memory
    {
        let data = app
            .mem_history
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let inner_width = (halves[1].width as usize).saturating_sub(4);
        let spark = sparkline_str(data, inner_width);
        let last_mem = process.and_then(|p| p.memory_bytes).unwrap_or(0) as f64 / 1_048_576.0;
        let text = format!("{} {:>7.1}MB", spark, last_mem);
        let para = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Memory "))
            .style(Style::default().fg(Color::Magenta));
        f.render_widget(para, halves[1]);
    }
}

// ---------------------------------------------------------------------------
// Footer help bar
// ---------------------------------------------------------------------------

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    // If there is a status message, show it (overrides normal help text).
    let text = if let Some((msg, _)) = &app.status_message {
        Span::styled(
            format!(" {}", msg),
            Style::default().fg(Color::Yellow).bold(),
        )
    } else if app.search_mode {
        let q = app.search_query.as_deref().unwrap_or("");
        Span::styled(
            format!(" /{}_  Esc:cancel  Enter:confirm", q),
            Style::default().fg(Color::Cyan),
        )
    } else {
        Span::styled(
            " j/k:nav  Tab:tab  r:restart  s:stop  d:delete  /:search  S:sort  R:rev  q:quit",
            Style::default().fg(Color::DarkGray),
        )
    };

    let para = Paragraph::new(Line::from(text));
    f.render_widget(para, area);
}

// ---------------------------------------------------------------------------
// Line helpers
// ---------------------------------------------------------------------------

fn kv_line(key: &str, val: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:<12}", key), Style::default().fg(Color::DarkGray)),
        Span::raw(val.to_string()),
    ])
}

fn kv_colored_line(key: &str, val: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:<12}", key), Style::default().fg(Color::DarkGray)),
        Span::styled(val.to_string(), Style::default().fg(color)),
    ])
}

// ---------------------------------------------------------------------------
// Data fetching
// ---------------------------------------------------------------------------

async fn fetch_processes(client: &IpcClient, app: &mut App) {
    if let Ok(resp) = client
        .call(methods::PROCESS_LIST, serde_json::json!(null))
        .await
    {
        if let Some(result) = resp.result {
            // Accept both a plain array and a {"processes": [...]} wrapper.
            let procs: Option<Vec<ProcessInfo>> =
                serde_json::from_value(result.clone()).ok().or_else(|| {
                    result
                        .get("processes")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                });

            if let Some(procs) = procs {
                app.processes = procs;
                // Clamp selection to valid range.
                if !app.processes.is_empty() && app.selected_process >= app.processes.len() {
                    app.selected_process = app.processes.len() - 1;
                }
            }
        }
    }
}

fn fetch_logs(paths: &MhostPaths, app: &mut App) {
    let name = match app.selected_process_name() {
        Some(n) => n.to_string(),
        None => return,
    };
    let instance = app
        .processes
        .get(app.selected_process)
        .map(|p| p.instance)
        .unwrap_or(0);

    let log_path = paths.process_out_log(&name, instance);
    if let Ok(lines) = mhost_logs::reader::tail(&log_path, 500) {
        app.log_lines = lines;
    }
}

async fn send_action(client: &IpcClient, method: &str, name: &str, app: &mut App) {
    match client
        .call(method, serde_json::json!({ "name": name }))
        .await
    {
        Ok(resp) if resp.error.is_none() => {
            app.set_status(format!("OK: {}", name));
        }
        Ok(resp) => {
            let msg = resp
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| "unknown error".into());
            app.set_status(format!("Error: {}", msg));
        }
        Err(e) => {
            app.set_status(format!("IPC error: {}", e));
        }
    }
}
