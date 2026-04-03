use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, qr_data: &str) {
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Scan QR Code ")
        .title_style(theme::title_style())
        .borders(Borders::ALL)
        .border_style(theme::focused_border());

    let inner = block.inner(area);

    let lines = match qrcode::QrCode::new(qr_data.as_bytes()) {
        Ok(code) => render_qr_halfblock(&code, inner.width as usize, inner.height as usize),
        Err(_) => {
            vec![
                Line::from(""),
                Line::from(vec![Span::styled(
                    "  Failed to generate QR code",
                    theme::muted_style(),
                )]),
                Line::from(""),
                Line::from(vec![Span::raw(format!(
                    "  Data: {}...",
                    &qr_data[..qr_data.len().min(40)]
                ))]),
            ]
        }
    };

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Render a QR code using Unicode half-block characters.
/// Each terminal cell represents 2 vertical modules, halving the height.
/// Uses ▀ (upper half), ▄ (lower half), █ (full block), and space.
fn render_qr_halfblock(
    code: &qrcode::QrCode,
    max_width: usize,
    max_height: usize,
) -> Vec<Line<'static>> {
    let modules = code.width(); // modules per side (e.g., 33 for version 4)
    let quiet = 1; // 1-module quiet zone on each side

    let total_modules = modules + quiet * 2;

    // Determine scale: each module is `scale` characters wide, and 2 modules tall per row
    // We need total_modules * scale <= max_width AND ceil(total_modules/2) + header <= max_height
    let max_scale_w = max_width / total_modules;
    let rows_needed = (total_modules + 1) / 2; // half-block: 2 modules per row
    let available_rows = max_height.saturating_sub(2); // leave room for header text
    let max_scale_h = if rows_needed > 0 {
        available_rows / rows_needed
    } else {
        1
    };
    let scale = max_scale_w.min(max_scale_h).max(1);

    let white = Style::default().fg(Color::White).bg(Color::White);
    let black = Style::default().fg(Color::Black).bg(Color::Black);
    let top_half = Style::default().fg(Color::Black).bg(Color::White); // ▀ = top black, bottom white
    let bot_half = Style::default().fg(Color::White).bg(Color::Black); // ▀ = top white, bottom black

    // Helper to check if a module is dark (true = dark/black)
    let is_dark = |row: isize, col: isize| -> bool {
        let r = row - quiet as isize;
        let c = col - quiet as isize;
        if r < 0 || c < 0 || r >= modules as isize || c >= modules as isize {
            false // quiet zone = white
        } else {
            code[(r as usize, c as usize)] == qrcode::Color::Dark
        }
    };

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Header
    let pad = (max_width.saturating_sub(total_modules * scale)) / 2;
    let pad_str: String = " ".repeat(pad);

    // Process pairs of rows (top_row, bottom_row) → one terminal line
    let mut y = 0isize;
    while y < total_modules as isize {
        let top_row = y;
        let bot_row = y + 1;

        // Each terminal row is `scale` lines tall (but since we use half-blocks,
        // scale=1 means 1 terminal row per 2 QR rows)
        for _sy in 0..scale {
            let mut spans: Vec<Span<'static>> = Vec::new();
            spans.push(Span::raw(pad_str.clone()));

            for x in 0..total_modules as isize {
                let top_dark = is_dark(top_row, x);
                let bot_dark = if bot_row < total_modules as isize {
                    is_dark(bot_row, x)
                } else {
                    false // padding row = white
                };

                let (ch, style) = match (top_dark, bot_dark) {
                    (false, false) => (" ", white),
                    (true, true) => (" ", black),
                    (true, false) => ("\u{2580}", top_half), // ▀ upper half block
                    (false, true) => ("\u{2580}", bot_half), // ▀ with inverted colors
                };

                let cell: String = ch.repeat(scale);
                spans.push(Span::styled(cell, style));
            }

            lines.push(Line::from(spans));
        }

        y += 2;
    }

    // Footer instruction
    lines.push(Line::from(""));
    let instruction = "  Open WhatsApp > Linked Devices > Link a Device";
    let inst_pad = (max_width.saturating_sub(instruction.len())) / 2;
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(inst_pad)),
        Span::styled(instruction, theme::muted_style()),
    ]));

    lines
}
