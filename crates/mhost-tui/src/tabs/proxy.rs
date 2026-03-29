use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, _app: &App) {
    let paragraph = Paragraph::new("Proxy routes will be displayed here")
        .block(Block::default().borders(Borders::ALL).title(" Proxy "));

    f.render_widget(paragraph, area);
}
