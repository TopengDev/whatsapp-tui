use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, input: &str) {
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Command ")
        .title_style(theme::title_style())
        .borders(Borders::ALL)
        .border_style(theme::focused_border());

    let line = Line::from(vec![
        Span::styled(":", theme::muted_style()),
        Span::raw(input),
    ]);

    let paragraph = Paragraph::new(line).block(block);
    frame.render_widget(paragraph, area);
}
