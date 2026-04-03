use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::SearchResult;
use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, query: &str, results: &[SearchResult]) {
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Search ")
        .title_style(theme::title_style())
        .borders(Borders::ALL)
        .border_style(theme::focused_border());

    let mut lines = vec![Line::from(vec![
        Span::styled("/", theme::muted_style()),
        Span::raw(query),
    ])];

    lines.push(Line::from(""));

    if results.is_empty() && !query.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "No results",
            theme::muted_style(),
        )]));
    } else {
        for result in results.iter().take(10) {
            lines.push(Line::from(vec![
                Span::styled(&result.chat_name, theme::title_style()),
                Span::raw(": "),
                Span::raw(&result.snippet),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}
