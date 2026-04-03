# whatsapp-tui — Architecture

> Phase 4 output. Based on research at `~/claude/wa-cli-research.md` and scope at `docs/ideation/03-scope.md`.

---

## System Overview

Single Rust binary, single process. Three major subsystems communicate through a unified event channel.

```
                          ┌─────────────────────────────────┐
                          │         tokio runtime           │
                          │                                 │
  ┌───────────────┐       │  ┌───────────┐  ┌───────────┐  │
  │ Terminal       │──tx──→│  │           │  │           │  │
  │ (EventStream)  │       │  │  Event    │  │  App      │  │
  └───────────────┘       │  │  Channel  │──→  State    │  │
  ┌───────────────┐       │  │  (mpsc)   │  │           │  │
  │ WA Client      │──tx──→│  │           │  │  ┌─────┐ │  │
  │ (whatsapp-rust)│       │  └───────────┘  │  │ UI  │ │  │
  └───────────────┘       │                  │  └──┬──┘ │  │
  ┌───────────────┐       │  ┌───────────┐   │     │    │  │
  │ Tick Timer     │──tx──→│  │ SQLite    │←──│─────┘    │  │
  │ (interval)     │       │  │ (app db)  │   └──────────┘  │
  └───────────────┘       │  └───────────┘                  │
                          │  ┌───────────┐                  │
                          │  │ SQLite    │ ← whatsapp-rust  │
                          │  │ (session) │   manages this   │
                          │  └───────────┘                  │
                          └─────────────────────────────────┘
```

**Key principle:** All mutation flows through events. The main loop is: **receive event → update state → render**. No widget ever mutates app state directly.

---

## Crate Layout

Single crate. No workspace — premature for a personal tool. Well-organized modules provide the boundaries.

```
whatsapp-tui/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── docs/
│   └── ideation/
│       ├── 03-scope.md
│       └── 04-architecture.md
├── config/
│   └── default.toml          # shipped default config (embedded via include_str!)
└── src/
    ├── main.rs                # entry point: CLI args, init runtime, run app
    ├── app.rs                 # App struct, top-level event dispatch, state transitions
    ├── config.rs              # Config struct, TOML loading, XDG path resolution
    ├── event.rs               # AppEvent enum, event channel setup
    │
    ├── wa/                    # WhatsApp protocol layer
    │   ├── mod.rs             # re-exports
    │   ├── client.rs          # WaClient — connection, reconnect, send operations
    │   ├── auth.rs            # auth flows (QR display, pairing code input)
    │   ├── events.rs          # WaEvent enum — maps whatsapp-rust events to our types
    │   └── media.rs           # media download, cache management, xdg-open
    │
    ├── store/                 # Persistence layer (app database — NOT session db)
    │   ├── mod.rs             # Store struct, connection pool, migrations
    │   ├── schema.sql         # embedded SQL schema
    │   ├── messages.rs        # message CRUD, FTS5 search, pagination
    │   ├── chats.rs           # chat CRUD, ordering, unread counts
    │   ├── contacts.rs        # contact CRUD
    │   ├── reactions.rs       # reaction CRUD
    │   └── groups.rs          # group member CRUD
    │
    ├── ui/                    # TUI rendering (pure functions — state in, frame out)
    │   ├── mod.rs             # top-level render() dispatch
    │   ├── layout.rs          # pane layout constraints, resize handling
    │   ├── chat_list.rs       # left pane: chat list widget
    │   ├── messages.rs        # middle pane: message history rendering
    │   ├── input.rs           # message input bar (wraps tui-textarea)
    │   ├── info_panel.rs      # right pane: contact/group info
    │   ├── status_bar.rs      # bottom: mode, connection status, version
    │   ├── header.rs          # top of chat pane: name, typing indicator
    │   ├── theme.rs           # Color/Style constants
    │   └── overlay/           # modal overlays rendered on top of main layout
    │       ├── mod.rs
    │       ├── command.rs     # : command palette
    │       ├── search.rs      # / search results
    │       ├── emoji.rs       # emoji picker grid
    │       ├── confirm.rs     # yes/no confirmation dialog
    │       └── qr.rs          # QR code rendering for auth
    │
    ├── keys/                  # Keybinding layer
    │   ├── mod.rs             # dispatch(key, mode, focus) -> Option<Action>
    │   └── action.rs          # Action enum — every possible user action
    │
    └── notify.rs              # desktop notification wrapper (notify-rust)
```

**22 source files.** Each has a clear, single responsibility. No file should exceed ~500 lines — split if it does.

---

## Module Responsibilities

### `main.rs` — Bootstrap

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::parse();
    let config = Config::load()?;
    tracing_init(&config)?;

    let store = Store::open(&config.data_dir())?;
    let (event_tx, event_rx) = mpsc::unbounded_channel::<AppEvent>();

    let wa_client = WaClient::new(&config, event_tx.clone()).await?;
    let terminal = setup_terminal()?;

    let mut app = App::new(config, store, wa_client, event_tx.clone());
    app.run(terminal, event_rx).await?;

    restore_terminal()?;
    Ok(())
}
```

No logic here. Parse args, build dependencies, hand off to `App::run`.

### `app.rs` — Central State Machine

The `App` struct owns all mutable state. It is the **single source of truth**.

```rust
pub struct App {
    // State
    mode: AppMode,
    focus: AppFocus,
    chats: Vec<Chat>,
    selected_chat_idx: usize,
    active_chat: Option<ActiveChat>,
    connection_status: ConnectionStatus,
    should_quit: bool,

    // Overlay state
    overlay: Option<Overlay>,
    command_input: String,
    search_query: String,
    search_results: Vec<SearchResult>,

    // Dependencies
    config: Config,
    store: Store,
    wa: WaClient,
    event_tx: mpsc::UnboundedSender<AppEvent>,
}

pub struct ActiveChat {
    pub jid: String,
    pub name: String,
    pub is_group: bool,
    pub messages: Vec<Message>,
    pub members: Vec<GroupMember>,     // empty for DMs
    pub scroll_offset: usize,
    pub input: TextArea<'static>,     // tui-textarea instance
    pub reply_to: Option<MessageId>,  // if replying
    pub typing_jids: HashSet<String>, // who is typing
}
```

**State transitions are explicit.** Every handler returns nothing — it mutates `&mut self` directly. No message-passing between components, no reducer pattern. This is a personal tool, not a framework.

```rust
impl App {
    pub async fn run(
        &mut self,
        mut terminal: Terminal<impl Backend>,
        mut event_rx: mpsc::UnboundedReceiver<AppEvent>,
    ) -> Result<()> {
        // Spawn terminal event reader
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            let mut reader = EventStream::new();
            while let Some(Ok(event)) = reader.next().await {
                if tx.send(AppEvent::Terminal(event)).is_err() {
                    break;
                }
            }
        });

        // Connect to WhatsApp (triggers auth flow if needed)
        self.wa.connect().await?;

        // Load chats from store
        self.load_chats()?;

        // Main loop
        let mut tick = interval(Duration::from_millis(250));
        loop {
            // Render
            terminal.draw(|frame| ui::render(frame, self))?;

            // Wait for next event
            tokio::select! {
                Some(event) = event_rx.recv() => self.handle_event(event).await,
                _ = tick.tick() => self.handle_tick(),
            }

            if self.should_quit {
                self.wa.disconnect().await?;
                break;
            }
        }
        Ok(())
    }

    async fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Terminal(ev) => self.handle_terminal(ev).await,
            AppEvent::Wa(ev) => self.handle_wa(ev).await,
            AppEvent::Task(result) => self.handle_task_result(result),
        }
    }
}
```

### `event.rs` — Unified Event Types

```rust
/// Every event that can occur in the application.
pub enum AppEvent {
    /// Terminal input (key press, mouse, resize)
    Terminal(crossterm::event::Event),

    /// WhatsApp protocol event
    Wa(WaEvent),

    /// Result from a background task (media download, etc.)
    Task(TaskResult),
}

/// Results from spawned background tasks
pub enum TaskResult {
    MediaDownloaded {
        message_id: String,
        local_path: PathBuf,
    },
    MediaOpenFailed {
        message_id: String,
        error: String,
    },
    HistorySynced {
        chat_jid: String,
        message_count: usize,
    },
}
```

### `wa/events.rs` — WhatsApp Events (mapped from whatsapp-rust)

```rust
/// Our domain events, mapped from whatsapp-rust's raw events.
/// This decouples app logic from the protocol library's API surface.
pub enum WaEvent {
    // Connection
    Connected,
    Disconnected { reason: String },
    LoggedOut,

    // Auth
    QrCode(String),             // QR data to render
    PairingCode(String),        // 8-digit code to display
    AuthSuccess,

    // Messages
    MessageReceived(Message),
    MessageEdited { id: String, new_content: String, timestamp: i64 },
    MessageDeleted { id: String, chat_jid: String },
    Receipt { message_id: String, status: MessageStatus },

    // Reactions
    ReactionReceived(Reaction),

    // Presence
    TypingStarted { chat_jid: String, user_jid: String },
    TypingStopped { chat_jid: String, user_jid: String },
    PresenceUpdate { jid: String, online: bool, last_seen: Option<i64> },

    // Chat metadata
    ChatUpdate(Chat),
    ContactUpdate(Contact),
    GroupMembersUpdate { group_jid: String, members: Vec<GroupMember> },

    // History sync
    HistoryMessages { chat_jid: String, messages: Vec<Message> },
}
```

**Why this mapping layer exists:** whatsapp-rust is pre-1.0 and its event types will change. By mapping to our own types at the boundary, protocol library changes don't ripple through the entire codebase. Only `wa/events.rs` and `wa/client.rs` need updating when upstream changes.

### `wa/client.rs` — Protocol Wrapper

```rust
pub struct WaClient {
    // whatsapp-rust client instance (exact type TBD — depends on their API)
    inner: WhatsAppClient,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    config: WaConfig,
    reconnect_state: ReconnectState,
}

struct ReconnectState {
    attempt: u32,
    max_retries: u32,
    base_delay: Duration,
}

impl WaClient {
    /// Connect to WhatsApp. If no session exists, triggers auth flow
    /// by sending QrCode/PairingCode events through the channel.
    pub async fn connect(&mut self) -> Result<()>;

    /// Disconnect cleanly
    pub async fn disconnect(&mut self) -> Result<()>;

    /// Send a text message
    pub async fn send_message(&self, chat_jid: &str, text: &str) -> Result<String>;

    /// Send a reply
    pub async fn send_reply(&self, chat_jid: &str, text: &str, reply_to: &str) -> Result<String>;

    /// Send a reaction
    pub async fn send_reaction(&self, chat_jid: &str, message_id: &str, emoji: &str) -> Result<()>;

    /// Edit own message
    pub async fn edit_message(&self, chat_jid: &str, message_id: &str, new_text: &str) -> Result<()>;

    /// Delete message (revoke)
    pub async fn delete_message(&self, chat_jid: &str, message_id: &str) -> Result<()>;

    /// Send read receipt
    pub async fn send_read_receipt(&self, chat_jid: &str, message_ids: &[String]) -> Result<()>;

    /// Send typing indicator
    pub async fn send_typing(&self, chat_jid: &str, typing: bool) -> Result<()>;

    /// Download media to cache dir, returns local path
    pub async fn download_media(&self, message: &Message) -> Result<PathBuf>;

    /// Reconnect with exponential backoff (called internally on disconnect)
    async fn reconnect(&mut self);
}
```

**Reconnection logic:**
```
attempt 1: wait 1s
attempt 2: wait 2s
attempt 3: wait 4s
attempt 4: wait 8s
...
attempt N: wait min(2^N seconds, 60s)
max retries: configurable (default 10)
```

On each reconnect attempt, send `ConnectionStatus::Reconnecting { attempt, max }` to update the status bar. On success, re-sync any missed messages. On max retries exceeded, show fatal error overlay.

### `keys/action.rs` — Action Enum

```rust
/// Every discrete action the user can trigger.
/// Keybindings map to these. Commands (`:quit`) also resolve to these.
pub enum Action {
    // Navigation
    NextItem,
    PrevItem,
    FirstItem,
    LastItem,
    HalfPageDown,
    HalfPageUp,
    OpenChat,
    CloseChat,

    // Focus
    CycleFocus,
    FocusLeft,
    FocusRight,
    ToggleInfoPanel,

    // Mode transitions
    EnterInsert,
    ExitInsert,
    EnterCommand,
    EnterSearch,
    ExitOverlay,

    // Messaging
    SendMessage,
    ReplyToSelected,
    EditSelected,
    DeleteSelected,
    ReactToSelected,
    YankSelected,
    OpenMedia,

    // Chat management
    MuteChat,
    ArchiveChat,
    PinChat,

    // Search / Command
    SubmitCommand(String),
    SubmitSearch(String),
    SearchNext,
    SearchPrev,

    // App
    Quit,
    Confirm,
    Cancel,
}
```

### `keys/mod.rs` — Flat Dispatch (v0.1)

```rust
/// Maps a key event to an action based on current mode and focus.
/// v0.1: flat match statements. v0.3: configurable HashMap. v1.0: modalkit.
pub fn dispatch(
    key: &KeyEvent,
    mode: AppMode,
    focus: AppFocus,
    pending: &mut KeyBuffer,
) -> Option<Action> {
    // Handle two-key sequences (gg, Ctrl-w h, etc.)
    if let Some(action) = pending.try_complete(key, mode, focus) {
        return Some(action);
    }

    match mode {
        AppMode::Normal => dispatch_normal(key, focus, pending),
        AppMode::Insert => dispatch_insert(key),
        AppMode::Command => dispatch_command(key),
        AppMode::Search => dispatch_search(key),
    }
}

/// Minimal buffer for two-key sequences like gg, G, Ctrl-w h/l/i
pub struct KeyBuffer {
    pending: Option<KeyEvent>,
    timeout: Instant,
}

impl KeyBuffer {
    pub fn try_complete(
        &mut self,
        key: &KeyEvent,
        mode: AppMode,
        focus: AppFocus,
    ) -> Option<Action> {
        if let Some(first) = self.pending.take() {
            if self.timeout.elapsed() > Duration::from_millis(500) {
                // Timed out — treat new key as fresh
                return None;
            }
            // Match two-key sequences
            match (first.code, key.code) {
                (KeyCode::Char('g'), KeyCode::Char('g')) => return Some(Action::FirstItem),
                (KeyCode::Char('g'), _) => return None, // invalid sequence
                _ => return None,
            }
        }

        // Check if this key starts a multi-key sequence
        match key.code {
            KeyCode::Char('g') if mode == AppMode::Normal => {
                self.pending = Some(*key);
                self.timeout = Instant::now();
                None // consumed, waiting for next key
            }
            _ => None, // not a sequence starter
        }
    }
}
```

---

## Data Model

### Two Separate SQLite Databases

1. **Session database** — managed by whatsapp-rust's `sqlite-storage` crate. Stores identity keys, pre-keys, session state, app state sync. **We never read or write this directly.**

2. **App database** — managed by our `Store`. Stores messages, chats, contacts, reactions, group members. Located at `~/.local/share/whatsapp-tui/app.db`.

This separation means whatsapp-rust upgrades can't corrupt our message history, and we can't accidentally break session crypto state.

### Schema

```sql
-- schema.sql (embedded in binary via include_str!)

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
    archived        INTEGER NOT NULL DEFAULT 0
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
    status          TEXT NOT NULL DEFAULT 'sent'
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
```

### Domain Types

```rust
// Shared across modules. Defined in a types.rs or kept module-local.

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
}

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
    pub reactions: Vec<Reaction>,  // loaded eagerly with message
}

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

pub enum MessageStatus {
    Pending,   // not yet sent
    Sent,      // single check
    Delivered, // double check
    Read,      // blue check
    Failed,    // send failed
}

pub struct Contact {
    pub jid: String,
    pub name: Option<String>,
    pub push_name: Option<String>,
    pub phone: Option<String>,
    pub profile_pic_url: Option<String>,
}

pub struct Reaction {
    pub message_id: String,
    pub sender_jid: String,
    pub emoji: String,
    pub timestamp: i64,
}

pub struct GroupMember {
    pub jid: String,
    pub is_admin: bool,
    pub is_super_admin: bool,
}
```

---

## Store Layer

No trait — just a struct wrapping `rusqlite::Connection`. Only one backend will ever exist.

```rust
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
        Ok(())
    }

    // --- Messages ---
    pub fn insert_message(&self, msg: &Message) -> Result<()>;
    pub fn get_messages(&self, chat_jid: &str, before_ts: Option<i64>, limit: u32) -> Result<Vec<Message>>;
    pub fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<(Message, String)>>;  // (msg, highlighted_snippet)
    pub fn update_status(&self, id: &str, status: MessageStatus) -> Result<()>;
    pub fn mark_edited(&self, id: &str, new_content: &str) -> Result<()>;
    pub fn mark_deleted(&self, id: &str) -> Result<()>;

    // --- Chats ---
    pub fn upsert_chat(&self, chat: &Chat) -> Result<()>;
    pub fn get_chats_ordered(&self) -> Result<Vec<Chat>>;  // pinned first, then by last_message_ts
    pub fn set_unread_count(&self, jid: &str, count: i32) -> Result<()>;
    pub fn set_muted(&self, jid: &str, muted: bool) -> Result<()>;
    pub fn set_pinned(&self, jid: &str, pinned: bool) -> Result<()>;
    pub fn set_archived(&self, jid: &str, archived: bool) -> Result<()>;

    // --- Contacts ---
    pub fn upsert_contact(&self, contact: &Contact) -> Result<()>;
    pub fn get_contact(&self, jid: &str) -> Result<Option<Contact>>;

    // --- Reactions ---
    pub fn upsert_reaction(&self, r: &Reaction) -> Result<()>;
    pub fn remove_reaction(&self, message_id: &str, sender_jid: &str) -> Result<()>;
    pub fn get_reactions(&self, message_id: &str) -> Result<Vec<Reaction>>;

    // --- Groups ---
    pub fn set_group_members(&self, group_jid: &str, members: &[GroupMember]) -> Result<()>;
    pub fn get_group_members(&self, group_jid: &str) -> Result<Vec<GroupMember>>;
}
```

**Message pagination:** `get_messages` uses cursor-based pagination (`before_ts`) instead of offset-based. When the user scrolls to the top of a chat, load the next 50 messages older than the oldest currently displayed. This scales regardless of chat size.

**Search:** FTS5 `MATCH` query with `snippet()` for highlighted results. Results return the message + a text snippet showing the match in context.

---

## UI Architecture

### Rendering Model

ratatui is immediate-mode: every frame, you describe the entire UI, and ratatui diffs against the previous frame to emit minimal ANSI escapes. **Widgets are stateless render functions** — they take a state slice and a `Frame` region, and draw.

```rust
// ui/mod.rs

pub fn render(frame: &mut Frame, app: &App) {
    match &app.overlay {
        Some(overlay) => {
            render_main(frame, app);
            render_overlay(frame, app, overlay);
        }
        None => render_main(frame, app),
    }
}

fn render_main(frame: &mut Frame, app: &App) {
    // Split into panes based on info panel visibility
    let chunks = if app.show_info_panel {
        Layout::horizontal([
            Constraint::Percentage(25),  // chat list
            Constraint::Percentage(50),  // messages
            Constraint::Percentage(25),  // info panel
        ])
    } else {
        Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(75),
        ])
    }
    .split(frame.area());

    chat_list::render(frame, chunks[0], app);

    // Split right side into header + messages + input + status
    let right_chunks = Layout::vertical([
        Constraint::Length(1),    // header
        Constraint::Min(1),      // messages
        Constraint::Length(3),    // input
        Constraint::Length(1),    // status bar
    ])
    .split(chunks[1]);

    header::render(frame, right_chunks[0], app);
    messages::render(frame, right_chunks[1], app);
    input::render(frame, right_chunks[2], app);
    status_bar::render(frame, right_chunks[3], app);

    if app.show_info_panel && chunks.len() > 2 {
        info_panel::render(frame, chunks[2], app);
    }
}
```

### Focus & Highlighting

The currently focused pane gets a highlighted border. Unfocused panes get a dimmed border. This is the only visual indicator of focus — no cursor jumping between panes.

```rust
fn pane_border_style(focus: AppFocus, this_pane: AppFocus) -> Style {
    if focus == this_pane {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
```

### Message Rendering

Each message is a multi-line block:

```
  Alice  10:30                          ← sender + timestamp (colored by sender)
  Hey, are you coming tonight?          ← content
  [thumbsup x2, heart x1]              ← reactions (if any)

  You  10:32  ✓✓                        ← own message + read receipt
  > Hey, are you coming tonight?        ← quoted reply (dimmed)
  Yeah, I'll be there around 8          ← content
```

Own messages are right-aligned or have a distinct color. Group messages show sender name colored by a hash of their JID (consistent color per person).

### Overlay Rendering

Overlays render on top of the main layout using a centered popup area:

```rust
fn render_overlay(frame: &mut Frame, app: &App, overlay: &Overlay) {
    let area = centered_rect(60, 40, frame.area()); // 60% width, 40% height
    frame.render_widget(Clear, area); // clear background

    match overlay {
        Overlay::Command(input) => overlay::command::render(frame, area, input),
        Overlay::Search(state) => overlay::search::render(frame, area, state),
        Overlay::EmojiPicker(state) => overlay::emoji::render(frame, area, state),
        Overlay::Confirm(prompt) => overlay::confirm::render(frame, area, prompt),
        Overlay::QrCode(data) => overlay::qr::render(frame, area, data),
    }
}
```

---

## Event Flow — Complete Path Examples

### User sends a message

```
1. User presses Enter in Insert mode
2. Terminal EventStream captures KeyEvent(Enter)
3. Sent via event_tx as AppEvent::Terminal(KeyEvent(Enter))
4. Main loop receives, calls app.handle_terminal(ev)
5. keys::dispatch → Action::SendMessage (because mode=Insert, focus=MessageInput)
6. app reads input text from TextArea, clears it
7. app creates a local Message with status=Pending, inserts into store
8. app adds message to active_chat.messages (visible immediately)
9. app spawns tokio task: wa.send_message(jid, text)
10. Task completes → sends AppEvent::Wa(WaEvent::Receipt { status: Sent })
11. Main loop receives, app updates message status in store + in-memory
12. Next render shows ✓ on the message
```

### Incoming message while viewing another chat

```
1. whatsapp-rust receives protobuf message from WebSocket
2. wa/client.rs event handler maps it to WaEvent::MessageReceived(msg)
3. Sent via event_tx as AppEvent::Wa(WaEvent::MessageReceived(msg))
4. Main loop receives, calls app.handle_wa(ev)
5. app.store.insert_message(&msg)
6. app.store.set_unread_count(msg.chat_jid, current + 1)
7. app updates self.chats (bump chat to top, increment unread)
8. If msg.chat_jid != active_chat.jid:
   a. Send desktop notification via notify.rs
   b. Chat list re-renders with updated unread count
9. If msg.chat_jid == active_chat.jid:
   a. Append to active_chat.messages
   b. Auto-scroll to bottom (if already at bottom)
   c. Send read receipt via wa.send_read_receipt()
10. Next render shows the new state
```

### Reconnection after network drop

```
1. whatsapp-rust detects WebSocket close
2. WaClient receives disconnect, sends WaEvent::Disconnected
3. app sets connection_status = Disconnected, status bar updates
4. WaClient begins reconnect loop internally:
   a. Wait exponential backoff delay
   b. Send connection_status = Reconnecting { attempt: N }
   c. Attempt connection
   d. If failed, go to (a) with increased delay
   e. If max retries hit, send fatal error event
5. On successful reconnect:
   a. Send WaEvent::Connected
   b. whatsapp-rust syncs missed messages automatically
   c. Missed messages arrive as WaEvent::MessageReceived / HistoryMessages
   d. app processes them normally
   e. Status bar shows Connected
```

---

## Configuration

```rust
// config.rs

#[derive(Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub connection: ConnectionConfig,
    #[serde(default)]
    pub notifications: NotificationConfig,
}

#[derive(Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub confirm_quit: bool,
    #[serde(default = "default_timestamp_format")]
    pub timestamp_format: String,
}

#[derive(Deserialize)]
pub struct AppearanceConfig {
    #[serde(default = "default_message_max_width")]
    pub message_max_width: u16,  // default 80
}

#[derive(Deserialize)]
pub struct ConnectionConfig {
    #[serde(default = "default_max_retries")]
    pub reconnect_max_retries: u32,  // default 10
    #[serde(default = "default_base_delay")]
    pub reconnect_base_delay_ms: u64,  // default 1000
}

#[derive(Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub desktop: bool,
    #[serde(default = "default_true")]
    pub show_preview: bool,
    #[serde(default)]
    pub muted_chats: bool,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .expect("no config dir")
            .join("whatsapp-tui");

        let config_path = config_dir.join("config.toml");
        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            Ok(toml::from_str(&content)?)
        } else {
            // Create config dir + default config on first run
            fs::create_dir_all(&config_dir)?;
            let default = include_str!("../config/default.toml");
            fs::write(&config_path, default)?;
            Ok(Self::default())
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        dirs::data_dir()
            .expect("no data dir")
            .join("whatsapp-tui")
    }

    pub fn cache_dir(&self) -> PathBuf {
        dirs::cache_dir()
            .expect("no cache dir")
            .join("whatsapp-tui")
    }
}
```

---

## Logging

```rust
// Initialized in main.rs

fn tracing_init(config: &Config) -> Result<()> {
    let log_dir = config.data_dir().join("logs");
    fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "whatsapp-tui.log");
    tracing_subscriber::fmt()
        .with_writer(file_appender)
        .with_env_filter("whatsapp_tui=debug,whatsapp_rust=info")
        .with_ansi(false)
        .init();

    Ok(())
}
```

Logs go to file only — never to stdout/stderr (that's the terminal). Rolling daily, filter at debug level for our code, info for whatsapp-rust. No log rotation cleanup in v0.1 — disk is cheap.

---

## Notifications

```rust
// notify.rs

use notify_rust::Notification;

pub fn send(chat_name: &str, sender_name: &str, preview: Option<&str>, config: &NotificationConfig) {
    if !config.desktop {
        return;
    }

    let mut notif = Notification::new();
    notif
        .summary(&format!("{} — {}", chat_name, sender_name))
        .appname("whatsapp-tui")
        .icon("whatsapp")     // system theme icon if available
        .timeout(5000);

    if config.show_preview {
        if let Some(text) = preview {
            notif.body(text);
        }
    }

    // Fire and forget — notification failure is not critical
    let _ = notif.show();
}
```

---

## Concurrency Model

- **Main thread:** event loop (select! over channels) + rendering. This is the only thread that touches `App` state.
- **Terminal reader task:** `tokio::spawn`, reads `EventStream`, sends to channel. Runs for app lifetime.
- **WhatsApp client tasks:** whatsapp-rust internally spawns its own tokio tasks for WebSocket I/O, encryption, etc. Events flow to us via our registered callback → channel.
- **Background tasks:** media downloads, heavy search queries. Spawned via `tokio::spawn`, send `TaskResult` back through the event channel when complete.

**No mutexes, no shared state.** Everything flows through channels into the single-threaded event loop. This eliminates data races by design.

---

## Directory Layout at Runtime

```
~/.config/whatsapp-tui/
└── config.toml                    # user configuration

~/.local/share/whatsapp-tui/
├── app.db                         # our SQLite (messages, chats, contacts)
├── session.db                     # whatsapp-rust's SQLite (crypto, session)
├── media/                         # downloaded media cache
│   ├── images/
│   ├── videos/
│   ├── audio/
│   ├── documents/
│   └── stickers/
└── logs/
    └── whatsapp-tui.2026-04-03.log

~/.cache/whatsapp-tui/
└── (thumbnails, temp files)
```

---

## Dependencies (Cargo.toml)

```toml
[package]
name = "whatsapp-tui"
version = "0.1.0"
edition = "2021"
rust-version = "1.88"
description = "Terminal WhatsApp client with vim keybindings"
license = "MIT"

[dependencies]
# TUI
ratatui = { version = "0.30", features = ["crossterm"] }
crossterm = { version = "0.29", features = ["event-stream"] }

# Async
tokio = { version = "1", features = ["full"] }
futures = "0.3"

# WhatsApp protocol
whatsapp-rust = "0.5"
# OR: wa-rs = "0.2"  # if stable Rust required

# Widgets
tui-textarea = { version = "0.7", features = ["crossterm"] }
tui-scrollview = "0.6"
tui-widget-list = "0.15"

# Storage
rusqlite = { version = "0.34", features = ["bundled", "fts5"] }

# Config
serde = { version = "1", features = ["derive"] }
toml = "0.8"

# CLI
clap = { version = "4", features = ["derive"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"

# Notifications
notify-rust = "4"

# Paths
dirs = "6"

# Clipboard
arboard = "3"

# QR rendering
qrcode = "0.14"

# Time formatting
chrono = "0.4"

# Error handling
anyhow = "1"
thiserror = "2"
```

---

## Key Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Single crate vs workspace | Single crate | No external consumers, premature to split. Modules provide boundaries. |
| Store: trait vs struct | Struct | Only one backend (SQLite), no testing with mocks needed for personal tool. |
| State: centralized vs distributed | Centralized in `App` | Widgets are pure render functions. No widget-local state except `TextArea`. |
| Concurrency: channels vs shared state | Channels (mpsc) | No mutexes needed. All state mutation happens in main loop. |
| Two SQLite databases | Yes | Session DB managed by whatsapp-rust, app DB by us. Isolation prevents cross-contamination on library upgrades. |
| Event mapping layer | Yes (`WaEvent`) | Decouples from whatsapp-rust's pre-1.0 API. Only `wa/` module changes on upstream breaks. |
| Flat keybinding dispatch | v0.1 only | Ship fast, iterate. `KeyBuffer` handles gg and Ctrl-w prefixes. Upgrade path to HashMap (v0.3) then modalkit (v1.0) is clean. |
| Message pagination | Cursor-based | `before_ts` scales to any chat size. No offset counting. |
| Render on every event | Yes | ratatui diffs are sub-millisecond. No dirty flags, no complexity. |
| Error handling crate | anyhow + thiserror | anyhow for app-level, thiserror for typed errors at module boundaries. |
