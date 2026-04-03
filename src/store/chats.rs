use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct Chat {
    pub jid: String,
    pub name: String,
    pub is_group: bool,
    pub last_message_ts: Option<i64>,
    pub last_message_preview: Option<String>,
    pub unread_count: i32,
    pub muted: bool,
    pub pinned: bool,
    pub archived: bool,
    /// Linked Identity JID — maps this chat to its LID for push name resolution.
    pub lid_jid: Option<String>,
}

pub fn upsert(conn: &Connection, chat: &Chat) -> Result<()> {
    conn.execute(
        "INSERT INTO chats (jid, name, is_group, last_message_ts, \
         last_message_preview, unread_count, muted, pinned, archived, lid_jid) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
         ON CONFLICT(jid) DO UPDATE SET \
           name = CASE WHEN excluded.name != '' THEN excluded.name ELSE chats.name END, \
           is_group = excluded.is_group, \
           last_message_ts = COALESCE(excluded.last_message_ts, chats.last_message_ts), \
           last_message_preview = COALESCE(excluded.last_message_preview, chats.last_message_preview), \
           unread_count = CASE WHEN excluded.unread_count > 0 THEN excluded.unread_count ELSE chats.unread_count END, \
           muted = excluded.muted, \
           pinned = excluded.pinned, \
           archived = excluded.archived, \
           lid_jid = COALESCE(excluded.lid_jid, chats.lid_jid)",
        params![
            chat.jid,
            chat.name,
            chat.is_group as i32,
            chat.last_message_ts,
            chat.last_message_preview,
            chat.unread_count,
            chat.muted as i32,
            chat.pinned as i32,
            chat.archived as i32,
            chat.lid_jid,
        ],
    )?;
    Ok(())
}

/// Update only the last message metadata for a chat, preserving all other fields.
pub fn touch_last_message(
    conn: &Connection,
    jid: &str,
    timestamp: i64,
    preview: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE chats SET last_message_ts = ?1, last_message_preview = ?2 WHERE jid = ?3",
        params![timestamp, preview, jid],
    )?;
    Ok(())
}

pub fn get_ordered(conn: &Connection) -> Result<Vec<Chat>> {
    let mut stmt = conn.prepare(
        "SELECT jid, name, is_group, last_message_ts, last_message_preview, \
         unread_count, muted, pinned, archived, lid_jid \
         FROM chats \
         WHERE jid NOT LIKE '%@lid' AND archived = 0 \
         ORDER BY pinned DESC, last_message_ts DESC NULLS LAST",
    )?;
    let rows = stmt
        .query_map([], |row| {
            let is_group: i32 = row.get(2)?;
            let muted: i32 = row.get(6)?;
            let pinned: i32 = row.get(7)?;
            let archived: i32 = row.get(8)?;
            Ok(Chat {
                jid: row.get(0)?,
                name: row.get(1)?,
                is_group: is_group != 0,
                last_message_ts: row.get(3)?,
                last_message_preview: row.get(4)?,
                unread_count: row.get(5)?,
                muted: muted != 0,
                pinned: pinned != 0,
                archived: archived != 0,
                lid_jid: row.get(9)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Bulk-resolve chat names from the contacts table.
/// Updates any chat whose name still looks like a phone number
/// if a matching contact has a saved name.
pub fn resolve_names_from_contacts(conn: &Connection) -> Result<usize> {
    let changed = conn.execute(
        "UPDATE chats SET name = ( \
            SELECT COALESCE(c.name, c.push_name) \
            FROM contacts c \
            WHERE c.jid = chats.jid \
            AND COALESCE(c.name, c.push_name) IS NOT NULL \
            AND COALESCE(c.name, c.push_name) != '' \
         ) \
         WHERE (chats.name GLOB '[0-9]*' OR chats.name GLOB '+[0-9]*') \
         AND EXISTS ( \
            SELECT 1 FROM contacts c \
            WHERE c.jid = chats.jid \
            AND COALESCE(c.name, c.push_name) IS NOT NULL \
            AND COALESCE(c.name, c.push_name) != '' \
         )",
        [],
    )?;
    Ok(changed)
}

/// Find a chat by its LID JID and update its name.
pub fn set_name_by_lid(conn: &Connection, lid_jid: &str, name: &str) -> Result<bool> {
    let changed = conn.execute(
        "UPDATE chats SET name = ?1 WHERE lid_jid = ?2 AND (name = '' OR name GLOB '[0-9]*')",
        params![name, lid_jid],
    )?;
    Ok(changed > 0)
}

pub fn set_unread_count(conn: &Connection, jid: &str, count: i32) -> Result<()> {
    conn.execute(
        "UPDATE chats SET unread_count = ?1 WHERE jid = ?2",
        params![count, jid],
    )?;
    Ok(())
}

/// Update chat name only if the new name is non-empty and the chat exists.
pub fn set_name(conn: &Connection, jid: &str, name: &str) -> Result<()> {
    if !name.is_empty() {
        conn.execute(
            "UPDATE chats SET name = ?1 WHERE jid = ?2",
            params![name, jid],
        )?;
    }
    Ok(())
}

pub fn set_muted(conn: &Connection, jid: &str, muted: bool) -> Result<()> {
    conn.execute(
        "UPDATE chats SET muted = ?1 WHERE jid = ?2",
        params![muted as i32, jid],
    )?;
    Ok(())
}

pub fn set_pinned(conn: &Connection, jid: &str, pinned: bool) -> Result<()> {
    conn.execute(
        "UPDATE chats SET pinned = ?1 WHERE jid = ?2",
        params![pinned as i32, jid],
    )?;
    Ok(())
}

pub fn set_archived(conn: &Connection, jid: &str, archived: bool) -> Result<()> {
    conn.execute(
        "UPDATE chats SET archived = ?1 WHERE jid = ?2",
        params![archived as i32, jid],
    )?;
    Ok(())
}
