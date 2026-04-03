use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

fn default_true() -> bool {
    true
}

fn default_timestamp_format() -> String {
    "%H:%M".to_string()
}

fn default_message_max_width() -> u16 {
    80
}

fn default_max_retries() -> u32 {
    10
}

fn default_base_delay() -> u64 {
    1000
}

#[derive(Clone, Deserialize)]
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

#[derive(Clone, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub confirm_quit: bool,
    #[serde(default = "default_timestamp_format")]
    pub timestamp_format: String,
}

#[derive(Clone, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default = "default_message_max_width")]
    pub message_max_width: u16,
}

#[derive(Clone, Deserialize)]
pub struct ConnectionConfig {
    #[serde(default = "default_max_retries")]
    pub reconnect_max_retries: u32,
    #[serde(default = "default_base_delay")]
    pub reconnect_base_delay_ms: u64,
}

#[derive(Clone, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub desktop: bool,
    #[serde(default = "default_true")]
    pub show_preview: bool,
    #[serde(default)]
    pub muted_chats: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            appearance: AppearanceConfig::default(),
            connection: ConnectionConfig::default(),
            notifications: NotificationConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            confirm_quit: default_true(),
            timestamp_format: default_timestamp_format(),
        }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            message_max_width: default_message_max_width(),
        }
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            reconnect_max_retries: default_max_retries(),
            reconnect_base_delay_ms: default_base_delay(),
        }
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            desktop: default_true(),
            show_preview: default_true(),
            muted_chats: false,
        }
    }
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
        let dir = dirs::data_dir().expect("no data dir").join("whatsapp-tui");
        fs::create_dir_all(&dir).expect("failed to create data dir");
        dir
    }

    pub fn cache_dir(&self) -> PathBuf {
        let dir = dirs::cache_dir()
            .expect("no cache dir")
            .join("whatsapp-tui");
        fs::create_dir_all(&dir).expect("failed to create cache dir");
        dir
    }
}
