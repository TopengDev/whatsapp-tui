# whatsapp-tui

Rust TUI WhatsApp client with vim keybindings. Built by chilldawg/christopher (TopengDev), elpabl0 is a contributor.

## Stack
- **Language**: Rust 1.88+
- **TUI**: ratatui + crossterm
- **WA Protocol**: whatsapp-rust (pure Rust, no baileys/node)
- **Storage**: rusqlite (bundled SQLite)
- **Image rendering**: ratatui-image (Kitty/Sixel/Ghostty graphics protocol)
- **Notifications**: notify-rust

## Architecture
- `src/wa/` — WhatsApp protocol: auth, client, events, media
- `src/store/` — SQLite storage: chats, contacts, groups, messages, reactions
- `src/ui/` — TUI rendering: chat list, messages, header, input, info panel, overlays
- `src/keys/` — Vim keybinding dispatch (NORMAL/INSERT/COMMAND/SEARCH modes)
- `src/config.rs` — TOML config
- `src/app.rs` — App state machine

## Build & Run
```bash
cargo build --release
./target/release/whatsapp-tui
# Or just: wa (symlinked at ~/.local/bin/wa)
```

## Testing Environment
- elpabl0: Apple M1 Pro, 16GB, macOS 26.1, Ghostty terminal inside tmux
- christopher: Windows (separate testing)

## Key Info
- Repo: github.com/TopengDev/whatsapp-tui
- elpabl0 has write access
- QR auth on first launch
- Config at ~/.config/whatsapp-tui/ (or config/default.toml in repo)
