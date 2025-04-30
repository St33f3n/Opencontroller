use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use toml;
use tracing::{debug, info};

use crate::mapping;
use crate::mqtt;
use crate::mqtt::message_manager::MQTTMessage;

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
pub struct Config {
    session_name: String,
    last_session: Option<String>,
    path: PathBuf,
    available_sessions: HashMap<String, Config>,
    ui_config: UIConfig,
    connection_config: ConnectionConfig,
    controller_config: ControllerConfig,
    saved_msg: SavedMessages,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct UIConfig {
    theme: Theme,
    fps: u8,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct NetworkConfig {
    network: NetworkConnection,
    state: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct NetworkConnection {
    network: String,
    key: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ConnectionConfig {
    mqtt_config: mqtt::config::MqttConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ControllerConfig {
    keyboard_mapping: mapping::keyboard::KeyboardConfig,
    elrs_mapping: mapping::elrs::ELRSConfig,
}
#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct SavedMessages {
    msg: Vec<MQTTMessage>,
}

pub struct ConfigPortal {
    ui_config: Arc<RwLock<UIConfig>>,
    controller_config: Arc<RwLock<ControllerConfig>>,
    connection_config: Arc<RwLock<ConnectionConfig>>,
    msg_save: Arc<RwLock<SavedMessages>>,
}
