use crate::ui::{self, MQTTServer};
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct MqttConfig {
    pub available_topics: Vec<String>,
    pub subbed_topics: Vec<String>,
    pub server: ui::common::MQTTServer,
    pub available_servers: Vec<ui::common::MQTTServer>,
    pub poll_frequency: usize,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            poll_frequency: 10,
            available_topics: Vec::new(),
            server: MQTTServer::default(),
            available_servers: Vec::new(),
            subbed_topics: Vec::new(),
        }
    }
}
