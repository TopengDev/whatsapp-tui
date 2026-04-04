use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, AppFocus};
use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == AppFocus::ChatList;
    let filtering = app.chat_filter.is_some();
    let border_style = if focused || filtering {
        theme::focused_border()
    } else {
        theme::unfocused_border()
    };

    let make_block = || {
        Block::default()
            .title(if filtering { " Find Chat " } else { " Chats " })
            .title_style(theme::title_style())
            .borders(Borders::ALL)
            .border_style(border_style)
    };

    // When filtering, render border + filter input, list goes in inner area
    let list_area = if filtering {
        let block = make_block();
        let inner = block.inner(area);
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);
        frame.render_widget(block, area);

        if let Some(ref query) = app.chat_filter {
            let filter_line = Line::from(vec![
                Span::styled(" / ", Style::default().fg(theme::ACCENT)),
                Span::raw(query),
                Span::styled("█", Style::default().fg(theme::TEXT_PRIMARY)),
            ]);
            frame.render_widget(Paragraph::new(filter_line), chunks[0]);
        }
        chunks[1]
    } else {
        area
    };

    let display_chats = if filtering {
        app.filtered_chats()
    } else {
        app.chats.clone()
    };
    let selected_idx = if filtering { app.chat_filter_idx } else { app.selected_chat_idx };

    if display_chats.is_empty() {
        let msg = if filtering { "No matches" } else { "No chats yet" };
        let items: Vec<ListItem> = vec![ListItem::new(Line::from(vec![Span::styled(
            msg,
            theme::muted_style(),
        )]))];
        let list = if filtering {
            List::new(items)
        } else {
            List::new(items).block(make_block())
        };
        frame.render_widget(list, list_area);
        return;
    }

    let items: Vec<ListItem> = display_chats
        .iter()
        .enumerate()
        .map(|(i, chat)| {
            let mut name_spans = Vec::new();

            // Pin indicator
            if chat.pinned {
                name_spans.push(Span::styled("* ", Style::default().fg(theme::ACCENT)));
            }

            // Chat name
            let name_style = if i == selected_idx && (focused || filtering) {
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
                let truncated = crate::util::truncate(preview, 30);
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
    state.select(Some(selected_idx));

    let list = List::new(items).highlight_style(theme::selected_style());
    let list = if filtering {
        list
    } else {
        list.block(make_block())
    };

    frame.render_stateful_widget(list, list_area, &mut state);
}
