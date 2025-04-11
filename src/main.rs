pub mod config;
pub mod controller;
pub mod mapping; 
pub mod ui;

use crate::controller::controller::{
    ButtonEvent, ButtonState, ControllerHandle, ControllerOutput, ControllerSettings,
    JoystickPosition, TriggerValue,
};
use crate::mapping::{
    MappedEvent, MappingEngineManager, keyboard::KeyboardConfig, elrs::ELRSConfig,
    custom::CustomConfig, MappingType,
};
use crate::ui::OpencontrollerUI;
use color_eyre::{eyre::eyre, eyre::Report, Result};
use eframe::egui;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

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

    // Controller starten und Receiver erhalten
    let controller_handle = ControllerHandle::spawn(Some(controller_settings))
        .map_err(|e| eyre!("Failed to spawn controller: {}", e))?;
    let controller_rx = controller_handle.subscribe();
    
    // Kanäle für die verschiedenen Event-Typen erstellen
    let (keyboard_tx, keyboard_rx) = mpsc::channel(100);
    let (elrs_tx, elrs_rx) = mpsc::channel(100);
    let (custom_tx, custom_rx) = mpsc::channel(100);
    
    // Mapping-Engine-Manager erstellen
    let mut mapping_manager = MappingEngineManager::new(
        controller_rx.clone(),
        keyboard_tx,
        elrs_tx,
        custom_tx,
    );
    
    // Standard-Keyboard-Mapping aktivieren
    let keyboard_config = KeyboardConfig::default_config();
    if let Err(e) = mapping_manager.activate_mapping(Box::new(keyboard_config)).await {
        error!("Failed to activate keyboard mapping: {}", e);
    }
    
    // Task für Keyboard-Event-Verarbeitung starten
    let ui_keyboard_task = tokio::spawn(async move {
        process_keyboard_events(keyboard_rx).await;
    });
    
    // Task für ELRS-Event-Verarbeitung starten
    let elrs_task = tokio::spawn(async move {
        process_elrs_events(elrs_rx).await;
    });
    
    // Task für Custom-Event-Verarbeitung starten
    let custom_task = tokio::spawn(async move {
        process_custom_events(custom_rx).await;
    });
    
    // UI starten
    info!("Starting UI with mapping manager");
    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = egui::ViewportBuilder::default().with_fullscreen(true);

    eframe::run_native(
        "OpenController",
        native_options,
        Box::new(|cc| Ok(Box::new(OpencontrollerUI::new(cc, controller_rx)))),
    );

    Ok(())
}

/// Verarbeitet Keyboard-Events aus dem Mapping-System
async fn process_keyboard_events(mut keyboard_rx: mpsc::Receiver<MappedEvent>) {
    info!("Starting keyboard event processor");
    while let Some(event) = keyboard_rx.recv().await {
        match event {
            MappedEvent::KeyboardEvent { key_code } => {
                debug!("Received {} keyboard events", key_code.len());
                // Hier würden die Events an die UI weitergegeben werden
            }
            _ => {
                warn!("Received non-keyboard event in keyboard channel");
            }
        }
    }
    info!("Keyboard event processor terminated");
}

/// Verarbeitet ELRS-Events aus dem Mapping-System
async fn process_elrs_events(mut elrs_rx: mpsc::Receiver<MappedEvent>) {
    info!("Starting ELRS event processor");
    while let Some(event) = elrs_rx.recv().await {
        match event {
            MappedEvent::ELRSData { pre_package } => {
                debug!("Received ELRS data with {} channels", pre_package.len());
                // Hier würden die ELRS-Daten verarbeitet werden
            }
            _ => {
                warn!("Received non-ELRS event in ELRS channel");
            }
        }
    }
    info!("ELRS event processor terminated");
}

/// Verarbeitet Custom-Events aus dem Mapping-System
async fn process_custom_events(mut custom_rx: mpsc::Receiver<MappedEvent>) {
    info!("Starting custom event processor");
    while let Some(event) = custom_rx.recv().await {
        match event {
            MappedEvent::CustomEvent { event_type } => {
                debug!("Received custom event with {} fields", event_type.len());
                // Hier würden die benutzerdefinierten Events verarbeitet werden
            }
            _ => {
                warn!("Received non-custom event in custom channel");
            }
        }
    }
    info!("Custom event processor terminated");
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
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .pretty()
        .init();
}