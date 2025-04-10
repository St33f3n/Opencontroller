#![crate_name = "opencontroller"]
pub mod config;
pub mod controller;
pub mod ui;
pub mod mapping;


use crate::controller::controller::{
    ButtonEvent, ButtonState, ControllerHandle, ControllerOutput, ControllerSettings,
    JoystickPosition, TriggerValue,
};
use crate::ui::OpencontrollerUI;
use color_eyre::{eyre::eyre, eyre::Report, Result};
use eframe::egui;
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
    // UI starten
    info!("Starting UI");
    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = egui::ViewportBuilder::default().with_fullscreen(true);

    eframe::run_native(
        "OpenController",
        native_options,
        Box::new(|cc| Ok(Box::new(OpencontrollerUI::new(cc, controller_rx)))),
    );

    Ok(())
}

fn setup() -> Result<()> {
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
