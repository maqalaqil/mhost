use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let text = if let Some(p) = app.processes.get(app.selected_process) {
        format!(
            "Process: {}\nCPU: {:.1}%\nMemory: {:.1} MB\nUptime: {}\nRestarts: {}",
            p.config.name,
            p.cpu_percent.unwrap_or(0.0),
            p.memory_bytes.unwrap_or(0) as f64 / 1_048_576.0,
            p.format_uptime(),
            p.restart_count,
        )
    } else {
        "No process selected".into()
    };

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Metrics "));

    f.render_widget(paragraph, area);
}
