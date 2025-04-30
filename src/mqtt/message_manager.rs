use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::fmt;
use tokio::sync::mpsc;

#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MQTTMessage {
    pub topic: String,
    pub content: String,
    pub timestamp: NaiveDateTime,
}

impl fmt::Display for MQTTMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let preview = if self.content.len() > 10 {
            &self.content[10..] // Nimm alles ab Position 10
        } else {
            &self.content // Verwende den ganzen String
        };
        write!(f, "{} - {}", self.timestamp, preview)
    }
}

impl MQTTMessage {
    pub fn from_topic(topic: String, content: String) -> Self {
        MQTTMessage {
            topic,
            content,
            timestamp: chrono::Local::now().naive_local(),
        }
    }

    pub fn render(&self) -> String {
        format!("{}: {}\n{}", self.timestamp, self.topic, self.content)
    }
}

pub struct MsgManager {
    pub received_msg: mpsc::Sender<MQTTMessage>,
    pub distribution_msg: mpsc::Receiver<MQTTMessage>,
}
