use std::path::PathBuf;

use crate::wa::events::WaEvent;

/// Every event that can occur in the application.
pub enum AppEvent {
    /// Terminal input (key press, mouse, resize)
    Terminal(crossterm::event::Event),

    /// WhatsApp protocol event
    Wa(WaEvent),

    /// Result from a background task (media download, etc.)
    Task(TaskResult),

    /// Downloaded media bytes ready for inline rendering.
    MediaData { message_id: String, data: Vec<u8> },
}

/// Results from spawned background tasks
pub enum TaskResult {
    MediaDownloaded {
        message_id: String,
        local_path: PathBuf,
    },
    MediaOpenFailed {
        message_id: String,
        error: String,
    },
    HistorySynced {
        chat_jid: String,
        message_count: usize,
    },
}
