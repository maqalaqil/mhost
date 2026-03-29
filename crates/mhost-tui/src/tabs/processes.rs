use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        "#", "Name", "Status", "PID", "CPU%", "Memory", "Uptime", "Restarts",
    ])
    .style(Style::default().bold());

    let rows: Vec<Row> = app
        .processes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let row_style = if i == app.selected_process {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            let status_style = match p.status {
                mhost_core::process::ProcessStatus::Online => {
                    Style::default().fg(Color::Green)
                }
                mhost_core::process::ProcessStatus::Starting
                | mhost_core::process::ProcessStatus::Stopping => {
                    Style::default().fg(Color::Yellow)
                }
                _ => Style::default().fg(Color::Red),
            };

            Row::new(vec![
                Cell::from(i.to_string()),
                Cell::from(p.config.name.clone()),
                Cell::from(p.status.to_string()).style(status_style),
                Cell::from(p.pid.map(|v| v.to_string()).unwrap_or_else(|| "-".into())),
                Cell::from(
                    p.cpu_percent
                        .map(|v| format!("{:.1}", v))
                        .unwrap_or_else(|| "-".into()),
                ),
                Cell::from(
                    p.memory_bytes
                        .map(|v| format!("{:.1}MB", v as f64 / 1_048_576.0))
                        .unwrap_or_else(|| "-".into()),
                ),
                Cell::from(p.format_uptime()),
                Cell::from(p.restart_count.to_string()),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(4),
        Constraint::Length(20),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Processes "));

    f.render_widget(table, area);
}
