//! Strategy pattern traits for controller input mapping
//!
//! Defines the core interfaces for pluggable mapping strategies that transform
//! controller input into various output formats. Each strategy implements the
//! same lifecycle and mapping interface while targeting different protocols.
//!
//! # Strategy Pattern
//!
//! ```text
//! MappingConfig ──► MappingStrategy ──► MappedEvent
//!     │                   │                │
//!  validate()           map()         (output format)
//!  create_strategy()    initialize()
//!                       shutdown()
//! ```
//!
use crate::controller;
use crate::controller::controller_handle::ControllerOutput;
use crate::mapping::{MappedEvent, MappingError};
use std::fmt::{Debug, Display};

use super::keyboard::Section;
// Available mapping strategy types
///
/// Each type corresponds to a different output format and use case.
/// Multiple types can be active simultaneously for parallel output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MappingType {
    /// Keyboard events for UI navigation and text input
    Keyboard,

    /// ELRS/CRSF protocol for RC vehicle control
    ELRS,

    /// Custom protocols for future wireless extensions
    Custom,
}

impl Display for MappingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MappingType::Keyboard => write!(f, "Keyboard"),
            MappingType::ELRS => write!(f, "ELRS"),
            MappingType::Custom => write!(f, "Custom"),
        }
    }
}

/// Configuration trait for mapping strategies
///
/// Provides factory pattern for creating and validating mapping strategies.
/// Configurations are loaded from ConfigPortal and used to instantiate strategies.
pub trait MappingConfig: Send + Sync + 'static {
    /// Validates configuration before strategy creation
    ///
    /// Should check for required fields, valid ranges, and internal consistency.
    /// Called before `create_strategy()` to fail fast on invalid configurations.
    fn validate(&self) -> Result<(), MappingError>;

    /// Creates a strategy trait object from this configuration
    ///
    /// Returns a boxed trait object ready for use in a mapping engine.
    /// Configuration should be validated before calling this method.
    fn create_strategy(&self) -> Result<Box<dyn MappingStrategy>, MappingError>;

    /// Returns the mapping type this configuration produces
    fn get_type(&self) -> MappingType;

    /// Human-readable configuration name
    fn get_name(&self) -> String {
        format!("{} Mapping Configuration", self.get_type())
    }

    /// Configuration description for UI display
    fn get_description(&self) -> String {
        format!("Configuration for {} mapping", self.get_type())
    }
}

/// Core mapping strategy trait
///
/// Transforms controller input into protocol-specific output events.
/// Strategies maintain internal state through `MappingContext` and can
/// implement rate limiting for CPU efficiency.
///
/// # Lifecycle
///
/// 1. **initialize()** - One-time setup, load configuration
/// 2. **map()** - Called repeatedly for each controller input
/// 3. **shutdown()** - Cleanup when engine is deactivated
///
/// # Trait Objects
///
/// Strategies are used as trait objects (`Box<dyn MappingStrategy>`) for
/// dynamic dispatch, enabling different implementations to be stored in
/// the same collection and called through the same trait methods.
pub trait MappingStrategy: Send + Sync + 'static {
    /// Transforms controller input into mapped output event
    ///
    /// Returns `None` if no output should be generated for this input.
    /// Called frequently (every 20ms) by the mapping engine.
    ///
    /// # Arguments
    ///
    /// * `input` - Current controller state with button events, analog values, etc.
    ///
    /// # Returns
    ///
    /// * `Some(MappedEvent)` - Generated output event for this input
    /// * `None` - No output for this input (filtered, rate limited, etc.)
    fn map(&mut self, input: &ControllerOutput) -> Option<MappedEvent>;

    /// One-time initialization when strategy is activated
    ///
    /// Load configuration, initialize internal state, setup connections.
    /// Called once when the mapping engine starts this strategy.
    fn initialize(&mut self) -> Result<(), MappingError>;

    /// Cleanup when strategy is deactivated
    ///
    /// Release resources, save state, close connections.
    /// Called once when the mapping engine shuts down this strategy.
    fn shutdown(&mut self);

    /// Rate limiting interval in milliseconds
    ///
    /// If specified, the mapping engine will skip calls to `map()` until
    /// this interval has elapsed since the last call. Useful for protocols
    /// with limited bandwidth or to reduce CPU usage on SBCs.
    ///
    /// # Returns
    ///
    /// * `Some(ms)` - Minimum milliseconds between map() calls
    /// * `None` - No rate limiting (default implementation)
    fn get_rate_limit(&self) -> Option<u64> {
        None
    }

    /// Returns the mapping type this strategy implements
    fn get_type(&self) -> MappingType;
}

/// Persistent context for stateful mapping strategies
///
/// Maintains state between `map()` calls for strategies that need to track
/// changes, calculate deltas, or implement complex state machines.
///
/// # Use Cases
///
/// - **Button state tracking**: Remember previous button states for edge detection
/// - **Analog section tracking**: Track joystick regions for keyboard mapping
/// - **Protocol state**: Accumulate data for multi-packet protocols
/// - **Timing**: Track timestamps for time-based mappings
#[derive(Debug, Default, Clone)]
pub struct MappingContext {
    /// Previous button states for edge detection
    ///
    /// Allows strategies to detect button press/release transitions
    /// and implement hold-time based logic.
    pub last_button_states: std::collections::HashMap<
        crate::controller::controller_handle::ButtonType,
        controller::controller_handle::ButtonEventState,
    >,

    /// Previous joystick regions for keyboard mapping
    ///
    /// Used by keyboard strategy to track which analog regions
    /// the joysticks were in during the previous mapping cycle.
    pub last_sections: (Section, Section),

    /// Protocol-specific accumulated data
    ///
    /// Generic storage for strategies that need to build up data
    /// over multiple mapping cycles before generating output.
    pub accumulated_data: std::collections::HashMap<String, Vec<u8>>,

    /// Last processing timestamp
    ///
    /// Enables time-based logic like rate limiting, timeouts,
    /// or time-delta calculations within strategies.
    pub last_timestamp: Option<std::time::SystemTime>,
}
