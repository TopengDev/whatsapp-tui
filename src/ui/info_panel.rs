use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, AppFocus};
use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == AppFocus::InfoPanel;
    let border_style = if focused {
        theme::focused_border()
    } else {
        theme::unfocused_border()
    };

    let block = Block::default()
        .title(" Info ")
        .title_style(theme::title_style())
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

    let mut lines = Vec::new();

    // Chat name
    lines.push(Line::from(vec![Span::styled(
        &active.name,
        theme::title_style(),
    )]));
    lines.push(Line::from(""));

    // JID
    lines.push(Line::from(vec![
        Span::styled("JID: ", theme::muted_style()),
        Span::raw(&active.jid),
    ]));
    lines.push(Line::from(""));

    if active.is_group {
        lines.push(Line::from(vec![Span::styled(
            format!("Members: {}", active.members.len()),
            theme::muted_style(),
        )]));
        lines.push(Line::from(""));

        for member in &active.members {
            let name = active
                .sender_names
                .get(&member.jid)
                .map(|s| s.as_str())
                .unwrap_or_else(|| {
                    let raw = member.jid.split('@').next().unwrap_or("?");
                    // Show phone numbers but hide meaningless LID numbers
                    if member.jid.contains("@lid") { "Member" } else { raw }
                });
            let suffix = if member.is_super_admin {
                " [owner]"
            } else if member.is_admin {
                " [admin]"
            } else {
                ""
            };
            lines.push(Line::from(vec![
                Span::raw(format!("  {}", name)),
                Span::styled(suffix, theme::muted_style()),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}
