use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let active = match &app.active_chat {
        Some(chat) => chat,
        None => {
            let empty = Paragraph::new(Line::from(vec![Span::styled(
                " whatsapp-tui",
                theme::title_style(),
            )]));
            frame.render_widget(empty, area);
            return;
        }
    };

    let mut spans = vec![Span::styled(
        format!(" {}", active.name),
        theme::title_style(),
    )];

    if active.is_group {
        spans.push(Span::styled(
            format!(" ({} members)", active.members.len()),
            theme::muted_style(),
        ));
    }

    // Typing indicator
    if !active.typing_jids.is_empty() {
        let typers: Vec<String> = active
            .typing_jids
            .iter()
            .map(|jid| {
                active
                    .sender_names
                    .get(jid)
                    .cloned()
                    .unwrap_or_else(|| jid.split('@').next().unwrap_or("?").to_string())
            })
            .collect();
        let typing_text = if typers.len() == 1 {
            format!("{} is typing...", typers[0])
        } else {
            format!("{} are typing...", typers.join(", "))
        };
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            typing_text,
            Style::default().fg(theme::ACCENT),
        ));
    }

    let header = Paragraph::new(Line::from(spans));
    frame.render_widget(header, area);
}
