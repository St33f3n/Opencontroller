use tokio::sync::watch;
use tracing::{debug, error, info, warn};

// Re-export types that need to be public
pub use crate::controller::event_collector::{ButtonState, ButtonType, JoystickType, RawControllerEvent, TriggerType, CollectorSettings, CollectorError, CollectorHandle};
pub use crate::controller::event_processor::{ControllerOutput, JoystickPosition, TriggerValue, ButtonEvent, ButtonEventState, ProcessorSettings, ProcessorError, ProcessorHandle};

// Controller settings for both components
#[derive(Clone, Debug)]
pub struct ControllerSettings {
    pub collection_interval_ms: u64,
    pub button_press_threshold_ms: u32,
    pub joystick_deadzone: f32,
}

impl Default for ControllerSettings {
    fn default() -> Self {
        Self {
            collection_interval_ms: 130,
            button_press_threshold_ms: 30,
            joystick_deadzone: 0.05,
        }
    }
}

// Controller errors
#[derive(Debug, thiserror::Error)]
pub enum ControllerError {
    #[error("Collector error: {0}")]
    CollectorError(#[from] CollectorError),

    #[error("Processor error: {0}")]
    ProcessorError(#[from] ProcessorError),
    
    #[error("Channel error: {0}")]
    ChannelError(String),
    
    #[error("Initialization error: {0}")]
    InitializationError(String),
}

// Public handle for the complete controller system
pub struct ControllerHandle {
    state_receiver: watch::Receiver<ControllerOutput>,
}

impl ControllerHandle {
    // Spawn both collector and processor
    pub fn spawn(settings: Option<ControllerSettings>) -> Result<Self, ControllerError> {
        info!("Initializing Controller system with settings: {:?}", settings);
        
        // Break down the settings
        let settings = settings.unwrap_or_default();
        let collector_settings = CollectorSettings {
            joystick_deadzone: settings.joystick_deadzone,
        };
        let processor_settings = ProcessorSettings {
            processing_interval_ms: settings.collection_interval_ms,
            button_press_threshold_ms: settings.button_press_threshold_ms,
        };
        
        debug!("Split settings: collector={:?}, processor={:?}", 
              collector_settings, processor_settings);

        // Create MPSC channel for event communication
        let (event_sender, event_receiver) = tokio::sync::mpsc::channel(1000);
        debug!("Created event channel with buffer capacity 100");
        
        // Create and spawn the event collector
        info!("Creating Event Collector");
        let collector_handle = CollectorHandle::spawn(Some(collector_settings), event_sender)?;
        info!("Event Collector spawned successfully");
        
        // Create and spawn the event processor
        info!("Creating Event Processor");
        let processor_handle = ProcessorHandle::spawn(
            event_receiver,
            Some(processor_settings),
        )?;
        info!("Event Processor spawned successfully");
        
        // Get state receiver from processor
        let state_receiver = processor_handle.subscribe();
        debug!("Retrieved controller state receiver for UI access");
        
        info!("Controller system initialized successfully");
        Ok(Self { state_receiver })
    }

    // Get a receiver for the controller state
    pub fn subscribe(&self) -> watch::Receiver<ControllerOutput> {
        debug!("New subscriber to controller state");
        self.state_receiver.clone()
    }
}