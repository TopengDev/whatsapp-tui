use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::ui::theme;

/// Common reaction emojis.
const EMOJIS: &[&str] = &[
    "\u{1F44D}", // thumbs up
    "\u{2764}",  // red heart
    "\u{1F602}", // joy
    "\u{1F62E}", // open mouth
    "\u{1F622}", // cry
    "\u{1F64F}", // pray
    "\u{1F525}", // fire
    "\u{1F389}", // party
    "\u{1F44F}", // clap
    "\u{1F914}", // thinking
];

pub fn render(frame: &mut Frame, area: Rect, selected_idx: usize) {
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" React ")
        .title_style(theme::title_style())
        .borders(Borders::ALL)
        .border_style(theme::focused_border());

    let spans: Vec<Span> = EMOJIS
        .iter()
        .enumerate()
        .map(|(i, emoji)| {
            if i == selected_idx {
                Span::styled(format!("[{}] ", emoji), theme::selected_style())
            } else {
                Span::raw(format!(" {}  ", emoji))
            }
        })
        .collect();

    let paragraph = Paragraph::new(Line::from(spans)).block(block);
    frame.render_widget(paragraph, area);
}

pub fn emoji_count() -> usize {
    EMOJIS.len()
}

pub fn emoji_at(idx: usize) -> &'static str {
    EMOJIS.get(idx).unwrap_or(&EMOJIS[0])
}
