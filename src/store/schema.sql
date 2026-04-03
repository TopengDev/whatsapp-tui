PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Chats (DMs and groups unified)
CREATE TABLE IF NOT EXISTS chats (
    jid             TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    is_group        INTEGER NOT NULL DEFAULT 0,
    last_message_ts INTEGER,
    last_message_preview TEXT,
    unread_count    INTEGER NOT NULL DEFAULT 0,
    muted           INTEGER NOT NULL DEFAULT 0,
    pinned          INTEGER NOT NULL DEFAULT 0,
    archived        INTEGER NOT NULL DEFAULT 0,
    lid_jid         TEXT
);

CREATE INDEX IF NOT EXISTS idx_chats_order
    ON chats(pinned DESC, last_message_ts DESC);

-- Contacts
CREATE TABLE IF NOT EXISTS contacts (
    jid             TEXT PRIMARY KEY,
    name            TEXT,
    push_name       TEXT,
    phone           TEXT,
    profile_pic_url TEXT
);

-- Messages
CREATE TABLE IF NOT EXISTS messages (
    id              TEXT PRIMARY KEY,
    chat_jid        TEXT NOT NULL REFERENCES chats(jid),
    sender_jid      TEXT NOT NULL,
    timestamp       INTEGER NOT NULL,
    content         TEXT,
    message_type    TEXT NOT NULL DEFAULT 'text',
    media_mime      TEXT,
    media_size      INTEGER,
    media_filename  TEXT,
    media_local_path TEXT,
    reply_to_id     TEXT,
    reply_to_preview TEXT,
    edited          INTEGER NOT NULL DEFAULT 0,
    deleted         INTEGER NOT NULL DEFAULT 0,
    from_me         INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'sent',
    media_direct_path TEXT,
    media_key       BLOB,
    media_file_sha256 BLOB,
    media_file_enc_sha256 BLOB
);

CREATE INDEX IF NOT EXISTS idx_messages_chat_ts
    ON messages(chat_jid, timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_messages_chat_id
    ON messages(chat_jid, id);

-- FTS5 full-text search on message content
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content,
    content=messages,
    content_rowid=rowid,
    tokenize='unicode61'
);

-- Triggers to keep FTS in sync
CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content)
        VALUES('delete', old.rowid, old.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE OF content ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content)
        VALUES('delete', old.rowid, old.content);
    INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;

-- Reactions (one per sender per message)
CREATE TABLE IF NOT EXISTS reactions (
    message_id      TEXT NOT NULL REFERENCES messages(id),
    sender_jid      TEXT NOT NULL,
    emoji           TEXT NOT NULL,
    timestamp       INTEGER NOT NULL,
    PRIMARY KEY (message_id, sender_jid)
);

-- Group members
CREATE TABLE IF NOT EXISTS group_members (
    group_jid       TEXT NOT NULL REFERENCES chats(jid),
    member_jid      TEXT NOT NULL,
    is_admin        INTEGER NOT NULL DEFAULT 0,
    is_super_admin  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (group_jid, member_jid)
);

-- Schema version tracking
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);

INSERT OR IGNORE INTO schema_version VALUES (1);

-- Migration v2: add lid_jid column for LID↔phone JID mapping (safe for existing DBs)
-- SQLite doesn't have IF NOT EXISTS for ALTER TABLE, so we handle errors in Rust
