use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, AppFocus, AppMode};
use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == AppFocus::Input;
    let in_insert = app.mode == AppMode::Insert;
    let border_style = if focused || in_insert {
        theme::focused_border()
    } else {
        theme::unfocused_border()
    };

    let title = if in_insert { " INSERT " } else { " Input " };

    let block = Block::default()
        .title(title)
        .title_style(if in_insert {
            Style::default().fg(theme::MODE_INSERT)
        } else {
            theme::title_style()
        })
        .borders(Borders::ALL)
        .border_style(border_style);

    let active = match &app.active_chat {
        Some(chat) => chat,
        None => {
            let empty = Paragraph::new("").block(block);
            frame.render_widget(empty, area);
            return;
        }
    };

    let display_text = if active.input_buf.is_empty() && !in_insert {
        Span::styled(
            "Type a message... (i to enter insert mode)",
            theme::muted_style(),
        )
    } else if active.input_buf.is_empty() {
        Span::styled("Type a message...", theme::muted_style())
    } else {
        Span::raw(&active.input_buf)
    };

    let mut lines = Vec::new();

    // Show reply indicator if replying
    if let Some(ref reply_id) = active.reply_to {
        lines.push(Line::from(vec![
            Span::styled("Replying to ", theme::muted_style()),
            Span::styled(reply_id.as_str(), Style::default().fg(theme::ACCENT)),
        ]));
    }

    // Cursor indicator in insert mode
    if in_insert {
        lines.push(Line::from(vec![
            Span::raw(&active.input_buf),
            Span::styled("\u{2588}", Style::default().fg(theme::ACCENT)), // block cursor
        ]));
    } else {
        lines.push(Line::from(vec![display_text]));
    }

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}
