//! # Persistence Module
//!
//! ## Why This Module Exists
//! The persistence module provides the foundation for OpenController's configuration management system.
//! It defines the data models and infrastructure needed to save, load, and manage application
//! configurations across sessions. This enables users to maintain multiple independent setups
//! for different use cases (e.g., different Smart Home environments, RC vehicle configurations,
//! or MQTT debugging scenarios).
//!
//! ## Key Abstractions
//! - **Session-Based Configuration**: Each session contains a complete snapshot of application state
//! - **Modular Configuration Types**: Different aspects (UI, Controller, Connections) are separated
//!   for independent management and easier debugging
//! - **Type-Safe Serialization**: All configuration uses strongly-typed structs with serde
//! - **Hierarchical Organization**: Configuration is organized from general (Theme) to specific (MappingConfig)
//!
//! ## Error Handling Strategy
//! Uses `color_eyre` for rich error context in file operations and configuration validation.
//! Each configuration type provides sensible defaults to ensure the application can function
//! even with missing or corrupted configuration files.
//!
//! ## Design Philosophy
//! The module follows a "fail-safe" approach where missing configuration gracefully degrades
//! to defaults rather than preventing application startup. This ensures OpenController remains
//! usable even in degraded scenarios.

pub mod config_portal;
pub mod persistence_worker;
pub mod session_client;

use crate::mapping::{elrs::ELRSConfig, keyboard::KeyboardConfig};
use crate::mqtt::{config::MqttConfig, message_manager::MQTTMessage};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Defines color scheme and visual styling for the application UI.
///
/// ## Design Rationale
/// Uses RGB tuples instead of a color library to minimize dependencies and ensure
/// serialization compatibility. All colors are stored as u8 values (0-255) for
/// direct compatibility with egui's Color32 type.
///
/// ## Usage Context
/// Applied by the UI system to customize the visual appearance. Supports the
/// project's goal of providing a configurable interface that can be adapted
/// to different environments and user preferences.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Theme {
    /// Border color for UI elements (frames, separators)
    border_color: (u8, u8, u8),
    /// Primary background color for main UI areas
    background_color_one: (u8, u8, u8),
    /// Secondary background color for nested elements
    background_color_two: (u8, u8, u8),
    /// Tertiary background color for deepest UI elements
    background_color_three: (u8, u8, u8),
    /// Primary text color
    text_color: (u8, u8, u8),
    /// Primary highlight color for active elements
    highlight_color: (u8, u8, u8),
    /// Secondary highlight color for alternate states
    highlight_color_two: (u8, u8, u8),
    /// Primary frame color
    frame_color: (u8, u8, u8),
    /// Secondary frame color for nested frames
    frame_color_two: (u8, u8, u8),
}

/// Contains UI-specific configuration including theming and performance settings.
///
/// ## Design Rationale
/// Separates visual configuration (theme) from performance configuration (fps)
/// to allow independent management. The FPS setting enables optimization for
/// different hardware capabilities, particularly important for Raspberry Pi deployment.
///
/// ## Usage Context
/// Loaded by the UI system during initialization and applied to the egui context.
/// Changes require UI restart to take effect, which is handled by the session
/// switching mechanism.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct UIConfig {
    /// Visual styling configuration
    theme: Theme,
    /// Target frames per second for UI rendering
    fps: u8,
}

/// Network configuration for wireless connectivity management.
///
/// ## Design Rationale
/// Designed to support future wireless management features. Currently minimal
/// but structured for expansion to include multiple network profiles and
/// connection state management.
///
/// ## Usage Context
/// Intended for future integration with system network management, particularly
/// for Raspberry Pi deployments where WiFi configuration might be managed
/// through the OpenController interface.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct NetworkConfig {
    /// Current network connection settings
    pub network: NetworkConnection,
    /// Connection state description
    pub state: String,
}

/// Represents a single network connection with credentials.
///
/// Basic network connection information including SSID and password.
/// Designed for future expansion to support different authentication methods.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct NetworkConnection {
    /// Network SSID (Service Set Identifier)
    pub network: String,
    /// Network password/key
    pub key: String,
}

/// Configuration for external communication protocols and services.
///
/// ## Design Rationale
/// Currently focused on MQTT but designed to accommodate additional protocols
/// (433MHz, 866MHz, LoRa, etc.) as the project expands toward its FlipperZero-like
/// vision. The structure allows for protocol-specific configuration without
/// breaking existing sessions.
///
/// ## Usage Context
/// Used by communication modules to establish connections and configure
/// protocol-specific behavior. Changes take effect when the respective
/// communication module is restarted or reconfigured.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ConnectionConfig {
    /// MQTT broker and topic configuration
    pub mqtt_config: MqttConfig,
}

/// Configuration for controller input mapping strategies.
///
/// ## Design Rationale
/// Separates different mapping types to allow independent configuration and
/// future expansion. Each mapping type can be enabled/disabled independently,
/// supporting the project's goal of flexible, multi-protocol controller support.
///
/// ## Usage Context
/// Applied by the mapping engine system to determine how controller inputs
/// are translated to output events. Changes require mapping engine restart,
/// which is handled through the session switching mechanism.
///
/// ## Extension Points
/// Future mapping types (433MHz, LoRa, etc.) can be added as additional fields
/// without breaking existing configurations.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ControllerConfig {
    /// Keyboard input mapping configuration
    pub keyboard_mapping: KeyboardConfig,
    /// ELRS (ExpressLRS) drone control mapping configuration
    pub elrs_mapping: ELRSConfig,
}

/// Container for user-saved MQTT messages for reuse and debugging.
///
/// ## Design Rationale
/// Simple wrapper around a vector to allow for future expansion (metadata,
/// categorization, etc.) without breaking the serialization format.
/// Supports the MQTT debugging use case by allowing users to save and
/// replay commonly used messages.
///
/// ## Usage Context
/// Managed by the MQTT UI components to provide message history and quick
/// access to frequently used MQTT payloads during debugging sessions.
#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct SavedMessages {
    /// Collection of saved MQTT messages
    pub msg: Vec<MQTTMessage>,
}

/// Metadata and state information for a configuration session.
///
/// ## Design Rationale
/// Contains session management information separate from actual configuration
/// data to enable efficient session listing and switching without loading
/// full configuration data. The session registry (available_sessions) enables
/// quick discovery of existing sessions.
///
/// ## Usage Context
/// Used by the session management system to track session metadata and
/// provide session discovery functionality. Updated whenever sessions
/// are created, loaded, or deleted.
///
/// ## Performance Notes
/// The available_sessions HashMap enables O(1) session lookup and avoids
/// filesystem scanning during normal operations. Only updated during
/// session management operations.
#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct SessionConfig {
    /// Name of the current session
    pub session_name: String,
    /// Previously active session for fallback scenarios
    pub last_session: Option<String>,
    /// Filesystem path to the session directory
    pub path: PathBuf,
    /// Registry of all known sessions and their paths
    pub available_sessions: HashMap<String, PathBuf>,
}
