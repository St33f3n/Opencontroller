pub mod controller;
pub mod mapping;
pub mod mqtt;
pub mod persistence;
pub mod ui;

use crate::controller::controller_handle::{ControllerHandle, ControllerSettings};
use crate::mapping::{keyboard::KeyboardConfig, MappingEngineManager};
use crate::persistence::config_portal::ConfigPortal;
use crate::persistence::persistence_worker::SessionAction;
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

    let (config_portal, config_actions_sender) = setup_config().await?;

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

    let (ui_mqtt_config_tx, ui_mqtt_config_rx) =
        watch::channel(get_mqtt_conf(config_portal.clone()));
    let portal = config_portal.clone();
    let client = config_actions_sender.clone();
    let mqtt_handl = tokio::spawn(async move {
        let mut mqtt_handle = MQTTHandle { active: true };

        mqtt_handle
            .start_connection(
                ui_mqtt_msg_rx,
                mqtt_ui_msg_tx,
                ui_mqtt_config_rx,
                activate_mqtt_rx,
                portal,
                client,
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
                config_portal,
                config_actions_sender,
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

async fn setup_config() -> Result<(Arc<ConfigPortal>, ConfigClient)> {
    // Stelle sicher, dass eine Standardkonfiguration existiert
    ConfigPortal::ensure_default_config().await?;

    // Erstelle das ConfigPortal
    let config_portal = Arc::new(ConfigPortal::new().await?);

    // Starte Autosave-Task (alle 5 Minuten)
    let _autosave_handle = ConfigPortal::start_autosave_task(config_portal.clone(), 300).await;

    // Erstelle den Config-Worker für asynchrone Konfigurationsoperationen
    let (config_client, _config_worker) = ConfigPortal::create_config_worker(config_portal.clone());

    Ok((config_portal, config_client))
}

fn get_mqtt_conf(config_portal: Arc<ConfigPortal>) -> crate::mqtt::config::MqttConfig {
    let mut available_topics = Vec::new();

    match config_portal.connection_config().try_read() {
        Ok(connection_guard) => {
            available_topics = connection_guard.mqtt_config.available_topics.clone()
        }
        Err(e) => warn!("Unable to read: {}", e),
    }
    let mut subbed_topics = Vec::new();
    match config_portal.connection_config().try_read() {
        Ok(connection_guard) => subbed_topics = connection_guard.mqtt_config.subbed_topics.clone(),
        Err(e) => warn!("Unable to read: {}", e),
    }

    let mut server = MQTTServer::default();
    match config_portal.connection_config().try_read() {
        Ok(connection_guard) => server = connection_guard.mqtt_config.server.clone(),
        Err(e) => warn!("Unable to read: {}", e),
    }

    let mut available_servers = Vec::new();
    match config_portal.connection_config().try_read() {
        Ok(connection_guard) => {
            available_servers = connection_guard.mqtt_config.available_servers.clone()
        }
        Err(e) => warn!("Unable to read: {}", e),
    }

    MqttConfig {
        available_topics,
        subbed_topics,
        server,
        available_servers,
        poll_frequency: 10,
    }
}
