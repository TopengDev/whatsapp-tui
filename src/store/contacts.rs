use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct Contact {
    pub jid: String,
    pub name: Option<String>,
    pub push_name: Option<String>,
    pub phone: Option<String>,
    pub profile_pic_url: Option<String>,
}

pub fn upsert(conn: &Connection, contact: &Contact) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO contacts (jid, name, push_name, phone, profile_pic_url) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            contact.jid,
            contact.name,
            contact.push_name,
            contact.phone,
            contact.profile_pic_url,
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, jid: &str) -> Result<Option<Contact>> {
    let mut stmt = conn.prepare(
        "SELECT jid, name, push_name, phone, profile_pic_url FROM contacts WHERE jid = ?1",
    )?;
    let mut rows = stmt.query_map(params![jid], |row| {
        Ok(Contact {
            jid: row.get(0)?,
            name: row.get(1)?,
            push_name: row.get(2)?,
            phone: row.get(3)?,
            profile_pic_url: row.get(4)?,
        })
    })?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}
