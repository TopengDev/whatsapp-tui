use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct Reaction {
    pub message_id: String,
    pub sender_jid: String,
    pub emoji: String,
    pub timestamp: i64,
}

pub fn upsert(conn: &Connection, r: &Reaction) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO reactions (message_id, sender_jid, emoji, timestamp) \
         VALUES (?1, ?2, ?3, ?4)",
        params![r.message_id, r.sender_jid, r.emoji, r.timestamp],
    )?;
    Ok(())
}

pub fn remove(conn: &Connection, message_id: &str, sender_jid: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM reactions WHERE message_id = ?1 AND sender_jid = ?2",
        params![message_id, sender_jid],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, message_id: &str) -> Result<Vec<Reaction>> {
    let mut stmt = conn.prepare(
        "SELECT message_id, sender_jid, emoji, timestamp FROM reactions WHERE message_id = ?1",
    )?;
    let rows = stmt
        .query_map(params![message_id], |row| {
            Ok(Reaction {
                message_id: row.get(0)?,
                sender_jid: row.get(1)?,
                emoji: row.get(2)?,
                timestamp: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
