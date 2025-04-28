use std::time::Duration;

use chrono::NaiveDateTime;
use rumqttc::tokio_rustls::rustls::client::danger;
use rumqttc::{Client, MqttOptions, QoS};
use tokio::sync::{mpsc, watch};
use tokio::task;

use super::message_manager::{MQTTMessage, MsgManager};
use super::{config, message_manager};

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Failed,
    Reconnecting,
}

#[derive(Clone, Debug, Default)]
pub struct MQTTStatus {
    pub connection_state: ConnectionState,
    pub error_messages: Vec<String>,
    pub messages_received: usize,
    pub messages_sent: usize,
    pub last_activity: Option<chrono::DateTime<chrono::Local>>,
}

pub struct MqttHandler {
    status: MQTTStatus,
    client: rumqttc::Client,
    connection: rumqttc::Connection,
    config: config::MqttConfig,
    msg_manager: MsgManager,
}

impl MqttHandler {
    pub fn new(
        config: config::MqttConfig,
        msg_in: mpsc::Receiver<MQTTMessage>,
        msg_out: mpsc::Sender<MQTTMessage>,
    ) -> Self {
        let msg_manager = MsgManager {
            received_msg: msg_out,
            distribution_msg: msg_in,
        };
        let server_comps: Vec<&str> = config.server.url.split(":").collect();
        let server_addr = *server_comps.first().unwrap_or(&" ");
        let port = server_comps.get(1).unwrap_or(&"1883").parse().unwrap();
        let mut mqtt_options = MqttOptions::new("OpenController", server_addr, port);
        mqtt_options
            .set_credentials(config.server.user.clone(), config.server.pw.clone())
            .set_keep_alive(Duration::from_secs(5));

        let (client, connection) = Client::new(mqtt_options, 100);

        let status = MQTTStatus::default();

        MqttHandler {
            status,
            client,
            connection,
            config,
            msg_manager,
        }
    }
}
