#![crate_name = "controll_machine"]

pub mod config;
pub mod ui;

use color_eyre::{eyre::eyre, eyre::Report, Result};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

fn main() -> Result<()> {
    setup()?;
    ui::run_ui()?;

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

use statum::{machine, state};

// 1. Define your states as an enum.
#[state]
pub enum LightState {
    Off,
    On,
}

// 2. Define your machine with the #[machine] attribute.
#[machine]
pub struct LightSwitch<S: LightState> {
    name: String, // Contextual, Machine-wide fields go here, like clients, configs, an identifier, etc.
}

// 3. Implement transitions for each state.
impl LightSwitch<Off> {
    pub fn switch_on(self) -> LightSwitch<On> {
        //Note: we consume self and return a new state
        self.transition()
    }
}

impl LightSwitch<On> {
    pub fn switch_off(self) -> LightSwitch<Off> {
        self.transition()
    }
}
