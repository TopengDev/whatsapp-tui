use ratatui::layout::{Constraint, Layout, Rect};

/// Split the full area into panes: chat_list | chat_area | (optional) info_panel.
pub fn main_layout(area: Rect, show_info_panel: bool) -> Vec<Rect> {
    if show_info_panel {
        Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(50),
            Constraint::Percentage(25),
        ])
        .split(area)
        .to_vec()
    } else {
        Layout::horizontal([Constraint::Percentage(25), Constraint::Percentage(75)])
            .split(area)
            .to_vec()
    }
}

/// Split the chat area into: header | messages | input | status_bar.
pub fn chat_area_layout(area: Rect, input_height: u16) -> Vec<Rect> {
    Layout::vertical([
        Constraint::Length(1),            // header
        Constraint::Min(1),               // messages
        Constraint::Length(input_height),  // input (3 normal, 4 when replying)
        Constraint::Length(1),            // status bar
    ])
    .split(area)
    .to_vec()
}

/// Centered popup rect (percentage of parent).
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}
