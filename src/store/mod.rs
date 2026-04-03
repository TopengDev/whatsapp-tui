pub mod chats;
pub mod contacts;
pub mod groups;
pub mod messages;
pub mod reactions;

use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use crate::store::{
    chats::Chat, contacts::Contact, groups::GroupMember, messages::Message, reactions::Reaction,
};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(data_dir: &Path) -> Result<Self> {
        let db_path = data_dir.join("app.db");
        let conn = Connection::open(&db_path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let schema = include_str!("schema.sql");
        self.conn.execute_batch(schema)?;
        // v2: add lid_jid column if missing
        let _ = self.conn.execute("ALTER TABLE chats ADD COLUMN lid_jid TEXT", []);
        // v3: add media download params to messages
        let _ = self.conn.execute("ALTER TABLE messages ADD COLUMN media_direct_path TEXT", []);
        let _ = self.conn.execute("ALTER TABLE messages ADD COLUMN media_key BLOB", []);
        let _ = self.conn.execute("ALTER TABLE messages ADD COLUMN media_file_sha256 BLOB", []);
        let _ = self.conn.execute("ALTER TABLE messages ADD COLUMN media_file_enc_sha256 BLOB", []);
        Ok(())
    }

    /// Bulk-insert a chat and its messages inside a single transaction.
    /// Much faster than individual inserts during history sync.
    pub fn bulk_sync_chat(&self, chat: &Chat, messages: &[Message]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        chats::upsert(&tx, chat)?;
        for msg in messages {
            // Use INSERT OR IGNORE to skip duplicates silently
            let _ = messages::insert(&tx, msg);
        }
        tx.commit()?;
        Ok(())
    }

    // --- Messages ---

    pub fn insert_message(&self, msg: &Message) -> Result<()> {
        messages::insert(&self.conn, msg)
    }

    pub fn get_messages(
        &self,
        chat_jid: &str,
        before_ts: Option<i64>,
        limit: u32,
    ) -> Result<Vec<Message>> {
        messages::get(&self.conn, chat_jid, before_ts, limit)
    }

    pub fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<(Message, String)>> {
        messages::search(&self.conn, query, limit)
    }

    pub fn update_message_status(&self, id: &str, status: &str) -> Result<()> {
        messages::update_status(&self.conn, id, status)
    }

    pub fn mark_edited(&self, id: &str, new_content: &str) -> Result<()> {
        messages::mark_edited(&self.conn, id, new_content)
    }

    pub fn mark_deleted(&self, id: &str) -> Result<()> {
        messages::mark_deleted(&self.conn, id)
    }

    // --- Chats ---

    pub fn upsert_chat(&self, chat: &Chat) -> Result<()> {
        chats::upsert(&self.conn, chat)
    }

    pub fn set_chat_name(&self, jid: &str, name: &str) -> Result<()> {
        chats::set_name(&self.conn, jid, name)
    }

    /// Bulk-resolve chat names from the contacts table.
    pub fn resolve_chat_names(&self) -> Result<usize> {
        chats::resolve_names_from_contacts(&self.conn)
    }

    /// Update chat name by LID JID (only if current name looks like a phone number).
    pub fn set_chat_name_by_lid(&self, lid_jid: &str, name: &str) -> Result<bool> {
        chats::set_name_by_lid(&self.conn, lid_jid, name)
    }

    pub fn get_chats_ordered(&self) -> Result<Vec<Chat>> {
        chats::get_ordered(&self.conn)
    }

    pub fn touch_last_message(
        &self,
        jid: &str,
        timestamp: i64,
        preview: Option<&str>,
    ) -> Result<()> {
        chats::touch_last_message(&self.conn, jid, timestamp, preview)
    }

    pub fn set_unread_count(&self, jid: &str, count: i32) -> Result<()> {
        chats::set_unread_count(&self.conn, jid, count)
    }

    pub fn set_muted(&self, jid: &str, muted: bool) -> Result<()> {
        chats::set_muted(&self.conn, jid, muted)
    }

    pub fn set_pinned(&self, jid: &str, pinned: bool) -> Result<()> {
        chats::set_pinned(&self.conn, jid, pinned)
    }

    pub fn set_archived(&self, jid: &str, archived: bool) -> Result<()> {
        chats::set_archived(&self.conn, jid, archived)
    }

    /// Build a JID → display name map from contacts + chats tables.
    pub fn get_display_names(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut map = std::collections::HashMap::new();
        // From contacts (saved names + push names)
        let mut stmt = self.conn.prepare(
            "SELECT jid, COALESCE(name, push_name) FROM contacts \
             WHERE COALESCE(name, push_name) IS NOT NULL AND COALESCE(name, push_name) != ''",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            if let Ok((jid, name)) = row {
                map.insert(jid, name);
            }
        }
        // From chats (for groups and contacts resolved via ContactUpdate)
        let mut stmt2 = self.conn.prepare(
            "SELECT jid, name FROM chats WHERE name NOT GLOB '[0-9]*' AND name != ''",
        )?;
        let rows2 = stmt2.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows2 {
            if let Ok((jid, name)) = row {
                map.entry(jid).or_insert(name);
            }
        }
        Ok(map)
    }

    // --- Contacts ---

    pub fn upsert_contact(&self, contact: &Contact) -> Result<()> {
        contacts::upsert(&self.conn, contact)
    }

    pub fn get_contact(&self, jid: &str) -> Result<Option<Contact>> {
        contacts::get(&self.conn, jid)
    }

    // --- Reactions ---

    pub fn upsert_reaction(&self, r: &Reaction) -> Result<()> {
        reactions::upsert(&self.conn, r)
    }

    pub fn remove_reaction(&self, message_id: &str, sender_jid: &str) -> Result<()> {
        reactions::remove(&self.conn, message_id, sender_jid)
    }

    pub fn get_reactions(&self, message_id: &str) -> Result<Vec<Reaction>> {
        reactions::get(&self.conn, message_id)
    }

    // --- Groups ---

    pub fn set_group_members(&self, group_jid: &str, members: &[GroupMember]) -> Result<()> {
        groups::set_members(&self.conn, group_jid, members)
    }

    pub fn get_group_members(&self, group_jid: &str) -> Result<Vec<GroupMember>> {
        groups::get_members(&self.conn, group_jid)
    }
}
