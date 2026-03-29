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
use ratatui::widgets::{Block, Borders, Tabs};

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

    // Initial data fetch before first draw
    fetch_processes(client, &mut app).await;

    while app.running {
        terminal.draw(|f| draw(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_default();

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match handle_key(&mut app, key) {
                    Action::Quit => app.running = false,
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            fetch_processes(client, &mut app).await;
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(f.area());

    // Tab bar
    let tab_titles: Vec<Line> = vec!["Processes", "Logs", "Metrics", "Proxy"]
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
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs_widget, chunks[0]);

    // Active tab content
    match app.current_tab {
        app::Tab::Processes => tabs::processes::render(f, chunks[1], app),
        app::Tab::Logs => tabs::logs::render(f, chunks[1], app),
        app::Tab::Metrics => tabs::metrics::render(f, chunks[1], app),
        app::Tab::Proxy => tabs::proxy::render(f, chunks[1], app),
    }
}

// ---------------------------------------------------------------------------
// Data fetching
// ---------------------------------------------------------------------------

async fn fetch_processes(client: &IpcClient, app: &mut App) {
    if let Ok(resp) = client.call(methods::PROCESS_LIST, serde_json::json!(null)).await {
        if let Some(result) = resp.result {
            if let Ok(procs) = serde_json::from_value::<Vec<ProcessInfo>>(result) {
                app.processes = procs;
            }
        }
    }
}
