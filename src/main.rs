mod app;
mod config;
mod event;
mod keys;
mod notify;
mod store;
mod ui;
mod util;
mod wa;

use std::fs;
use std::io;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tracing_appender;
use tracing_subscriber;

use app::App;
use config::Config;
use event::AppEvent;
use store::Store;
use wa::client::WaClient;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    tracing_init(&config)?;

    let store = Store::open(&config.data_dir())?;
    let (event_tx, event_rx) = mpsc::unbounded_channel::<AppEvent>();

    // Detect terminal image protocol BEFORE entering raw mode/alternate screen.
    let picker = ratatui_image::picker::Picker::from_query_stdio()
        .unwrap_or_else(|_| ratatui_image::picker::Picker::from_fontsize((8, 16)));

    let wa_client = WaClient::new(&config, event_tx.clone()).await?;
    let terminal = setup_terminal()?;

    let mut app = App::new(config, store, wa_client, event_tx, picker);
    let result = app.run(terminal, event_rx).await;

    restore_terminal()?;
    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    // Explicitly disable kitty keyboard protocol enhancement.
    // It causes garbled input in Ghostty+tmux and other multiplexer combos.
    let _ = execute!(
        stdout,
        crossterm::event::PopKeyboardEnhancementFlags
    );
    // Drain any pending input from tmux send-keys or shell startup.
    // Without this, the Enter key from `tmux send-keys 'cargo run' Enter`
    // arrives after raw mode is enabled and gets routed as a keypress.
    while crossterm::event::poll(std::time::Duration::from_millis(50))? {
        let _ = crossterm::event::read()?;
    }
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}

fn tracing_init(config: &Config) -> Result<()> {
    let log_dir = config.data_dir().join("logs");
    fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "whatsapp-tui.log");
    tracing_subscriber::fmt()
        .with_writer(file_appender)
        // Capture our crate at debug, whatsapp-rust libs at info, everything else at warn
        .with_env_filter("whatsapp_tui=debug,whatsapp_rust=info,wacore=info,warn")
        .with_ansi(false)
        .init();

    // Bridge the `log` crate (used by whatsapp-rust) to tracing
    // tracing-log is included by tracing-subscriber by default
    Ok(())
}
