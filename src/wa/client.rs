use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;
use wacore::types::events::Event;
use waproto::whatsapp as wa;
use whatsapp_rust::bot::Bot;
use whatsapp_rust::store::SqliteStore;
use whatsapp_rust::transport::TokioWebSocketTransportFactory;
use whatsapp_rust::transport::UreqHttpClient;
use whatsapp_rust::Client;
use whatsapp_rust::TokioRuntime;

use crate::config::Config;
use crate::event::AppEvent;
use crate::wa::events::{map_wa_event, WaEvent};

pub struct WaClient {
    pub(crate) client: Option<Arc<Client>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    config: Config,
    session_path: PathBuf,
}

impl WaClient {
    pub async fn new(config: &Config, event_tx: mpsc::UnboundedSender<AppEvent>) -> Result<Self> {
        let session_path = config.data_dir().join("session.db");
        Ok(Self {
            client: None,
            event_tx,
            config: config.clone(),
            session_path,
        })
    }

    /// Connect to WhatsApp.
    /// If no session exists, triggers auth flow by sending QrCode events.
    /// On success, sends Connected event and begins receiving messages.
    pub async fn connect(&mut self) -> Result<()> {
        let session_str = self.session_path.to_string_lossy().to_string();
        tracing::info!("opening session db: {}", session_str);

        let backend = Arc::new(SqliteStore::new(&session_str).await?);

        // Bridge whatsapp-rust events into our mpsc channel
        let event_tx = self.event_tx.clone();
        let mut bot = Bot::builder()
            .with_backend(backend)
            .with_transport_factory(TokioWebSocketTransportFactory::new())
            .with_http_client(UreqHttpClient::new())
            .with_runtime(TokioRuntime)
            .on_event(move |event, _client| {
                let tx = event_tx.clone();
                async move {
                    // Log every single event that arrives
                    let event_tag = match &event {
                        Event::Connected(_) => "Connected",
                        Event::Disconnected(_) => "Disconnected",
                        Event::PairSuccess(_) => "PairSuccess",
                        Event::PairingQrCode { .. } => "PairingQrCode",
                        Event::PairingCode { .. } => "PairingCode",
                        Event::LoggedOut(_) => "LoggedOut",
                        Event::Message(_, _) => "Message",
                        Event::HistorySync(_) => "HistorySync",
                        Event::JoinedGroup(_) => "JoinedGroup",
                        Event::ChatPresence(_) => "ChatPresence",
                        Event::PushNameUpdate(_) => "PushNameUpdate",
                        Event::ContactUpdate(_) => "ContactUpdate",
                        Event::Receipt(_) => "Receipt",
                        _other => "Other",
                    };
                    if event_tag == "Other" {
                        let dbg = format!("{:?}", &event);
                        let preview = crate::util::truncate_bare(&dbg, 120);
                        tracing::info!("wa event Other: {}", preview);
                    } else {
                        tracing::info!("wa event: {}", event_tag);
                    }

                    if let Some(wa_event) = map_wa_event(event) {
                        let _ = tx.send(AppEvent::Wa(wa_event));
                    }
                }
            })
            .build()
            .await?;

        // Store client handle for sending messages
        self.client = Some(bot.client());

        // Start the bot — spawns connection and event processing
        let handle = bot.run().await?;

        // Run bot lifecycle in background — signal app when bot stops
        let stop_tx = self.event_tx.clone();
        tokio::spawn(async move {
            match handle.await {
                Ok(()) => {
                    tracing::info!("bot handle completed — signaling reconnect");
                    let _ = stop_tx.send(AppEvent::Wa(WaEvent::BotStopped));
                }
                Err(e) => {
                    tracing::error!("bot handle error: {:?}", e);
                    let _ = stop_tx.send(AppEvent::Wa(WaEvent::BotStopped));
                }
            }
        });

        Ok(())
    }

    /// Disconnect cleanly.
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(client) = self.client.take() {
            client.disconnect().await;
        }
        Ok(())
    }

    /// Send a text message. Returns the message ID.
    pub async fn send_message(&self, chat_jid: &str, text: &str) -> Result<String> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not connected"))?;

        let jid = chat_jid
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid JID '{}': {:?}", chat_jid, e))?;

        let msg = wa::Message {
            conversation: Some(text.to_string()),
            ..Default::default()
        };

        let msg_id = client.send_message(jid, msg).await?;
        Ok(msg_id)
    }

    /// Send a reply (quoted message).
    pub async fn send_reply(
        &self,
        chat_jid: &str,
        text: &str,
        reply_to_id: &str,
        reply_to_text: Option<&str>,
    ) -> Result<String> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not connected"))?;

        let jid = chat_jid
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid JID: {:?}", e))?;

        // Build context info for quoting
        let context_info = wa::ContextInfo {
            stanza_id: Some(reply_to_id.to_string()),
            quoted_message: reply_to_text.map(|t| {
                Box::new(wa::Message {
                    conversation: Some(t.to_string()),
                    ..Default::default()
                })
            }),
            ..Default::default()
        };

        let msg = wa::Message {
            extended_text_message: Some(Box::new(wa::message::ExtendedTextMessage {
                text: Some(text.to_string()),
                context_info: Some(Box::new(context_info)),
                ..Default::default()
            })),
            ..Default::default()
        };

        let msg_id = client.send_message(jid, msg).await?;
        Ok(msg_id)
    }

    /// Send a reaction emoji to a message.
    pub async fn send_reaction(
        &self,
        chat_jid: &str,
        message_id: &str,
        from_me: bool,
        emoji: &str,
    ) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not connected"))?;

        let jid = chat_jid
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid JID: {:?}", e))?;

        let msg = wa::Message {
            reaction_message: Some(wa::message::ReactionMessage {
                key: Some(wa::MessageKey {
                    remote_jid: Some(chat_jid.to_string()),
                    from_me: Some(from_me),
                    id: Some(message_id.to_string()),
                    participant: None,
                }),
                text: Some(emoji.to_string()),
                sender_timestamp_ms: Some(chrono::Utc::now().timestamp_millis()),
                ..Default::default()
            }),
            ..Default::default()
        };

        client.send_message(jid, msg).await?;
        Ok(())
    }

    /// Send read receipts for messages.
    pub async fn send_read_receipt(&self, chat_jid: &str, message_ids: &[String]) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not connected"))?;

        let jid = chat_jid
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid JID: {:?}", e))?;

        client
            .mark_as_read(&jid, None, message_ids.to_vec())
            .await?;
        Ok(())
    }

    /// Download media bytes using stored download params.
    pub async fn download_media_bytes(
        &self,
        msg: &crate::store::messages::Message,
    ) -> Result<Vec<u8>> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not connected"))?;

        let direct_path = msg
            .media_direct_path
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no direct_path"))?;
        let media_key = msg
            .media_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no media_key"))?;
        let file_sha256 = msg
            .media_file_sha256
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no file_sha256"))?;
        let file_enc_sha256 = msg
            .media_file_enc_sha256
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no file_enc_sha256"))?;
        let file_length = msg.media_size.unwrap_or(0) as u64;

        let media_type = match msg.message_type {
            crate::store::messages::MessageType::Sticker => wacore::download::MediaType::Sticker,
            crate::store::messages::MessageType::Image => wacore::download::MediaType::Image,
            _ => wacore::download::MediaType::Image,
        };

        let data = client
            .download_from_params(
                direct_path,
                media_key,
                file_sha256,
                file_enc_sha256,
                file_length,
                media_type,
            )
            .await?;

        Ok(data)
    }

    /// Send an image with optional caption. Returns the message ID.
    pub async fn send_image(
        &self,
        chat_jid: &str,
        data: Vec<u8>,
        mime: &str,
        caption: Option<&str>,
        thumbnail: Option<Vec<u8>>,
        width: u32,
        height: u32,
    ) -> Result<String> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not connected"))?;

        let jid = chat_jid
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid JID: {:?}", e))?;

        let upload = client
            .upload(data, wacore::download::MediaType::Image)
            .await?;

        let msg = wa::Message {
            image_message: Some(Box::new(wa::message::ImageMessage {
                url: Some(upload.url),
                direct_path: Some(upload.direct_path),
                media_key: Some(upload.media_key),
                file_sha256: Some(upload.file_sha256),
                file_enc_sha256: Some(upload.file_enc_sha256),
                file_length: Some(upload.file_length),
                mimetype: Some(mime.to_string()),
                caption: caption.map(|s| s.to_string()),
                jpeg_thumbnail: thumbnail,
                width: Some(width),
                height: Some(height),
                ..Default::default()
            })),
            ..Default::default()
        };

        let msg_id = client.send_message(jid, msg).await?;
        Ok(msg_id)
    }

    /// Send a video with optional caption. Returns the message ID.
    pub async fn send_video(
        &self,
        chat_jid: &str,
        data: Vec<u8>,
        mime: &str,
        caption: Option<&str>,
    ) -> Result<String> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not connected"))?;

        let jid = chat_jid
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid JID: {:?}", e))?;

        let upload = client
            .upload(data, wacore::download::MediaType::Video)
            .await?;

        let msg = wa::Message {
            video_message: Some(Box::new(wa::message::VideoMessage {
                url: Some(upload.url),
                direct_path: Some(upload.direct_path),
                media_key: Some(upload.media_key),
                file_sha256: Some(upload.file_sha256),
                file_enc_sha256: Some(upload.file_enc_sha256),
                file_length: Some(upload.file_length),
                mimetype: Some(mime.to_string()),
                caption: caption.map(|s| s.to_string()),
                ..Default::default()
            })),
            ..Default::default()
        };

        let msg_id = client.send_message(jid, msg).await?;
        Ok(msg_id)
    }

    /// Send an audio file. Returns the message ID.
    pub async fn send_audio(
        &self,
        chat_jid: &str,
        data: Vec<u8>,
        mime: &str,
    ) -> Result<String> {
        let client = self.client.as_ref().ok_or_else(|| anyhow::anyhow!("not connected"))?;
        let jid = chat_jid.parse().map_err(|e| anyhow::anyhow!("invalid JID: {:?}", e))?;
        let upload = client.upload(data, wacore::download::MediaType::Audio).await?;
        let msg = wa::Message {
            audio_message: Some(Box::new(wa::message::AudioMessage {
                url: Some(upload.url),
                direct_path: Some(upload.direct_path),
                media_key: Some(upload.media_key),
                file_sha256: Some(upload.file_sha256),
                file_enc_sha256: Some(upload.file_enc_sha256),
                file_length: Some(upload.file_length),
                mimetype: Some(mime.to_string()),
                ..Default::default()
            })),
            ..Default::default()
        };
        let msg_id = client.send_message(jid, msg).await?;
        Ok(msg_id)
    }

    /// Send a document file. Returns the message ID.
    pub async fn send_document(
        &self,
        chat_jid: &str,
        data: Vec<u8>,
        mime: &str,
        filename: &str,
    ) -> Result<String> {
        let client = self.client.as_ref().ok_or_else(|| anyhow::anyhow!("not connected"))?;
        let jid = chat_jid.parse().map_err(|e| anyhow::anyhow!("invalid JID: {:?}", e))?;
        let upload = client.upload(data, wacore::download::MediaType::Document).await?;
        let msg = wa::Message {
            document_message: Some(Box::new(wa::message::DocumentMessage {
                url: Some(upload.url),
                direct_path: Some(upload.direct_path),
                media_key: Some(upload.media_key),
                file_sha256: Some(upload.file_sha256),
                file_enc_sha256: Some(upload.file_enc_sha256),
                file_length: Some(upload.file_length),
                mimetype: Some(mime.to_string()),
                file_name: Some(filename.to_string()),
                ..Default::default()
            })),
            ..Default::default()
        };
        let msg_id = client.send_message(jid, msg).await?;
        Ok(msg_id)
    }

    /// Fetch group metadata (participants, subject, etc.) from the server.
    /// Returns (subject, members).
    pub async fn get_group_info(
        &self,
        group_jid: &str,
    ) -> Result<(Option<String>, Vec<crate::store::groups::GroupMember>)> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("not connected"))?;

        let jid = group_jid
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid JID: {:?}", e))?;

        let metadata = client.groups().get_metadata(&jid).await?;

        let subject = if metadata.subject.is_empty() {
            None
        } else {
            Some(metadata.subject.clone())
        };

        let members = metadata
            .participants
            .iter()
            .map(|p| crate::store::groups::GroupMember {
                jid: p
                    .phone_number
                    .as_ref()
                    .map(|pn| pn.to_string())
                    .unwrap_or_else(|| p.jid.to_string()),
                is_admin: p.is_admin,
                is_super_admin: false,
            })
            .collect();

        Ok((subject, members))
    }

    /// Check if we're connected.
    pub fn is_connected(&self) -> bool {
        self.client
            .as_ref()
            .map(|c| c.is_connected())
            .unwrap_or(false)
    }
}
