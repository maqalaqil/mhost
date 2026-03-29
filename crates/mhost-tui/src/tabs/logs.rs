use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::App;

// ---------------------------------------------------------------------------
// Log level colour helper
// ---------------------------------------------------------------------------

fn line_color(line: &str) -> Color {
    let lower = line.to_lowercase();
    if lower.contains("\"level\":\"error\"")
        || lower.contains("level=error")
        || lower.contains("[error]")
        || lower.contains("error:")
    {
        Color::Red
    } else if lower.contains("\"level\":\"warn\"")
        || lower.contains("level=warn")
        || lower.contains("[warn]")
        || lower.contains("warn:")
    {
        Color::Yellow
    } else if lower.contains("\"level\":\"debug\"")
        || lower.contains("level=debug")
        || lower.contains("[debug]")
    {
        Color::DarkGray
    } else {
        Color::Reset
    }
}

// ---------------------------------------------------------------------------
// Highlight search term inside a line
// ---------------------------------------------------------------------------

fn highlighted_spans<'a>(line: &'a str, query: Option<&str>, base_color: Color) -> Line<'a> {
    let base_style = Style::default().fg(base_color);
    let hi_style = Style::default().fg(Color::Black).bg(Color::Yellow).bold();

    match query {
        Some(q) if !q.is_empty() => {
            let lower_line = line.to_lowercase();
            let lower_q = q.to_lowercase();
            let mut spans = Vec::new();
            let mut cursor = 0usize;

            for idx in lower_line.match_indices(&lower_q as &str).map(|(i, _)| i) {
                if cursor < idx {
                    spans.push(Span::styled(&line[cursor..idx], base_style));
                }
                spans.push(Span::styled(&line[idx..idx + q.len()], hi_style));
                cursor = idx + q.len();
            }
            if cursor < line.len() {
                spans.push(Span::styled(&line[cursor..], base_style));
            }
            Line::from(spans)
        }
        _ => Line::styled(line, base_style),
    }
}

// ---------------------------------------------------------------------------
// render — Full-screen log viewer
// ---------------------------------------------------------------------------

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let query = app.search_query.as_deref();

    let lines: Vec<Line> = app
        .log_lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let num = format!("{:>4} │ ", i + 1);
            let color = line_color(l);
            let content = highlighted_spans(l, query, color);
            // Prepend line number span.
            let mut spans = vec![Span::styled(num, Style::default().fg(Color::DarkGray))];
            spans.extend(content.spans);
            Line::from(spans)
        })
        .collect();

    let total = lines.len() as u16;
    let title = if app.search_mode {
        format!(" Logs — search: {} ({} lines) ", query.unwrap_or(""), total)
    } else {
        format!(" Logs ({} lines) ", total)
    };

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .scroll((app.scroll_offset, 0))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// render_mini — Compact last-N lines panel for the Processes split view
// ---------------------------------------------------------------------------

pub fn render_mini(f: &mut Frame, area: Rect, app: &App, max_lines: usize) {
    let name = app.selected_process_name().unwrap_or("—");

    let start = app.log_lines.len().saturating_sub(max_lines);
    let lines: Vec<Line> = app.log_lines[start..]
        .iter()
        .map(|l| {
            let color = line_color(l);
            Line::styled(l.as_str(), Style::default().fg(color))
        })
        .collect();

    let title = format!(" Live Logs — {} ", name);
    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}
