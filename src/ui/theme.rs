use ratatui::style::{Color, Modifier, Style};

// Brand / accent colors
pub const ACCENT: Color = Color::Rgb(37, 211, 102); // WhatsApp green
pub const ACCENT_DIM: Color = Color::Rgb(18, 140, 66);

// Pane borders
pub const BORDER_FOCUSED: Color = Color::Cyan;
pub const BORDER_UNFOCUSED: Color = Color::DarkGray;

// Text
pub const TEXT_PRIMARY: Color = Color::White;
pub const TEXT_SECONDARY: Color = Color::Gray;
pub const TEXT_MUTED: Color = Color::DarkGray;

// Status indicators
pub const STATUS_CONNECTED: Color = Color::Green;
pub const STATUS_DISCONNECTED: Color = Color::Red;
pub const STATUS_RECONNECTING: Color = Color::Yellow;

// Message elements
pub const OWN_MESSAGE: Color = Color::Rgb(37, 211, 102);
pub const UNREAD_BADGE: Color = Color::Rgb(37, 211, 102);
pub const SELECTED_BG: Color = Color::Rgb(40, 40, 60);

// Mode indicator
pub const MODE_NORMAL: Color = Color::Blue;
pub const MODE_INSERT: Color = Color::Green;
pub const MODE_COMMAND: Color = Color::Yellow;
pub const MODE_SEARCH: Color = Color::Magenta;

pub fn focused_border() -> Style {
    Style::default().fg(BORDER_FOCUSED)
}

pub fn unfocused_border() -> Style {
    Style::default().fg(BORDER_UNFOCUSED)
}

pub fn title_style() -> Style {
    Style::default()
        .fg(TEXT_PRIMARY)
        .add_modifier(Modifier::BOLD)
}

pub fn muted_style() -> Style {
    Style::default().fg(TEXT_MUTED)
}

pub fn selected_style() -> Style {
    Style::default().bg(SELECTED_BG).fg(TEXT_PRIMARY)
}

pub fn unread_style() -> Style {
    Style::default()
        .fg(UNREAD_BADGE)
        .add_modifier(Modifier::BOLD)
}
