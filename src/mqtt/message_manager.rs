use chrono::NaiveDateTime;
use std::fmt;
use tokio::sync::mpsc;

#[derive(Default, Clone, PartialEq, Eq)]
pub struct MQTTMessage {
    topic: String,
    content: String,
    timestamp: NaiveDateTime,
}

impl fmt::Display for MQTTMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut slice = self.content.clone();
        let preview = slice.split_off(10);
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
