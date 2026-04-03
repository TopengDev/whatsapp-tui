/// Media download, cache management, xdg-open.
/// Stubbed until whatsapp-rust is integrated.
use std::path::{Path, PathBuf};

use anyhow::Result;

/// Get the cached path for a media file, if it exists.
pub fn cached_path(cache_dir: &Path, message_id: &str, mime: &str) -> PathBuf {
    let ext = mime_to_ext(mime);
    cache_dir
        .join("media")
        .join(format!("{}.{}", message_id, ext))
}

/// Open a file with the system default application.
pub fn open_file(path: &Path) -> Result<()> {
    std::process::Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}

fn mime_to_ext(mime: &str) -> &str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "audio/ogg" => "ogg",
        "audio/mpeg" => "mp3",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}
