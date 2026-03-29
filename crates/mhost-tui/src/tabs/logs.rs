use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = app
        .log_lines
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Logs "))
        .scroll((app.scroll_offset, 0));

    f.render_widget(paragraph, area);
}
