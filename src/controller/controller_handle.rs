//! Controller Handle - Unified API for gamepad input processing
//!
//! Provides a high-level interface for the two-stage controller architecture:
//! raw event collection and structured event processing. Manages the lifecycle
//! of both subsystems and handles inter-thread communication.
//!

use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

pub use super::event_collector::{
    ButtonState, ButtonType, CollectorError, CollectorHandle, CollectorSettings, JoystickType,
    RawControllerEvent, TriggerType,
};
pub use super::event_processor::{
    ButtonEvent, ButtonEventState, ControllerOutput, JoystickPosition, ProcessorError,
    ProcessorHandle, ProcessorSettings, TriggerValue,
};

/// Configuration settings for the complete controller subsystem
///
/// Provides unified configuration for both event collection and processing stages.
/// Settings are automatically distributed to the appropriate subsystem components.
///
/// # Performance Impact
///
/// - `collection_interval_ms`: Lower values increase responsiveness but consume more CPU
/// - `button_press_threshold_ms`: Filters accidental button presses; too low may cause false positives
/// - `joystick_deadzone`: Prevents analog stick drift; too high reduces precision
///
/// # Examples
///
/// ```rust
/// use opencontroller::controller::ControllerSettings;
///
/// // High-performance gaming setup
/// let gaming_settings = ControllerSettings {
///     collection_interval_ms: 100,
///     button_press_threshold_ms: 20,
///     joystick_deadzone: 0.03,
/// };
///
/// // Relaxed Smart Home control
/// let smart_home_settings = ControllerSettings {
///     collection_interval_ms: 200,
///     button_press_threshold_ms: 50,
///     joystick_deadzone: 0.08,
/// };
/// ```
#[derive(Clone, Debug)]
pub struct ControllerSettings {
    /// Processing interval in milliseconds (applied to both collection and processing)
    ///
    /// Determines how frequently events are collected and processed. Based on human
    /// reaction times (~100-150ms). Lower values increase responsiveness but consume
    /// more CPU resources.
    pub collection_interval_ms: u64,

    /// Minimum button press duration in milliseconds to register as valid input
    ///
    /// Filters out accidental button presses and contact bounce. Values below 20ms
    /// may allow false positives, while values above 50ms may feel unresponsive.
    pub button_press_threshold_ms: u32,

    /// Analog stick deadzone as a fraction (0.0-1.0)
    ///
    /// Prevents analog stick drift by ignoring small movements near the center position.
    /// Typical values range from 0.03 (precise) to 0.1 (loose/worn controllers).
    pub joystick_deadzone: f32,
}

impl Default for ControllerSettings {
    /// Default settings optimized for general use
    ///
    /// Balances responsiveness with CPU efficiency and provides good input filtering
    /// for most gamepads and use cases.
    fn default() -> Self {
        Self {
            collection_interval_ms: 130,   // Based on human reaction time studies
            button_press_threshold_ms: 30, // Filters most accidental presses
            joystick_deadzone: 0.05,       // 5% deadzone for typical controllers
        }
    }
}

/// Errors that can occur during controller initialization or operation
///
/// Aggregates errors from both the collection and processing subsystems,
/// providing a unified error interface for the controller handle.
#[derive(Debug, thiserror::Error)]
pub enum ControllerError {
    /// Error from the event collection subsystem
    ///
    /// Typically indicates gamepad detection issues, driver problems,
    /// or hardware communication failures.
    #[error("Collector error: {0}")]
    CollectorError(#[from] CollectorError),

    /// Error from the event processing subsystem
    ///
    /// Usually indicates data processing failures, state machine errors,
    /// or output generation problems.
    #[error("Processor error: {0}")]
    ProcessorError(#[from] ProcessorError),

    /// Inter-thread communication error
    ///
    /// Occurs when channels between collector and processor are disconnected
    /// or buffer overflow happens.
    #[error("Channel error: {0}")]
    ChannelError(String),

    /// General initialization error
    ///
    /// Indicates system-level problems during controller subsystem startup.
    #[error("Initialization error: {0}")]
    InitializationError(String),
}

/// Handle for managing the complete controller subsystem lifecycle
///
/// Provides a unified interface for spawning and managing both the event collection
/// and processing threads. The handle itself is lightweight and primarily serves
/// as a factory for the subsystem components.
///
/// # Threading Model
///
/// Spawns two independent tokio tasks:
/// 1. **Collection Thread**: Polls gamepad hardware and generates raw events
/// 2. **Processing Thread**: Transforms raw events into structured output
///
/// Communication between threads uses a buffered mpsc channel (1000 events capacity).
///
/// # Resource Management
///
/// The spawned threads are fire-and-forget; they run until the application terminates.
/// No explicit cleanup is required as tokio handles task lifecycle automatically.
pub struct ControllerHandle {}

impl ControllerHandle {
    /// Spawns the complete controller subsystem with unified settings
    ///
    /// Creates and starts both the event collection and processing threads, establishing
    /// communication channels between them. The subsystem becomes operational immediately
    /// upon successful initialization.
    ///
    /// # Architecture Setup
    ///
    /// 1. **Settings Distribution**: Splits unified settings into subsystem-specific configurations
    /// 2. **Channel Creation**: Establishes 1000-event buffer between collector and processor
    /// 3. **Collector Spawn**: Starts raw event collection from gamepad hardware
    /// 4. **Processor Spawn**: Starts structured event processing and output generation
    ///
    /// # Thread Communication
    ///
    /// ```text
    /// CollectorHandle ─[RawControllerEvent]→ ProcessorHandle ─[ControllerOutput]→ Application
    ///                  (mpsc::channel(1000))                   (provided sender)
    /// ```
    ///
    /// # Arguments
    ///
    /// * `settings` - Optional configuration; uses defaults if None
    /// * `sender` - Channel for sending processed controller output to the application
    ///
    /// # Returns
    ///
    /// * `Ok(ControllerHandle)` - Subsystem successfully initialized and running
    /// * `Err(ControllerError)` - Initialization failed
    ///
    /// # Errors
    ///
    /// Returns `ControllerError` if:
    ///
    /// * [`ControllerError::CollectorError`] - Gamepad detection or driver issues
    /// * [`ControllerError::ProcessorError`] - Event processing initialization failed
    /// * [`ControllerError::InitializationError`] - System-level startup problems
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use opencontroller::controller::{ControllerHandle, ControllerSettings};
    /// use tokio::sync::mpsc;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let (tx, rx) = mpsc::channel(100);
    ///
    /// // Use default settings
    /// let handle = ControllerHandle::spawn(None, tx)?;
    ///
    /// // Use custom settings
    /// let settings = ControllerSettings {
    ///     collection_interval_ms: 100,
    ///     button_press_threshold_ms: 25,
    ///     joystick_deadzone: 0.03,
    /// };
    /// let (tx2, rx2) = mpsc::channel(100);
    /// let handle2 = ControllerHandle::spawn(Some(settings), tx2)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Notes
    ///
    /// The 1000-event internal buffer provides adequate headroom for burst input scenarios
    /// while maintaining low latency. The buffer size is optimized for typical gamepad
    /// usage patterns and should not require adjustment.
    pub fn spawn(
        settings: Option<ControllerSettings>,
        sender: mpsc::Sender<ControllerOutput>,
    ) -> Result<Self, ControllerError> {
        info!(
            "Initializing Controller system with settings: {:?}",
            settings
        );

        // Use default settings if none provided
        let settings = settings.unwrap_or_default();

        // Distribute settings to subsystem components
        let collector_settings = CollectorSettings {
            joystick_deadzone: settings.joystick_deadzone,
        };
        let processor_settings = ProcessorSettings {
            processing_interval_ms: settings.collection_interval_ms,
            button_press_threshold_ms: settings.button_press_threshold_ms,
        };

        debug!(
            "Split settings: collector={:?}, processor={:?}",
            collector_settings, processor_settings
        );

        // Create inter-thread communication channel
        let (event_sender, event_receiver) = tokio::sync::mpsc::channel(1000);
        debug!("Created event channel with buffer capacity 1000");

        // Spawn event collection subsystem
        info!("Creating Event Collector");
        let _collector_handle = CollectorHandle::spawn(Some(collector_settings), event_sender)?;
        info!("Event Collector spawned successfully");

        // Spawn event processing subsystem
        info!("Creating Event Processor");
        let _processor_handle =
            ProcessorHandle::spawn(event_receiver, sender, Some(processor_settings))?;
        info!("Event Processor spawned successfully");

        info!("Controller system initialized successfully");
        Ok(Self {})
    }
}
