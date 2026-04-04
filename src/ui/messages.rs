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

    // Precompute inner width for visual line tracking
    let inner = block.inner(area);
    let inner_width = inner.width as usize;

    // Track image positions in VISUAL lines: (message_id, visual_start, height, is_sticker)
    let mut image_positions: Vec<(String, usize, usize, bool)> = Vec::new();
    // Visual line count, computed for image positions and scroll
    let mut visual_line_count: usize;
    let visual_height = |line: &Line| -> usize {
        let w = line.width();
        if w == 0 || inner_width == 0 { 1 } else { (w + inner_width - 1) / inner_width }
    };

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
            // Priority: saved contact name > push_name from DB > phone number > "~LID"
            active.sender_names.get(&msg.sender_jid).cloned()
                .or_else(|| msg.sender_push_name.as_deref().map(|s| s.to_string()))
                .unwrap_or_else(|| {
                    let raw = msg.sender_jid.split('@').next().unwrap_or("?");
                    if msg.sender_jid.contains("@lid") {
                        // LID JIDs are opaque identifiers — show "Member" instead
                        // of meaningless numbers. Names resolve over time as
                        // real-time messages arrive with push names.
                        format!("~{}", &raw[..raw.len().min(4)])
                    } else {
                        raw.to_string()
                    }
                })
        };

        let sender_color = if msg.from_me {
            theme::OWN_MESSAGE
        } else {
            sender_color_hash(&sender_name)
        };

        let mut header_spans = vec![
            Span::styled(
                prefix,
                if is_selected {
                    Style::default().fg(theme::ACCENT).bg(theme::SELECTED_BG)
                } else {
                    Style::default()
                },
            ),
            Span::styled(
                sender_name,
                Style::default().fg(sender_color).patch(sel_style),
            ),
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
            header_spans.push(Span::styled(
                " (edited)",
                theme::muted_style().patch(sel_style),
            ));
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
            if let Some((_, cached_w, cached_h)) = active.media_cache.get(&msg.id) {
                let is_sticker = matches!(
                    msg.message_type,
                    crate::store::messages::MessageType::Sticker
                );
                let (cached_w, cached_h) = (*cached_w, *cached_h);
                let img_height = if is_sticker {
                    8usize
                } else {
                    let render_width = inner_width.saturating_sub(6).min(50);
                    let aspect = cached_h as f32 / cached_w.max(1) as f32;
                    let rows = (render_width as f32 * aspect / 2.0).round() as usize;
                    rows.saturating_sub(1).clamp(3, 14)
                };
                // Snapshot current visual position (accumulated incrementally)
                // Recompute here since lines were pushed since last update
                visual_line_count = lines.iter().map(&visual_height).sum();
                image_positions.push((msg.id.clone(), visual_line_count, img_height, is_sticker));
                // Reserve space with full-width blank lines so terminal
                // background doesn't bleed through
                let fill = " ".repeat(inner_width);
                for _ in 0..img_height {
                    lines.push(Line::from(vec![Span::styled(
                        fill.clone(),
                        sel_style,
                    )]));
                }
            } else {
                let type_label = msg.message_type.as_str();
                let hint = if matches!(
                    msg.message_type,
                    crate::store::messages::MessageType::Sticker
                ) && msg.media_direct_path.is_some()
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
        } else if matches!(msg.message_type, crate::store::messages::MessageType::Video) {
            // Video: show duration, GIF flag, caption
            let duration = format_duration(msg.media_duration_secs);
            let label = if msg.is_gif { "GIF" } else { "Video" };
            let hint = if msg.media_direct_path.is_some() { "'o' to play" } else { "" };
            lines.push(Line::from(vec![Span::styled(
                format!("{}\u{25B6} [{} {}] {}", content_prefix, label, duration, hint),
                Style::default().fg(theme::ACCENT).patch(sel_style),
            )]));
            if let Some(ref cap) = msg.caption {
                lines.push(Line::from(vec![Span::styled(
                    format!("{}{}", content_prefix, cap),
                    sel_style,
                )]));
            }
        } else if matches!(msg.message_type, crate::store::messages::MessageType::Audio) {
            // Audio/voice note: show duration, voice indicator
            let duration = format_duration(msg.media_duration_secs);
            let icon = if msg.is_voice_note { "\u{1F3A4}" } else { "\u{266B}" }; // 🎤 or ♫
            let label = if msg.is_voice_note { "Voice" } else { "Audio" };
            let hint = if msg.media_direct_path.is_some() { "'o' to play" } else { "" };
            lines.push(Line::from(vec![Span::styled(
                format!("{}{} [{} {}] {}", content_prefix, icon, label, duration, hint),
                Style::default().fg(theme::ACCENT).patch(sel_style),
            )]));
        } else if let Some(ref content) = msg.content {
            lines.push(Line::from(vec![Span::styled(
                format!("{}{}", content_prefix, content),
                sel_style,
            )]));
            // Link preview below the text
            if let Some(ref title) = msg.link_title {
                lines.push(Line::from(vec![Span::styled(
                    format!("{}  \u{1F517} {}", content_prefix, title), // 🔗
                    Style::default().fg(theme::TEXT_SECONDARY).patch(sel_style),
                )]));
                if let Some(ref desc) = msg.link_description {
                    let truncated = crate::util::truncate(desc, 60);
                    lines.push(Line::from(vec![Span::styled(
                        format!("{}  {}", content_prefix, truncated),
                        theme::muted_style().patch(sel_style),
                    )]));
                }
            }
        } else {
            let type_label = msg.message_type.as_str();
            lines.push(Line::from(vec![Span::styled(
                format!("{}[{}]", content_prefix, type_label),
                theme::muted_style().patch(sel_style),
            )]));
        }

        // Reactions
        if !msg.reactions.is_empty() {
            let reaction_text: Vec<String> =
                msg.reactions.iter().map(|r| r.emoji.clone()).collect();
            lines.push(Line::from(vec![Span::styled(
                format!("  {}", reaction_text.join(" ")),
                Style::default().fg(theme::TEXT_SECONDARY).patch(sel_style),
            )]));
        }

        // Blank line between messages
        lines.push(Line::from(""));
    }

    // Final visual line count for scroll calculation
    visual_line_count = lines.iter().map(&visual_height).sum();

    // Calculate scroll position from bottom offset.
    // scroll_from_bottom=0 means show newest, higher = further back in history.
    //
    // Paragraph with Wrap scrolls in VISUAL (wrapped) lines, not logical lines.
    // We must account for line wrapping to avoid clipping the bottom.
    let inner_height = inner.height as usize;
    let max_scroll = visual_line_count.saturating_sub(inner_height);
    let scroll = max_scroll
        .saturating_sub(active.scroll_from_bottom)
        .min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, area);

    // Render images at their reserved positions (overlay on top of paragraph)

    if let Some(ref mut active) = app.active_chat {
        for (msg_id, start_line, height, is_sticker) in &image_positions {
            // Calculate screen position: start_line - scroll = visible line offset
            let visible_y = (*start_line as isize) - (scroll as isize);
            if visible_y < 0 || visible_y >= inner.height as isize {
                continue; // Off-screen
            }
            let max_width = if *is_sticker { 20u16 } else { 50u16 };
            let img_area = Rect {
                x: inner.x + 4, // indent
                y: inner.y + visible_y as u16,
                width: inner.width.saturating_sub(6).min(max_width),
                height: (*height as u16).min(inner.height.saturating_sub(visible_y as u16)),
            };
            if img_area.height == 0 || img_area.width == 0 {
                continue;
            }
            if let Some((proto, _, _)) = active.media_cache.get_mut(msg_id) {
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
    let hash: u32 = jid
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    colors[(hash as usize) % colors.len()]
}

/// Format seconds into M:SS or H:MM:SS.
fn format_duration(secs: Option<u32>) -> String {
    match secs {
        Some(s) if s >= 3600 => format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60),
        Some(s) => format!("{}:{:02}", s / 60, s % 60),
        None => "?:??".to_string(),
    }
}
