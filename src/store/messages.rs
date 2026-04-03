use std::path::PathBuf;

use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct Message {
    pub id: String,
    pub chat_jid: String,
    pub sender_jid: String,
    pub timestamp: i64,
    pub content: Option<String>,
    pub message_type: MessageType,
    pub media_mime: Option<String>,
    pub media_size: Option<i64>,
    pub media_filename: Option<String>,
    pub media_local_path: Option<PathBuf>,
    pub reply_to_id: Option<String>,
    pub reply_to_preview: Option<String>,
    pub edited: bool,
    pub deleted: bool,
    pub from_me: bool,
    pub status: MessageStatus,
    pub reactions: Vec<super::reactions::Reaction>,
    /// Transient: push name from the sender (not stored in DB, used to update contacts)
    pub sender_push_name: Option<String>,
    /// Transient: raw media download params for stickers/images (not stored in DB)
    pub media_direct_path: Option<String>,
    pub media_key: Option<Vec<u8>>,
    pub media_file_sha256: Option<Vec<u8>>,
    pub media_file_enc_sha256: Option<Vec<u8>>,
    /// Duration in seconds for video/audio messages.
    pub media_duration_secs: Option<u32>,
    /// Whether this is a voice note (push-to-talk) vs regular audio.
    pub is_voice_note: bool,
    /// Link preview title (from ExtendedTextMessage).
    pub link_title: Option<String>,
    /// Link preview description (from ExtendedTextMessage).
    pub link_description: Option<String>,
    /// Link preview URL (matched_text from ExtendedTextMessage).
    pub link_url: Option<String>,
    /// Caption for images/videos.
    pub caption: Option<String>,
    /// Whether a video is a GIF.
    pub is_gif: bool,
}

#[derive(Debug, Clone)]
pub enum MessageType {
    Text,
    Image,
    Video,
    Audio,
    Document,
    Sticker,
    Location,
    Poll,
    Contact,
    Unknown(String),
}

impl MessageType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Document => "document",
            Self::Sticker => "sticker",
            Self::Location => "location",
            Self::Poll => "poll",
            Self::Contact => "contact",
            Self::Unknown(s) => s.as_str(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "text" => Self::Text,
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "document" => Self::Document,
            "sticker" => Self::Sticker,
            "location" => Self::Location,
            "poll" => Self::Poll,
            "contact" => Self::Contact,
            other => Self::Unknown(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageStatus {
    Pending,
    Sent,
    Delivered,
    Read,
    Failed,
}

impl MessageStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Delivered => "delivered",
            Self::Read => "read",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "sent" => Self::Sent,
            "delivered" => Self::Delivered,
            "read" => Self::Read,
            "failed" => Self::Failed,
            _ => Self::Sent,
        }
    }
}

pub fn insert(conn: &Connection, msg: &Message) -> Result<()> {
    // Ensure the parent chat exists (avoids FK constraint failures for
    // messages that arrive before HistorySync creates the chat row).
    conn.execute(
        "INSERT OR IGNORE INTO chats (jid, name, is_group) VALUES (?1, ?2, ?3)",
        params![
            msg.chat_jid,
            msg.chat_jid.split('@').next().unwrap_or("?"),
            msg.chat_jid.contains("@g.us") as i32,
        ],
    )?;

    conn.execute(
        "INSERT OR REPLACE INTO messages (id, chat_jid, sender_jid, timestamp, content, \
         message_type, media_mime, media_size, media_filename, media_local_path, \
         reply_to_id, reply_to_preview, edited, deleted, from_me, status, \
         media_direct_path, media_key, media_file_sha256, media_file_enc_sha256) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            msg.id,
            msg.chat_jid,
            msg.sender_jid,
            msg.timestamp,
            msg.content,
            msg.message_type.as_str(),
            msg.media_mime,
            msg.media_size,
            msg.media_filename,
            msg.media_local_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            msg.reply_to_id,
            msg.reply_to_preview,
            msg.edited as i32,
            msg.deleted as i32,
            msg.from_me as i32,
            msg.status.as_str(),
            msg.media_direct_path,
            msg.media_key,
            msg.media_file_sha256,
            msg.media_file_enc_sha256,
        ],
    )?;
    Ok(())
}

pub fn get(
    conn: &Connection,
    chat_jid: &str,
    before_ts: Option<i64>,
    limit: u32,
) -> Result<Vec<Message>> {
    // Query DESC to get the N most recent, then reverse to chronological (ASC) for display.
    let mut rows = if let Some(ts) = before_ts {
        let mut s = conn.prepare(
            "SELECT id, chat_jid, sender_jid, timestamp, content, message_type, \
             media_mime, media_size, media_filename, media_local_path, \
             reply_to_id, reply_to_preview, edited, deleted, from_me, status, \
             media_direct_path, media_key, media_file_sha256, media_file_enc_sha256 \
             FROM messages WHERE chat_jid = ?1 AND timestamp < ?2 \
             ORDER BY timestamp DESC LIMIT ?3",
        )?;
        let result = s
            .query_map(params![chat_jid, ts, limit], row_to_message)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        result
    } else {
        let mut s = conn.prepare(
            "SELECT id, chat_jid, sender_jid, timestamp, content, message_type, \
             media_mime, media_size, media_filename, media_local_path, \
             reply_to_id, reply_to_preview, edited, deleted, from_me, status, \
             media_direct_path, media_key, media_file_sha256, media_file_enc_sha256 \
             FROM messages WHERE chat_jid = ?1 \
             ORDER BY timestamp DESC LIMIT ?2",
        )?;
        let result = s
            .query_map(params![chat_jid, limit], row_to_message)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        result
    };
    rows.reverse(); // Chronological order: oldest first, newest last
    Ok(rows)
}

pub fn search(conn: &Connection, query: &str, limit: u32) -> Result<Vec<(Message, String)>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.chat_jid, m.sender_jid, m.timestamp, m.content, m.message_type, \
         m.media_mime, m.media_size, m.media_filename, m.media_local_path, \
         m.reply_to_id, m.reply_to_preview, m.edited, m.deleted, m.from_me, m.status, \
         m.media_direct_path, m.media_key, m.media_file_sha256, m.media_file_enc_sha256, \
         snippet(messages_fts, 0, '>>>', '<<<', '...', 32) as snip \
         FROM messages_fts \
         JOIN messages m ON m.rowid = messages_fts.rowid \
         WHERE messages_fts MATCH ?1 \
         ORDER BY rank LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![query, limit], |row| {
            let msg = row_to_message(row)?;
            let snippet: String = row.get(20)?;
            Ok((msg, snippet))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn update_status(conn: &Connection, id: &str, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE messages SET status = ?1 WHERE id = ?2",
        params![status, id],
    )?;
    Ok(())
}

pub fn mark_edited(conn: &Connection, id: &str, new_content: &str) -> Result<()> {
    conn.execute(
        "UPDATE messages SET content = ?1, edited = 1 WHERE id = ?2",
        params![new_content, id],
    )?;
    Ok(())
}

pub fn mark_deleted(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("UPDATE messages SET deleted = 1 WHERE id = ?1", params![id])?;
    Ok(())
}

fn row_to_message(row: &rusqlite::Row) -> rusqlite::Result<Message> {
    let media_local_path: Option<String> = row.get(9)?;
    let msg_type_str: String = row.get(5)?;
    let status_str: String = row.get(15)?;
    let edited: i32 = row.get(12)?;
    let deleted: i32 = row.get(13)?;
    let from_me: i32 = row.get(14)?;

    Ok(Message {
        id: row.get(0)?,
        chat_jid: row.get(1)?,
        sender_jid: row.get(2)?,
        timestamp: row.get(3)?,
        content: row.get(4)?,
        message_type: MessageType::from_str(&msg_type_str),
        media_mime: row.get(6)?,
        media_size: row.get(7)?,
        media_filename: row.get(8)?,
        media_local_path: media_local_path.map(PathBuf::from),
        reply_to_id: row.get(10)?,
        reply_to_preview: row.get(11)?,
        edited: edited != 0,
        deleted: deleted != 0,
        from_me: from_me != 0,
        status: MessageStatus::from_str(&status_str),
        reactions: Vec::new(), // loaded separately
        sender_push_name: None,
        media_direct_path: row.get(16)?,
        media_key: row.get(17)?,
        media_file_sha256: row.get(18)?,
        media_file_enc_sha256: row.get(19)?,
        media_duration_secs: None,
        is_voice_note: false,
        link_title: None,
        link_description: None,
        link_url: None,
        caption: None,
        is_gif: false,
    })
}
