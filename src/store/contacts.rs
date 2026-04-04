use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct Contact {
    pub jid: String,
    pub name: Option<String>,
    pub push_name: Option<String>,
    pub phone: Option<String>,
    pub profile_pic_url: Option<String>,
    /// Original LID JID when this contact was resolved from a LID→phone mapping.
    /// Used to migrate orphaned messages stored under the LID.
    pub lid_jid: Option<String>,
}

pub fn upsert(conn: &Connection, contact: &Contact) -> Result<()> {
    conn.execute(
        "INSERT INTO contacts (jid, name, push_name, phone, profile_pic_url, lid_jid) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(jid) DO UPDATE SET \
           name = COALESCE(excluded.name, contacts.name), \
           push_name = COALESCE(excluded.push_name, contacts.push_name), \
           phone = COALESCE(excluded.phone, contacts.phone), \
           profile_pic_url = COALESCE(excluded.profile_pic_url, contacts.profile_pic_url), \
           lid_jid = COALESCE(excluded.lid_jid, contacts.lid_jid)",
        params![
            contact.jid,
            contact.name,
            contact.push_name,
            contact.phone,
            contact.profile_pic_url,
            contact.lid_jid,
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, jid: &str) -> Result<Option<Contact>> {
    let mut stmt = conn.prepare(
        "SELECT jid, name, push_name, phone, profile_pic_url, lid_jid FROM contacts WHERE jid = ?1",
    )?;
    let mut rows = stmt.query_map(params![jid], |row| {
        Ok(Contact {
            jid: row.get(0)?,
            name: row.get(1)?,
            push_name: row.get(2)?,
            phone: row.get(3)?,
            profile_pic_url: row.get(4)?,
            lid_jid: row.get(5)?,
        })
    })?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}
