use ratatui::layout::Rect;
use ratatui::Frame;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

/// Render a cached protocol state into a frame area.
pub fn render_protocol(frame: &mut Frame, area: Rect, protocol: &mut StatefulProtocol) {
    let widget = StatefulImage::default().resize(Resize::Fit(None));
    // Clamp area to avoid overflow beyond the frame
    let safe_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.min(frame.area().width.saturating_sub(area.x)),
        height: area.height.min(frame.area().height.saturating_sub(area.y)),
    };
    if safe_area.width == 0 || safe_area.height == 0 {
        return;
    }
    frame.render_stateful_widget(widget, safe_area, protocol);
}

/// Create a new stateful protocol from raw image bytes.
/// Thumbnails to 800x800 to bound memory while keeping good quality
/// on HiDPI terminals. ratatui-image resizes further at render time.
/// Returns (protocol, original_width, original_height).
pub fn create_protocol(picker: &Picker, img_bytes: &[u8]) -> Option<(StatefulProtocol, u32, u32)> {
    let img = match image::load_from_memory(img_bytes) {
        Ok(img) => img,
        Err(e) => {
            tracing::warn!("failed to decode image ({} bytes): {}", img_bytes.len(), e);
            return None;
        }
    };
    let (w, h) = (img.width(), img.height());
    let thumb = img.thumbnail(800, 800);
    Some((picker.new_resize_protocol(thumb), w, h))
}
