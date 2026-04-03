# whatsapp-tui Roadmap

Goal: Make whatsapp-tui as close to the real WhatsApp app as possible.

## BUGS (fix first)
- [ ] **CRASH: emoji boundary panic** — `chat_list.rs:72` panics on multi-byte chars in message previews (e.g. `😅`). Use `.chars()` boundary check instead of byte slicing.

## Already Implemented (MVP)
- [x] QR code + pairing code auth
- [x] 3-pane layout (chat list, messages, info panel)
- [x] Send/receive text messages
- [x] Reply to specific messages (quote + reply)
- [x] Reactions (send + receive, emoji overlay)
- [x] Full group chat support (list, read, send, members)
- [x] Message search (FTS5)
- [x] 90-day history sync
- [x] Inline image rendering (Kitty/Sixel)
- [x] Media placeholders + external viewer (d to download, o to open)
- [x] Desktop notifications (notify-rust)
- [x] Vim keybindings (j/k, gg/G, /, :commands)
- [x] Mute/pin/archive chats
- [x] Copy message to clipboard (yank)
- [x] Delete messages
- [x] Edit messages
- [x] Graceful reconnect with backoff
- [x] XDG-compliant TOML config

## v0.2 — Polish & Media
- [ ] Fix all multi-byte char / emoji boundary issues across UI
- [ ] Sticker rendering inline
- [ ] Video thumbnail preview
- [ ] Audio/voice message duration display + playback via external player
- [ ] Document file download + open
- [ ] Link preview rendering (title + description from URLs)
- [ ] Image gallery view (scroll through media in a chat)
- [ ] GIF support
- [ ] Contact card display

## v0.3 — UX & Vim Power
- [ ] Configurable keybindings via config file
- [ ] Multi-key sequences (vim-style chords)
- [ ] Mouse support (click to select chat, scroll)
- [ ] Themes (dark/light/custom color schemes)
- [ ] Chat list sorting (by date, unread, pinned first)
- [ ] Unread message count badges
- [ ] Jump to unread messages in chat
- [ ] Message timestamp toggle (relative vs absolute)
- [ ] Multi-line message compose (shift+enter or similar)
- [ ] Message formatting (bold, italic, strikethrough, monospace)

## v0.4 — Communication Features
- [ ] Typing indicators (show who's typing)
- [ ] Read receipts (blue ticks)
- [ ] Online/last seen status
- [ ] Message forwarding
- [ ] Broadcast lists
- [ ] Message starring/bookmarking
- [ ] Chat export (text/JSON)
- [ ] Profile pictures in chat list
- [ ] Contact info panel (phone, about, shared groups)

## v0.5 — Group Management
- [ ] Create groups
- [ ] Add/remove members
- [ ] Admin management
- [ ] Group description/settings
- [ ] Group invite links
- [ ] Leave group
- [ ] Disappearing messages toggle

## v1.0 — Production
- [ ] modalkit integration (full vim fidelity)
- [ ] Packaging (AUR, brew, cargo install)
- [ ] README + docs + screenshots
- [ ] CI/CD pipeline (cargo test, clippy, fmt)
- [ ] Performance profiling + optimization
- [ ] Session persistence across restarts (no re-sync needed)
- [ ] Open source release

## Never (hard limits)
- Voice/video calls (no library supports this)
- Status/story posting (high ban risk)
- WhatsApp Business API features
- Payment integration
