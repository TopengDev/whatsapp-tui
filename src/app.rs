use std::collections::{HashMap, HashSet};
use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEventKind};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::config::Config;
use crate::event::{AppEvent, TaskResult};
use crate::keys;
use crate::keys::action::Action;
use crate::keys::KeyBuffer;
use crate::store::chats::Chat;
use crate::store::groups::GroupMember;
use crate::store::messages::Message;
use crate::store::Store;
use crate::ui;
use crate::wa::client::WaClient;
use crate::wa::events::WaEvent;

// --- Public types used by UI and keys ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    Insert,
    Command,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppFocus {
    ChatList,
    Messages,
    Input,
    InfoPanel,
}

#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting { attempt: u32, max: u32 },
    LoggedOut,
}

#[derive(Debug, Clone)]
pub enum Overlay {
    Command,
    Search,
    EmojiPicker { selected_idx: usize },
    Confirm { prompt: String },
    QrCode { data: String },
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub chat_name: String,
    pub snippet: String,
    pub message_id: String,
}

pub struct ActiveChat {
    pub jid: String,
    pub name: String,
    pub is_group: bool,
    pub messages: Vec<Message>,
    pub members: Vec<GroupMember>,
    /// Distance from the bottom (newest messages). 0 = at bottom.
    pub scroll_from_bottom: usize,
    /// Currently selected message index (for cursor navigation).
    pub selected_msg_idx: Option<usize>,
    pub input_buf: String,
    pub reply_to: Option<String>,
    pub typing_jids: HashSet<String>,
    /// JID → display name lookup for message senders.
    pub sender_names: HashMap<String, String>,
    /// Message ID → (protocol state, width, height) for inline image rendering.
    pub media_cache: HashMap<String, (StatefulProtocol, u32, u32)>,
}

// --- App ---

pub struct App {
    // State
    pub mode: AppMode,
    pub focus: AppFocus,
    pub chats: Vec<Chat>,
    pub selected_chat_idx: usize,
    pub active_chat: Option<ActiveChat>,
    pub connection_status: ConnectionStatus,
    pub should_quit: bool,

    // Overlay state
    pub overlay: Option<Overlay>,
    pub command_input: String,
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub show_info_panel: bool,

    // Dependencies
    pub config: Config,
    store: Store,
    wa: WaClient,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    pub picker: Picker,

    // Key state
    key_buffer: KeyBuffer,
    reconnect_attempt: u32,
}

impl App {
    pub fn new(
        config: Config,
        store: Store,
        wa: WaClient,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        picker: Picker,
    ) -> Self {
        Self {
            mode: AppMode::Normal,
            focus: AppFocus::ChatList,
            chats: Vec::new(),
            selected_chat_idx: 0,
            active_chat: None,
            connection_status: ConnectionStatus::Disconnected,
            should_quit: false,
            overlay: None,
            command_input: String::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            show_info_panel: false,
            config,
            store,
            wa,
            event_tx,
            picker,
            key_buffer: KeyBuffer::new(),
            reconnect_attempt: 0,
        }
    }

    pub async fn run(
        &mut self,
        mut terminal: Terminal<CrosstermBackend<io::Stdout>>,
        mut wa_rx: mpsc::UnboundedReceiver<AppEvent>,
    ) -> Result<()> {
        // Separate high-priority channel for terminal events so UI stays
        // responsive even during heavy history sync.
        let (term_tx, mut term_rx) = mpsc::unbounded_channel::<CrosstermEvent>();

        // Spawn terminal event reader → dedicated channel
        tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            while let Some(Ok(event)) = reader.next().await {
                if term_tx.send(event).is_err() {
                    break;
                }
            }
        });

        // Load existing chats from store
        self.load_chats()?;

        // Drain any stale terminal events that arrived during setup
        // (e.g., leftover bytes from tmux send-keys launch).
        while term_rx.try_recv().is_ok() {}

        // Connect before entering the main loop — ensures we're connected
        // (or at least attempting) before showing the chat UI.
        self.connection_status = ConnectionStatus::Reconnecting { attempt: 0, max: 0 };
        terminal.draw(|frame| ui::render(frame, self))?;
        if let Err(e) = self.wa.connect().await {
            tracing::error!("failed to connect: {}", e);
            self.connection_status = ConnectionStatus::Disconnected;
        }

        let mut tick = interval(Duration::from_millis(250));

        loop {
            terminal.draw(|frame| ui::render(frame, self))?;

            // Biased select: terminal events always win over WA events.
            // This keeps the UI responsive during history sync floods.
            tokio::select! {
                biased;

                Some(term_ev) = term_rx.recv() => {
                    self.handle_terminal(term_ev).await;
                }

                Some(wa_ev) = wa_rx.recv() => {
                    self.handle_event(wa_ev).await;

                    // Drain a batch of queued WA events without re-rendering
                    // between each one — massively speeds up sync processing.
                    for _ in 0..200 {
                        // But always check for terminal events first
                        if let Ok(term_ev) = term_rx.try_recv() {
                            self.handle_terminal(term_ev).await;
                            break;
                        }
                        match wa_rx.try_recv() {
                            Ok(ev) => self.handle_event(ev).await,
                            Err(_) => break,
                        }
                        if self.should_quit {
                            break;
                        }
                    }
                }

                _ = tick.tick() => {
                    self.handle_tick();
                }
            }

            if self.should_quit {
                self.wa.disconnect().await?;
                break;
            }
        }
        Ok(())
    }

    fn load_chats(&mut self) -> Result<()> {
        // Merge any LID duplicate chats before loading
        let _ = self.store.merge_lid_duplicates();
        self.chats = self.store.get_chats_ordered()?;
        Ok(())
    }

    async fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Terminal(_) => {}
            AppEvent::Wa(ev) => self.handle_wa(ev).await,
            AppEvent::MediaData { message_id, data } => {
                if let Some((proto, w, h)) = crate::ui::image::create_protocol(&self.picker, &data) {
                    if let Some(ref mut active) = self.active_chat {
                        active.media_cache.insert(message_id, (proto, w, h));
                    }
                }
            }
            AppEvent::Task(result) => self.handle_task_result(result),
            AppEvent::Reconnect => {
                if matches!(self.connection_status, ConnectionStatus::LoggedOut) {
                    return;
                }
                if let Err(e) = self.wa.connect().await {
                    tracing::error!("reconnect failed: {}", e);
                    self.connection_status = ConnectionStatus::Disconnected;
                }
            }
        }
    }

    async fn handle_terminal(&mut self, event: CrosstermEvent) {
        // Accept Press and Repeat, reject Release.
        // In terminals without kitty keyboard protocol (tmux, legacy),
        // all events arrive as Press. In kitty-enabled terminals,
        // we get Press + Release — only Release should be ignored.
        let key = match event {
            CrosstermEvent::Key(key) if key.kind != KeyEventKind::Release => key,
            CrosstermEvent::Resize(_, _) => return,
            _ => return,
        };

        // In insert mode, forward most keys to input buffer
        if self.mode == AppMode::Insert {
            if let Some(action) = keys::dispatch(&key, self.mode, self.focus, &mut self.key_buffer)
            {
                self.execute_action(action).await;
            } else if let Some(ref mut active) = self.active_chat {
                match key.code {
                    KeyCode::Char(c) => active.input_buf.push(c),
                    KeyCode::Backspace => {
                        active.input_buf.pop();
                    }
                    _ => {}
                }
            }
            return;
        }

        // In command mode, handle text input for overlay
        if self.mode == AppMode::Command {
            match key.code {
                KeyCode::Esc => {
                    self.mode = AppMode::Normal;
                    self.overlay = None;
                    self.command_input.clear();
                }
                KeyCode::Enter => {
                    let cmd = self.command_input.clone();
                    self.mode = AppMode::Normal;
                    self.overlay = None;
                    self.command_input.clear();
                    self.execute_command(&cmd).await;
                }
                KeyCode::Backspace => {
                    self.command_input.pop();
                }
                KeyCode::Char(c) => {
                    self.command_input.push(c);
                }
                _ => {}
            }
            return;
        }

        if self.mode == AppMode::Search {
            match key.code {
                KeyCode::Esc => {
                    self.mode = AppMode::Normal;
                    self.overlay = None;
                    self.search_query.clear();
                    self.search_results.clear();
                }
                KeyCode::Enter => {
                    self.perform_search();
                    self.mode = AppMode::Normal;
                    self.overlay = None;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                }
                _ => {}
            }
            return;
        }

        // Normal mode dispatch
        if let Some(action) = keys::dispatch(&key, self.mode, self.focus, &mut self.key_buffer) {
            self.execute_action(action).await;
        }
    }

    async fn execute_action(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.should_quit = true;
            }
            Action::NextItem => self.navigate_next(),
            Action::PrevItem => self.navigate_prev(),
            Action::FirstItem => self.navigate_first(),
            Action::LastItem => self.navigate_last(),
            Action::HalfPageDown => self.scroll_half_page_down(),
            Action::HalfPageUp => self.scroll_half_page_up(),
            Action::OpenChat => self.open_selected_chat().await,
            Action::CloseChat => self.close_chat(),
            Action::CycleFocus => self.cycle_focus(),
            Action::FocusLeft => self.focus = AppFocus::ChatList,
            Action::FocusRight => {
                if self.show_info_panel {
                    self.focus = AppFocus::InfoPanel;
                }
            }
            Action::ToggleInfoPanel => {
                self.show_info_panel = !self.show_info_panel;
            }
            Action::EnterInsert => {
                if self.active_chat.is_some() {
                    self.mode = AppMode::Insert;
                    self.focus = AppFocus::Input;
                }
            }
            Action::ExitInsert => {
                self.mode = AppMode::Normal;
                self.focus = AppFocus::Messages;
            }
            Action::EnterCommand => {
                self.mode = AppMode::Command;
                self.overlay = Some(Overlay::Command);
                self.command_input.clear();
            }
            Action::EnterSearch => {
                self.mode = AppMode::Search;
                self.overlay = Some(Overlay::Search);
                self.search_query.clear();
                self.search_results.clear();
            }
            Action::ExitOverlay => {
                self.mode = AppMode::Normal;
                self.overlay = None;
            }
            Action::SendMessage => {
                self.send_message().await;
            }
            Action::OpenMedia => {
                self.open_selected_media().await;
            }
            Action::DownloadMedia => {
                self.download_selected_media().await;
            }
            _ => {}
        }
    }

    async fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
        match parts.first().copied() {
            Some("q") | Some("quit") => self.should_quit = true,
            Some("info") => self.show_info_panel = !self.show_info_panel,
            Some("connect") | Some("login") => {
                tracing::info!("manual connect requested");
                self.connection_status = ConnectionStatus::Disconnected;
                if let Err(e) = self.wa.connect().await {
                    tracing::error!("connect failed: {}", e);
                }
            }
            _ => {
                tracing::warn!("unknown command: {}", cmd);
            }
        }
    }

    async fn handle_wa(&mut self, event: WaEvent) {
        match event {
            WaEvent::Connected => {
                let was_reconnect = self.reconnect_attempt > 0;
                self.connection_status = ConnectionStatus::Connected;
                self.reconnect_attempt = 0;
                // Dismiss QR overlay if still showing
                if matches!(self.overlay, Some(Overlay::QrCode { .. })) {
                    self.overlay = None;
                }
                if was_reconnect {
                    tracing::info!("reconnected — refreshing state");
                    let _ = self.load_chats();
                    if let Some(ref mut active) = self.active_chat {
                        if let Ok(msgs) = self.store.get_messages(&active.jid, None, 200) {
                            active.messages = msgs;
                        }
                    }
                } else {
                    tracing::info!("connected to WhatsApp");
                }
                // Resolve unnamed group subjects in the background
                self.resolve_unnamed_groups().await;
            }
            WaEvent::Disconnected { reason } => {
                self.connection_status = ConnectionStatus::Disconnected;
                tracing::warn!("disconnected: {}", reason);
            }
            WaEvent::BotStopped => {
                if matches!(self.connection_status, ConnectionStatus::LoggedOut) {
                    tracing::info!("bot stopped after logout, not reconnecting");
                    return;
                }
                self.reconnect_attempt += 1;
                let max = self.config.connection.reconnect_max_retries;
                if self.reconnect_attempt > max {
                    tracing::error!("max reconnect attempts ({}) exceeded", max);
                    self.connection_status = ConnectionStatus::Disconnected;
                    return;
                }
                let base = self.config.connection.reconnect_base_delay_ms;
                let delay = base * 2u64.pow(self.reconnect_attempt.min(5) - 1);
                tracing::info!(
                    "bot stopped, reconnecting in {}ms (attempt {}/{})",
                    delay,
                    self.reconnect_attempt,
                    max
                );
                self.connection_status = ConnectionStatus::Reconnecting {
                    attempt: self.reconnect_attempt,
                    max,
                };
                // Non-blocking: spawn delayed reconnect event
                let tx = self.event_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    let _ = tx.send(AppEvent::Reconnect);
                });
            }
            WaEvent::LoggedOut => {
                tracing::info!("logged out — session invalidated, need new QR scan");
                self.connection_status = ConnectionStatus::LoggedOut;
                // Delete stale session so next connect triggers QR auth
                let session_path = self.config.data_dir().join("session.db");
                if session_path.exists() {
                    let _ = std::fs::remove_file(&session_path);
                    tracing::info!("deleted stale session.db");
                }
            }
            WaEvent::QrCode(data) => {
                tracing::info!("QR code received, showing overlay");
                self.overlay = Some(Overlay::QrCode { data });
            }
            WaEvent::PairingCode(code) => {
                let formatted = crate::wa::auth::format_pairing_code(&code);
                tracing::info!("pairing code: {}", formatted);
            }
            WaEvent::AuthSuccess => {
                self.overlay = None;
                tracing::info!("auth successful");
            }
            WaEvent::HistorySync { chats, push_names } => {
                // Apply push names → update contacts and chat names
                for (jid, name) in &push_names {
                    // Normalize JID to full format for consistent lookup
                    let full_jid = if jid.contains('@') {
                        jid.clone()
                    } else {
                        format!("{}@s.whatsapp.net", jid)
                    };
                    let _ = self.store.upsert_contact(&crate::store::contacts::Contact {
                        jid: full_jid.clone(),
                        name: None,
                        push_name: Some(name.clone()),
                        phone: None,
                        profile_pic_url: None,
                        lid_jid: None,
                    });
                    let _ = self.store.set_chat_name(&full_jid, name);
                }

                for synced in chats {
                    // Harvest push names from messages → contacts table
                    for msg in &synced.messages {
                        if let Some(ref pn) = msg.sender_push_name {
                            if !pn.is_empty() && msg.sender_jid != "me" {
                                let sender = if msg.sender_jid.contains('@') {
                                    msg.sender_jid.clone()
                                } else {
                                    format!("{}@s.whatsapp.net", msg.sender_jid)
                                };
                                let _ = self.store.upsert_contact(
                                    &crate::store::contacts::Contact {
                                        jid: sender,
                                        name: None,
                                        push_name: Some(pn.clone()),
                                        phone: None,
                                        profile_pic_url: None,
                                        lid_jid: None,
                                    },
                                );
                            }
                        }
                    }

                    // Bulk insert chat + messages in a single transaction (fast)
                    if let Err(e) = self.store.bulk_sync_chat(&synced.chat, &synced.messages) {
                        tracing::error!("failed to sync chat {}: {}", synced.chat.jid, e);
                        continue;
                    }

                    // Store group members
                    if !synced.members.is_empty() {
                        let _ = self
                            .store
                            .set_group_members(&synced.chat.jid, &synced.members);
                    }

                    // If this chat is currently open, refresh its messages
                    if let Some(ref mut active) = self.active_chat {
                        if active.jid == synced.chat.jid {
                            if let Ok(msgs) = self.store.get_messages(&active.jid, None, 200) {
                                active.messages = msgs;
                            }
                            if !synced.members.is_empty() {
                                active.members = synced.members;
                            }
                        }
                    }
                }

                // Resolve any chat names that now have matching contacts
                let _ = self.store.resolve_chat_names();
                // Refresh chat list
                let _ = self.load_chats();
            }
            WaEvent::MessageReceived(mut msg) => {
                // Resolve LID chat JID → phone JID
                if msg.chat_jid.contains("@lid") && !msg.chat_jid.contains("@g.us") {
                    let resolved = self.store.resolve_lid_to_phone(&msg.chat_jid);
                    if resolved != msg.chat_jid {
                        msg.chat_jid = resolved;
                    } else if !msg.from_me {
                        // DM from counterparty via LID — no mapping exists yet.
                        // Try to find the phone-JID chat by matching push name
                        // to an existing contact, then save the mapping.
                        if let Some(ref pn) = msg.sender_push_name {
                            if let Ok(phone) = self.store.find_phone_jid_by_push_name(pn) {
                                let _ = self.store.set_chat_lid(&phone, &msg.chat_jid);
                                let _ = self.store.migrate_lid_messages(&msg.chat_jid, &phone);
                                msg.chat_jid = phone;
                            }
                        }
                    }
                }

                // Update contact push name if present
                if let Some(ref push_name) = msg.sender_push_name {
                    if !msg.from_me {
                        let _ = self.store.upsert_contact(&crate::store::contacts::Contact {
                            jid: msg.sender_jid.clone(),
                            name: None,
                            push_name: Some(push_name.clone()),
                            phone: None,
                            profile_pic_url: None,
                            lid_jid: None,
                        });
                        // For DM chats, update the chat name too
                        if !msg.chat_jid.contains("@g.us") {
                            let _ = self.store.set_chat_name(&msg.chat_jid, push_name);
                        }
                    }
                }

                // Store the message
                if let Err(e) = self.store.insert_message(&msg) {
                    tracing::error!("failed to store message: {}", e);
                }

                // Update only the last-message metadata on the chat — don't clobber
                // name, muted, pinned, archived, or unread_count.
                let preview = msg.content.as_deref().or(Some(match msg.message_type {
                    crate::store::messages::MessageType::Image => "[Image]",
                    crate::store::messages::MessageType::Video => "[Video]",
                    crate::store::messages::MessageType::Audio => "[Audio]",
                    crate::store::messages::MessageType::Document => "[Document]",
                    crate::store::messages::MessageType::Sticker => "[Sticker]",
                    crate::store::messages::MessageType::Location => "[Location]",
                    crate::store::messages::MessageType::Poll => "[Poll]",
                    crate::store::messages::MessageType::Contact => "[Contact]",
                    _ => "[Message]",
                }));
                let _ = self
                    .store
                    .touch_last_message(&msg.chat_jid, msg.timestamp, preview);

                // If the chat doesn't exist yet (new conversation), create it
                if !self.chats.iter().any(|c| c.jid == msg.chat_jid) {
                    let sender_name = msg.sender_jid.split('@').next().unwrap_or("?").to_string();
                    let _ = self.store.upsert_chat(&Chat {
                        jid: msg.chat_jid.clone(),
                        name: sender_name,
                        is_group: msg.chat_jid.contains("@g.us"),
                        last_message_ts: Some(msg.timestamp),
                        last_message_preview: preview.map(|s| s.to_string()),
                        unread_count: 0,
                        muted: false,
                        pinned: false,
                        archived: false,
                        lid_jid: None,
                    });
                }

                // Refresh chat list
                let _ = self.load_chats();

                // If viewing this chat, append and auto-scroll to bottom
                if let Some(ref mut active) = self.active_chat {
                    if active.jid == msg.chat_jid {
                        active.messages.push(msg.clone());
                        // Auto-scroll to show new message (large value, clamped in render)
                        active.scroll_from_bottom = 0;

                        // Send read receipt
                        let jid = active.jid.clone();
                        let ids = vec![msg.id.clone()];
                        let _ = self.wa.send_read_receipt(&jid, &ids).await;
                        return;
                    }
                }

                // Not viewing this chat — increment unread, notify
                if !msg.from_me {
                    if let Ok(chats) = self.store.get_chats_ordered() {
                        if let Some(chat) = chats.iter().find(|c| c.jid == msg.chat_jid) {
                            let _ = self
                                .store
                                .set_unread_count(&msg.chat_jid, chat.unread_count + 1);
                            let _ = self.load_chats();
                        }
                    }

                    let sender = msg.sender_jid.split('@').next().unwrap_or("?");
                    crate::notify::send(
                        &msg.chat_jid,
                        sender,
                        msg.content.as_deref(),
                        &self.config.notifications,
                    );
                }
            }
            WaEvent::Receipt { message_id, status } => {
                let _ = self
                    .store
                    .update_message_status(&message_id, status.as_str());
                if let Some(ref mut active) = self.active_chat {
                    if let Some(m) = active.messages.iter_mut().find(|m| m.id == message_id) {
                        m.status = status;
                    }
                }
            }
            WaEvent::ChatUpdate(chat) => {
                let _ = self.store.upsert_chat(&chat);
                let _ = self.load_chats();
            }
            WaEvent::ContactUpdate(contact) => {
                let _ = self.store.upsert_contact(&contact);
                let name = contact
                    .push_name
                    .as_deref()
                    .or(contact.name.as_deref())
                    .unwrap_or("");
                if !name.is_empty() {
                    let _ = self.store.set_chat_name(&contact.jid, name);
                    // If this contact has a LID, also update chats keyed by that LID
                    if let Some(ref lid) = contact.lid_jid {
                        let _ = self.store.set_chat_name_by_lid(lid, name);
                    }
                }

                // If this contact has a LID→phone mapping, migrate orphaned
                // messages that were stored under the LID JID during earlier
                // history syncs (before the phone mapping was available).
                if let Some(ref lid) = contact.lid_jid {
                    let _ = self.store.migrate_lid_messages(lid, &contact.jid);
                    // Also set the lid_jid on the phone chat for future lookups
                    let _ = self.store.set_chat_lid(&contact.jid, lid);
                }

                // Bulk-resolve in case chats arrived after earlier ContactUpdates
                let _ = self.store.resolve_chat_names();
                let _ = self.load_chats();
            }
            WaEvent::TypingStarted { chat_jid, user_jid } => {
                if let Some(ref mut active) = self.active_chat {
                    if active.jid == chat_jid {
                        active.typing_jids.insert(user_jid);
                    }
                }
            }
            WaEvent::TypingStopped { chat_jid, user_jid } => {
                if let Some(ref mut active) = self.active_chat {
                    if active.jid == chat_jid {
                        active.typing_jids.remove(&user_jid);
                    }
                }
            }
            WaEvent::GroupMembersUpdate {
                group_jid,
                members,
            } => {
                if members.is_empty() {
                    // Empty members = signal to re-fetch from server
                    if let Some(ref mut active) = self.active_chat {
                        if active.jid == group_jid {
                            match self.wa.get_group_info(&group_jid).await {
                                Ok((_subject, fetched)) => {
                                    let _ =
                                        self.store.set_group_members(&group_jid, &fetched);
                                    active.members = fetched;
                                }
                                Err(e) => {
                                    tracing::debug!(
                                        "re-fetch group members failed: {}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                } else {
                    let _ = self.store.set_group_members(&group_jid, &members);
                    if let Some(ref mut active) = self.active_chat {
                        if active.jid == group_jid {
                            active.members = members;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_task_result(&mut self, result: TaskResult) {
        match result {
            TaskResult::MediaDownloaded {
                message_id,
                local_path,
            } => {
                tracing::info!("media downloaded: {} -> {:?}", message_id, local_path);
            }
            TaskResult::MediaOpenFailed { message_id, error } => {
                tracing::error!("media open failed: {} -- {}", message_id, error);
            }
            TaskResult::HistorySynced {
                chat_jid,
                message_count,
            } => {
                tracing::info!("synced {} messages for {}", message_count, chat_jid);
            }
        }
    }

    fn handle_tick(&mut self) {
        // Periodic tasks: clear stale typing indicators, etc.
    }

    /// Fetch subjects from the server for groups whose name looks like a JID number.
    async fn resolve_unnamed_groups(&mut self) {
        let unnamed: Vec<String> = self
            .chats
            .iter()
            .filter(|c| {
                c.is_group
                    && c.name
                        .chars()
                        .all(|ch| ch.is_ascii_digit() || ch == '+' || ch == '-')
            })
            .map(|c| c.jid.clone())
            .collect();

        if unnamed.is_empty() {
            return;
        }
        tracing::info!("resolving {} unnamed groups", unnamed.len());
        let mut resolved = 0usize;
        for jid in &unnamed {
            match self.wa.get_group_info(jid).await {
                Ok((Some(subject), members)) => {
                    if !subject.is_empty() {
                        let _ = self.store.set_chat_name(jid, &subject);
                        resolved += 1;
                    }
                    if !members.is_empty() {
                        let _ = self.store.set_group_members(jid, &members);
                    }
                }
                Ok((None, _)) => {}
                Err(e) => {
                    tracing::debug!("group info fetch failed for {}: {}", jid, e);
                }
            }
        }
        if resolved > 0 {
            tracing::info!("resolved {} group names", resolved);
            let _ = self.load_chats();
        }
    }

    // --- Navigation ---

    fn navigate_next(&mut self) {
        match self.focus {
            AppFocus::ChatList => {
                if !self.chats.is_empty() {
                    self.selected_chat_idx = (self.selected_chat_idx + 1).min(self.chats.len() - 1);
                }
            }
            AppFocus::Messages => {
                // j/Down = move cursor to next (newer) message
                if let Some(ref mut active) = self.active_chat {
                    if active.messages.is_empty() {
                        return;
                    }
                    let last = active.messages.len() - 1;
                    let idx = match active.selected_msg_idx {
                        Some(i) => (i + 1).min(last),
                        None => last,
                    };
                    active.selected_msg_idx = Some(idx);
                    // Each message ≈ 3 rendered lines. Convert to line-based offset.
                    let msgs_from_bottom = last.saturating_sub(idx);
                    active.scroll_from_bottom = msgs_from_bottom.saturating_mul(3);
                }
            }
            _ => {}
        }
    }

    fn navigate_prev(&mut self) {
        match self.focus {
            AppFocus::ChatList => {
                self.selected_chat_idx = self.selected_chat_idx.saturating_sub(1);
            }
            AppFocus::Messages => {
                // k/Up = move cursor to previous (older) message
                if let Some(ref mut active) = self.active_chat {
                    if active.messages.is_empty() {
                        return;
                    }
                    let last = active.messages.len() - 1;
                    let idx = match active.selected_msg_idx {
                        Some(i) => i.saturating_sub(1),
                        None => last,
                    };
                    active.selected_msg_idx = Some(idx);
                    let msgs_from_bottom = last.saturating_sub(idx);
                    active.scroll_from_bottom = msgs_from_bottom.saturating_mul(3);

                    // Pagination: load older messages when cursor reaches the top
                    if idx == 0 && !active.messages.is_empty() {
                        let oldest_ts = active.messages[0].timestamp;
                        let jid = active.jid.clone();
                        if let Ok(older) =
                            self.store.get_messages(&jid, Some(oldest_ts), 100)
                        {
                            if !older.is_empty() {
                                let new_count = older.len();
                                let mut combined = older;
                                if let Some(ref mut active) = self.active_chat {
                                    combined.append(&mut active.messages);
                                    active.messages = combined;
                                    // Adjust cursor to stay on the same message
                                    active.selected_msg_idx = Some(new_count);
                                    active.scroll_from_bottom = active
                                        .messages
                                        .len()
                                        .saturating_sub(1)
                                        .saturating_sub(new_count)
                                        .saturating_mul(3);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn navigate_first(&mut self) {
        match self.focus {
            AppFocus::ChatList => self.selected_chat_idx = 0,
            AppFocus::Messages => {
                if let Some(ref mut active) = self.active_chat {
                    active.selected_msg_idx = Some(0);
                    let last = active.messages.len().saturating_sub(1);
                    active.scroll_from_bottom = last.saturating_mul(3);
                }
            }
            _ => {}
        }
    }

    fn navigate_last(&mut self) {
        match self.focus {
            AppFocus::ChatList => {
                if !self.chats.is_empty() {
                    self.selected_chat_idx = self.chats.len() - 1;
                }
            }
            AppFocus::Messages => {
                if let Some(ref mut active) = self.active_chat {
                    if !active.messages.is_empty() {
                        active.selected_msg_idx = Some(active.messages.len() - 1);
                    }
                    active.scroll_from_bottom = 0;
                }
            }
            _ => {}
        }
    }

    fn scroll_half_page_down(&mut self) {
        if let Some(ref mut active) = self.active_chat {
            let jump = 5usize; // 5 messages
            if let Some(ref mut idx) = active.selected_msg_idx {
                let last = active.messages.len().saturating_sub(1);
                *idx = (*idx + jump).min(last);
                active.scroll_from_bottom = last.saturating_sub(*idx).saturating_mul(3);
            } else {
                active.scroll_from_bottom = active.scroll_from_bottom.saturating_sub(jump * 3);
            }
        }
    }

    fn scroll_half_page_up(&mut self) {
        if let Some(ref mut active) = self.active_chat {
            let jump = 5usize;
            if let Some(ref mut idx) = active.selected_msg_idx {
                *idx = idx.saturating_sub(jump);
                let last = active.messages.len().saturating_sub(1);
                active.scroll_from_bottom = last.saturating_sub(*idx).saturating_mul(3);
            } else {
                active.scroll_from_bottom = active.scroll_from_bottom.saturating_add(jump * 3);
            }
        }
    }

    async fn open_selected_chat(&mut self) {
        if self.chats.is_empty() {
            return;
        }
        let chat = &self.chats[self.selected_chat_idx];

        let messages = self
            .store
            .get_messages(&chat.jid, None, 200)
            .unwrap_or_default();

        let mut members = if chat.is_group {
            self.store.get_group_members(&chat.jid).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Fetch group members and subject from server if needed
        let needs_member_fetch = members.is_empty()
            || members.iter().any(|m| m.jid.contains("@lid"));
        let name_is_numeric = chat.name.chars().all(|c| c.is_ascii_digit() || c == '+' || c == '-');
        let mut fetched_name: Option<String> = None;
        if chat.is_group && (needs_member_fetch || name_is_numeric) {
            match self.wa.get_group_info(&chat.jid).await {
                Ok((subject, fetched_members)) => {
                    if let Some(ref name) = subject {
                        if !name.is_empty() {
                            let _ = self.store.set_chat_name(&chat.jid, name);
                            fetched_name = Some(name.clone());
                        }
                    }
                    if !fetched_members.is_empty() {
                        let _ = self.store.set_group_members(&chat.jid, &fetched_members);
                        members = fetched_members;
                    }
                }
                Err(e) => {
                    tracing::debug!("failed to fetch group info for {}: {}", chat.jid, e);
                }
            }
        }

        // Auto-download stickers and images in background.
        let to_download: Vec<crate::store::messages::Message> = messages
            .iter()
            .filter(|m| {
                matches!(
                    m.message_type,
                    crate::store::messages::MessageType::Sticker
                        | crate::store::messages::MessageType::Image
                ) && m.media_direct_path.is_some()
            })
            .cloned()
            .collect();

        let last_idx = if messages.is_empty() {
            None
        } else {
            Some(messages.len() - 1)
        };

        let mut sender_names = self.store.get_display_names().unwrap_or_default();

        // Harvest push names from message history — covers senders not in contacts
        for msg in &messages {
            if let Some(ref pn) = msg.sender_push_name {
                if !pn.is_empty() && !sender_names.contains_key(&msg.sender_jid) {
                    sender_names.insert(msg.sender_jid.clone(), pn.clone());
                }
            }
        }

        // Resolve LID JIDs → phone JIDs for name lookup.
        // Covers both group member JIDs and message sender JIDs.
        let lid_jids: Vec<String> = members
            .iter()
            .map(|m| m.jid.clone())
            .chain(messages.iter().map(|m| m.sender_jid.clone()))
            .filter(|jid| jid.contains("@lid") && !sender_names.contains_key(jid))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for lid in &lid_jids {
            let resolved = self.store.resolve_lid_to_phone(lid);
            if resolved != *lid {
                if let Some(name) = sender_names.get(&resolved).cloned() {
                    sender_names.insert(lid.clone(), name);
                }
            }
        }

        let chat_name = fetched_name.unwrap_or_else(|| chat.name.clone());

        self.active_chat = Some(ActiveChat {
            jid: chat.jid.clone(),
            name: chat_name,
            is_group: chat.is_group,
            messages,
            members,
            scroll_from_bottom: 0,
            selected_msg_idx: last_idx,
            input_buf: String::new(),
            reply_to: None,
            typing_jids: HashSet::new(),
            sender_names,
            media_cache: HashMap::new(),
        });

        self.focus = AppFocus::Messages;

        // Auto-download stickers and images in background
        if !to_download.is_empty() {
            let wa = self.event_tx.clone();
            let client = self.wa.client.clone();
            tokio::spawn(async move {
                // Wait for connection to stabilize before downloading
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                for msg in to_download {
                    if let Some(ref client) = client {
                        let direct_path = match msg.media_direct_path.as_deref() {
                            Some(p) => p,
                            None => continue,
                        };
                        let media_key = match msg.media_key.as_deref() {
                            Some(k) => k,
                            None => continue,
                        };
                        let file_sha256 = match msg.media_file_sha256.as_deref() {
                            Some(s) => s,
                            None => continue,
                        };
                        let file_enc_sha256 = match msg.media_file_enc_sha256.as_deref() {
                            Some(s) => s,
                            None => continue,
                        };
                        let file_length = msg.media_size.unwrap_or(0) as u64;
                        let media_type = if matches!(
                            msg.message_type,
                            crate::store::messages::MessageType::Sticker
                        ) {
                            wacore::download::MediaType::Sticker
                        } else {
                            wacore::download::MediaType::Image
                        };

                        // Retry up to 3 times with backoff
                        for attempt in 0..3u32 {
                            if attempt > 0 {
                                tokio::time::sleep(std::time::Duration::from_secs(
                                    2u64.pow(attempt),
                                ))
                                .await;
                            }
                            match client
                                .download_from_params(
                                    direct_path,
                                    media_key,
                                    file_sha256,
                                    file_enc_sha256,
                                    file_length,
                                    media_type,
                                )
                                .await
                            {
                                Ok(data) => {
                                    let _ = wa.send(AppEvent::MediaData {
                                        message_id: msg.id.clone(),
                                        data,
                                    });
                                    break;
                                }
                                Err(e) => {
                                    if attempt == 2 {
                                        tracing::debug!(
                                            "auto-download failed for {}: {}",
                                            msg.id,
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }
        let _ = self.store.set_unread_count(&chat.jid, 0);
    }

    fn close_chat(&mut self) {
        if self.focus == AppFocus::ChatList {
            return;
        }
        self.focus = AppFocus::ChatList;
    }

    fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            AppFocus::ChatList => AppFocus::Messages,
            AppFocus::Messages => AppFocus::Input,
            AppFocus::Input => {
                if self.show_info_panel {
                    AppFocus::InfoPanel
                } else {
                    AppFocus::ChatList
                }
            }
            AppFocus::InfoPanel => AppFocus::ChatList,
        };
    }

    async fn download_selected_media(&mut self) {
        let (msg_id, msg_clone) = {
            let active = match self.active_chat.as_ref() {
                Some(a) => a,
                None => return,
            };
            let idx = match active.selected_msg_idx {
                Some(i) => i,
                None => return,
            };
            let msg = match active.messages.get(idx) {
                Some(m) => m,
                None => return,
            };
            tracing::debug!(
                "download_selected_media: idx={}, type={}, has_path={}, cached={}",
                idx, msg.message_type.as_str(),
                msg.media_direct_path.is_some(),
                active.media_cache.contains_key(&msg.id),
            );
            if active.media_cache.contains_key(&msg.id) {
                return; // Already cached
            }
            if msg.media_direct_path.is_none() {
                tracing::warn!("no download params for msg {}", msg.id);
                return;
            }
            (msg.id.clone(), msg.clone())
        };

        match self.wa.download_media_bytes(&msg_clone).await {
            Ok(data) => {
                tracing::info!("downloaded media for msg {}: {} bytes", msg_id, data.len());
                if let Some((proto, w, h)) = crate::ui::image::create_protocol(&self.picker, &data) {
                    if let Some(ref mut active) = self.active_chat {
                        active.media_cache.insert(msg_id.clone(), (proto, w, h));
                        tracing::info!("image cached for {} ({}x{})", msg_id, w, h);
                    }
                } else {
                    tracing::error!("create_protocol returned None for {}", msg_id);
                }
            }
            Err(e) => {
                tracing::error!("failed to download media for {}: {}", msg_id, e);
            }
        }
    }

    async fn open_selected_media(&mut self) {
        let active = match self.active_chat.as_ref() {
            Some(a) => a,
            None => return,
        };
        let idx = match active.selected_msg_idx {
            Some(i) => i,
            None => return,
        };
        let msg = match active.messages.get(idx) {
            Some(m) => m,
            None => return,
        };

        // Check if the message has a local media file
        if let Some(ref path) = msg.media_local_path {
            if path.exists() {
                tracing::info!("opening media: {:?}", path);
                let _ = crate::wa::media::open_file(path);
                return;
            }
        }

        // Download, save to cache dir, then open
        let msg_clone = msg.clone();
        match self.wa.download_media_bytes(&msg_clone).await {
            Ok(data) => {
                let ext = msg_clone
                    .media_mime
                    .as_deref()
                    .map(|m| match m {
                        "image/webp" => "webp",
                        "image/jpeg" => "jpg",
                        "image/png" => "png",
                        "video/mp4" => "mp4",
                        "audio/ogg" => "ogg",
                        _ => "bin",
                    })
                    .unwrap_or("bin");
                let cache_dir = self.config.cache_dir().join("media");
                let _ = std::fs::create_dir_all(&cache_dir);
                let path = cache_dir.join(format!("{}.{}", msg_clone.id, ext));
                if std::fs::write(&path, &data).is_ok() {
                    tracing::info!("opening media: {:?}", path);
                    let _ = crate::wa::media::open_file(&path);
                }
            }
            Err(e) => {
                tracing::warn!("cannot download media: {}", e);
            }
        }
    }

    async fn send_message(&mut self) {
        let active = match self.active_chat.as_mut() {
            Some(a) => a,
            None => return,
        };

        let text = active.input_buf.trim().to_string();
        if text.is_empty() {
            return;
        }

        active.input_buf.clear();

        // Create a local pending message for immediate display
        let local_id = format!("pending_{}", chrono::Utc::now().timestamp_millis());
        let jid = active.jid.clone();
        let reply_to = active.reply_to.take();

        let msg = Message {
            id: local_id.clone(),
            chat_jid: jid.clone(),
            sender_jid: "me".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            content: Some(text.clone()),
            message_type: crate::store::messages::MessageType::Text,
            media_mime: None,
            media_size: None,
            media_filename: None,
            media_local_path: None,
            reply_to_id: reply_to.clone(),
            reply_to_preview: None,
            edited: false,
            deleted: false,
            from_me: true,
            status: crate::store::messages::MessageStatus::Pending,
            reactions: Vec::new(),
            sender_push_name: None,
            media_direct_path: None,
            media_key: None,
            media_file_sha256: None,
            media_file_enc_sha256: None,
            media_duration_secs: None,
            is_voice_note: false,
            link_title: None,
            link_description: None,
            link_url: None,
            caption: None,
            is_gif: false,
            media_dimensions: None,
        };

        // Show immediately in the UI and scroll to bottom
        active.messages.push(msg.clone());
        active.scroll_from_bottom = 0;

        // Exit insert mode
        self.mode = AppMode::Normal;
        self.focus = AppFocus::Messages;

        // Send via WhatsApp
        let send_result = if let Some(ref reply_id) = reply_to {
            self.wa.send_reply(&jid, &text, reply_id, None).await
        } else {
            self.wa.send_message(&jid, &text).await
        };

        match send_result {
            Ok(real_id) => {
                // Update the message with the real server-assigned ID and Sent status
                let mut sent_msg = msg;
                sent_msg.id = real_id;
                sent_msg.status = crate::store::messages::MessageStatus::Sent;
                let _ = self.store.insert_message(&sent_msg);

                // Update in-memory: replace the pending message
                if let Some(ref mut active) = self.active_chat {
                    if let Some(m) = active.messages.iter_mut().find(|m| m.id == local_id) {
                        m.id = sent_msg.id;
                        m.status = crate::store::messages::MessageStatus::Sent;
                    }
                }
            }
            Err(e) => {
                tracing::error!("failed to send message: {}", e);
                // Mark as failed
                if let Some(ref mut active) = self.active_chat {
                    if let Some(m) = active.messages.iter_mut().find(|m| m.id == local_id) {
                        m.status = crate::store::messages::MessageStatus::Failed;
                    }
                }
            }
        }
    }

    fn perform_search(&mut self) {
        if self.search_query.is_empty() {
            return;
        }
        match self.store.search_messages(&self.search_query, 20) {
            Ok(results) => {
                self.search_results = results
                    .into_iter()
                    .map(|(msg, snippet)| SearchResult {
                        chat_name: msg.chat_jid.clone(),
                        snippet,
                        message_id: msg.id,
                    })
                    .collect();
            }
            Err(e) => {
                tracing::error!("search failed: {}", e);
            }
        }
    }
}
