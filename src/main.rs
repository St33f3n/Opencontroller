//! # OpenController
//!
//! A comprehensive gamepad-based control system for Smart Home and maker applications.
//!
//! OpenController transforms gamepad input into various output formats (keyboard events,
//! ELRS RC packets, MQTT messages) through a modular mapping system. The application
//! features a multi-threaded architecture with separate threads for input collection,
//! processing, mapping, MQTT communication, and UI rendering.
//!
//! ## Core Features
//!
//! - **Multi-threaded processing**: 8 specialized threads for optimal responsiveness
//! - **Configurable mapping**: Transform gamepad input to keyboard, ELRS, or custom formats
//! - **Session management**: Save/load different configuration profiles
//! - **MQTT integration**: Debug and control MQTT-based Smart Home systems
//! - **ELRS support**: Control RC vehicles through ExpressLRS protocol
//! - **Gamepad-centric UI**: Full application control via gamepad input
//!

pub mod controller;
pub mod mapping;
pub mod mqtt;
pub mod persistence;
pub mod ui;

use crate::controller::controller_handle::{ControllerHandle, ControllerSettings};
use crate::mapping::{keyboard::KeyboardConfig, MappingEngineManager};
use crate::persistence::config_portal::ConfigPortal;
use crate::persistence::persistence_worker::PersistenceManager;
use crate::ui::OpencontrollerUI;
use color_eyre::{eyre::eyre, Result};
use eframe::egui;
use mqtt::config::MqttConfig;
use mqtt::mqtt_handler::MQTTHandle;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;
use ui::MQTTServer;

/// Application entry point and system initialization
///
/// Initializes all subsystems in the correct order and establishes communication
/// channels between components. The application runs until the UI is closed.
///
/// # Architecture Initialization
///
/// 1. **Setup Phase**: Logging, error handling, and environment configuration
/// 2. **Persistence Layer**: Session management and configuration storage
/// 3. **Controller Subsystem**: Gamepad input collection and processing
/// 4. **Communication Channels**: Inter-thread message passing setup
/// 5. **Background Services**: MQTT handler and mapping engine manager
/// 6. **UI Launch**: Fullscreen egui application with gamepad control
///
/// # Threading Model
///
/// The application spawns multiple concurrent tasks:
/// - Controller collection and processing (2 threads)
/// - Mapping engines (variable, currently 1 thread)
/// - MQTT communication (1 thread)
/// - Persistence management with autosave (2 threads)
/// - UI rendering (main thread)
///
/// # Error Handling
///
/// Uses `color_eyre` for enhanced error reporting and `tracing` for structured logging.
/// Critical initialization failures will terminate the application with detailed error context.
///
/// # Examples
///
/// ```bash
/// # Run with default settings
/// cargo run
///
/// # Run with debug logging
/// RUST_LOG=debug cargo run
///
/// # Run with custom log level
/// RUST_LOG=opencontroller=trace cargo run
/// ```
///
/// # Panics
///
/// May panic during initialization if:
/// - Required system resources are unavailable
/// - UI framework fails to initialize
/// - Critical configuration errors occur
#[tokio::main]
async fn main() -> Result<()> {
    setup()?;

    // Initialize controller with human-optimized timing
    debug!("Initializing controller with default settings");
    let controller_settings = ControllerSettings {
        collection_interval_ms: 130,   // Based on ~100-150ms human reaction time
        button_press_threshold_ms: 30, // Filter accidental button presses
        joystick_deadzone: 0.05,       // 5% deadzone for analog sticks
    };

    // Initialize persistence layer
    let persistence_manager = PersistenceManager::new().await;
    let session_sender = persistence_manager.get_sender();
    let config_portal = persistence_manager.get_cfg_portal().await;

    // Create controller communication channel
    let (controller_output_sender, controller_output_receiver) = mpsc::channel(1000);

    // Spawn controller subsystem
    let _controller_handle =
        ControllerHandle::spawn(Some(controller_settings), controller_output_sender)
            .map_err(|e| eyre!("Failed to spawn controller: {}", e))?;

    // Create output channels for different mapping types
    let (ui_tx, ui_rx) = mpsc::channel(100);
    let (elrs_tx, elrs_rx) = mpsc::channel(100);
    let (custom_tx, custom_rx) = mpsc::channel(100);

    // MQTT communication channels
    let (activate_mqtt_tx, activate_mqtt_rx) = watch::channel(true);
    let (mqtt_ui_msg_tx, mqtt_ui_msg_rx) = mpsc::channel(100);
    let (ui_mqtt_msg_tx, ui_mqtt_msg_rx) = mpsc::channel(100);

    let session_sender_clone = session_sender.clone();

    // Spawn MQTT handler
    let portal = config_portal.clone();
    let _mqtt_handl = tokio::spawn(async move {
        let mut mqtt_handle = MQTTHandle { active: true };
        mqtt_handle
            .start_connection(
                ui_mqtt_msg_rx,
                mqtt_ui_msg_tx,
                activate_mqtt_rx,
                portal,
                session_sender_clone,
            )
            .await;
    });

    // Initialize and start mapping engine manager
    let mut manager = MappingEngineManager::new(
        controller_output_receiver,
        ui_tx,
        elrs_tx,
        custom_tx,
        config_portal.clone(),
    );

    manager
        .activate_mapping(mapping::MappingType::Keyboard)
        .await?;

    let _manager_handl = tokio::spawn(async move {
        let _res = manager.run_mapping().await;
    });

    // Launch UI in fullscreen mode
    debug!("Starting UI with mapping manager");
    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = egui::ViewportBuilder::default().with_fullscreen(true);

    eframe::run_native(
        "OpenController",
        native_options,
        Box::new(|cc| {
            Ok(Box::new(OpencontrollerUI::new(
                cc,
                ui_rx,
                mqtt_ui_msg_rx,
                ui_mqtt_msg_tx,
                config_portal,
                session_sender,
            )))
        }),
    );

    Ok(())
}

/// Configures application environment and error handling
///
/// Sets up essential runtime configuration including error reporting,
/// logging, and environment variables. Must be called before any
/// other application initialization.
///
/// # Configuration Applied
///
/// - **Error Handling**: Installs `color_eyre` for enhanced error reporting
/// - **Backtrace**: Disables Rust backtraces by default (set `RUST_LIB_BACKTRACE=1` to enable)
/// - **Logging**: Sets default log level to `INFO` if not specified
/// - **Tracing**: Initializes structured logging with thread and location information
///
/// # Environment Variables
///
/// - `RUST_LOG`: Controls logging verbosity (default: "info")
/// - `RUST_LIB_BACKTRACE`: Controls backtrace display (default: "0")
///
/// # Returns
///
/// * `Ok(())` - Setup completed successfully
/// * `Err(color_eyre::Report)` - Configuration failed
///
/// # Examples
///
/// ```bash
/// # Enable debug logging
/// RUST_LOG=debug cargo run
///
/// # Enable backtraces for debugging
/// RUST_LIB_BACKTRACE=1 cargo run
/// ```
fn setup() -> Result<()> {
    // Configure backtraces (disabled by default for cleaner user experience)
    if std::env::var("RUST_LIB_BACKTRACE").is_err() {
        std::env::set_var("RUST_LIB_BACKTRACE", "0")
    }

    // Install enhanced error reporting
    color_eyre::install()?;

    // Set default log level if not specified
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info")
    }

    setup_logging_env();
    Ok(())
}

/// Initializes structured logging with tracing-subscriber
///
/// Configures a formatted logger with thread information, file locations,
/// and pretty-printed output for development and debugging.
///
/// # Configuration
///
/// - **Max Level**: INFO (controlled by `RUST_LOG` environment variable)
/// - **Target Display**: Disabled to reduce noise
/// - **Thread IDs**: Enabled for multi-threaded debugging
/// - **File/Line**: Enabled for precise error location
/// - **Format**: Pretty-printed for human readability
///
/// # Note
///
/// This function should only be called once during application startup.
/// Multiple calls may result in logging configuration conflicts.
fn setup_logging_env() {
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false) // Hide target for cleaner output
        .with_thread_ids(true) // Essential for multi-threaded debugging
        .with_file(true) // Show source file
        .with_line_number(true) // Show line numbers
        .pretty() // Human-readable formatting
        .init();
}
