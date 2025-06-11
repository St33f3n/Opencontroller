//! Controller input mapping to multiple output formats
//!
//! Transforms [`crate::controller::event_processor::ControllerOutput`] into various target formats using pluggable strategies:
//! - **Keyboard**: egui events for UI control
//! - **ELRS**: RC packets for drone/vehicle control  
//! - **Custom**: Extensible format for future protocols
//!
//! # Architecture
//!
//! ```text
//! ControllerOutput ──► MappingEngine ──► MappedEvent
//!                      (with Strategy)    (Keyboard/ELRS/Custom)
//! ```
//!
//! Each mapping type runs in a separate thread with configurable rate limiting.
//! Engines use statum state machines for lifecycle management.
pub mod custom;
pub mod elrs;
pub mod engine;
pub mod error;
pub mod keyboard;
pub mod manager;
pub mod strategy;

// Re-exports for simpler API access
pub use engine::{MappingEngine, MappingEngineHandle, MappingEngineState};
pub use error::MappingError;
pub use manager::MappingEngineManager;
pub use strategy::{MappingConfig, MappingStrategy, MappingType};

use eframe::egui;
use std::collections::HashMap;

/// Output events from mapping engines
///
/// Each variant targets a specific subsystem. Multiple engines can run
/// simultaneously to send the same input to different outputs.
#[derive(Debug, Clone)]
pub enum MappedEvent {
    /// Keyboard events for UI integration
    ///
    /// Contains egui events that can be injected into the UI event loop
    /// for gamepad-controlled navigation and text input.
    KeyboardEvent { key_code: Vec<egui::Event> },

    /// ELRS data for RC vehicle control
    ///
    /// Pre-formatted channel data ready for CRSF protocol transmission.
    /// Keys are channel numbers (0-15), values are microsecond pulse widths.
    ELRSData { pre_package: HashMap<u16, u16> },

    /// Custom events for protocol extensions
    ///
    /// Flexible format for future wireless protocols (433MHz, LoRA, etc.).
    /// Keys identify data types, values contain protocol-specific payloads.
    CustomEvent {
        event_type: HashMap<String, Vec<u8>>,
    },
}

/// Rate limiter for CPU efficiency on SBCs
///
/// Prevents mapping engines from consuming excessive CPU when idle.
/// Each engine can configure its own rate based on protocol requirements.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    min_interval_ms: u64,

    last_event_time: std::time::Instant,
}

impl RateLimiter {
    /// Creates rate limiter with specified minimum interval
    pub fn new(min_interval_ms: u64) -> Self {
        Self {
            min_interval_ms,
            // Initialize to 1 second ago to allow immediate first event
            last_event_time: std::time::Instant::now() - std::time::Duration::from_secs(1),
        }
    }

    /// Checks if enough time has passed since last event
    ///
    /// Updates internal timestamp when returning true. This ensures
    /// consistent timing between events.
    pub fn should_process(&mut self) -> bool {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_event_time);

        if elapsed.as_millis() as u64 >= self.min_interval_ms {
            self.last_event_time = now;
            true
        } else {
            false
        }
    }
}
