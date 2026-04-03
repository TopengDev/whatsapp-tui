use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::{App, AppFocus};
use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == AppFocus::ChatList;
    let border_style = if focused {
        theme::focused_border()
    } else {
        theme::unfocused_border()
    };

    let block = Block::default()
        .title(" Chats ")
        .title_style(theme::title_style())
        .borders(Borders::ALL)
        .border_style(border_style);

    if app.chats.is_empty() {
        let items: Vec<ListItem> = vec![ListItem::new(Line::from(vec![Span::styled(
            "No chats yet",
            theme::muted_style(),
        )]))];
        let list = List::new(items).block(block);
        frame.render_widget(list, area);
        return;
    }

    let items: Vec<ListItem> = app
        .chats
        .iter()
        .enumerate()
        .map(|(i, chat)| {
            let mut name_spans = Vec::new();

            // Pin indicator
            if chat.pinned {
                name_spans.push(Span::styled("* ", Style::default().fg(theme::ACCENT)));
            }

            // Chat name
            let name_style = if i == app.selected_chat_idx && focused {
                theme::selected_style().add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_PRIMARY)
            };
            name_spans.push(Span::styled(&chat.name, name_style));

            // Unread badge
            if chat.unread_count > 0 {
                name_spans.push(Span::styled(
                    format!(" ({})", chat.unread_count),
                    theme::unread_style(),
                ));
            }

            // Muted indicator
            if chat.muted {
                name_spans.push(Span::styled(" [M]", theme::muted_style()));
            }

            let name_line = Line::from(name_spans);

            // Preview line — truncated last message
            let preview_line = if let Some(ref preview) = chat.last_message_preview {
                let truncated = if preview.len() > 30 {
                    format!("{}...", &preview[..30])
                } else {
                    preview.clone()
                };
                Line::from(vec![Span::styled(
                    format!(" {}", truncated),
                    theme::muted_style(),
                )])
            } else {
                Line::from("")
            };

            ListItem::new(vec![name_line, preview_line])
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.selected_chat_idx));

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected_style());

    frame.render_stateful_widget(list, area, &mut state);
}
