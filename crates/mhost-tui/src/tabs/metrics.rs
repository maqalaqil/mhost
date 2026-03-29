use ratatui::prelude::*;
use ratatui::widgets::*;

use mhost_core::process::ProcessStatus;

use crate::app::App;

// ---------------------------------------------------------------------------
// Sparkline helper
// ---------------------------------------------------------------------------

const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn sparkline_str(data: &[f64], width: usize) -> String {
    if data.is_empty() || width == 0 {
        return " ".repeat(width);
    }
    let max = data.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
    let start = data.len().saturating_sub(width);
    let chars: String = data[start..]
        .iter()
        .map(|&v| {
            let idx = ((v / max) * 7.0).round() as usize;
            BARS[idx.min(7)]
        })
        .collect();
    // Pad with spaces on the left when we have fewer points than width.
    // Use char count (not byte length) because bar chars are multi-byte.
    let char_count = chars.chars().count();
    if char_count < width {
        let pad = width - char_count;
        format!("{}{}", " ".repeat(pad), chars)
    } else {
        chars
    }
}

// ---------------------------------------------------------------------------
// render — Summary metrics for all processes
// ---------------------------------------------------------------------------

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    // Split area: top = table, bottom = charts for selected process.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_summary_table(f, chunks[0], app);
    render_sparklines_panel(f, chunks[1], app);
}

fn render_summary_table(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        Cell::from("Name").style(Style::default().bold()),
        Cell::from("Status").style(Style::default().bold()),
        Cell::from("CPU%").style(Style::default().bold()),
        Cell::from("Memory").style(Style::default().bold()),
        Cell::from("Uptime").style(Style::default().bold()),
        Cell::from("Restarts").style(Style::default().bold()),
    ])
    .bottom_margin(1);

    let rows: Vec<Row> = app
        .processes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let row_style = if i == app.selected_process {
                Style::default().bg(Color::DarkGray).bold()
            } else {
                Style::default()
            };

            let status_color = match p.status {
                ProcessStatus::Online => Color::Green,
                ProcessStatus::Starting | ProcessStatus::Stopping => Color::Yellow,
                _ => Color::Red,
            };

            // CPU bar (8 chars wide).
            let cpu_bar_width = 8;
            let cpu_val = p.cpu_percent.unwrap_or(0.0) as f64;
            let cpu_data = app
                .cpu_history
                .get(&p.config.name)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let cpu_spark = sparkline_str(cpu_data, cpu_bar_width);
            let cpu_text = format!(
                "{} {:>5.1}%",
                cpu_spark,
                p.cpu_percent.unwrap_or(0.0)
            );

            let mem_mb = p.memory_bytes.unwrap_or(0) as f64 / 1_048_576.0;

            Row::new(vec![
                Cell::from(p.config.name.clone()),
                Cell::from(p.status.to_string()).style(Style::default().fg(status_color)),
                Cell::from(cpu_text),
                Cell::from(format!("{:.1}MB", mem_mb)),
                Cell::from(p.format_uptime()),
                Cell::from(p.restart_count.to_string()),
            ])
            .style(row_style)
            // suppress unused warning on cpu_val
            .height({
                let _ = cpu_val;
                1
            })
        })
        .collect();

    let widths = [
        Constraint::Length(22),
        Constraint::Length(12),
        Constraint::Length(20),
        Constraint::Length(12),
        Constraint::Length(14),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" All Process Metrics "));

    f.render_widget(table, area);
}

fn render_sparklines_panel(f: &mut Frame, area: Rect, app: &App) {
    let selected = app.processes.get(app.selected_process);
    let name = selected
        .map(|p| p.config.name.as_str())
        .unwrap_or("—");

    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // CPU sparkline.
    let cpu_data = app
        .cpu_history
        .get(name)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    let cpu_width = (halves[0].width as usize).saturating_sub(4);
    let cpu_spark = sparkline_str(cpu_data, cpu_width);
    let cpu_last = selected
        .and_then(|p| p.cpu_percent)
        .unwrap_or(0.0);
    let cpu_text = format!("{}\n{:.1}%", cpu_spark, cpu_last);
    let cpu_para = Paragraph::new(cpu_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" CPU — {} ", name)),
        )
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(cpu_para, halves[0]);

    // Memory sparkline.
    let mem_data = app
        .mem_history
        .get(name)
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    let mem_width = (halves[1].width as usize).saturating_sub(4);
    let mem_spark = sparkline_str(mem_data, mem_width);
    let mem_last = selected
        .and_then(|p| p.memory_bytes)
        .unwrap_or(0) as f64
        / 1_048_576.0;
    let mem_text = format!("{}\n{:.1}MB", mem_spark, mem_last);
    let mem_para = Paragraph::new(mem_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Memory — {} ", name)),
        )
        .style(Style::default().fg(Color::Magenta));
    f.render_widget(mem_para, halves[1]);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparkline_str_empty() {
        let s = sparkline_str(&[], 10);
        assert_eq!(s.len(), 10);
        assert!(s.chars().all(|c| c == ' '));
    }

    #[test]
    fn test_sparkline_str_single_value() {
        let s = sparkline_str(&[50.0], 1);
        // Should be a bar character.
        assert!(BARS.contains(&s.chars().next().unwrap()));
    }

    #[test]
    fn test_sparkline_str_max_value_gives_full_bar() {
        let s = sparkline_str(&[100.0], 1);
        assert_eq!(s, "█");
    }

    #[test]
    fn test_sparkline_str_zero_gives_lowest_bar() {
        let s = sparkline_str(&[0.0], 1);
        assert_eq!(s, "▁");
    }

    #[test]
    fn test_sparkline_str_width_zero_returns_empty() {
        let s = sparkline_str(&[10.0, 20.0], 0);
        assert_eq!(s, "");
    }

    #[test]
    fn test_sparkline_str_pads_when_fewer_points_than_width() {
        let s = sparkline_str(&[100.0], 5);
        assert_eq!(s.chars().count(), 5);
        // The first 4 chars should be spaces.
        let chars: Vec<char> = s.chars().collect();
        for c in &chars[..4] {
            assert_eq!(*c, ' ');
        }
        assert_eq!(chars[4], '█');
    }

    #[test]
    fn test_sparkline_str_truncates_to_width() {
        let data: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let s = sparkline_str(&data, 10);
        assert_eq!(s.chars().count(), 10);
    }
}
