use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, AppMode, ConnectionStatus};
use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let mode_label = match app.mode {
        AppMode::Normal => "NORMAL",
        AppMode::Insert => "INSERT",
        AppMode::Command => "COMMAND",
        AppMode::Search => "SEARCH",
    };
    let mode_color = match app.mode {
        AppMode::Normal => theme::MODE_NORMAL,
        AppMode::Insert => theme::MODE_INSERT,
        AppMode::Command => theme::MODE_COMMAND,
        AppMode::Search => theme::MODE_SEARCH,
    };

    let conn_label = match &app.connection_status {
        ConnectionStatus::Connected => "Connected",
        ConnectionStatus::Disconnected => "Disconnected",
        ConnectionStatus::Reconnecting { attempt, max } => {
            // We leak a tiny string for the status bar — fine for display
            // In practice we'd format into a buffer on App
            return render_reconnecting(frame, area, app, mode_label, mode_color, *attempt, *max);
        }
        ConnectionStatus::LoggedOut => "Logged Out",
    };
    let conn_color = match &app.connection_status {
        ConnectionStatus::Connected => theme::STATUS_CONNECTED,
        ConnectionStatus::Disconnected => theme::STATUS_DISCONNECTED,
        ConnectionStatus::LoggedOut => theme::STATUS_DISCONNECTED,
        ConnectionStatus::Reconnecting { .. } => theme::STATUS_RECONNECTING,
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", mode_label),
            Style::default().fg(ratatui::style::Color::Black).bg(mode_color),
        ),
        Span::raw(" "),
        Span::styled(conn_label, Style::default().fg(conn_color)),
        Span::raw("  "),
        Span::styled("whatsapp-tui v0.1.0", theme::muted_style()),
    ]);

    let bar = Paragraph::new(line);
    frame.render_widget(bar, area);
}

fn render_reconnecting(
    frame: &mut Frame,
    area: Rect,
    _app: &App,
    mode_label: &str,
    mode_color: ratatui::style::Color,
    attempt: u32,
    max: u32,
) {
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", mode_label),
            Style::default().fg(ratatui::style::Color::Black).bg(mode_color),
        ),
        Span::raw(" "),
        Span::styled(
            format!("Reconnecting ({}/{})", attempt, max),
            Style::default().fg(theme::STATUS_RECONNECTING),
        ),
        Span::raw("  "),
        Span::styled("whatsapp-tui v0.1.0", theme::muted_style()),
    ]);
    let bar = Paragraph::new(line);
    frame.render_widget(bar, area);
}
