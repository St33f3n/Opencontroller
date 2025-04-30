use chrono::{DateTime, Local};
use gilrs::{Axis, Button, Event, EventType, Gamepad, GamepadId, Gilrs};
use serde::{Deserialize, Serialize};
use statum::{machine, state};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

// Raw controller event with precise chrono timestamps
#[derive(Debug, Clone)]
pub enum RawControllerEvent {
    JoystickMove {
        stick: JoystickType,
        x: f32,
        y: f32,
        timestamp: DateTime<Local>,
    },
    TriggerMove {
        trigger: TriggerType,
        value: f32,
        timestamp: DateTime<Local>,
    },
    ButtonEvent {
        button_type: ButtonType,
        button_state: ButtonState,
        timestamp: DateTime<Local>,
    },
}

// Joystick type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JoystickType {
    Left,
    Right,
}

// Trigger type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TriggerType {
    Left,
    Right,
}

// Button state
#[derive(Clone, Debug, PartialEq)]
pub enum ButtonState {
    Pressed,
    Released,
}

// Button type
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ButtonType {
    A,
    B,
    X,
    Y,
    Start,
    Select,
    LeftBumper,
    RightBumper,
    LeftStick,
    RightStick,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    Guide,
    // Add other buttons as needed
}

// Collector settings
#[derive(Clone, Debug)]
pub struct CollectorSettings {
    pub joystick_deadzone: f32,
}

impl Default for CollectorSettings {
    fn default() -> Self {
        Self {
            joystick_deadzone: 0.05,
        }
    }
}

// Collector errors
#[derive(Debug, thiserror::Error)]
pub enum CollectorError {
    #[error("Failed to initialize collector: {0}")]
    InitializationError(String),

    #[error("Failed to collect events: {0}")]
    EventCollectionError(String),

    #[error("Failed to send event: {0}")]
    EventSendError(String),

    #[error("No gamepad connected: {0}")]
    NoGamepadError(String),
}

// Define collector states using statum's state macro
#[state]
#[derive(Debug, Clone)]
pub enum CollectionState {
    Initializing,
    Collecting,
}

#[machine]
#[derive(Debug)]
pub struct EventCollector<S: CollectionState> {
    // Gilrs context
    gilrs: Gilrs,

    // Active gamepad
    active_gamepad: Option<GamepadId>,

    // Collector settings
    settings: CollectorSettings,

    // Channel for sending events to processor
    event_sender: mpsc::Sender<RawControllerEvent>,

    // Last seen joystick values (to calculate deltas)
    last_left_stick_x: f32,
    last_left_stick_y: f32,
    last_right_stick_x: f32,
    last_right_stick_y: f32,
}

// Implementation of methods available in all states
impl<S: CollectionState> EventCollector<S> {
    // Helper method to update settings
    pub fn update_settings(&mut self, settings: CollectorSettings) {
        self.settings = settings;
    }

    // Get a reference to the current settings
    pub fn settings(&self) -> &CollectorSettings {
        &self.settings
    }
}

// Implementation for Initializing state
impl EventCollector<Initializing> {
    pub fn create(
        settings: Option<CollectorSettings>,
        event_sender: mpsc::Sender<RawControllerEvent>,
    ) -> Result<Self, CollectorError> {
        let settings = settings.unwrap_or_default();
        debug!("Creating Event Collector with settings: {:?}", settings);

        // Initialize gilrs with logging
        info!("Initializing gilrs controller interface");
        let gilrs = match Gilrs::new() {
            Ok(g) => {
                info!("Successfully initialized gilrs");
                g
            }
            Err(e) => {
                error!("Failed to initialize gilrs: {}", e);
                return Err(CollectorError::InitializationError(e.to_string()));
            }
        };

        debug!("Creating new EventCollector instance");
        Ok(Self::new(
            gilrs,
            None,
            settings,
            event_sender,
            0.0, // last_left_stick_x
            0.0, // last_left_stick_y
            0.0, // last_right_stick_x
            0.0, // last_right_stick_y
        ))
    }

    // Initialize the controller and transition to Collecting state
    pub fn initialize(mut self) -> Result<EventCollector<Collecting>, CollectorError> {
        info!(
            "Initializing Event Collector with deadzone: {}",
            self.settings.joystick_deadzone
        );

        // Find an active gamepad
        let gamepads: Vec<(GamepadId, Gamepad<'_>)> = self.gilrs.gamepads().collect();

        if gamepads.is_empty() {
            warn!("No gamepad connected, continuing in idle mode");
        } else {
            info!("Found {} gamepads:", gamepads.len());
            for (idx, (id, gamepad)) in gamepads.iter().enumerate() {
                info!(
                    "  [{}] ID: {}, Name: {}, UUID: {:?}",
                    idx,
                    id,
                    gamepad.name(),
                    gamepad.uuid()
                );
            }
            //TODO Dynamic change for active gamepad from UI
            // Try to use the first gamepad, or the second if available
            let index = if gamepads.len() > 1 { 1 } else { 0 };
            let (id, gamepad) = &gamepads[index];
            self.active_gamepad = Some(*id);
            info!("Selected gamepad: {} ({})", gamepad.name(), id);

            // Log connected buttons and axes for debugging
        }

        info!("Event Collector initialized, transitioning to Collecting state");
        Ok(self.transition())
    }
}

// Implementation for Controller in Collecting state
impl EventCollector<Collecting> {
    // Collect a single event and send it to the queue
    pub fn collect_next_event(&mut self) -> Result<(), CollectorError> {
        // Check for next event
        if let Some(Event {
            id, event, time, ..
        }) = self.gilrs.next_event()
        {
            // Only process events from the active gamepad if one is set
            if let Some(active_id) = self.active_gamepad {
                if id != active_id {
                    debug!("Skipping event from non-active gamepad: {:?}", id);
                    return Ok(()); // Skip events from other gamepads
                }
            }

            // Log the raw event at debug level
            debug!("Processing gilrs event: {:?} at time: {:?}", event, time);

            // Convert gilrs event to our internal event type with chrono timestamp
            if let Some(raw_event) = self.convert_gilrs_event(event) {
                // Log important button events at info level
                match &raw_event {
                    RawControllerEvent::ButtonEvent {
                        button_type,
                        button_state,
                        timestamp,
                    } => {
                        info!(
                            "Button event: {:?} {:?} at {}",
                            button_type,
                            button_state,
                            timestamp.format("%H:%M:%S.%3f")
                        );
                    }
                    _ => debug!("Captured event: {:?}", raw_event),
                }

                // Send the event to the processor queue
                match self.event_sender.try_send(raw_event) {
                    Ok(_) => debug!("Event sent to processor queue"),
                    Err(e) => {
                        error!("Failed to send event to processor: {}", e);
                        return Err(CollectorError::EventSendError(e.to_string()));
                    }
                }
            } else {
                debug!("Event ignored due to filtering or mapping");
            }
        }

        Ok(())
    }

    // Run the collector in a loop
    pub fn run_collection_loop(&mut self) -> Result<(), CollectorError> {
        info!("Starting Event Collector loop");

        // For performance monitoring
        let mut event_count = 0;
        let mut last_log_time = Local::now();
        let log_interval = chrono::Duration::seconds(10);

        loop {
            // This is a non-blocking call that checks for new events
            if let Err(e) = self.collect_next_event() {
                error!("Error collecting event: {}", e);
                // Continue despite errors to maintain the loop
            } else {
                // Increment event counter for stats
                event_count += 1;
            }

            // Log performance stats periodically
            let now = Local::now();
            if now - last_log_time > log_interval {
                info!(
                    "Event Collector stats: processed {} events in last {} seconds (avg {:.2}/sec)",
                    event_count,
                    log_interval.num_seconds(),
                    event_count as f64 / log_interval.num_seconds() as f64
                );
                event_count = 0;
                last_log_time = now;
            }

            // Small sleep to prevent 100% CPU usage
            // This is a compromise between responsiveness and CPU usage
            std::thread::sleep(std::time::Duration::from_micros(100));
        }
    }

    // Convert gilrs event to internal event type with chrono timestamp
    fn convert_gilrs_event(&mut self, event: EventType) -> Option<RawControllerEvent> {
        let now = Local::now(); // Use chrono for precise timestamp

        match event {
            EventType::AxisChanged(axis, value, _) => {
                debug!("Axis changed: {:?} = {:.4}", axis, value);

                match axis {
                    Axis::LeftStickX => {
                        let new_value = apply_deadzone(value, self.settings.joystick_deadzone);
                        let delta = new_value - self.last_left_stick_x;

                        // Only log significant changes to avoid spam
                        if delta.abs() > 0.05 {
                            debug!(
                                "Left stick X: {:.4} -> {:.4} (delta: {:.4})",
                                self.last_left_stick_x, new_value, delta
                            );
                        }

                        let raw_event = RawControllerEvent::JoystickMove {
                            stick: JoystickType::Left,
                            x: new_value,
                            y: self.last_left_stick_y,
                            timestamp: now,
                        };
                        self.last_left_stick_x = new_value;
                        Some(raw_event)
                    }
                    Axis::LeftStickY => {
                        let new_value = apply_deadzone(value, self.settings.joystick_deadzone);
                        let delta = new_value - self.last_left_stick_y;

                        if delta.abs() > 0.05 {
                            debug!(
                                "Left stick Y: {:.4} -> {:.4} (delta: {:.4})",
                                self.last_left_stick_y, new_value, delta
                            );
                        }

                        let raw_event = RawControllerEvent::JoystickMove {
                            stick: JoystickType::Left,
                            x: self.last_left_stick_x,
                            y: new_value,
                            timestamp: now,
                        };
                        self.last_left_stick_y = new_value;
                        Some(raw_event)
                    }
                    Axis::RightStickX => {
                        let new_value = apply_deadzone(value, self.settings.joystick_deadzone);
                        let delta = new_value - self.last_right_stick_x;

                        if delta.abs() > 0.05 {
                            debug!(
                                "Right stick X: {:.4} -> {:.4} (delta: {:.4})",
                                self.last_right_stick_x, new_value, delta
                            );
                        }

                        let raw_event = RawControllerEvent::JoystickMove {
                            stick: JoystickType::Right,
                            x: new_value,
                            y: self.last_right_stick_y,
                            timestamp: now,
                        };
                        self.last_right_stick_x = new_value;
                        Some(raw_event)
                    }
                    Axis::RightStickY => {
                        let new_value = apply_deadzone(value, self.settings.joystick_deadzone);
                        let delta = new_value - self.last_right_stick_y;

                        if delta.abs() > 0.05 {
                            debug!(
                                "Right stick Y: {:.4} -> {:.4} (delta: {:.4})",
                                self.last_right_stick_y, new_value, delta
                            );
                        }

                        let raw_event = RawControllerEvent::JoystickMove {
                            stick: JoystickType::Right,
                            x: self.last_right_stick_x,
                            y: new_value,
                            timestamp: now,
                        };
                        self.last_right_stick_y = new_value;
                        Some(raw_event)
                    }
                    Axis::LeftZ => {
                        let new_value = apply_deadzone(value, self.settings.joystick_deadzone);
                        if new_value > 0.1 {
                            debug!("Left trigger: {:.4}", new_value);
                        }

                        Some(RawControllerEvent::TriggerMove {
                            trigger: TriggerType::Left,
                            value: new_value,
                            timestamp: now,
                        })
                    }
                    Axis::RightZ => {
                        let new_value = apply_deadzone(value, self.settings.joystick_deadzone);
                        if new_value > 0.1 {
                            debug!("Right trigger: {:.4}", new_value);
                        }

                        Some(RawControllerEvent::TriggerMove {
                            trigger: TriggerType::Right,
                            value: new_value,
                            timestamp: now,
                        })
                    }
                    _ => {
                        debug!("Ignoring unsupported axis: {:?}", axis);
                        None
                    }
                }
            }
            EventType::ButtonPressed(button, _) => {
                info!(
                    "Button pressed: {:?} at {}",
                    button,
                    now.format("%H:%M:%S.%3f")
                );
                map_button(button).map(|button_type| {
                    debug!("Mapped to button type: {:?}", button_type);
                    RawControllerEvent::ButtonEvent {
                        button_type,
                        button_state: ButtonState::Pressed,
                        timestamp: now,
                    }
                })
            }
            EventType::ButtonReleased(button, _) => {
                info!(
                    "Button released: {:?} at {}",
                    button,
                    now.format("%H:%M:%S.%3f")
                );
                map_button(button).map(|button_type| {
                    debug!("Mapped to button type: {:?}", button_type);
                    RawControllerEvent::ButtonEvent {
                        button_type,
                        button_state: ButtonState::Released,
                        timestamp: now,
                    }
                })
            }
            EventType::ButtonRepeated(button, _) => {
                debug!("Button repeat ignored: {:?}", button);
                None
            }
            EventType::Connected => {
                info!("Controller connected event detected");
                None // This should trigger re-initialization in a production system
            }
            EventType::Disconnected => {
                warn!("Controller disconnected event detected");
                None // This should trigger safe shutdown in a production system
            }
            _ => {
                debug!("Unhandled event type: {:?}", event);
                None
            }
        }
    }
}

// Public interface for spawning and running the collector
pub struct CollectorHandle {
    event_sender: mpsc::Sender<RawControllerEvent>,
}

impl CollectorHandle {
    // Create a new collector and spawn it as a tokio task
    pub fn spawn(
        settings: Option<CollectorSettings>,
        event_sender: mpsc::Sender<RawControllerEvent>,
    ) -> Result<Self, CollectorError> {
        info!("Spawning Event Collector with settings: {:?}", settings);

        // Save a clone of the sender to return
        let sender_clone = event_sender.clone();

        // Initialize collector in Initializing state
        let collector = EventCollector::create(settings, event_sender)?;
        info!("Successfully created EventCollector instance");

        // Spawn tokio task for collector
        info!("Spawning Event Collector task");
        let task_handle = tokio::spawn(async move {
            // Initialize and start collector loop
            match collector.initialize() {
                Ok(mut collecting_state) => {
                    info!("Event Collector initialization successful, starting collection loop");
                    if let Err(e) = collecting_state.run_collection_loop() {
                        error!("Collector task terminated with error: {}", e);
                    } else {
                        info!("Event Collector task finished successfully"); // This shouldn't happen in practice
                    }
                }
                Err(e) => {
                    error!("Failed to initialize Event Collector: {}", e);
                }
            }
        });

        debug!("Tokio task spawned with handle: {:?}", task_handle);
        info!("Event Collector successfully started");

        Ok(Self {
            event_sender: sender_clone,
        })
    }

    // Get a sender for raw events
    pub fn event_sender(&self) -> mpsc::Sender<RawControllerEvent> {
        self.event_sender.clone()
    }
}

// Helper function to map gilrs Button to our ButtonType
fn map_button(button: Button) -> Option<ButtonType> {
    match button {
        Button::South => Some(ButtonType::A),
        Button::East => Some(ButtonType::B),
        Button::West => Some(ButtonType::Y),
        Button::North => Some(ButtonType::X),
        Button::Start => Some(ButtonType::Start),
        Button::Select => Some(ButtonType::Select),
        Button::LeftTrigger => Some(ButtonType::LeftBumper),
        Button::RightTrigger => Some(ButtonType::RightBumper),
        Button::LeftThumb => Some(ButtonType::LeftStick),
        Button::RightThumb => Some(ButtonType::RightStick),
        Button::DPadUp => Some(ButtonType::DPadUp),
        Button::DPadDown => Some(ButtonType::DPadDown),
        Button::DPadLeft => Some(ButtonType::DPadLeft),
        Button::DPadRight => Some(ButtonType::DPadRight),
        Button::Mode => Some(ButtonType::Guide),
        _ => None,
    }
}

// Helper function to apply deadzone to analog stick values
fn apply_deadzone(value: f32, deadzone: f32) -> f32 {
    if value.abs() < deadzone {
        0.0
    } else {
        // Rescale the value to the range outside the deadzone
        let sign = if value < 0.0 { -1.0 } else { 1.0 };
        sign * (value.abs() - deadzone) / (1.0 - deadzone)
    }
}
