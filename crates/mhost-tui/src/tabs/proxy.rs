use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, _app: &App) {
    let text = vec![
        Line::from(vec![
            Span::styled("Proxy Routes", Style::default().bold().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from("No routes configured."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Hint: ", Style::default().bold()),
            Span::raw("configure reverse-proxy routes in mhost.toml → [[proxy.routes]]"),
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Proxy "))
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}
