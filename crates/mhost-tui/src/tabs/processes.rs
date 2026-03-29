use ratatui::prelude::*;
use ratatui::widgets::*;

use mhost_core::process::ProcessStatus;

use crate::app::{App, SortColumn};

// ---------------------------------------------------------------------------
// Status dot helper
// ---------------------------------------------------------------------------

fn status_dot_and_label(status: &ProcessStatus) -> (char, Color) {
    match status {
        ProcessStatus::Online => ('●', Color::Green),
        ProcessStatus::Starting | ProcessStatus::Stopping => ('●', Color::Yellow),
        ProcessStatus::Stopped | ProcessStatus::Errored => ('●', Color::Red),
    }
}

// ---------------------------------------------------------------------------
// Column header with sort indicator
// ---------------------------------------------------------------------------

fn col_header(label: &str, sort_col: &SortColumn, ascending: bool, target: SortColumn) -> String {
    if *sort_col == target {
        let arrow = if ascending { "▲" } else { "▼" };
        format!("{} {}", label, arrow)
    } else {
        label.to_string()
    }
}

// ---------------------------------------------------------------------------
// render
// ---------------------------------------------------------------------------

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let name_hdr = col_header("Name", &app.sort_by, app.sort_ascending, SortColumn::Name);
    let status_hdr = col_header("Status", &app.sort_by, app.sort_ascending, SortColumn::Status);
    let cpu_hdr = col_header("CPU%", &app.sort_by, app.sort_ascending, SortColumn::Cpu);
    let mem_hdr = col_header("Memory", &app.sort_by, app.sort_ascending, SortColumn::Memory);
    let up_hdr = col_header("Uptime", &app.sort_by, app.sort_ascending, SortColumn::Uptime);
    let rst_hdr = col_header("↺", &app.sort_by, app.sort_ascending, SortColumn::Restarts);

    let header = Row::new(vec![
        Cell::from("#"),
        Cell::from(name_hdr),
        Cell::from(status_hdr),
        Cell::from("PID"),
        Cell::from(cpu_hdr),
        Cell::from(mem_hdr),
        Cell::from(up_hdr),
        Cell::from(rst_hdr),
    ])
    .style(Style::default().bold())
    .bottom_margin(1);

    let sorted = app.sorted_processes();

    let rows: Vec<Row> = sorted
        .iter()
        .enumerate()
        .map(|(display_idx, p)| {
            let row_style = if display_idx == app.selected_process {
                Style::default().bg(Color::DarkGray).bold()
            } else {
                Style::default()
            };

            let (dot, dot_color) = status_dot_and_label(&p.status);
            let status_text = format!("{} {}", dot, p.status);

            Row::new(vec![
                Cell::from(display_idx.to_string()),
                Cell::from(p.config.name.clone()),
                Cell::from(status_text).style(Style::default().fg(dot_color)),
                Cell::from(
                    p.pid
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".into()),
                ),
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
        Constraint::Length(22),
        Constraint::Length(14),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(11),
        Constraint::Length(13),
        Constraint::Length(5),
    ];

    let title = if app.search_mode {
        format!(
            " Processes — search: {} ",
            app.search_query.as_deref().unwrap_or("")
        )
    } else if let Some(q) = &app.search_query {
        if !q.is_empty() {
            format!(" Processes [filter: {}] ", q)
        } else {
            " Processes ".into()
        }
    } else {
        " Processes ".into()
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(Style::default().bg(Color::DarkGray));

    f.render_widget(table, area);
}
