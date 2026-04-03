use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, prompt: &str) {
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Confirm ")
        .title_style(theme::title_style())
        .borders(Borders::ALL)
        .border_style(theme::focused_border());

    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::raw(format!("  {}", prompt))]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  [y] Yes  ", theme::unread_style()),
            Span::styled("  [n] No  ", theme::muted_style()),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}
