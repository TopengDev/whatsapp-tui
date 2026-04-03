use wacore::proto_helpers::MessageExt;
use wacore::types::events::Event;
use waproto::whatsapp as wa;

use crate::store::{
    chats::Chat, contacts::Contact, groups::GroupMember, messages::Message,
    messages::MessageStatus, messages::MessageType, reactions::Reaction,
};

/// Our domain events, mapped from whatsapp-rust's raw events.
/// This decouples app logic from the protocol library's API surface.
#[derive(Debug)]
pub enum WaEvent {
    // Connection
    Connected,
    Disconnected {
        reason: String,
    },
    BotStopped,
    LoggedOut,

    // Auth
    QrCode(String),
    PairingCode(String),
    AuthSuccess,

    // Messages
    MessageReceived(Message),
    MessageEdited {
        id: String,
        new_content: String,
        timestamp: i64,
    },
    MessageDeleted {
        id: String,
        chat_jid: String,
    },
    Receipt {
        message_id: String,
        status: MessageStatus,
    },

    // Reactions
    ReactionReceived(Reaction),

    // Presence
    TypingStarted {
        chat_jid: String,
        user_jid: String,
    },
    TypingStopped {
        chat_jid: String,
        user_jid: String,
    },
    PresenceUpdate {
        jid: String,
        online: bool,
        last_seen: Option<i64>,
    },

    // Chat metadata
    ChatUpdate(Chat),
    ContactUpdate(Contact),
    GroupMembersUpdate {
        group_jid: String,
        members: Vec<GroupMember>,
    },

    // History sync — batch of chats with their messages, and push names
    HistorySync {
        chats: Vec<SyncedChat>,
        push_names: Vec<(String, String)>, // (jid, name)
    },
}

/// A chat with its messages and members, from history sync.
#[derive(Debug)]
pub struct SyncedChat {
    pub chat: Chat,
    pub messages: Vec<Message>,
    pub members: Vec<GroupMember>,
}

/// Map a whatsapp-rust Event into our domain WaEvent.
/// Returns None for events we don't care about.
pub fn map_wa_event(event: Event) -> Option<WaEvent> {
    match event {
        Event::Connected(_) => Some(WaEvent::Connected),

        Event::Disconnected(_) => Some(WaEvent::Disconnected {
            reason: "connection lost".to_string(),
        }),

        Event::LoggedOut(_) => Some(WaEvent::LoggedOut),

        Event::PairingQrCode { code, .. } => Some(WaEvent::QrCode(code)),

        Event::PairingCode { code, .. } => Some(WaEvent::PairingCode(code)),

        Event::PairSuccess(_) => Some(WaEvent::AuthSuccess),

        Event::Message(wa_msg, info) => {
            let msg = convert_realtime_message(&wa_msg, &info);
            Some(WaEvent::MessageReceived(msg))
        }

        Event::ChatPresence(update) => {
            let chat_jid = update.source.chat.to_string();
            let user_jid = update.source.sender.to_string();
            match update.state {
                wacore::types::presence::ChatPresence::Composing => {
                    Some(WaEvent::TypingStarted { chat_jid, user_jid })
                }
                wacore::types::presence::ChatPresence::Paused => {
                    Some(WaEvent::TypingStopped { chat_jid, user_jid })
                }
            }
        }

        Event::HistorySync(sync) => {
            let chats = convert_history_sync(&sync);
            // Extract push names from sync metadata
            let push_names: Vec<(String, String)> = sync
                .pushnames
                .iter()
                .filter_map(|pn| {
                    let id = pn.id.as_ref()?;
                    let name = pn.pushname.as_ref()?;
                    if id.is_empty() || name.is_empty() {
                        None
                    } else {
                        Some((id.clone(), name.clone()))
                    }
                })
                .collect();

            if chats.is_empty() && push_names.is_empty() {
                None
            } else {
                Some(WaEvent::HistorySync { chats, push_names })
            }
        }

        Event::JoinedGroup(lazy_conv) => {
            // History sync conversations arrive as JoinedGroup(LazyConversation).
            // Decode with messages for full history.
            tracing::debug!("JoinedGroup event received, decoding conversation...");
            match lazy_conv.get_with_messages() {
                Some(conv) => {
                    tracing::info!(
                        "JoinedGroup decoded: jid={}, name={:?}, msgs={}",
                        conv.id,
                        conv.display_name.as_deref().or(conv.name.as_deref()),
                        conv.messages.len()
                    );
                    if let Some(synced) = convert_conversation(&conv) {
                        Some(WaEvent::HistorySync {
                            chats: vec![synced],
                            push_names: Vec::new(),
                        })
                    } else {
                        None
                    }
                }
                None => {
                    tracing::warn!(
                        "JoinedGroup: get_with_messages() returned None, trying metadata-only"
                    );
                    if let Some(conv) = lazy_conv.get() {
                        tracing::info!("JoinedGroup fallback: jid={}", conv.id);
                        if let Some(synced) = convert_conversation(conv) {
                            Some(WaEvent::HistorySync {
                                chats: vec![synced],
                                push_names: Vec::new(),
                            })
                        } else {
                            None
                        }
                    } else {
                        tracing::error!("JoinedGroup: both decode methods failed");
                        None
                    }
                }
            }
        }

        Event::ContactUpdate(update) => {
            // Main source of contact names during sync.
            // ContactAction has full_name, first_name, lid_jid, pn_jid.
            let action = &update.action;
            let name = action
                .full_name
                .clone()
                .or_else(|| action.first_name.clone());
            // The event JID may be a LID. pn_jid gives the phone number JID.
            let phone_jid = action
                .pn_jid
                .clone()
                .unwrap_or_else(|| update.jid.to_string());

            if let Some(ref contact_name) = name {
                if !contact_name.is_empty() {
                    // Emit as ContactUpdate — this UPDATES existing chat names
                    // but does NOT create new chat entries for contacts without chats.
                    return Some(WaEvent::ContactUpdate(Contact {
                        jid: phone_jid,
                        name: Some(contact_name.clone()),
                        push_name: None,
                        phone: None,
                        profile_pic_url: None,
                    }));
                }
            }
            None
        }

        Event::PushNameUpdate(update) => {
            let jid = update.jid.to_string();
            Some(WaEvent::ContactUpdate(Contact {
                jid,
                name: None,
                push_name: Some(update.new_push_name),
                phone: None,
                profile_pic_url: None,
            }))
        }

        Event::MuteUpdate(update) => {
            let jid = update.jid.to_string();
            let muted = update.action.muted.unwrap_or(false);
            Some(WaEvent::ChatUpdate(Chat {
                jid,
                name: String::new(), // will be merged, not overwritten
                is_group: false,
                last_message_ts: None,
                last_message_preview: None,
                unread_count: 0,
                muted,
                pinned: false,
                archived: false,
                lid_jid: None,
            }))
        }

        // Events we don't map yet
        _ => None,
    }
}

/// Convert a real-time incoming message to our domain Message type.
fn convert_realtime_message(
    wa_msg: &wa::Message,
    info: &wacore::types::message::MessageInfo,
) -> Message {
    let content = wa_msg.text_content().map(|s| s.to_string());
    let msg_type = detect_message_type(wa_msg);

    let (media_mime, media_size, media_filename) = extract_media_info(wa_msg);
    let (reply_to_id, reply_to_preview) = extract_reply_context(wa_msg);
    let (dl_path, dl_key, dl_sha, dl_enc_sha) = extract_download_params(wa_msg);
    let meta = extract_media_meta(wa_msg);

    Message {
        id: info.id.clone(),
        chat_jid: info.source.chat.to_string(),
        sender_jid: info.source.sender.to_string(),
        timestamp: info.timestamp.timestamp(),
        content,
        message_type: msg_type,
        media_mime,
        media_size,
        media_filename,
        media_local_path: None,
        reply_to_id,
        reply_to_preview,
        edited: matches!(
            info.edit,
            wacore::types::message::EditAttribute::MessageEdit
        ),
        deleted: false,
        from_me: info.source.is_from_me,
        status: if info.source.is_from_me {
            MessageStatus::Sent
        } else {
            MessageStatus::Delivered
        },
        reactions: Vec::new(),
        sender_push_name: if info.push_name.is_empty() {
            None
        } else {
            Some(info.push_name.clone())
        },
        media_direct_path: dl_path,
        media_key: dl_key,
        media_file_sha256: dl_sha,
        media_file_enc_sha256: dl_enc_sha,
        media_duration_secs: meta.duration_secs,
        is_voice_note: meta.is_voice_note,
        link_title: meta.link_title,
        link_description: meta.link_description,
        link_url: meta.link_url,
        caption: meta.caption,
        is_gif: meta.is_gif,
    }
}

/// Convert history sync conversations into our domain types.
fn convert_history_sync(sync: &wa::HistorySync) -> Vec<SyncedChat> {
    sync.conversations
        .iter()
        .filter_map(|conv| convert_conversation(conv))
        .collect()
}

/// Convert a single protobuf Conversation to our domain types.
fn convert_conversation(conv: &wa::Conversation) -> Option<SyncedChat> {
    if conv.id.is_empty() {
        return None;
    }

    // If the conversation is indexed by LID, use the phone JID instead.
    // This merges LID chats into phone-indexed entries so they appear as one.
    let chat_jid = if conv.id.contains("@lid") {
        match conv.pn_jid.as_ref() {
            Some(pn) if !pn.is_empty() => pn.clone(),
            _ => conv.id.clone(), // No phone mapping — keep LID as fallback
        }
    } else {
        conv.id.clone()
    };

    let is_group = chat_jid.contains("@g.us");

    let unread = conv.unread_count.unwrap_or(0) as i32;
    let last_ts = conv
        .conversation_timestamp
        .map(|t| t as i64)
        .or_else(|| conv.last_msg_timestamp.map(|t| t as i64));
    let pinned_time = conv.pinned.unwrap_or(0);
    let muted_until = conv.mute_end_time.unwrap_or(0);
    let archived = conv.archived.unwrap_or(false);

    // Convert messages (do this BEFORE resolving name, so we can extract push names)
    let mut messages = Vec::new();
    let mut best_push_name: Option<String> = None;
    for hist_msg in &conv.messages {
        if let Some(ref wmi) = hist_msg.message {
            // Extract push name from the other person's messages for DM chat naming
            if !is_group {
                let is_from_me = wmi.key.from_me.unwrap_or(false);
                if !is_from_me {
                    if let Some(ref pn) = wmi.push_name {
                        if !pn.is_empty() {
                            best_push_name = Some(pn.clone());
                        }
                    }
                }
            }
            if let Some(msg) = convert_history_message(wmi, &chat_jid) {
                messages.push(msg);
            }
        }
    }

    // Log what name data we actually have for DM chats
    if !is_group {
        tracing::info!(
            "DM chat {}: display_name={:?}, name={:?}, lid_jid={:?}, pn_jid={:?}, push_name_from_msgs={:?}, msgs_count={}",
            &chat_jid,
            conv.display_name.as_deref(),
            conv.name.as_deref(),
            conv.lid_jid.as_deref(),
            conv.pn_jid.as_deref(),
            best_push_name.as_deref(),
            conv.messages.len(),
        );
    }

    // Resolve chat name: display_name > name > push_name from messages > phone number
    let name = conv
        .display_name
        .clone()
        .or_else(|| conv.name.clone())
        .or(best_push_name)
        .unwrap_or_else(|| chat_jid.split('@').next().unwrap_or("?").to_string());

    // Sort messages by timestamp ascending
    messages.sort_by_key(|m| m.timestamp);

    // Build preview from last message — include type label for non-text
    let last_preview = messages.last().map(|m| {
        if let Some(ref text) = m.content {
            crate::util::truncate(text, 50)
        } else {
            match m.message_type {
                MessageType::Image => "[Image]".to_string(),
                MessageType::Video => "[Video]".to_string(),
                MessageType::Audio => "[Audio]".to_string(),
                MessageType::Document => "[Document]".to_string(),
                MessageType::Sticker => "[Sticker]".to_string(),
                MessageType::Location => "[Location]".to_string(),
                MessageType::Poll => "[Poll]".to_string(),
                MessageType::Contact => "[Contact]".to_string(),
                _ => "[Message]".to_string(),
            }
        }
    });

    let chat = Chat {
        jid: chat_jid,
        name,
        is_group,
        last_message_ts: last_ts,
        last_message_preview: last_preview,
        unread_count: unread,
        muted: muted_until > 0,
        pinned: pinned_time > 0,
        archived,
        lid_jid: conv.lid_jid.clone(),
    };

    // Extract group members from conversation participants
    let members: Vec<GroupMember> = if is_group {
        conv.participant
            .iter()
            .map(|p| {
                let rank = p.rank.unwrap_or(0);
                GroupMember {
                    jid: p.user_jid.clone(),
                    is_admin: rank == 1 || rank == 2,
                    is_super_admin: rank == 2,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    Some(SyncedChat {
        chat,
        messages,
        members,
    })
}

/// Convert a WebMessageInfo from history sync to our domain Message.
fn convert_history_message(wmi: &wa::WebMessageInfo, chat_jid: &str) -> Option<Message> {
    let key = &wmi.key;
    let wa_msg = wmi.message.as_ref()?;

    let msg_id = key.id.clone().unwrap_or_default();
    if msg_id.is_empty() {
        return None;
    }

    let from_me = key.from_me.unwrap_or(false);
    let sender_jid = if from_me {
        "me".to_string()
    } else {
        // For group messages, the actual sender is in wmi.participant (top-level),
        // NOT key.participant (which is for reply quoting context).
        wmi.participant
            .clone()
            .or_else(|| key.participant.clone())
            .or_else(|| key.remote_jid.clone())
            .unwrap_or_else(|| chat_jid.to_string())
    };

    let timestamp = wmi.message_timestamp.unwrap_or(0) as i64;
    let content = wa_msg.text_content().map(|s| s.to_string());
    let msg_type = detect_message_type(wa_msg);
    let (media_mime, media_size, media_filename) = extract_media_info(wa_msg);
    let (reply_to_id, reply_to_preview) = extract_reply_context(wa_msg);
    let (dl_path, dl_key, dl_sha, dl_enc_sha) = extract_download_params(wa_msg);
    let meta = extract_media_meta(wa_msg);

    let status_int = wmi.status.unwrap_or(0);
    let status = match status_int {
        0 => MessageStatus::Pending,
        1 => MessageStatus::Sent,
        2 => MessageStatus::Delivered,
        3 | 4 => MessageStatus::Read,
        5 => MessageStatus::Failed,
        _ => MessageStatus::Sent,
    };

    Some(Message {
        id: msg_id,
        chat_jid: chat_jid.to_string(),
        sender_jid,
        timestamp,
        content,
        message_type: msg_type,
        media_mime,
        media_size,
        media_filename,
        media_local_path: None,
        reply_to_id,
        reply_to_preview,
        edited: false,
        deleted: false,
        from_me,
        status,
        reactions: Vec::new(),
        sender_push_name: wmi.push_name.clone(),
        media_direct_path: dl_path,
        media_key: dl_key,
        media_file_sha256: dl_sha,
        media_file_enc_sha256: dl_enc_sha,
        media_duration_secs: meta.duration_secs,
        is_voice_note: meta.is_voice_note,
        link_title: meta.link_title,
        link_description: meta.link_description,
        link_url: meta.link_url,
        caption: meta.caption,
        is_gif: meta.is_gif,
    })
}

/// Detect message type from the protobuf message.
fn detect_message_type(msg: &wa::Message) -> MessageType {
    if msg.conversation.is_some() || msg.extended_text_message.is_some() {
        MessageType::Text
    } else if msg.image_message.is_some() {
        MessageType::Image
    } else if msg.video_message.is_some() {
        MessageType::Video
    } else if msg.audio_message.is_some() {
        MessageType::Audio
    } else if msg.document_message.is_some() {
        MessageType::Document
    } else if msg.sticker_message.is_some() {
        MessageType::Sticker
    } else if msg.location_message.is_some() || msg.live_location_message.is_some() {
        MessageType::Location
    } else if msg.poll_creation_message.is_some() || msg.poll_update_message.is_some() {
        MessageType::Poll
    } else if msg.contact_message.is_some() || msg.contacts_array_message.is_some() {
        MessageType::Contact
    } else {
        // Handle ephemeral/view-once wrappers
        let base = msg.get_base_message();
        if !std::ptr::eq(base, msg) {
            // Wrapped message — recurse on the base
            detect_message_type(base)
        } else {
            MessageType::Unknown("unknown".to_string())
        }
    }
}

/// Extract media metadata from the protobuf message.
fn extract_media_info(msg: &wa::Message) -> (Option<String>, Option<i64>, Option<String>) {
    if let Some(ref img) = msg.image_message {
        return (
            img.mimetype.clone(),
            img.file_length.map(|l| l as i64),
            None,
        );
    }
    if let Some(ref vid) = msg.video_message {
        return (
            vid.mimetype.clone(),
            vid.file_length.map(|l| l as i64),
            None,
        );
    }
    if let Some(ref aud) = msg.audio_message {
        return (
            aud.mimetype.clone(),
            aud.file_length.map(|l| l as i64),
            None,
        );
    }
    if let Some(ref doc) = msg.document_message {
        return (
            doc.mimetype.clone(),
            doc.file_length.map(|l| l as i64),
            doc.file_name.clone(),
        );
    }
    (None, None, None)
}

/// Extract reply/quote context from the protobuf message.
fn extract_reply_context(msg: &wa::Message) -> (Option<String>, Option<String>) {
    // Check extended text for context_info
    if let Some(ref ext) = msg.extended_text_message {
        if let Some(ref ctx) = ext.context_info {
            let quoted_id = ctx.stanza_id.clone();
            let quoted_text = ctx
                .quoted_message
                .as_ref()
                .and_then(|m| m.text_content().map(|s| s.to_string()));
            if quoted_id.is_some() || quoted_text.is_some() {
                return (quoted_id, quoted_text);
            }
        }
    }
    (None, None)
}

/// Rich media metadata for display.
pub struct MediaMeta {
    pub duration_secs: Option<u32>,
    pub is_voice_note: bool,
    pub is_gif: bool,
    pub caption: Option<String>,
    pub link_title: Option<String>,
    pub link_description: Option<String>,
    pub link_url: Option<String>,
}

fn extract_media_meta(msg: &wa::Message) -> MediaMeta {
    let mut meta = MediaMeta {
        duration_secs: None,
        is_voice_note: false,
        is_gif: false,
        caption: None,
        link_title: None,
        link_description: None,
        link_url: None,
    };

    if let Some(ref vid) = msg.video_message {
        meta.duration_secs = vid.seconds;
        meta.caption = vid.caption.clone();
        meta.is_gif = vid.gif_playback.unwrap_or(false);
    }
    if let Some(ref aud) = msg.audio_message {
        meta.duration_secs = aud.seconds;
        meta.is_voice_note = aud.ptt.unwrap_or(false);
    }
    if let Some(ref img) = msg.image_message {
        meta.caption = img.caption.clone();
    }
    if let Some(ref ext) = msg.extended_text_message {
        meta.link_title = ext.title.clone();
        meta.link_description = ext.description.clone();
        meta.link_url = ext.matched_text.clone();
    }

    meta
}

/// Extract media download parameters for stickers and images.
fn extract_download_params(
    msg: &wa::Message,
) -> (
    Option<String>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
) {
    if let Some(ref stk) = msg.sticker_message {
        return (
            stk.direct_path.clone(),
            stk.media_key.clone(),
            stk.file_sha256.clone(),
            stk.file_enc_sha256.clone(),
        );
    }
    if let Some(ref img) = msg.image_message {
        return (
            img.direct_path.clone(),
            img.media_key.clone(),
            img.file_sha256.clone(),
            img.file_enc_sha256.clone(),
        );
    }
    (None, None, None, None)
}
