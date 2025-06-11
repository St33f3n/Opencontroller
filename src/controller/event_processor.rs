//! Event processor with state machine for structured controller output
//!
//! Transforms raw gamepad events into structured [`ControllerOutput`] using a 3-state machine:
//! Waiting → Processing → Updating → repeat
//!
//! Key features:
//! - Button release tracking across cycles for held buttons
//! - Min/max/delta calculation for analog inputs
//! - 130ms processing intervals optimized for human reaction time

use chrono::{DateTime, Local};
use statum::{machine, state};
use std::collections::HashMap;
use std::time::SystemTime;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::event_collector::{
    ButtonState, ButtonType, JoystickType, RawControllerEvent, TriggerType,
};
/// Button state for tracking duration across processing cycles
#[derive(Clone, Debug, PartialEq)]
pub enum ButtonEventState {
    Held,     // Button is still being pressed
    Complete, // Button has been released
}

/// Main output structure containing all processed controller state
///
/// Supports key combinations through multiple simultaneous button events.
/// All analog values include current position plus min/max/delta tracking.
#[derive(Clone, Debug)]
pub struct ControllerOutput {
    pub left_stick: JoystickPosition,
    pub right_stick: JoystickPosition,
    pub left_trigger: TriggerValue,
    pub right_trigger: TriggerValue,
    pub button_events: Vec<ButtonEvent>,
    pub timestamp: SystemTime,
}

impl Default for ControllerOutput {
    fn default() -> Self {
        Self {
            left_stick: Default::default(),
            right_stick: Default::default(),
            left_trigger: Default::default(),
            right_trigger: Default::default(),
            button_events: Vec::new(),
            timestamp: SystemTime::now(),
        }
    }
}

/// Joystick position with tracking data for mapping engines
#[derive(Clone, Debug)]
pub struct JoystickPosition {
    pub x: f32,
    pub y: f32,
    pub x_min: f32, // Min value seen this cycle
    pub x_max: f32, // Max value seen this cycle
    pub y_min: f32,
    pub y_max: f32,
    pub delta_x: f32, // Change from last cycle
    pub delta_y: f32,
}
impl Default for JoystickPosition {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            x_min: 0.0,
            x_max: 0.0,
            y_min: 0.0,
            y_max: 0.0,
            delta_x: 0.0,
            delta_y: 0.0,
        }
    }
}

/// Trigger value with tracking data
#[derive(Clone, Debug)]
pub struct TriggerValue {
    pub value: f32,
    pub min: f32,   // Min value seen this cycle
    pub max: f32,   // Max value seen this cycle
    pub delta: f32, // Change from last cycle
}
impl Default for TriggerValue {
    fn default() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 0.0,
            delta: 0.0,
        }
    }
}

/// Button event with duration tracking
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonEvent {
    pub button: ButtonType,
    pub duration_ms: f64,        // How long button has been held
    pub state: ButtonEventState, // Held or Complete
}

#[derive(Clone, Debug)]
struct PendingButtonRelease {
    timestamp: DateTime<Local>,
}

#[derive(Debug, Clone)]
pub struct EventBatch {
    pub events: Vec<RawControllerEvent>,
}

/// Processor configuration
#[derive(Clone, Debug)]
pub struct ProcessorSettings {
    pub processing_interval_ms: u64,
    pub button_press_threshold_ms: u32,
}

impl Default for ProcessorSettings {
    fn default() -> Self {
        Self {
            processing_interval_ms: 130,
            button_press_threshold_ms: 30,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessorError {
    #[error("Failed to initialize processor: {0}")]
    InitializationError(String),

    #[error("Failed to receive events: {0}")]
    EventReceiveError(String),

    #[error("Failed to process events: {0}")]
    EventProcessingError(String),

    #[error("Failed to update state: {0}")]
    StateUpdateError(String),
}

// State machine states for event processing pipeline
#[state]
#[derive(Debug, Clone)]
pub enum ProcessingState {
    Waiting,                // Collecting events from queue
    Processing(EventBatch), // Processing collected events
    Updating,               // Broadcasting processed output
}

/// Event processor using statum state machine
///
/// Manages button release tracking across cycles to handle held buttons correctly.
/// Processes events in batches every 130ms for optimal responsiveness.
#[machine]
#[derive(Debug)]
pub struct EventProcessor<S: ProcessingState> {
    event_receiver: mpsc::Receiver<RawControllerEvent>,
    settings: ProcessorSettings,
    output: ControllerOutput,
    state_sender: mpsc::Sender<ControllerOutput>,
    // Critical: tracks buttons pressed in previous cycles without release events
    pending_button_releases: HashMap<ButtonType, PendingButtonRelease>,
}

impl<S: ProcessingState> EventProcessor<S> {
    pub fn update_settings(&mut self, settings: ProcessorSettings) {
        self.settings = settings;
    }

    pub fn settings(&self) -> &ProcessorSettings {
        &self.settings
    }
}

impl EventProcessor<Waiting> {
    /// Creates processor in Waiting state
    pub fn create(
        event_receiver: mpsc::Receiver<RawControllerEvent>,
        output_sender: mpsc::Sender<ControllerOutput>,
        settings: Option<ProcessorSettings>,
    ) -> Result<Self, ProcessorError> {
        let settings = settings.unwrap_or_default();

        // Create initial output state
        let output = ControllerOutput::default();
        debug!("Created initial ControllerOutput state");

        Ok(Self::new(
            event_receiver,
            settings,
            output,
            output_sender,
            HashMap::new(),
        ))
    }

    /// Collects all available events from queue and transitions to Processing
    pub async fn wait_and_collect(mut self) -> Result<EventProcessor<Processing>, ProcessorError> {
        let mut events = Vec::new();

        // Drain all available events
        loop {
            match self.event_receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    return Err(ProcessorError::EventReceiveError(
                        "Event channel disconnected".to_string(),
                    ));
                }
            }
        }

        let event_batch = EventBatch { events };
        Ok(self.transition_with(event_batch))
    }
}
impl EventProcessor<Processing> {
    /// Processes collected events and transitions to Updating state
    ///
    /// Handles button release tracking across cycles - buttons without release events
    /// are tracked as "held" with continuously updated duration.
    pub fn process_events(mut self) -> Result<EventProcessor<Updating>, ProcessorError> {
        let raw_events = if let Some(event_batch) = self.get_state_data() {
            event_batch.events.clone()
        } else {
            Vec::new()
        };

        let has_pending_releases = !self.pending_button_releases.is_empty();

        if raw_events.is_empty() && !has_pending_releases {
            self.output.button_events.clear();
        } else {
            if !raw_events.is_empty() {
                self.process_joystick_events(&raw_events)?;
                self.process_trigger_events(&raw_events)?;
            }
            // Always process buttons if we have new events OR pending releases
            self.process_button_events(&raw_events)?;
        }

        self.output.timestamp = SystemTime::now();
        Ok(self.transition())
    }
    fn process_joystick_events(
        &mut self,
        events: &[RawControllerEvent],
    ) -> Result<(), ProcessorError> {
        // Storage for left and right stick values
        let mut left_x_values = Vec::new();
        let mut left_y_values = Vec::new();
        let mut right_x_values = Vec::new();
        let mut right_y_values = Vec::new();

        // Extract all joystick values
        for event in events {
            if let RawControllerEvent::JoystickMove {
                stick,
                x,
                y,
                timestamp: _,
            } = event
            {
                match stick {
                    JoystickType::Left => {
                        left_x_values.push(*x);
                        left_y_values.push(*y);
                    }
                    JoystickType::Right => {
                        right_x_values.push(*x);
                        right_y_values.push(*y);
                    }
                }
            }
        }

        // Process left stick
        if !left_x_values.is_empty() || !left_y_values.is_empty() {
            // Get latest values or keep current values
            let left_x = left_x_values
                .last()
                .cloned()
                .unwrap_or(self.output.left_stick.x);
            let left_y = left_y_values
                .last()
                .cloned()
                .unwrap_or(self.output.left_stick.y);

            // Include current values in min/max calculation
            let mut extended_left_x_values = left_x_values.clone();
            let mut extended_left_y_values = left_y_values.clone();
            extended_left_x_values.push(self.output.left_stick.x);
            extended_left_y_values.push(self.output.left_stick.y);

            // Find min/max values
            let left_x_min = extended_left_x_values
                .iter()
                .fold(f32::MAX, |acc, &val| acc.min(val));
            let left_x_max = extended_left_x_values
                .iter()
                .fold(f32::MIN, |acc, &val| acc.max(val));
            let left_y_min = extended_left_y_values
                .iter()
                .fold(f32::MAX, |acc, &val| acc.min(val));
            let left_y_max = extended_left_y_values
                .iter()
                .fold(f32::MIN, |acc, &val| acc.max(val));

            // Calculate deltas
            let left_delta_x = left_x - self.output.left_stick.x;
            let left_delta_y = left_y - self.output.left_stick.y;

            // Update controller output
            self.output.left_stick = JoystickPosition {
                x: left_x,
                y: left_y,
                x_min: left_x_min,
                x_max: left_x_max,
                y_min: left_y_min,
                y_max: left_y_max,
                delta_x: left_delta_x,
                delta_y: left_delta_y,
            };
        }

        // Process right stick
        if !right_x_values.is_empty() || !right_y_values.is_empty() {
            // Get latest values or keep current values
            let right_x = right_x_values
                .last()
                .cloned()
                .unwrap_or(self.output.right_stick.x);
            let right_y = right_y_values
                .last()
                .cloned()
                .unwrap_or(self.output.right_stick.y);

            // Include current values in min/max calculation
            let mut extended_right_x_values = right_x_values.clone();
            let mut extended_right_y_values = right_y_values.clone();
            extended_right_x_values.push(self.output.right_stick.x);
            extended_right_y_values.push(self.output.right_stick.y);

            // Find min/max values
            let right_x_min = extended_right_x_values
                .iter()
                .fold(f32::MAX, |acc, &val| acc.min(val));
            let right_x_max = extended_right_x_values
                .iter()
                .fold(f32::MIN, |acc, &val| acc.max(val));
            let right_y_min = extended_right_y_values
                .iter()
                .fold(f32::MAX, |acc, &val| acc.min(val));
            let right_y_max = extended_right_y_values
                .iter()
                .fold(f32::MIN, |acc, &val| acc.max(val));

            // Calculate deltas
            let right_delta_x = right_x - self.output.right_stick.x;
            let right_delta_y = right_y - self.output.right_stick.y;

            // Update controller output
            self.output.right_stick = JoystickPosition {
                x: right_x,
                y: right_y,
                x_min: right_x_min,
                x_max: right_x_max,
                y_min: right_y_min,
                y_max: right_y_max,
                delta_x: right_delta_x,
                delta_y: right_delta_y,
            };
        }

        Ok(())
    }

    // Process trigger events
    fn process_trigger_events(
        &mut self,
        events: &[RawControllerEvent],
    ) -> Result<(), ProcessorError> {
        // Storage for left and right trigger values
        let mut left_values = Vec::new();
        let mut right_values = Vec::new();

        // Extract all trigger values
        for event in events {
            if let RawControllerEvent::TriggerMove {
                trigger,
                value,
                timestamp: _,
            } = event
            {
                match trigger {
                    TriggerType::Left => {
                        left_values.push(*value);
                    }
                    TriggerType::Right => {
                        right_values.push(*value);
                    }
                }
            }
        }

        // Process left trigger
        if !left_values.is_empty() {
            // Get latest value or keep current value
            let left_value = left_values
                .last()
                .cloned()
                .unwrap_or(self.output.left_trigger.value);

            // Include current value in min/max calculation
            let mut extended_left_values = left_values.clone();
            extended_left_values.push(self.output.left_trigger.value);

            // Find min/max values
            let left_min_value = extended_left_values
                .iter()
                .fold(f32::MAX, |acc, &val| acc.min(val));
            let left_max_value = extended_left_values
                .iter()
                .fold(f32::MIN, |acc, &val| acc.max(val));

            // Calculate delta
            let left_delta = left_value - self.output.left_trigger.value;

            // Update controller output
            self.output.left_trigger = TriggerValue {
                value: left_value,
                min: left_min_value,
                max: left_max_value,
                delta: left_delta,
            };
        }

        // Process right trigger
        if !right_values.is_empty() {
            // Get latest value or keep current value
            let right_value = right_values
                .last()
                .cloned()
                .unwrap_or(self.output.right_trigger.value);

            // Include current value in min/max calculation
            let mut extended_right_values = right_values.clone();
            extended_right_values.push(self.output.right_trigger.value);

            // Find min/max values
            let right_min_value = extended_right_values
                .iter()
                .fold(f32::MAX, |acc, &val| acc.min(val));
            let right_max_value = extended_right_values
                .iter()
                .fold(f32::MIN, |acc, &val| acc.max(val));

            // Calculate delta
            let right_delta = right_value - self.output.right_trigger.value;

            // Update controller output
            self.output.right_trigger = TriggerValue {
                value: right_value,
                min: right_min_value,
                max: right_max_value,
                delta: right_delta,
            };
        }

        Ok(())
    }

    fn process_button_events(
        &mut self,
        events: &[RawControllerEvent],
    ) -> Result<(), ProcessorError> {
        // Clear existing button events
        self.output.button_events.clear();

        // Group events by button
        let mut events_per_button: HashMap<ButtonType, Vec<(ButtonState, DateTime<Local>)>> =
            HashMap::new();

        // Add pending button releases to the event map
        for (button, release) in &self.pending_button_releases {
            debug!(
                "Pending Release Type: {:?}, Timestamp: {:?}",
                button, release.timestamp
            );
            events_per_button.insert(
                button.clone(),
                vec![(ButtonState::Pressed, release.timestamp)],
            );
        }

        // Clear pending releases to rebuild them
        self.pending_button_releases = HashMap::new();

        // Process new button events
        for event in events {
            if let RawControllerEvent::ButtonEvent {
                button_type,
                button_state,
                timestamp,
            } = event
            {
                if !events_per_button.contains_key(button_type) {
                    events_per_button.insert(
                        button_type.clone(),
                        vec![(button_state.clone(), *timestamp)],
                    );
                } else {
                    events_per_button
                        .get_mut(button_type)
                        .unwrap()
                        .push((button_state.clone(), *timestamp));
                }
            }
        }

        // Current time for calculating held button durations
        let now = Local::now();

        // Process each button's events
        let mut processed_button_events: Vec<ButtonEvent> = Vec::new();
        for (button, events) in &mut events_per_button {
            // Sort events by timestamp
            events.sort_by(|event1, event2| event1.1.cmp(&event2.1));
            debug!("Sorted events for button {:?}: {:?}", button, events);

            let mut i = 0;
            while i < events.len() {
                let event = &events[i];

                if event.0 == ButtonState::Pressed {
                    // Check if there's a corresponding Release event
                    if i + 1 < events.len() && events[i + 1].0 == ButtonState::Released {
                        let next_event = &events[i + 1];

                        // Calculate duration using chrono
                        let duration = next_event.1 - event.1;
                        let duration_ms = duration.num_milliseconds() as f64;

                        debug!("Button {:?} press duration: {}ms", button, duration_ms);

                        // Add button event with precise duration
                        processed_button_events.push(ButtonEvent {
                            button: button.clone(),
                            duration_ms,
                            state: ButtonEventState::Complete,
                        });

                        // Skip both events
                        i += 2;
                    } else {
                        // No release event found - button is still held

                        // Calculate duration from press time to now
                        let duration = now - event.1;
                        let duration_ms = duration.num_milliseconds() as f64;

                        debug!(
                            "Button {:?} press duration: {}ms (Held)",
                            button, duration_ms
                        );

                        // Add button event with calculated duration and Held state
                        processed_button_events.push(ButtonEvent {
                            button: button.clone(),
                            duration_ms,
                            state: ButtonEventState::Held,
                        });

                        // Save as pending for next cycle
                        debug!("Saving pending button release for {:?}", button);
                        self.pending_button_releases
                            .insert(button.clone(), PendingButtonRelease { timestamp: event.1 });

                        // Skip just this event
                        i += 1;
                    }
                } else if event.0 == ButtonState::Released {
                    // A Release without a Pressed - unusual situation
                    error!(
                        "Found a Released event without a preceding Pressed event: {:?}",
                        event
                    );
                    i += 1;
                }
            }
        }

        // Update output with processed button events
        self.output.button_events = processed_button_events;
        Ok(())
    }
}

// Implementation for Updating state
impl EventProcessor<Updating> {
    /// Broadcasts processed output and transitions back to Waiting
    pub fn update_state(self) -> Result<EventProcessor<Waiting>, ProcessorError> {
        debug!("Updating controller state through watch channel");

        // Prepare debug summary
        let summary = format!(
            "L:({:.2},{:.2}) R:({:.2},{:.2}) LT:{:.2} RT:{:.2} Buttons:{}",
            self.output.left_stick.x,
            self.output.left_stick.y,
            self.output.right_stick.x,
            self.output.right_stick.y,
            self.output.left_trigger.value,
            self.output.right_trigger.value,
            self.output.button_events.len()
        );

        let send_result = self.state_sender.try_send(self.output.clone());

        // Send updated state through watch channel
        match send_result {
            Ok(_) => {
                debug!("State updated successfully: {}", summary);
            }
            Err(e) => {
                error!("Failed to update controller state: {}", e);
                return Err(ProcessorError::StateUpdateError(format!(
                    "Failed to send state update: {}",
                    e
                )));
            }
        }

        // Transition back to Waiting state
        debug!("Transitioning back to Waiting state");
        Ok(self.transition())
    }
}

// Public interface for spawning and running the processor
pub struct ProcessorHandle {
    state_sender: mpsc::Sender<ControllerOutput>,
}

impl ProcessorHandle {
    // Create a new processor and spawn it as a tokio task
    pub fn spawn(
        event_receiver: mpsc::Receiver<RawControllerEvent>,
        output_sender: mpsc::Sender<ControllerOutput>,
        settings: Option<ProcessorSettings>,
    ) -> Result<Self, ProcessorError> {
        info!("Spawning Event Processor with settings: {:?}", settings);

        let processor = EventProcessor::create(event_receiver, output_sender.clone(), settings)?;

        let _task_handle = tokio::spawn(async move {
            if let Err(e) = run_processor_loop(processor).await {
                error!("Processor task terminated with error: {}", e);
            }
        });

        Ok(Self {
            state_sender: output_sender,
        })
    }
}

// Run the processor loop
async fn run_processor_loop(mut processor: EventProcessor<Waiting>) -> Result<(), ProcessorError> {
    let settings = processor.settings().clone();
    info!(
        "Starting processor loop with {}ms interval",
        settings.processing_interval_ms
    );

    // Create interval for processing cycle
    let mut interval_timer = tokio::time::interval(tokio::time::Duration::from_millis(
        settings.processing_interval_ms,
    ));

    // Stats for performance monitoring
    let mut cycles = 0;
    let mut total_events = 0;
    let mut last_stats_time = Local::now();
    let stats_interval = chrono::Duration::seconds(30);

    // Main processor loop
    info!("Entering main processor loop");
    loop {
        debug!(
            "Waiting for next interval tick ({} ms)",
            settings.processing_interval_ms
        );
        // Wait for the next interval tick
        interval_timer.tick().await;

        let cycle_start = Local::now();
        debug!(
            "Starting processing cycle at {}",
            cycle_start.format("%H:%M:%S.%3f")
        );

        // Run one cycle of the processor state machine
        let processing_state = processor.wait_and_collect().await?;

        // Count events in this cycle for stats
        let event_count = if let Some(event_batch) = processing_state.get_state_data() {
            event_batch.events.len()
        } else {
            0
        };
        total_events += event_count;

        let updating_state = processing_state.process_events()?;
        processor = updating_state.update_state()?;

        // Increment cycle counter
        cycles += 1;

        // Calculate cycle duration
        let cycle_end = Local::now();
        let cycle_duration = cycle_end - cycle_start;
        debug!(
            "Processing cycle completed in {:.2} ms",
            cycle_duration.num_milliseconds()
        );

        // Log stats periodically
        let now = Local::now();
        if now - last_stats_time > stats_interval {
            let elapsed_seconds = (now - last_stats_time).num_seconds();
            debug!(
                "Processor stats: {} cycles, {} events in {} seconds",
                cycles, total_events, elapsed_seconds
            );
            debug!(
                "Average: {:.2} events/cycle, {:.2} cycles/sec, {:.2} events/sec",
                total_events as f64 / cycles as f64,
                cycles as f64 / elapsed_seconds as f64,
                total_events as f64 / elapsed_seconds as f64
            );

            // Reset counters
            cycles = 0;
            total_events = 0;
            last_stats_time = now;
        }

        // Check if interval time needs to be updated (in case settings changed)
        let new_interval_time =
            tokio::time::Duration::from_millis(processor.settings().processing_interval_ms);

        if new_interval_time != interval_timer.period() {
            debug!(
                "Updating interval time to {} ms",
                processor.settings().processing_interval_ms
            );
            interval_timer = tokio::time::interval(new_interval_time);
        }

        if let Err(e) = processor.state_sender.try_send(processor.output.clone()) {
            warn!("Failed to send controller output: {}", e);
        }
    }
}
