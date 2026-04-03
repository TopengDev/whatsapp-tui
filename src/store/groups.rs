use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct GroupMember {
    pub jid: String,
    pub is_admin: bool,
    pub is_super_admin: bool,
}

pub fn set_members(conn: &Connection, group_jid: &str, members: &[GroupMember]) -> Result<()> {
    conn.execute(
        "DELETE FROM group_members WHERE group_jid = ?1",
        params![group_jid],
    )?;
    let mut stmt = conn.prepare(
        "INSERT INTO group_members (group_jid, member_jid, is_admin, is_super_admin) \
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    for m in members {
        stmt.execute(params![
            group_jid,
            m.jid,
            m.is_admin as i32,
            m.is_super_admin as i32,
        ])?;
    }
    Ok(())
}

pub fn get_members(conn: &Connection, group_jid: &str) -> Result<Vec<GroupMember>> {
    let mut stmt = conn.prepare(
        "SELECT member_jid, is_admin, is_super_admin FROM group_members WHERE group_jid = ?1",
    )?;
    let rows = stmt
        .query_map(params![group_jid], |row| {
            let is_admin: i32 = row.get(1)?;
            let is_super_admin: i32 = row.get(2)?;
            Ok(GroupMember {
                jid: row.get(0)?,
                is_admin: is_admin != 0,
                is_super_admin: is_super_admin != 0,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
