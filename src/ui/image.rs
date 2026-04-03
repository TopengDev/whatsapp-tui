use ratatui::layout::Rect;
use ratatui::Frame;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

/// Render a cached protocol state into a frame area.
pub fn render_protocol(frame: &mut Frame, area: Rect, protocol: &mut StatefulProtocol) {
    let widget = StatefulImage::default().resize(Resize::Fit(None));
    frame.render_stateful_widget(widget, area, protocol);
}

/// Create a new stateful protocol from raw image bytes.
/// Pre-thumbnails to max 200x200 to keep encoding fast.
pub fn create_protocol(picker: &Picker, img_bytes: &[u8]) -> Option<StatefulProtocol> {
    let img = image::load_from_memory(img_bytes).ok()?;
    // Thumbnail for fast encoding — stickers are small anyway
    let thumb = img.thumbnail(200, 200);
    Some(picker.new_resize_protocol(thumb))
}
