pub mod config;
pub mod controller;
pub mod mapping;
pub mod mqtt;
pub mod ui;

use crate::controller::controller_handle::{ControllerHandle, ControllerSettings};
use crate::mapping::{keyboard::KeyboardConfig, MappingEngineManager};
use crate::mqtt::mqtt_handler::MQTTConnection;
use crate::ui::OpencontrollerUI;
use color_eyre::{eyre::eyre, Result};
use eframe::egui;
use mqtt::mqtt_handler::{self, MQTTHandle};
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;
use ui::MQTTServer;

#[tokio::main]
async fn main() -> Result<()> {
    setup()?;

    // Controller initialisieren
    info!("Initializing controller with default settings");
    let controller_settings = ControllerSettings {
        collection_interval_ms: 130,
        button_press_threshold_ms: 30,
        joystick_deadzone: 0.05,
    };

    let (controller_output_sender, controller_output_receiver) = mpsc::channel(1000);

    // Controller starten und Receiver erhalten
    let _controller_handle =
        ControllerHandle::spawn(Some(controller_settings), controller_output_sender)
            .map_err(|e| eyre!("Failed to spawn controller: {}", e))?;

    // Kanäle für die verschiedenen Event-Typen erstellen
    let (ui_tx, ui_rx) = mpsc::channel(100);
    let (elrs_tx, elrs_rx) = mpsc::channel(100);
    let (custom_tx, custom_rx) = mpsc::channel(100);

    let (activate_mqtt_tx, activate_mqtt_rx) = watch::channel(true);
    let (mqtt_ui_msg_tx, mqtt_ui_msg_rx) = mpsc::channel(100);
    let (ui_mqtt_msg_tx, ui_mqtt_msg_rx) = mpsc::channel(100);

    let (ui_mqtt_config_tx, ui_mqtt_config_rx) = watch::channel(mqtt::config::MqttConfig {
        subbed_topics: Vec::new(),
        server: MQTTServer::default(),
        poll_frequency: 5,
    });

    let mqtt_handl = tokio::spawn(async move {
        let mut mqtt_handle = MQTTHandle { active: true };

        mqtt_handle
            .start_connection(
                ui_mqtt_msg_rx,
                mqtt_ui_msg_tx,
                ui_mqtt_config_rx,
                activate_mqtt_rx,
            )
            .await;
    });

    let keyboard_conversion = Box::new(KeyboardConfig::default_config());

    let mut manager =
        MappingEngineManager::new(controller_output_receiver, ui_tx, elrs_tx, custom_tx);

    manager.activate_mapping(keyboard_conversion).await?;

    let _manager_handl = tokio::spawn(async move {
        let _res = manager.run_mapping().await;
    });

    // UI starten
    info!("Starting UI with mapping manager");
    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = egui::ViewportBuilder::default().with_fullscreen(true);

    eframe::run_native(
        "OpenController",
        native_options,
        Box::new(|cc| {
            Ok(Box::new(OpencontrollerUI::new(
                cc,
                ui_rx,
                ui_mqtt_config_tx,
                mqtt_ui_msg_rx,
                ui_mqtt_msg_tx,
            )))
        }),
    );

    Ok(())
}

fn setup() -> Result<()> {
    // ... Bestehender Setup-Code ...
    if std::env::var("RUST_LIB_BACKTRACE").is_err() {
        std::env::set_var("RUST_LIB_BACKTRACE", "0")
    }
    color_eyre::install()?;
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info")
    }
    setup_logging_env();
    Ok(())
}

fn setup_logging_env() {
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .pretty()
        .init();
}
