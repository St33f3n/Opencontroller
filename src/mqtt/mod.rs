//! # MQTT Integration Module
//!
//! Provides complete MQTT client functionality for OpenController's Smart Home and IoT
//! debugging capabilities. This module implements a robust, state-machine-driven MQTT
//! client with dynamic configuration, bidirectional message routing, and UI integration.
//!
//! ## Why This Module Exists
//!
//! MQTT is a central protocol in Smart Home and IoT ecosystems. This module enables
//! OpenController to serve as a powerful debugging and monitoring tool by providing:
//! - Live MQTT broker connections with dynamic configuration
//! - Real-time message monitoring and publishing capabilities  
//! - Topic subscription management through the UI
//! - Message history and persistence for debugging workflows
//!
//! ## Module Architecture
//!
//! The MQTT system is organized into three focused submodules:
//!
//! ```text
//! mqtt/
//! ├── config.rs           - Configuration structures and defaults
//! ├── message_manager.rs  - Message representation and routing
//! └── mqtt_handler.rs     - Connection state machine and protocol handling
//! ```
//!
//! ## Design Philosophy
//!
//! - **Separation of Concerns**: Configuration, message handling, and connection logic
//!   are cleanly separated for maintainability and testing
//! - **UI-Driven Operation**: All functionality is accessible through the UI without
//!   requiring configuration file editing or command-line interaction
//! - **Robust Connection Management**: State machine ensures reliable connection
//!   lifecycle with automatic recovery and configuration updates
//! - **Developer-Friendly**: Designed for debugging and monitoring use cases rather
//!   than production message routing
//!
//! ## Integration with OpenController
//!
//! The MQTT module integrates with the broader OpenController architecture through:
//! - **Configuration Portal**: Persistent storage of MQTT settings and message history
//! - **UI System**: Real-time configuration changes and message display
//! - **Session Management**: Automatic saving of connections and subscriptions
//! - **Channel Architecture**: Clean separation from other controller modules
//!
//! ## Usage Examples
//!
//! **Debugging IoT Devices:**
//! 1. Connect to device's MQTT broker through UI
//! 2. Subscribe to device telemetry topics
//! 3. Monitor real-time sensor data and system messages
//! 4. Send test commands and observe device responses
//!
//! **Smart Home Monitoring:**
//! 1. Connect to home automation MQTT broker
//! 2. Subscribe to device status and sensor topics
//! 3. Monitor system health and automation triggers
//! 4. Test automation scenarios with manual message publishing

pub mod config;
pub mod message_manager;
pub mod mqtt_handler;
