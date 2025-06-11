//! # MQTT Configuration Management
//!
//! Defines configuration structures for MQTT broker connections, topic subscriptions,
//! and polling behavior. This module provides the data structures used throughout
//! the MQTT system for connection management and message routing.
//!
//! ## Why This Module Exists
//!
//! Centralizes all MQTT-related configuration in a single, serializable structure
//! that can be persisted across application sessions. This enables users to save
//! their broker connections, topic subscriptions, and preferences without manual
//! reconfiguration on each startup.
//!
//! ## Design Rationale
//!
//! - **Separation of Concerns**: Configuration is separate from connection logic
//! - **UI Integration**: Directly compatible with UI configuration forms
//! - **Persistence**: Serializable for session management and user preferences
//! - **Flexibility**: Supports multiple servers and dynamic topic management
//!
//! ## Configuration Strategy
//!
//! The configuration supports a multi-server environment where users can:
//! - Define multiple MQTT brokers for different use cases
//! - Switch between servers without losing topic subscriptions
//! - Maintain separate topic lists per server configuration
//! - Adjust polling frequency based on use case requirements

use crate::ui::{self, MQTTServer};
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};

/// Central configuration for all MQTT connection and subscription settings.
///
/// ## Design Rationale
/// Combines broker connection details with topic management and polling behavior
/// in a single structure for atomic configuration updates and simplified persistence.
/// The configuration is UI-driven, allowing users to modify all settings through
/// the interface without requiring configuration file editing.
///
/// ## Usage Context
/// This configuration is:
/// - Created and modified through the UI MQTT menu
/// - Persisted as part of user sessions
/// - Used by the MQTT connection state machine for broker communication
/// - Synchronized between UI changes and backend connection logic
///
/// ## State Synchronization
/// Changes to this configuration trigger MQTT connection updates through
/// the state machine's configuration polling mechanism, ensuring real-time
/// response to user preference changes.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct MqttConfig {
    /// All available topics that can be subscribed to.
    ///
    /// Maintained as a master list of known topics across all servers.
    /// Users can add custom topics through the UI, and this list persists
    /// across sessions for easy reselection without retyping.
    pub available_topics: Vec<String>,

    /// Currently subscribed topics for active message reception.
    ///
    /// Subset of available_topics that determines which messages the
    /// application will receive and display. Changes to this list trigger
    /// subscribe/unsubscribe operations on the active MQTT connection.
    pub subbed_topics: Vec<String>,

    /// Active MQTT server configuration for current connection.
    ///
    /// Contains broker URL, credentials, and connection status. This is the
    /// server currently being used for MQTT communication, selected from
    /// the available_servers list through the UI.
    pub server: ui::common::MQTTServer,

    /// List of configured MQTT servers for easy switching.
    ///
    /// Allows users to maintain multiple broker configurations (e.g.,
    /// development, production, local test brokers) and switch between
    /// them without re-entering connection details.
    pub available_servers: Vec<ui::common::MQTTServer>,

    /// Polling frequency for configuration changes (Hz).
    ///
    /// Determines how often the MQTT state machine checks for configuration
    /// updates from the UI. Higher values provide more responsive UI updates
    /// but increase CPU usage. Lower values reduce overhead but may delay
    /// response to user changes.
    ///
    /// ## Performance Considerations
    /// - **High frequency (>20Hz)**: Responsive UI, higher CPU usage
    /// - **Medium frequency (5-15Hz)**: Good balance for most use cases  
    /// - **Low frequency (<5Hz)**: Minimal overhead, acceptable for background monitoring
    pub poll_frequency: usize,
}

impl Default for MqttConfig {
    /// Creates a minimal default MQTT configuration for initial setup.
    ///
    /// ## Default Values Rationale
    /// - **Empty lists**: No assumptions about user's MQTT environment
    /// - **Default server**: Uses MQTTServer::default() for consistent empty state
    /// - **10Hz polling**: Balances responsiveness with CPU efficiency
    ///
    /// ## Usage Context
    /// Used when:
    /// - Application starts for the first time (no saved configuration)
    /// - Configuration loading fails (fallback to safe defaults)
    /// - User explicitly resets MQTT settings to defaults
    ///
    /// ## Design Philosophy
    /// Provides a safe, empty configuration that won't attempt unwanted
    /// connections while allowing users to gradually build their setup
    /// through the UI without configuration file manipulation.
    fn default() -> Self {
        Self {
            // 10Hz provides good responsiveness for UI changes without excessive overhead
            poll_frequency: 10,

            // Start with empty topic lists - user will add as needed
            available_topics: Vec::new(),
            subbed_topics: Vec::new(),

            // No default server - prevents unintended connections
            server: MQTTServer::default(),
            available_servers: Vec::new(),
        }
    }
}
