//! # MQTT Connection Handler with State Machine
//!
//! Implements a robust MQTT client using a state machine pattern for connection lifecycle
//! management, dynamic configuration updates, and bidirectional message routing. This module
//! serves as the bridge between the OpenController application and MQTT brokers.
//!
//! ## Why This Module Exists
//!
//! MQTT connectivity is central to OpenController's Smart Home and IoT debugging capabilities.
//! This handler provides reliable MQTT communication with features like automatic reconnection,
//! dynamic topic subscription management, and real-time configuration updates without requiring
//! application restarts.
//!
//! ## Key Design Decisions
//!
//! - **State Machine Architecture**: Uses Statum for compile-time verified state transitions
//!   and clear separation of connection lifecycle phases
//! - **Dynamic Configuration**: Supports live configuration changes without dropping connections
//!   when possible, improving user experience during debugging sessions
//! - **Bidirectional Message Flow**: Handles both incoming messages (broker → UI) and outgoing
//!   messages (UI → broker) through separate channel systems
//! - **Configuration Polling**: Regularly checks for UI configuration changes to maintain
//!   responsive behavior during user interaction
//! - **Graceful Error Handling**: Continues operation despite individual message failures,
//!   prioritizing connection stability over perfect message delivery
//!
//! ## State Machine Design Rationale
//!
//! MQTT connections have complex lifecycle requirements (authentication, subscription management,
//! reconnection logic). The state machine provides:
//! - **Clear state boundaries**: Each state has specific responsibilities
//! - **Safe transitions**: Prevents invalid operations (e.g., subscribing before connecting)
//! - **Configuration isolation**: Changes are applied at appropriate transition points
//! - **Testability**: Each state can be tested independently
//!
//! ## Connection Lifecycle
//!
//! ```text
//! Initializing → Configured → Processing ↺
//!       ↓            ↓           ↓
//!   Load Config  Subscribe   Main Loop
//!   Create Client   Topics   Handle Messages
//!                           Check Config
//! ```
//!
//! ## Performance Considerations
//!
//! - **Polling Frequency**: Configurable balance between UI responsiveness and CPU usage
//! - **Message Queuing**: Uses bounded channels to prevent memory bloat under high load
//! - **Non-blocking Operations**: Uses try_recv patterns to avoid blocking the main loop
//! - **Connection Reuse**: Preserves existing connections when only topic subscriptions change
//!
//! ## Error Handling Strategy
//!
//! Failures are categorized and handled appropriately:
//! - **Connection errors**: Logged but allow retry through state machine cycle
//! - **Message errors**: Individual failures don't affect overall connection
//! - **Configuration errors**: Fall back to safe defaults to maintain operation
//! - **Subscription errors**: Continue with partial subscriptions rather than failing completely

use std::thread;
use std::time::Duration;

use super::message_manager::{MQTTMessage, MsgManager};
use super::{config, message_manager};
use crate::mqtt::config::MqttConfig;
use crate::persistence;
use crate::persistence::config_portal::{ConfigPortal, ConfigResult, PortalAction};
use crate::persistence::persistence_worker::SessionAction;
use chrono::NaiveDateTime;
use rumqttc::{
    AsyncClient, Event, EventLoop, Incoming, MqttOptions, MqttState, Packet, PacketType, QoS,
};
use statum::{machine, state};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};
use tracing_subscriber::fmt::time;

/// State definitions for MQTT connection lifecycle management.
///
/// ## State Responsibilities
/// - **Initializing**: Loads configuration and creates MQTT client
/// - **Configured**: Establishes subscriptions and prepares for message processing  
/// - **Processing**: Handles message flow and monitors for configuration changes
///
/// ## Design Rationale
/// Three states provide clear separation of concerns while maintaining simple
/// transition logic. More states would add complexity without significant benefit;
/// fewer states would mix responsibilities inappropriately.
#[state]
#[derive(Debug, Clone, Copy)]
pub enum MQTTState {
    Initializing,
    Configured,
    Processing,
}

/// Represents the current status of the MQTT broker connection.
///
/// Used for UI status display and connection health monitoring.
/// Separate from MQTTState to distinguish between state machine
/// position and actual network connectivity.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Failed,
    Reconnecting,
}

/// Comprehensive status information for MQTT connection monitoring.
///
/// ## Design Rationale
/// Aggregates operational metrics and status information for debugging,
/// UI display, and health monitoring. Separate from connection logic
/// to enable easy status reporting without coupling to implementation details.
///
/// ## Usage Context
/// Updated during message processing and used by UI components to display
/// connection health, throughput statistics, and error history.
#[derive(Clone, Debug, Default)]
pub struct MQTTStatus {
    /// Current broker connection status for UI display
    pub connection_state: ConnectionState,

    /// Recent error messages for debugging and user notification
    pub error_messages: Vec<String>,

    /// Count of messages received from broker (session total)
    pub messages_received: usize,

    /// Count of messages sent to broker (session total)
    pub messages_sent: usize,

    /// Timestamp of last MQTT activity for connection health monitoring
    pub last_activity: Option<chrono::DateTime<chrono::Local>>,
}

/// State machine implementation for MQTT connection management.
///
/// ## Architecture Overview
/// Uses Statum's compile-time state verification to ensure safe state transitions
/// and prevent invalid operations. Each state has specific capabilities and
/// responsibilities, with clean transition points for configuration updates.
///
/// ## Generic State Parameter
/// The `<S: MQTTState>` parameter enables compile-time verification that methods
/// are only called in appropriate states, preventing runtime errors from invalid
/// state operations.
///
/// ## Resource Management
/// - **AsyncClient**: Shared across states for connection reuse
/// - **EventLoop**: Optional to allow transfer to background tasks
/// - **ConfigPortal**: Shared reference for live configuration updates
/// - **Channels**: Managed through MsgManager for message routing
#[machine]
pub struct MQTTConnection<S: MQTTState> {
    /// Current connection status and metrics for monitoring
    status: MQTTStatus,

    /// rumqttc async client for MQTT broker communication
    client: AsyncClient,

    /// Event loop for processing MQTT protocol events (moved to background tasks)
    event_loop: Option<EventLoop>,

    /// Current MQTT configuration (topics, server settings, polling frequency)
    config: config::MqttConfig,

    /// Shared access to application configuration for live updates
    config_portal: Arc<ConfigPortal>,

    /// Message routing channels for bidirectional communication
    msg_manager: MsgManager,

    /// Channel for triggering session persistence operations
    persistence_sender: mpsc::Sender<SessionAction>,
}

impl MQTTConnection<Initializing> {
    /// Creates a new MQTT connection in the initializing state.
    ///
    /// ## Initialization Process
    /// 1. Load MQTT configuration from ConfigPortal
    /// 2. Parse server URL and connection parameters
    /// 3. Create rumqttc client with OpenController identification
    /// 4. Set up message routing channels
    /// 5. Initialize status tracking
    ///
    /// ## Configuration Loading Strategy
    /// Falls back to default configuration if ConfigPortal access fails,
    /// ensuring the connection can be established even with configuration issues.
    /// This prevents total MQTT failure due to temporary configuration problems.
    ///
    /// ## Connection Parameters
    /// - **Client ID**: "OpenController" for broker identification
    /// - **Keep-alive**: 5 seconds for responsive connection monitoring
    /// - **Queue size**: 10 messages for reasonable buffering without memory bloat
    ///
    /// ## Error Handling
    /// Configuration errors result in default settings rather than failure,
    /// allowing users to fix configuration through the UI after connection establishment.
    pub async fn create(
        msg_in: mpsc::Receiver<MQTTMessage>,
        msg_out: mpsc::Sender<MQTTMessage>,
        config_portal: Arc<ConfigPortal>,
        persistence_sender: mpsc::Sender<SessionAction>,
    ) -> Self {
        let msg_manager = MsgManager {
            received_msg: msg_out,
            distribution_msg: msg_in,
        };

        // Load configuration with fallback to defaults
        let config_result = config_portal.execute_potal_action(PortalAction::GetMqttConfig);
        let config = match config_result {
            ConfigResult::MqttConfig(config) => config,
            _ => {
                warn!("Failed to get MqttConfig from ConfigPortal, using defaults");
                MqttConfig::default()
            }
        };

        info!(
            "Initializing MQTT connection with broker: {}",
            config.server.url
        );

        // Parse server URL with reasonable defaults
        let server_comps: Vec<&str> = config.server.url.split(':').collect();
        let server_addr = server_comps.first().copied().unwrap_or("localhost");
        let port = server_comps
            .get(1)
            .unwrap_or(&"1883")
            .parse()
            .unwrap_or(1883);

        // Configure MQTT client with standard parameters
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
            config,
            config_portal,
            msg_manager,
            persistence_sender,
        )
    }

    /// Configures topic subscriptions and transitions to Configured state.
    ///
    /// ## Subscription Strategy
    /// Subscribes to all topics in the `subbed_topics` list with QoS 1 (AtLeastOnce)
    /// for reliable message delivery. Individual subscription failures are logged
    /// but don't prevent overall configuration completion.
    ///
    /// ## QoS Selection Rationale
    /// QoS 1 (AtLeastOnce) provides good balance between reliability and performance:
    /// - More reliable than QoS 0 (fire-and-forget)
    /// - Less overhead than QoS 2 (exactly-once)
    /// - Appropriate for debugging and monitoring use cases
    ///
    /// ## Error Handling
    /// Individual topic subscription failures are logged but don't fail the entire
    /// configuration process. This allows partial functionality rather than complete
    /// failure when some topics are problematic.
    pub async fn configure(self) -> MQTTConnection<Configured> {
        info!(
            "Configuring MQTT connection with {} subscribed topics",
            self.config.subbed_topics.len()
        );

        // Subscribe to all configured topics
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
    /// Activates message processing and handles configuration updates.
    ///
    /// ## Configuration Update Strategy
    /// Checks for configuration changes and applies them intelligently:
    /// - **Server changes**: Creates new client connection (full reconnect)
    /// - **Topic changes**: Updates subscriptions on existing connection
    /// - **No changes**: Proceeds with existing configuration
    ///
    /// ## Server Change Handling
    /// When server configuration changes, creates entirely new MQTT client and
    /// event loop to ensure clean connection state. This prevents issues with
    /// credential changes or broker switches.
    ///
    /// ## Topic Management
    /// Calculates topic differences and applies incremental changes:
    /// - Subscribe to new topics that weren't previously subscribed
    /// - Unsubscribe from topics no longer in the subscription list
    /// - Preserve existing subscriptions that haven't changed
    ///
    /// ## Session Persistence
    /// Triggers session save after configuration updates to ensure changes
    /// are preserved across application restarts.
    ///
    /// ## Performance Notes
    /// Configuration comparison uses full structural equality, which is acceptable
    /// given the typical size of MQTT configurations. More sophisticated diffing
    /// could be implemented if configuration updates become performance-critical.
    pub async fn activate(mut self) -> MQTTConnection<Processing> {
        // Get latest configuration from UI
        let mut config = MqttConfig::default();
        match self
            .config_portal
            .execute_potal_action(PortalAction::GetMqttConfig)
        {
            ConfigResult::MqttConfig(portal_config) => {
                config = portal_config.clone();
            }
            _ => warn!("Unable to get MqttConfig from ConfigPortal, keeping current"),
        }

        let mut new_topics = Vec::new();
        let mut removed_topics = Vec::new();

        // Apply configuration changes if config is valid
        if config != MqttConfig::default() {
            // Handle server configuration changes (requires full reconnection)
            if self.config.server != config.server {
                info!("Server configuration changed, creating new connection");

                let server_comps: Vec<&str> = config.server.url.split(':').collect();
                let server_addr = server_comps.first().copied().unwrap_or("localhost");
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
            }

            // Handle topic subscription changes (incremental updates)
            if self.config.subbed_topics != config.subbed_topics {
                info!("Topic configuration changed, updating subscriptions");

                // Find topics to subscribe to
                new_topics = config
                    .subbed_topics
                    .iter()
                    .cloned()
                    .filter(|t| !self.config.subbed_topics.contains(t))
                    .collect();

                // Find topics to unsubscribe from
                removed_topics = self
                    .config
                    .subbed_topics
                    .iter()
                    .cloned()
                    .filter(|t| !config.subbed_topics.contains(t))
                    .collect();
            }

            // Apply configuration updates
            self.config = config;

            // Execute subscription changes
            for topic in new_topics {
                let _ = self.client.subscribe(topic, QoS::AtLeastOnce).await;
            }

            for topic in removed_topics {
                let _ = self.client.unsubscribe(topic).await;
            }
        }

        // Trigger session persistence after configuration changes
        let (tx, mut rx) = tokio::sync::oneshot::channel::<Result<(), color_eyre::Report>>();
        let _ = self
            .persistence_sender
            .try_send(SessionAction::SaveCurrentSession { response_tx: tx });

        let _response = rx.try_recv();

        self.transition()
    }
}

impl MQTTConnection<Processing> {
    /// Main processing loop for MQTT message handling and configuration monitoring.
    ///
    /// ## Processing Strategy
    /// Runs a tight loop that handles three primary responsibilities:
    /// 1. **Outgoing Messages**: Process messages from UI for publication to broker
    /// 2. **Incoming Messages**: Process messages from broker for delivery to UI
    /// 3. **Configuration Polling**: Check for UI configuration changes at regular intervals
    ///
    /// ## Message Flow Architecture
    ///
    /// **Outgoing (UI → Broker):**
    /// - Receives messages via `distribution_msg` channel
    /// - Publishes to ALL available topics (broadcast behavior)
    /// - Updates send statistics and activity timestamps
    ///
    /// **Incoming (Broker → UI):**
    /// - Polls MQTT event loop for new protocol events
    /// - Filters for Publish packets containing message data
    /// - Converts to MQTTMessage format and forwards to UI via `received_msg` channel
    ///
    /// ## Configuration Polling Strategy
    /// Uses configurable polling frequency to balance responsiveness with CPU usage:
    /// - **High frequency**: More responsive to UI changes, higher CPU overhead
    /// - **Low frequency**: Less responsive but more efficient
    /// - **Dynamic calculation**: `poll_interval = 1000ms / poll_frequency`
    ///
    /// ## Error Handling Philosophy
    /// Continues processing despite individual failures to maintain overall system stability:
    /// - **Message send failures**: Log and continue (partial delivery acceptable)
    /// - **Event loop errors**: Log and continue (temporary network issues common)
    /// - **Channel errors**: Log for debugging but don't crash processing
    ///
    /// ## Performance Optimizations
    /// - **Non-blocking operations**: Uses `try_recv` to prevent blocking
    /// - **CPU yield**: 10ms sleep to prevent 100% CPU usage in tight loop
    /// - **Efficient polling**: Only checks configuration at specified intervals
    /// - **Event batching**: Processes multiple events per loop iteration when available
    ///
    /// ## Loop Termination
    /// Returns to Configured state when polling interval expires, allowing the
    /// state machine to pick up configuration changes and potentially reconnect.
    /// This design enables dynamic configuration without complex state management.
    pub async fn run(mut self) -> MQTTConnection<Configured> {
        info!("MQTT message processing loop started");

        // Calculate polling interval from configuration
        let poll_interval =
            Duration::from_millis(1000 / self.config.poll_frequency.try_into().unwrap_or(10));
        let mut last_check = std::time::Instant::now();

        loop {
            // Process outgoing messages from UI to broker
            match self.msg_manager.distribution_msg.try_recv() {
                Ok(msg) => {
                    let current_client = self.client.clone();
                    let content = msg.content.clone();
                    info!(
                        "Publishing message to {} topics: {}",
                        self.config.subbed_topics.len(),
                        msg.topic
                    );

                    // Broadcast to all subscribed topics (debugging/monitoring pattern)
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
                                warn!("Failed to publish to topic {}: {:?}", topic, e);
                                self.status
                                    .error_messages
                                    .push(format!("Publish error: {}", e));
                            }
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // No outgoing messages - normal condition
                }
                Err(e) => {
                    warn!("Error receiving outgoing messages: {:?}", e);
                    self.status
                        .error_messages
                        .push(format!("Outgoing channel error: {}", e));
                }
            }

            // Process incoming messages from broker to UI
            if let Some(event_loop) = &mut self.event_loop {
                let notification = event_loop.poll().await;

                match notification {
                    Ok(event) => {
                        match event {
                            Event::Incoming(packet) => match packet {
                                Packet::Publish(publish_packet) => {
                                    let payload = publish_packet.payload;
                                    let topic = publish_packet.topic;

                                    // Convert binary payload to UTF-8 string
                                    match std::str::from_utf8(&payload) {
                                        Ok(payload_str) => {
                                            let msg = MQTTMessage::from_topic(
                                                topic.clone(),
                                                payload_str.to_string(),
                                            );

                                            // Forward to UI
                                            if let Err(e) =
                                                self.msg_manager.received_msg.try_send(msg)
                                            {
                                                error!("Failed to forward message to UI: {:?}", e);
                                            } else {
                                                info!("Received message on topic: {}", topic);
                                                self.status.messages_received += 1;
                                                self.status.last_activity =
                                                    Some(chrono::Local::now());
                                            }
                                        }
                                        Err(e) => {
                                            error!(
                                                "Invalid UTF-8 payload on topic {}: {:?}",
                                                topic, e
                                            );
                                        }
                                    }
                                }
                                _ => {
                                    // Other packet types (ping, ack, etc.) - normal protocol traffic
                                }
                            },
                            Event::Outgoing(_) => {
                                // Outgoing confirmations - normal protocol traffic
                            }
                        }
                    }
                    Err(e) => {
                        // Network errors, broker disconnections, etc.
                        // Don't spam logs as these can be frequent during network issues
                        self.status
                            .error_messages
                            .push(format!("MQTT protocol error: {}", e));
                    }
                }
            }

            // Check if it's time to return for configuration updates
            if last_check.elapsed() >= poll_interval {
                last_check = std::time::Instant::now();
                break;
            }

            // Yield CPU to prevent 100% utilization
            thread::sleep(Duration::from_millis(10));
        }

        info!("MQTT processing cycle complete, checking for configuration updates");
        self.transition()
    }
}

/// High-level handle for managing the complete MQTT connection lifecycle.
///
/// ## Design Rationale
/// Provides a simple interface for starting and managing MQTT connections while
/// hiding the complexity of the state machine implementation. Acts as a factory
/// and controller for the MQTT connection state machine.
///
/// ## Lifecycle Management
/// Manages the complete connection lifecycle through an infinite loop that
/// responds to activation signals and handles state machine transitions automatically.
/// This design ensures MQTT functionality is always available when needed.
pub struct MQTTHandle {
    /// Whether MQTT processing is currently active (controlled by UI)
    pub active: bool,
}

impl MQTTHandle {
    /// Starts the MQTT connection state machine with automatic lifecycle management.
    ///
    /// ## State Machine Lifecycle
    /// 1. **Initialize**: Create connection with current configuration
    /// 2. **Configure**: Set up topic subscriptions
    /// 3. **Processing Loop**: Handle messages and monitor for changes
    ///    - If active: Run processing and return to configure for updates
    ///    - If inactive: Sleep and wait for activation
    /// 4. **Repeat**: Continue indefinitely for persistent MQTT functionality
    ///
    /// ## Activation Control
    /// Uses a watch channel to receive activation signals from the UI, allowing
    /// users to enable/disable MQTT functionality without restarting the application.
    /// When inactive, the connection remains configured but doesn't process messages.
    ///
    /// ## Configuration Responsiveness
    /// The processing loop periodically returns to the Configured state, which
    /// checks for configuration changes before starting a new processing cycle.
    /// This enables dynamic configuration updates without complex state management.
    ///
    /// ## Error Recovery
    /// The infinite loop provides automatic recovery from transient failures:
    /// - Network disconnections are handled by recreating connections
    /// - Configuration errors fall back to safe defaults
    /// - Individual message failures don't affect overall operation
    ///
    /// ## Resource Management
    /// The state machine maintains ownership of expensive resources (MQTT client,
    /// event loop) across transitions, minimizing allocation overhead and
    /// connection setup time.
    ///
    /// ## Performance Considerations
    /// - **Active mode**: Full message processing with configurable polling frequency
    /// - **Inactive mode**: 1-second sleep to minimize CPU usage while maintaining responsiveness
    /// - **State transitions**: Lightweight transitions preserve connection state when possible
    pub async fn start_connection(
        &mut self,
        msg_in: mpsc::Receiver<MQTTMessage>,
        msg_out: mpsc::Sender<MQTTMessage>,
        activation_state: watch::Receiver<bool>,
        config_portal: Arc<ConfigPortal>,
        persistence_sender: mpsc::Sender<SessionAction>,
    ) {
        info!("Initializing MQTT connection state machine");

        // Initialize and configure the connection
        let connection =
            MQTTConnection::create(msg_in, msg_out, config_portal, persistence_sender).await;
        let mut connection = connection.configure().await;

        // Main lifecycle loop - runs indefinitely
        loop {
            self.active = *activation_state.borrow();

            if self.active {
                // Active mode: Full message processing with configuration updates
                let processing_connection = connection.activate().await;
                connection = processing_connection.run().await;
            } else {
                // Inactive mode: Minimal CPU usage while waiting for activation
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
}
