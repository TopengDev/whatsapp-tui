pub mod action;

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppFocus, AppMode};
use action::Action;

/// Maps a key event to an action based on current mode and focus.
pub fn dispatch(
    key: &KeyEvent,
    mode: AppMode,
    focus: AppFocus,
    pending: &mut KeyBuffer,
) -> Option<Action> {
    // Handle two-key sequences (gg, etc.)
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

fn dispatch_normal(key: &KeyEvent, _focus: AppFocus, pending: &mut KeyBuffer) -> Option<Action> {
    match key.code {
        // Quit
        KeyCode::Char('q') => Some(Action::Quit),

        // Navigation
        KeyCode::Char('j') | KeyCode::Down => Some(Action::NextItem),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::PrevItem),
        KeyCode::Char('G') => Some(Action::LastItem),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::HalfPageDown)
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::HalfPageUp)
        }

        // Enter chat / open
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => Some(Action::OpenChat),
        KeyCode::Char('h') | KeyCode::Left => Some(Action::CloseChat),

        // Focus
        KeyCode::Tab => Some(Action::CycleFocus),
        KeyCode::Char('H') => Some(Action::FocusLeft),
        KeyCode::Char('L') => Some(Action::FocusRight),
        KeyCode::Char('\\') => Some(Action::ToggleInfoPanel),

        // Mode transitions
        KeyCode::Char('i') => Some(Action::EnterInsert),
        KeyCode::Char(':') => Some(Action::EnterCommand),
        KeyCode::Char('/') => Some(Action::EnterSearch),

        // Message actions
        KeyCode::Char('r') => Some(Action::ReplyToSelected),
        KeyCode::Char('e') => Some(Action::EditSelected),
        KeyCode::Char('x') => Some(Action::DeleteSelected),
        KeyCode::Char('R') => Some(Action::ReactToSelected),
        KeyCode::Char('y') => Some(Action::YankSelected),
        KeyCode::Char('o') => Some(Action::OpenMedia),
        KeyCode::Char('d') => Some(Action::DownloadMedia),

        // Chat management
        KeyCode::Char('m') => Some(Action::MuteChat),
        KeyCode::Char('a') => Some(Action::ArchiveChat),
        KeyCode::Char('p') => Some(Action::PinChat),

        // Search nav
        KeyCode::Char('n') => Some(Action::SearchNext),
        KeyCode::Char('N') => Some(Action::SearchPrev),

        // Two-key sequence starters
        KeyCode::Char('g') => {
            pending.pending = Some(*key);
            pending.timeout = Instant::now();
            None
        }

        _ => None,
    }
}

fn dispatch_insert(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ExitInsert),
        KeyCode::Enter => Some(Action::SendMessage),
        _ => None, // tui-textarea handles text input directly
    }
}

fn dispatch_command(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ExitOverlay),
        _ => None, // overlay handles text input
    }
}

fn dispatch_search(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ExitOverlay),
        _ => None, // overlay handles text input
    }
}

/// Minimal buffer for two-key sequences like gg.
pub struct KeyBuffer {
    pending: Option<KeyEvent>,
    timeout: Instant,
}

impl KeyBuffer {
    pub fn new() -> Self {
        Self {
            pending: None,
            timeout: Instant::now(),
        }
    }

    pub fn try_complete(
        &mut self,
        key: &KeyEvent,
        _mode: AppMode,
        _focus: AppFocus,
    ) -> Option<Action> {
        if let Some(first) = self.pending.take() {
            if self.timeout.elapsed() > std::time::Duration::from_millis(500) {
                return None;
            }
            match (first.code, key.code) {
                (KeyCode::Char('g'), KeyCode::Char('g')) => return Some(Action::FirstItem),
                _ => return None,
            }
        }
        None
    }
}
