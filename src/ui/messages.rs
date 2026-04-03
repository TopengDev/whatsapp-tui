use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, AppFocus};
use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.focus == AppFocus::Messages;
    let border_style = if focused {
        theme::focused_border()
    } else {
        theme::unfocused_border()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let active = match &app.active_chat {
        Some(chat) => chat,
        None => {
            let placeholder = Paragraph::new(Line::from(vec![Span::styled(
                "Select a chat to start messaging",
                theme::muted_style(),
            )]))
            .block(block);
            frame.render_widget(placeholder, area);
            return;
        }
    };

    if active.messages.is_empty() {
        let empty = Paragraph::new(Line::from(vec![Span::styled(
            "No messages yet",
            theme::muted_style(),
        )]))
        .block(block);
        frame.render_widget(empty, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    let selected_idx = active.selected_msg_idx;
    let is_focused = app.focus == AppFocus::Messages;

    // Track image positions: (message_id, start_line, height)
    let mut image_positions: Vec<(String, usize, usize)> = Vec::new();
    let img_height = 8usize;

    for (msg_idx, msg) in active.messages.iter().enumerate() {
        let is_selected = is_focused && selected_idx == Some(msg_idx);
        let sel_style = if is_selected {
            Style::default().bg(theme::SELECTED_BG)
        } else {
            Style::default()
        };

        // Cursor indicator for selected message
        let prefix = if is_selected { "\u{25b6} " } else { "  " };

        // Sender + timestamp line
        let ts = chrono::DateTime::from_timestamp(msg.timestamp, 0)
            .map(|dt| dt.format(&app.config.general.timestamp_format).to_string())
            .unwrap_or_default();

        let sender_name = if msg.from_me {
            "You".to_string()
        } else {
            // Priority: push_name from message > contacts/chats lookup > phone number
            msg.sender_push_name
                .as_deref()
                .map(|s| s.to_string())
                .or_else(|| active.sender_names.get(&msg.sender_jid).cloned())
                .unwrap_or_else(|| {
                    msg.sender_jid.split('@').next().unwrap_or("?").to_string()
                })
        };

        let sender_color = if msg.from_me {
            theme::OWN_MESSAGE
        } else {
            sender_color_hash(&msg.sender_jid)
        };

        let mut header_spans = vec![
            Span::styled(prefix, if is_selected {
                Style::default().fg(theme::ACCENT).bg(theme::SELECTED_BG)
            } else {
                Style::default()
            }),
            Span::styled(sender_name, Style::default().fg(sender_color).patch(sel_style)),
            Span::styled(format!("  {}", ts), theme::muted_style().patch(sel_style)),
        ];

        // Status indicator for own messages
        if msg.from_me {
            let status_icon = match msg.status {
                crate::store::messages::MessageStatus::Pending => " ...",
                crate::store::messages::MessageStatus::Sent => " \u{2713}",
                crate::store::messages::MessageStatus::Delivered => " \u{2713}\u{2713}",
                crate::store::messages::MessageStatus::Read => " \u{2713}\u{2713}",
                crate::store::messages::MessageStatus::Failed => " \u{2717}",
            };
            let status_color = match msg.status {
                crate::store::messages::MessageStatus::Read => theme::ACCENT,
                crate::store::messages::MessageStatus::Failed => theme::STATUS_DISCONNECTED,
                _ => theme::TEXT_MUTED,
            };
            header_spans.push(Span::styled(
                status_icon,
                Style::default().fg(status_color).patch(sel_style),
            ));
        }

        if msg.edited {
            header_spans.push(Span::styled(" (edited)", theme::muted_style().patch(sel_style)));
        }

        lines.push(Line::from(header_spans));

        // Reply quote
        if let Some(ref preview) = msg.reply_to_preview {
            lines.push(Line::from(vec![Span::styled(
                format!("  > {}", preview),
                theme::muted_style().patch(sel_style),
            )]));
        }

        // Content
        let content_prefix = "  ";
        if msg.deleted {
            lines.push(Line::from(vec![Span::styled(
                format!("{}\u{2205} This message was deleted", content_prefix),
                theme::muted_style().patch(sel_style),
            )]));
        } else if matches!(
            msg.message_type,
            crate::store::messages::MessageType::Sticker
                | crate::store::messages::MessageType::Image
        ) {
            // Image/sticker rendering is handled after the paragraph via StatefulImage.
            // Here we just reserve space with a placeholder.
            if active.media_cache.contains_key(&msg.id) {
                // Reserve lines and record position for image overlay
                image_positions.push((msg.id.clone(), lines.len(), img_height));
                for _ in 0..img_height {
                    lines.push(Line::from(vec![Span::styled(
                        format!("{}  ", content_prefix),
                        sel_style,
                    )]));
                }
            } else {
                let type_label = msg.message_type.as_str();
                let hint = if matches!(msg.message_type, crate::store::messages::MessageType::Sticker)
                    && msg.media_direct_path.is_some()
                {
                    "downloading..."
                } else if msg.media_direct_path.is_some() {
                    "press 'd' to load"
                } else {
                    "no media data"
                };
                lines.push(Line::from(vec![Span::styled(
                    format!("{}[{} - {}]", content_prefix, type_label, hint),
                    theme::muted_style().patch(sel_style),
                )]));
            }
        } else if let Some(ref content) = msg.content {
            lines.push(Line::from(vec![Span::styled(
                format!("{}{}", content_prefix, content),
                sel_style,
            )]));
        } else {
            let type_label = msg.message_type.as_str();
            lines.push(Line::from(vec![Span::styled(
                format!("{}[{}]", content_prefix, type_label),
                theme::muted_style().patch(sel_style),
            )]));
        }

        // Reactions
        if !msg.reactions.is_empty() {
            let reaction_text: Vec<String> = msg
                .reactions
                .iter()
                .map(|r| r.emoji.clone())
                .collect();
            lines.push(Line::from(vec![Span::styled(
                format!("  {}", reaction_text.join(" ")),
                Style::default().fg(theme::TEXT_SECONDARY).patch(sel_style),
            )]));
        }

        // Blank line between messages
        lines.push(Line::from(""));
    }

    // Calculate scroll position from bottom offset.
    // scroll_from_bottom=0 means show newest, higher = further back in history.
    let inner_height = block.inner(area).height as usize;
    let total_lines = lines.len();
    let max_scroll = total_lines.saturating_sub(inner_height);
    let scroll = max_scroll.saturating_sub(active.scroll_from_bottom).min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, area);

    // Render images at their reserved positions (overlay on top of paragraph)
    let inner = Rect {
        x: area.x + 1, // inside border
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if let Some(ref mut active) = app.active_chat {
        for (msg_id, start_line, height) in &image_positions {
            // Calculate screen position: start_line - scroll = visible line offset
            let visible_y = (*start_line as isize) - (scroll as isize);
            if visible_y < 0 || visible_y >= inner.height as isize {
                continue; // Off-screen
            }
            let img_area = Rect {
                x: inner.x + 4, // indent
                y: inner.y + visible_y as u16,
                width: inner.width.saturating_sub(6).min(30),
                height: (*height as u16).min(inner.height.saturating_sub(visible_y as u16)),
            };
            if img_area.height == 0 || img_area.width == 0 {
                continue;
            }
            if let Some(proto) = active.media_cache.get_mut(msg_id) {
                super::image::render_protocol(frame, img_area, proto);
            }
        }
    }
}

/// Deterministic color for a JID — consistent per person.
fn sender_color_hash(jid: &str) -> ratatui::style::Color {
    let colors = [
        ratatui::style::Color::Red,
        ratatui::style::Color::Yellow,
        ratatui::style::Color::Blue,
        ratatui::style::Color::Magenta,
        ratatui::style::Color::Cyan,
        ratatui::style::Color::LightRed,
        ratatui::style::Color::LightYellow,
        ratatui::style::Color::LightBlue,
        ratatui::style::Color::LightMagenta,
        ratatui::style::Color::LightCyan,
    ];
    let hash: u32 = jid.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    colors[(hash as usize) % colors.len()]
}
