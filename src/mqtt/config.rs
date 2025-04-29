use crate::ui;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MqttConfig {
    pub subbed_topics: Vec<String>,
    pub server: ui::MQTTServer,
    pub poll_frequency: usize,
}
