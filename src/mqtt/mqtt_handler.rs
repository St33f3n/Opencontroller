use std::thread;
use std::time::Duration;

use super::message_manager::{MQTTMessage, MsgManager};
use super::{config, message_manager};
use chrono::NaiveDateTime;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, Packet, PacketType, QoS};
use statum::{machine, state};
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};
use tracing_subscriber::fmt::time;

#[state]
#[derive(Debug, Clone, Copy)]
pub enum MQTTState {
    Initializing,
    Configured,
    Processing,
}

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

#[machine]
pub struct MQTTConnection<S: MQTTState> {
    status: MQTTStatus,
    client: AsyncClient,
    event_loop: Option<EventLoop>, // Wird in separatem Task verwendet
    ui_config: watch::Receiver<config::MqttConfig>,
    config: config::MqttConfig,
    msg_manager: MsgManager,
}

impl MQTTConnection<Initializing> {
    pub async fn create(
        msg_in: mpsc::Receiver<MQTTMessage>,
        msg_out: mpsc::Sender<MQTTMessage>,
        ui_config: watch::Receiver<config::MqttConfig>,
    ) -> Self {
        let msg_manager = MsgManager {
            received_msg: msg_out,
            distribution_msg: msg_in,
        };

        let config = ui_config.borrow().clone();
        let server_comps: Vec<&str> = config.server.url.split(':').collect();

        let server_addr = server_comps.first().copied().unwrap_or(" ");
        let port = server_comps
            .get(1)
            .unwrap_or(&"1883")
            .parse()
            .unwrap_or(1883);

        let mut mqtt_options = MqttOptions::new("OpenController", server_addr, port);
        mqtt_options
            .set_credentials(config.server.user.clone(), config.server.pw.clone())
            .set_keep_alive(Duration::from_secs(5));

        let (client, eventloop) = AsyncClient::new(mqtt_options, 10);
        let status = MQTTStatus::default();

        Self::new(
            status,
            client,
            Some(eventloop),
            ui_config,
            config,
            msg_manager,
        )
    }

    pub async fn configure(self) -> MQTTConnection<Configured> {
        info!(
            "Configuring MQTT connection with {} topics",
            self.config.subbed_topics.len()
        );

        for topic in &self.config.subbed_topics {
            match self.client.subscribe(topic, QoS::AtLeastOnce).await {
                Ok(_) => info!("Successfully subscribed to topic: {}", topic),
                Err(e) => error!("Failed to subscribe to topic {}: {}", topic, e),
            }
        }

        self.transition()
    }
}

impl MQTTConnection<Configured> {
    pub async fn activate(mut self) -> MQTTConnection<Processing> {
        let config = self.ui_config.borrow().clone();
        let mut new_topics = Vec::new();
        let mut removed_topics = Vec::new();
        // Server hat sich geändert, Verbindung neu aufbauen
        if self.config.server != config.server {
            info!("Server configuration changed, reconnecting...");

            let server_comps: Vec<&str> = config.server.url.split(':').collect();
            let server_addr = server_comps.first().copied().unwrap_or(" ");
            let port = server_comps
                .get(1)
                .unwrap_or(&"1883")
                .parse()
                .unwrap_or(1883);

            let mut mqtt_options = MqttOptions::new("OpenController", server_addr, port);
            mqtt_options
                .set_credentials(config.server.user.clone(), config.server.pw.clone())
                .set_keep_alive(Duration::from_secs(5));

            let (client, eventloop) = AsyncClient::new(mqtt_options, 10);
            self.client = client;
            self.event_loop = Some(eventloop);
            self.config = config.clone();
        }
        // Nur die Topics haben sich geändert
        if self.config.subbed_topics != config.subbed_topics {
            info!("Topic configuration changed, updating subscriptions");

            // Neue Topics identifizieren und abonnieren
            new_topics = config
                .subbed_topics
                .iter()
                .cloned()
                .filter(|t| !self.config.subbed_topics.contains(t))
                .collect();

            // Entfernte Topics identifizieren und abmelden
            removed_topics = self
                .config
                .subbed_topics
                .iter()
                .cloned()
                .filter(|t| !config.subbed_topics.contains(t))
                .collect();

            // Konfiguration aktualisieren
            self.config = config;
        }

        for topic in new_topics {
            let _ = self.client.subscribe(topic, QoS::AtLeastOnce).await;
        }

        for topic in removed_topics {
            let _ = self.client.unsubscribe(topic).await;
        }

        self.transition()
    }
}

impl MQTTConnection<Processing> {
    pub async fn run(mut self) -> MQTTConnection<Configured> {
        info!("MQTT connection processing started");

        // Timer für regelmäßige Konfigurationsprüfungen aus der Konfiguration entnehmen
        let poll_interval = Duration::from_millis(1000 / self.config.poll_frequency as u64);
        let mut last_check = std::time::Instant::now();

        loop {
            // Nachrichten senden
            match self.msg_manager.distribution_msg.try_recv() {
                Ok(msg) => {
                    let current_client = self.client.clone();
                    let content = msg.content.clone();
                    info!(
                        "Sending message to {} topics",
                        self.config.subbed_topics.len()
                    );

                    for topic in &self.config.subbed_topics {
                        match current_client
                            .publish(topic, QoS::AtLeastOnce, false, content.clone())
                            .await
                        {
                            Ok(_) => {
                                self.status.messages_sent += 1;
                                self.status.last_activity = Some(chrono::Local::now());
                            }
                            Err(e) => {
                                warn!("Error {:?} while sending to topic {}", e, topic);
                                self.status
                                    .error_messages
                                    .push(format!("Send error: {}", e));
                            }
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // Kein Fehler, einfach keine Nachricht verfügbar
                }
                Err(e) => {
                    warn!("Error while receiving messages to send: {:?}", e);
                    self.status
                        .error_messages
                        .push(format!("Receive error: {}", e));
                }
            }

            if let Some(event_handl) = &mut self.event_loop {
                let notification = event_handl.poll().await;

                match notification {
                    Ok(result) => {
                        info!("MQTT event received: {:?}", result);
                        // Hier könnte die Nachricht an die UI weitergeleitet werden

                        match result {
                            Event::Incoming(packet) => match packet {
                                Packet::Publish(rec) => {
                                    let payload = rec.payload;
                                    let topic = rec.topic;

                                    match std::str::from_utf8(&payload) {
                                        Ok(payload) => {
                                            let msg =
                                                MQTTMessage::from_topic(topic, payload.to_string());
                                            let response =
                                                self.msg_manager.received_msg.try_send(msg);
                                            if let Err(e) = response {
                                                error!("Failed to send MQTT-Msg to UI: {:?}", e);
                                            }
                                        }
                                        Err(e) => {
                                            error!("No valid payload: {:?}", e);
                                        }
                                    }
                                }
                                _ => {}
                            },
                            Event::Outgoing(out) => {}
                        }

                        self.status.messages_received += 1;
                        self.status.last_activity = Some(chrono::Local::now());
                    }
                    Err(e) => {
                        warn!("Error receiving MQTT event: {:?}", e);
                        self.status
                            .error_messages
                            .push(format!("MQTT event error: {}", e));
                    }
                }
            }

            // Konfigurationsänderungen prüfen mit der konfigurierten Frequenz
            if last_check.elapsed() >= poll_interval {
                if self.config != *self.ui_config.borrow() {
                    info!("Configuration change detected, reconfiguring");
                    break;
                }
                last_check = std::time::Instant::now();
            }

            // Kurze Pause, um CPU-Last zu reduzieren
            std::thread::sleep(Duration::from_millis(10));
        }

        info!("MQTT connection returning to configured state");
        self.transition()
    }
}

pub struct MQTTHandle {
    pub active: bool,
}

impl MQTTHandle {
    pub async fn start_connection(
        &mut self,
        msg_in: mpsc::Receiver<MQTTMessage>,
        msg_out: mpsc::Sender<MQTTMessage>,
        ui_config: watch::Receiver<config::MqttConfig>,
        activation_state: watch::Receiver<bool>,
    ) {
        info!("Starting MQTT connection state machine");

        // Initialisieren und konfigurieren der Verbindung
        let connection = MQTTConnection::create(msg_in, msg_out, ui_config).await;
        let mut connection = connection.configure().await;

        // Hauptschleife für Zustandsübergänge - hier ist der kritische Teil
        loop {
            self.active = *activation_state.borrow();
            if self.active {
                // Von Configured zu Processing
                let processing_connection = connection.activate().await;
                // Von Processing zurück zu Configured
                connection = processing_connection.run().await;
            } else {
                thread::sleep(Duration::from_secs(1));
            }

            // Diese Schleife läuft endlos weiter und behält die Ownership bei jedem Durchlauf
        }
    }
}
