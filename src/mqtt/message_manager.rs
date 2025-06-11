//! # MQTT Message Management
//!
//! Provides data structures and utilities for MQTT message handling, including
//! message representation, formatting, and channel-based message routing between
//! the MQTT connection and UI components.
//!
//! ## Why This Module Exists
//!
//! Centralizes MQTT message representation and provides a clean abstraction layer
//! between raw MQTT protocol data and the application's message handling logic.
//! The module handles message formatting, timestamping, and channel routing for
//! seamless integration between MQTT backend and UI display.
//!
//! ## Design Rationale
//!
//! - **Simple Message Model**: Focus on essential data (topic, content, timestamp)
//! - **UI-Friendly Formatting**: Multiple display formats for different UI contexts
//! - **Channel-Based Routing**: Clean separation between message reception and distribution
//! - **Serializable State**: Messages can be persisted for session management
//!
//! ## Message Flow Architecture
//!
//! Messages flow through the system via channel-based routing:
//! 1. **Reception**: MQTT handler receives messages from broker
//! 2. **Distribution**: Messages are sent to UI via `received_msg` sender
//! 3. **Transmission**: UI sends messages via `distribution_msg` receiver
//! 4. **Display**: UI formats messages using built-in display methods

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::fmt;
use tokio::sync::mpsc;

/// Represents a single MQTT message with metadata for application processing.
///
/// ## Design Rationale
/// Combines the essential MQTT message data (topic + payload) with application
/// metadata (timestamp) in a single, serializable structure. This enables
/// message persistence, UI display, and debugging without requiring complex
/// data transformations.
///
/// ## Timestamp Strategy
/// Uses `NaiveDateTime` instead of timezone-aware DateTime to simplify
/// serialization and avoid timezone complexity in UI display. Since messages
/// are typically displayed in local context, the timezone information is
/// usually not critical for user understanding.
///
/// ## Serialization Support
/// Implements Serde traits for:
/// - Session persistence (saving message history)
/// - Configuration export/import
/// - Debug logging and analysis
#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MQTTMessage {
    /// MQTT topic the message was published to.
    ///
    /// Used for filtering, routing, and display organization in the UI.
    /// Follows MQTT topic conventions (hierarchical, slash-separated).
    pub topic: String,

    /// Message payload content as UTF-8 string.
    ///
    /// Assumes text-based content for UI display. Binary payloads should
    /// be converted to appropriate string representation before creating
    /// MQTTMessage instances.
    pub content: String,

    /// When the message was received or created by the application.
    ///
    /// Set automatically during message creation. Used for chronological
    /// ordering in UI display and message history management.
    pub timestamp: NaiveDateTime,
}

impl fmt::Display for MQTTMessage {
    /// Provides a compact preview format for message list display.
    ///
    /// ## Display Strategy
    /// Shows timestamp followed by content preview for quick message scanning
    /// in UI lists. The content is truncated to the first 10 characters
    /// to provide context while maintaining compact display.
    ///
    /// ## Content Preview Logic
    /// - **Short content (≤10 chars)**: Shows full content
    /// - **Long content (>10 chars)**: Shows first 11 characters (positions 0-10)
    ///
    /// This provides a meaningful preview of message content while keeping
    /// list entries compact and scannable in the UI.
    ///
    /// ## Output Format
    /// ```text
    /// 2023-12-01 14:30:25 - Hello World
    /// 2023-12-01 14:30:26 - This is a l... (truncated)
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let preview = if self.content.len() > 10 {
            &self.content[..=10] // First 11 characters (0-10 inclusive)
        } else {
            &self.content // Use entire string if short
        };
        write!(f, "{} - {}", self.timestamp, preview)
    }
}

impl MQTTMessage {
    /// Creates a new MQTT message with automatic timestamping.
    ///
    /// ## Timestamp Behavior
    /// Uses local system time at creation moment. This means the timestamp
    /// represents when the application processed the message, not necessarily
    /// when it was originally published to the MQTT broker.
    ///
    /// ## Usage Context
    /// Used for:
    /// - Creating messages from received MQTT data
    /// - Creating messages for UI transmission to MQTT broker
    /// - Generating test messages for development
    ///
    /// # Examples
    /// ```rust
    /// let msg = MQTTMessage::from_topic(
    ///     "sensors/temperature".to_string(),
    ///     "23.5".to_string()
    /// );
    /// ```
    pub fn from_topic(topic: String, content: String) -> Self {
        MQTTMessage {
            topic,
            content,
            timestamp: chrono::Local::now().naive_local(),
        }
    }

    /// Renders message in detailed format for full message display.
    ///
    /// Provides complete message information including timestamp, topic,
    /// and full content. Used in contexts where full message details
    /// are needed (message details view, logging, export).
    ///
    /// ## Output Format
    /// ```text
    /// 2023-12-01 14:30:25: sensors/temperature
    /// 23.5
    /// ```
    ///
    /// ## Design Choice
    /// Separates topic and content with newline for improved readability
    /// when displaying full message details, especially for longer topics
    /// or multi-line content.
    pub fn render(&self) -> String {
        format!("{}: {}\n{}", self.timestamp, self.topic, self.content)
    }
}

/// Channel-based message router for MQTT message flow management.
///
/// ## Design Rationale
/// Provides a simple, directional message routing system using Tokio channels
/// to separate message reception (from MQTT broker) and message distribution
/// (to UI and other consumers). This creates clear data flow boundaries and
/// enables easy testing and debugging.
///
/// ## Channel Architecture
/// - **received_msg**: Sender for messages received from MQTT broker
/// - **distribution_msg**: Receiver for messages to be sent to MQTT broker
///
/// ## Usage Pattern
/// ```rust
/// // MQTT handler sends received messages
/// manager.received_msg.send(message).await?;
///
/// // MQTT handler receives outgoing messages
/// if let Some(message) = manager.distribution_msg.recv().await {
///     // Send to MQTT broker
/// }
/// ```
///
/// ## Thread Safety
/// Uses Tokio MPSC channels which are designed for cross-thread communication,
/// allowing the MQTT handler to run in a separate async task while safely
/// communicating with UI components.
pub struct MsgManager {
    /// Channel sender for messages received from MQTT broker.
    ///
    /// MQTT connection handler uses this to forward incoming messages
    /// to UI and other message consumers. Multiple receivers can be
    /// created from the corresponding receiver for message distribution.
    pub received_msg: mpsc::Sender<MQTTMessage>,

    /// Channel receiver for messages to be published to MQTT broker.
    ///
    /// MQTT connection handler polls this receiver to get messages
    /// that should be published to the broker. UI components use
    /// the corresponding sender to queue outgoing messages.
    pub distribution_msg: mpsc::Receiver<MQTTMessage>,
}
