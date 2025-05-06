use crate::ui;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct MqttConfig {
    pub subbed_topics: Vec<String>,
    pub server: ui::common::MQTTServer,
    pub poll_frequency: usize,
}
