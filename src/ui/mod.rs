pub mod chat_list;
pub mod header;
pub mod image;
pub mod info_panel;
pub mod input;
pub mod layout;
pub mod messages;
pub mod overlay;
pub mod status_bar;
pub mod theme;

use ratatui::Frame;

use crate::app::{App, Overlay};

/// Top-level render dispatch. Called every frame.
pub fn render(frame: &mut Frame, app: &mut App) {
    render_main(frame, app);

    if let Some(ref overlay) = app.overlay {
        let overlay = overlay.clone();
        render_overlay(frame, app, &overlay);
    }
}

fn render_main(frame: &mut Frame, app: &mut App) {
    let chunks = layout::main_layout(frame.area(), app.show_info_panel);

    chat_list::render(frame, chunks[0], app);

    // Split chat area into header + messages + input + status bar
    let chat_chunks = layout::chat_area_layout(chunks[1]);

    header::render(frame, chat_chunks[0], app);
    messages::render(frame, chat_chunks[1], app);
    input::render(frame, chat_chunks[2], app);
    status_bar::render(frame, chat_chunks[3], app);

    if app.show_info_panel && chunks.len() > 2 {
        info_panel::render(frame, chunks[2], app);
    }
}

fn render_overlay(frame: &mut Frame, app: &App, overlay_kind: &Overlay) {
    match overlay_kind {
        Overlay::QrCode { data } => {
            // QR needs a large area — nearly full screen
            let area = layout::centered_rect(90, 90, frame.area());
            overlay::qr::render(frame, area, data);
        }
        other => {
            let area = layout::centered_rect(60, 40, frame.area());
            match other {
                Overlay::Command => {
                    overlay::command::render(frame, area, &app.command_input);
                }
                Overlay::Search => {
                    overlay::search::render(frame, area, &app.search_query, &app.search_results);
                }
                Overlay::EmojiPicker { selected_idx } => {
                    overlay::emoji::render(frame, area, *selected_idx);
                }
                Overlay::Confirm { prompt } => {
                    overlay::confirm::render(frame, area, prompt);
                }
                Overlay::QrCode { .. } => unreachable!(),
            }
        }
    }
}
