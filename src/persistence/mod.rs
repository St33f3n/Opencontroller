pub mod config_portal;
pub mod persistence_worker;
pub mod session_client;

use crate::mapping::{elrs::ELRSConfig, keyboard::KeyboardConfig};
use crate::mqtt::{config::MqttConfig, message_manager::MQTTMessage};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Theme {
    border_color: (u8, u8, u8),
    background_color_one: (u8, u8, u8),
    background_color_two: (u8, u8, u8),
    background_color_three: (u8, u8, u8),
    text_color: (u8, u8, u8),
    highlight_color: (u8, u8, u8),
    highlight_color_two: (u8, u8, u8),
    frame_color: (u8, u8, u8),
    frame_color_two: (u8, u8, u8),
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct UIConfig {
    theme: Theme,
    fps: u8,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct NetworkConfig {
    pub network: NetworkConnection,
    pub state: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct NetworkConnection {
    pub network: String,
    pub key: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ConnectionConfig {
    pub mqtt_config: MqttConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ControllerConfig {
    pub keyboard_mapping: KeyboardConfig,
    pub elrs_mapping: ELRSConfig,
}
#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct SavedMessages {
    pub msg: Vec<MQTTMessage>,
}

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct SessionConfig {
    pub session_name: String,
    pub last_session: Option<String>,
    pub path: PathBuf,
    pub available_sessions: HashMap<String, PathBuf>,
}
