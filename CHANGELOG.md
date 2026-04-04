# Changelog

All notable changes to this project will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.2] - 2026-04-04

### Fixed

- Checkmark status mapping off-by-one (WA proto: 0=ERROR not PENDING, etc.)
- Chat ordering regression from history sync overwriting newer timestamps with older ones
- Image layout: reserved space now matches actual rendered height using decoded dimensions
- Group names missing after sync — fetches subjects from WA server on connect
- Group member names showing raw LID numbers — displays `~XXXX` prefix, resolves via real-time push names
- Contact JID normalization: push names stored under full `@s.whatsapp.net` format
- `set_chat_name_by_lid` called with wrong argument (phone JID instead of LID)
- Image download failing silently due to unstable WA connection

### Added

- `--resync` CLI flag to wipe databases and trigger fresh QR pairing
- Auto-download images alongside stickers when opening a chat (with retry + backoff)
- Proactive group subject resolution for unnamed groups on connect
- `sender_push_name` persisted in messages table for group sender display
- LID→phone mapping preserved from ContactUpdate events without names
- Force-exit on quit to prevent stale process keeping old binary in memory

## [0.1.1] - 2026-04-03

### Fixed

- Counterparty messages not rendering due to LID/phone JID mismatch in history sync and real-time delivery
- Chat list ordering using stale conversation metadata instead of actual message timestamps
- Contact name resolution wiped by partial updates (INSERT OR REPLACE → COALESCE upsert)
- Image download required message selection that wasn't set on chat open
- Image/sticker rendering: proper sizing (50x12 images, 20x8 stickers), visual line positioning, macOS `open` support
- Message pane scroll clipping due to logical vs visual line count mismatch
- tmux send-keys launch race condition (stale event drain)

### Added

- LID→phone JID migration on ContactUpdate events
- Index on `chats(lid_jid)` for fast LID resolution
- Real-time LID JID resolution for incoming messages

## [0.1.0] - 2026-04-02

### Added

- Initial release: QR auth, chat list, messages, vim keybindings
- Rich media display (video, voice notes, link previews)
- Message pagination and search
- Desktop notifications
- SQLite storage with FTS5 search

[0.1.2]: https://github.com/TopengDev/whatsapp-tui/releases/tag/v0.1.2
[0.1.1]: https://github.com/TopengDev/whatsapp-tui/releases/tag/v0.1.1
[0.1.0]: https://github.com/TopengDev/whatsapp-tui/releases/tag/v0.1.0
