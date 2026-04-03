use notify_rust::Notification;

use crate::config::NotificationConfig;

pub fn send(
    chat_name: &str,
    sender_name: &str,
    preview: Option<&str>,
    config: &NotificationConfig,
) {
    if !config.desktop {
        return;
    }

    let mut notif = Notification::new();
    notif
        .summary(&format!("{} -- {}", chat_name, sender_name))
        .appname("whatsapp-tui")
        .icon("whatsapp")
        .timeout(5000);

    if config.show_preview {
        if let Some(text) = preview {
            notif.body(text);
        }
    }

    // Fire and forget — notification failure is not critical
    let _ = notif.show();
}
