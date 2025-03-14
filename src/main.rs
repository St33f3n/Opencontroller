#![crate_name = "opencontroller"]

pub mod config;
pub mod ui;

use crate::ui::ControllerState;
use crate::ui::OpencontrollerUI;
use color_eyre::{eyre::eyre, eyre::Report, Result};
use eframe::egui;
use tokio::sync::watch::{self, Receiver};
use tokio::task;
use tokio::time::Duration;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]

async fn main() -> Result<()> {
    setup()?;
    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = egui::ViewportBuilder::default().with_fullscreen(true);
    let (tx, rx) = watch::channel(ControllerState::default());

    eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| Ok(Box::new(OpencontrollerUI::new(cc, rx)))),
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
