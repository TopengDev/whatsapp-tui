# whatsapp-tui — MVP Scope

> Phase 3 output. Research: `~/claude/wa-cli-research.md` (784 lines). Decisions: `~/.claude/projects/-home-christopher-claude/memory/project_wa_tui_cli.md`.

---

## Core Flow

**Navigate chats, read messages, send messages — daily-drivable from day one.**

The user launches `whatsapp-tui`, authenticates (first run only), and lands in a two-pane view: chat list on the left, message history on the right. They navigate chats with vim motions (j/k), read scrollback, press `i` to compose, `Enter` to send. Groups work identically to DMs. Reactions, replies, edits all work. The app stays connected in the background, syncs messages, and sends desktop notifications.

This is not a demo or prototype — it's a replacement-grade terminal WhatsApp client from v0.1.

---

## MVP Features

### Authentication
- **QR code authentication** — render QR in terminal using Unicode block characters
- **Phone number + pairing code** — enter number, approve from phone notification
- **Session persistence** — store credentials in SQLite, no re-auth on restart
- **Re-auth flow** — detect expired session, prompt re-authentication gracefully

### Message History
- **Sync ~90 days of history** on first connect (WhatsApp provides this to companion devices)
- **Store all messages in SQLite** with timestamps, sender, chat ID, message type
- **FTS5 indexing** — full-text search across all messages from day one (it's built into the storage layer anyway)
- **Incremental sync** — on subsequent launches, fetch only new messages since last disconnect

### UI Layout
```
+------------------+--------------------------------------+
| Contacts         | Chat: Alice                    [N]  |
|                  |--------------------------------------|
| > Alice     (2)  | Alice  10:30                        |
|   Bob            | Hey, are you coming tonight?         |
|   Work Group     |                                     |
|   Mom            | You  10:32                          |
|   Dev Chat       | Yeah, I'll be there around 8        |
|                  |                                     |
|                  | Alice  10:33                        |
|                  | Perfect! See you then               |
|                  |                                     |
|                  |--------------------------------------|
|                  | > Type a message...           [INS] |
|------------------+--------------------------------------|
| NORMAL | whatsapp-tui                          v0.1.0 |
+---------------------------------------------------------+
```

- **Left pane:** Chat list — name, unread count, last message preview, timestamp
- **Right pane:** Message history (top) + input bar (bottom)
- **Info panel:** Toggleable right panel (contact/group info) — `Ctrl-w i` to toggle
- **Status bar:** Bottom — current mode, connection status, app version
- **Header bar:** Top of chat pane — chat name, online/typing status, mode indicator

### Vim Keybindings (Flat Match — v0.1)

**Contact List (Normal mode):**
| Key | Action |
|---|---|
| `j` / `k` | Navigate chats |
| `gg` / `G` | First / last chat |
| `Ctrl-d` / `Ctrl-u` | Half-page scroll |
| `/` | Search chats (fuzzy filter) |
| `Enter` or `l` | Open selected chat |
| `q` | Quit (with confirmation) |

**Chat View (Normal mode):**
| Key | Action |
|---|---|
| `j` / `k` | Scroll messages |
| `gg` / `G` | Oldest / newest message |
| `Ctrl-d` / `Ctrl-u` | Half-page scroll |
| `/` | Search messages (FTS5) |
| `h` or `Esc` | Back to chat list |
| `i` | Enter Insert mode (compose message) |
| `r` | Reply to selected message |
| `e` | React to selected message (emoji picker) |
| `d` | Delete message (confirm prompt) |
| `E` | Edit selected message (own messages only) |
| `y` | Yank/copy message text to clipboard |
| `o` | Open link/media in external viewer |

**Message Input (Insert mode):**
| Key | Action |
|---|---|
| (type normally) | Compose message |
| `Enter` | Send message |
| `Shift-Enter` | Insert newline |
| `Esc` | Exit to Normal mode |
| `Ctrl-w` | Delete word backward |
| `Ctrl-u` | Clear line |

**Panel Navigation:**
| Key | Action |
|---|---|
| `Tab` | Cycle focus: Chat List -> Messages -> Input |
| `Ctrl-w h` / `Ctrl-w l` | Switch panel left/right |
| `Ctrl-w i` | Toggle info panel |

**Command Mode (`:`):**
| Command | Action |
|---|---|
| `:q` | Quit |
| `:search <term>` | Search messages |
| `:mute` | Mute current chat |
| `:archive` | Archive current chat |
| `:pin` | Pin current chat |
| `:info` | Toggle info panel |

### Media Handling
- **Text placeholders** for all media types:
  - `[Image]`, `[Video 2:34]`, `[Audio 0:45]`, `[Document: report.pdf]`, `[Sticker]`, `[Location: -6.2, 106.8]`
- **External viewer** — press `o` on a media message to download and open with `xdg-open`
- **No inline rendering** in v0.1 — deferred to v2 (ratatui-image)

### Group Chats
- **List groups** in chat list alongside DMs (with group icon indicator)
- **Read group messages** with sender name displayed
- **Send messages** to groups
- **View member list** in info panel
- **Group metadata** — name, description, participant count, admins
- **No group management** in v0.1 — no create/invite/kick/leave (deferred to v2)

### Reactions, Replies, Edits
- **Send reactions** — `e` on a message opens emoji picker, select to react
- **Receive reactions** — display reactions below messages (e.g., `[thumbsup x2, heart x1]`)
- **Reply to messages** — `r` quotes the selected message, compose reply
- **Display replies** — show quoted message context above the reply
- **Edit own messages** — `E` on own message reopens it for editing
- **Display edits** — show `(edited)` indicator
- **Delete messages** — `d` with confirmation, "Delete for Everyone"

### Notifications
- **Desktop notifications** via `notify-rust` — new messages when chat isn't focused
- **Visual indicators** in chat list:
  - Unread count badge (bold, highlighted)
  - Bold chat name for unread chats
  - Last message preview + timestamp
- **Typing indicators** — show "typing..." in chat header when contact is typing
- **Read receipts** — display sent/delivered/read status on own messages (single/double check, blue check)

### Connection & Error Handling
- **Connection status** in status bar — Connected / Connecting / Disconnected / Reconnecting
- **Automatic reconnection** with exponential backoff on disconnect
- **Graceful degradation** — show cached messages while reconnecting
- **Error display** — non-fatal errors shown in status bar, auto-dismiss after timeout
- **Fatal errors** — full-screen error with diagnostic info and restart suggestion
- **Logging** — `tracing` to file (`~/.local/share/whatsapp-tui/logs/`) for debugging

### Configuration
- **XDG-compliant** from day one:
  - Config: `~/.config/whatsapp-tui/config.toml`
  - Data: `~/.local/share/whatsapp-tui/` (SQLite DB, media cache, logs)
  - Cache: `~/.cache/whatsapp-tui/`
- **config.toml structure:**
  ```toml
  [general]
  # notification_sound = false  # terminal bell on notification
  # confirm_quit = true
  # timestamp_format = "%H:%M"

  [appearance]
  # theme = "default"
  # show_avatars = false
  # message_max_width = 80

  [connection]
  # reconnect_max_retries = 10
  # reconnect_base_delay_ms = 1000

  [notifications]
  # desktop = true
  # show_preview = true        # show message content in notification
  # muted_chats = false        # notify for muted chats
  ```

### Storage (SQLite + FTS5)
- **Messages table** — id, chat_id, sender_jid, timestamp, content, message_type, media_path, reply_to, edited, deleted
- **Chats table** — jid, name, is_group, last_message_ts, unread_count, muted, pinned, archived
- **Contacts table** — jid, name, push_name, phone, profile_pic_url
- **FTS5 virtual table** — indexed on message content for fast full-text search
- **Session/auth data** — managed by whatsapp-rust's sqlite-storage

---

## Explicitly Out of Scope for MVP

| Feature | Deferred To | Why |
|---|---|---|
| Inline image rendering | v2 | Requires ratatui-image, terminal capability detection, rendering pipeline |
| TOML-configurable keybindings | v0.3 | Flat match is sufficient for MVP, config layer adds complexity |
| Multi-key sequences (gg, Ctrl-w h) | v0.1 has basics | Full prefix-matching HashMap engine deferred to v0.3 |
| modalkit integration | v1.0 | Count prefix, dot-repeat, registers — vim power-user features |
| Group management (create/invite/kick) | v2 | Read + send is sufficient, management is rare |
| Status/Stories posting | Never | High ban risk per research |
| Voice/video calls | Never | No unofficial library supports this |
| Broadcast lists | Never | Phone-only feature |
| Newsletter management | v2+ | Fragile API, low priority |
| Sticker sending | v2 | Receiving shows placeholder, sending needs asset pipeline |
| Custom themes | v2 | Default theme with sensible colors first |
| Mouse support | v0.3 | Vim-first, mouse is secondary |
| Multiple accounts | v3+ | Single account covers personal use |
| Message pinning in chat | v2 | Low priority |
| Disappearing messages config | v2 | Display works, toggling deferred |

---

## Launch Criteria

v0.1 is "done" when Christopher can:

1. Launch `whatsapp-tui` and authenticate via QR or pairing code
2. See all chats with unread counts, sorted by recent activity
3. Navigate chats with j/k, open with Enter/l
4. Read full message history (90-day sync) with scrollback
5. Send text messages to any DM or group
6. Reply to, react to, edit, and delete messages
7. Search messages across all chats with `/`
8. Receive desktop notifications for new messages
9. See typing indicators and read receipts
10. Have the app gracefully reconnect on network issues
11. Close and reopen without re-authenticating

**The bar:** "I reach for this instead of opening WhatsApp Web."

---

## Phase Roadmap

### v0.1 — MVP (Target: daily-drivable)
Everything in "MVP Features" above. Single binary, works on Christopher's setup (alacritty + tmux). Private repo.

### v0.2 — Media & Polish
- Inline image previews via ratatui-image (sixel/kitty protocol detection)
- Sticker rendering (WebP decode + display)
- Audio/video duration + thumbnail display
- Document download progress indicator
- Contact/group profile pictures in info panel
- Message formatting (bold, italic, strikethrough, monospace)
- Link preview rendering

### v0.3 — Vim Power & Customization
- HashMap prefix-matching keybinding engine
- TOML-configurable keybindings (`~/.config/whatsapp-tui/keys.toml`)
- Multi-key sequences with which-key popup
- Mouse support (click to select chat, scroll messages)
- Custom color themes via TOML
- Chat pinning, archiving, muting from UI

### v0.4 — Group Management & Advanced Features
- Create groups, invite members, kick, leave
- Group admin actions (change name, description, settings)
- Disappearing messages toggle
- Message forwarding
- Contact blocking/unblocking
- Export chat history
- Polls (create and vote)

### v1.0 — Production Release
- modalkit integration — full vim fidelity (counts, dot-repeat, registers, macros)
- Robust error recovery for all edge cases
- Performance optimization (large chat history, many groups)
- Man page, shell completions (bash, zsh, fish)
- AUR / Homebrew / cargo install packaging
- Open source release (if decided)

---

## Success Metrics

| Metric | Target |
|---|---|
| Daily usage | Christopher uses it as primary WhatsApp interface |
| Startup time | < 2 seconds to interactive (cached auth) |
| Message send latency | < 500ms from Enter to delivered |
| Memory usage | < 50MB RSS (target < 20MB) |
| Binary size | < 20MB (release, stripped) |
| Crash rate | Zero crashes in normal usage over 1 week |
| Reconnection | Auto-recovers within 30s of network restoration |

---

## Tech Stack (Locked)

| Layer | Crate | Version |
|---|---|---|
| TUI framework | ratatui + crossterm | 0.30 / 0.29 |
| Async runtime | tokio | 1.x (full features) |
| WhatsApp protocol | whatsapp-rust (or wa-rs for stable) | 0.5 |
| Message input | tui-textarea | 0.7 |
| Scrollable views | tui-scrollview | 0.6 |
| List widget | tui-widget-list | 0.15 |
| SQLite | rusqlite (bundled, fts5) | latest |
| Config | toml + serde | 0.8 / 1.0 |
| CLI args | clap | 4.x |
| Logging | tracing + tracing-appender | 0.1 |
| Notifications | notify-rust | latest |
| XDG paths | dirs | 5.x |
| Clipboard | arboard | latest |
| Serialization | serde + serde_json | 1.x |
