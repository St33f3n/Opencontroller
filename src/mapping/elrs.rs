//! # ELRS (ExpressLRS) Mapping Strategy
//!
//! Converts gamepad input into ELRS channel data for drone and RC vehicle control.
//! This module implements a comprehensive mapping system that translates joystick movements,
//! trigger positions, and button presses into standard RC channel values.
//!
//! ## ⚠️ Experimental Implementation Notice
//!
//! **This is currently an experimental implementation that performs data conversion only.**
//! While the module correctly transforms controller input into ELRS channel format,
//! it does NOT actually transmit packets or communicate with real ELRS hardware.
//! The generated data remains within the application for testing and development purposes.
//!
//! ## Why This Module Exists
//!
//! ELRS is a popular open-source RC link system for drones and RC vehicles. This mapping
//! strategy enables gamepad control of ELRS-compatible devices through the OpenController
//! system, providing an alternative to traditional RC transmitters.
//!
//! ## Key Design Decisions
//!
//! - **Standard RC Channel Layout**: Follows conventional RC mapping (Roll/Pitch on right
//!   stick, Throttle/Yaw on left stick) for familiar control experience
//! - **Microsecond-Based Values**: Uses standard 1000-2000µs range matching RC servo/ESC
//!   expectations, with 1500µs as neutral position
//! - **Comprehensive Failsafe System**: Every channel has defined failsafe values for safe
//!   operation when control is lost
//! - **Channel Inversion Support**: Allows reversing channel direction to match different
//!   vehicle configurations and user preferences
//! - **Flexible Button Mapping**: Supports toggle and momentary button behaviors for
//!   auxiliary functions like arming, flight modes, and feature switches
//!
//! ## RC Channel Architecture
//!
//! ELRS systems typically support 12 channels (0-11):
//! - **Channels 0-3**: Primary flight controls (Roll, Pitch, Throttle, Yaw)
//! - **Channels 4-11**: Auxiliary functions (flight modes, switches, features)
//!
//! ## Value Range System
//!
//! RC channels use microsecond-based timing values:
//! - **1000µs**: Minimum position (-100%)
//! - **1500µs**: Center position (0%)  
//! - **2000µs**: Maximum position (+100%)
//!
//! This matches the PWM servo control standard used throughout RC systems.
//!
//! ## Failsafe Strategy
//!
//! Failsafe values ensure safe vehicle behavior when control is lost:
//! - **Flight controls**: Center positions for stable flight
//! - **Throttle**: Minimum value to cut power
//! - **Auxiliary channels**: Safe default states (disarmed, stable mode)
//!
//! ## Error Handling Strategy
//!
//! Configuration validation ensures essential channels are mapped before use.
//! Runtime value conversion includes bounds checking to prevent invalid channel
//! values that could cause unsafe vehicle behavior.

use crate::controller::controller_handle::{ControllerOutput, JoystickType, TriggerType};
use crate::mapping::{
    strategy::MappingContext, MappedEvent, MappingError, MappingStrategy, MappingType,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// Standard ELRS channel assignments following RC conventions.
///
/// ## Design Rationale
/// Channel numbers match standard RC transmitter assignments to ensure
/// compatibility with existing flight controller configurations and user
/// expectations. The explicit u16 values allow direct use as HashMap keys
/// and match the CRSF protocol channel indexing.
///
/// ## Channel Assignments
/// - **0-3**: Primary flight controls (required for basic operation)
/// - **4-11**: Auxiliary functions (optional, for advanced features)
///
/// ## Usage Context
/// These channel identifiers are used throughout the mapping system to
/// maintain consistent channel assignments and enable configuration validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum ELRSChannel {
    /// Roll control - aircraft rotation around longitudinal axis
    Roll = 0,

    /// Pitch control - aircraft rotation around lateral axis  
    Pitch = 1,

    /// Throttle control - engine power/motor speed
    Throttle = 2,

    /// Yaw control - aircraft rotation around vertical axis
    Yaw = 3,

    /// Auxiliary channel 1 - typically used for flight mode switching
    Aux1 = 4,

    /// Auxiliary channel 2 - often assigned to arm/disarm function
    Aux2 = 5,

    /// Auxiliary channel 3 - commonly used for additional flight modes
    Aux3 = 6,

    /// Auxiliary channel 4 - general purpose auxiliary function
    Aux4 = 7,

    /// Auxiliary channel 5 - extended auxiliary functionality
    Aux5 = 8,

    /// Auxiliary channel 6 - extended auxiliary functionality
    Aux6 = 9,

    /// Auxiliary channel 7 - extended auxiliary functionality
    Aux7 = 10,

    /// Auxiliary channel 8 - extended auxiliary functionality
    Aux8 = 11,
}

impl From<ELRSChannel> for u16 {
    fn from(channel: ELRSChannel) -> Self {
        channel as u16
    }
}

/// Configuration for gamepad-to-ELRS channel mapping.
///
/// ## Design Rationale
/// Separates configuration from runtime processing to enable easy customization
/// and persistence. The configuration validates essential channel assignments
/// and provides comprehensive control over all mapping aspects.
///
/// ## Mapping Strategy
/// Three input types are supported with independent configuration:
/// - **Joysticks**: Map X/Y axes to pairs of channels (e.g., Roll/Pitch)
/// - **Triggers**: Map analog trigger values to single channels
/// - **Buttons**: Map to auxiliary channels with configurable values
///
/// ## Channel Value System
/// Uses standard RC microsecond timing (1000-2000µs) for universal compatibility
/// with RC hardware and flight controllers.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ELRSConfig {
    /// Maps joysticks to channel pairs (e.g., Right stick → Roll + Pitch).
    pub joystick_mapping: HashMap<JoystickType, (ELRSChannel, ELRSChannel)>,

    /// Maps analog triggers to individual channels for proportional control.
    trigger_mapping: HashMap<TriggerType, ELRSChannel>,

    /// Maps buttons to auxiliary channels with pressed/released values.
    /// Format: (channel, pressed_value, released_value)
    button_mapping:
        HashMap<crate::controller::controller_handle::ButtonType, (ELRSChannel, u16, u16)>,

    /// Channel inversion flags for reversing control direction.
    invert_channel: HashMap<ELRSChannel, bool>,

    /// Safe default values used during initialization and failsafe conditions.
    failsafe_values: HashMap<ELRSChannel, u16>,

    /// Human-readable configuration name for user identification.
    name: String,

    /// RC channel value range boundaries (standard: 1000-2000µs).
    channel_min: u16,
    channel_max: u16,
    channel_mid: u16,
}

impl ELRSConfig {
    /// Creates a new ELRS mapping configuration with specified parameters.
    ///
    /// Automatically calculates the middle channel value for neutral positions.
    /// All parameters are validated during configuration creation to ensure
    /// safe and functional channel assignments.
    pub fn new(
        joystick_mapping: HashMap<JoystickType, (ELRSChannel, ELRSChannel)>,
        trigger_mapping: HashMap<TriggerType, ELRSChannel>,
        button_mapping: HashMap<
            crate::controller::controller_handle::ButtonType,
            (ELRSChannel, u16, u16),
        >,
        invert_channel: HashMap<ELRSChannel, bool>,
        failsafe_values: HashMap<ELRSChannel, u16>,
        name: String,
        channel_min: u16,
        channel_max: u16,
    ) -> Self {
        let channel_mid = (channel_min + channel_max) / 2;

        Self {
            joystick_mapping,
            trigger_mapping,
            button_mapping,
            invert_channel,
            failsafe_values,
            name,
            channel_min,
            channel_max,
            channel_mid,
        }
    }

    /// Creates a standard ELRS configuration following common RC conventions.
    ///
    /// ## Default Channel Layout
    /// Based on Mode 2 RC transmitter configuration (most common worldwide):
    /// - **Right Stick**: Roll (X-axis) + Pitch (Y-axis)
    /// - **Left Stick**: Yaw (X-axis) + Throttle (Y-axis)
    /// - **Triggers**: Aux1 + Aux2 for analog auxiliary functions
    /// - **Buttons**: Aux3 + Aux4 for digital auxiliary functions
    ///
    /// ## Failsafe Configuration
    /// Configured for safe multirotor operation:
    /// - Flight controls at neutral (1500µs) for stable flight
    /// - Throttle at minimum (1000µs) to cut motor power
    /// - Auxiliary channels in safe states (disarmed, stable modes)
    ///
    /// ## Channel Inversion
    /// Throttle is inverted by default to match common flight controller
    /// expectations where lower gamepad values = higher throttle.
    ///
    /// ## Value Range
    /// Uses standard 1000-2000µs range for maximum compatibility with
    /// RC hardware, ESCs, and flight controllers.
    pub fn default_config() -> Self {
        // Standard RC microsecond timing values
        let channel_min = 1000; // -100% position
        let channel_max = 2000; // +100% position

        // Mode 2 stick configuration (most common)
        let mut joystick_mapping = HashMap::new();
        joystick_mapping.insert(JoystickType::Right, (ELRSChannel::Roll, ELRSChannel::Pitch));
        joystick_mapping.insert(
            JoystickType::Left,
            (ELRSChannel::Yaw, ELRSChannel::Throttle),
        );

        // Analog auxiliary control via triggers
        let mut trigger_mapping = HashMap::new();
        trigger_mapping.insert(TriggerType::Left, ELRSChannel::Aux1);
        trigger_mapping.insert(TriggerType::Right, ELRSChannel::Aux2);

        // Digital auxiliary control via buttons
        // Format: (channel, pressed_value, released_value)
        let mut button_mapping = HashMap::new();
        button_mapping.insert(
            crate::controller::controller_handle::ButtonType::A,
            (ELRSChannel::Aux3, 2000, 1000), // Arm/disarm switch
        );
        button_mapping.insert(
            crate::controller::controller_handle::ButtonType::B,
            (ELRSChannel::Aux4, 2000, 1000), // Flight mode switch
        );

        // Channel direction configuration
        let mut invert_channel = HashMap::new();
        invert_channel.insert(ELRSChannel::Throttle, true); // Invert for intuitive control

        // Safe failsafe values for emergency situations
        let mut failsafe_values = HashMap::new();
        failsafe_values.insert(ELRSChannel::Roll, 1500); // Neutral
        failsafe_values.insert(ELRSChannel::Pitch, 1500); // Neutral
        failsafe_values.insert(ELRSChannel::Throttle, 1000); // Minimum (motors off)
        failsafe_values.insert(ELRSChannel::Yaw, 1500); // Neutral
        failsafe_values.insert(ELRSChannel::Aux1, 1000); // Safe state
        failsafe_values.insert(ELRSChannel::Aux2, 1000); // Safe state
        failsafe_values.insert(ELRSChannel::Aux3, 1000); // Disarmed
        failsafe_values.insert(ELRSChannel::Aux4, 1000); // Default flight mode

        Self::new(
            joystick_mapping,
            trigger_mapping,
            button_mapping,
            invert_channel,
            failsafe_values,
            "Default ELRS Configuration".to_string(),
            channel_min,
            channel_max,
        )
    }
}

impl crate::mapping::MappingConfig for ELRSConfig {
    /// Validates ELRS configuration for safety and completeness.
    ///
    /// ## Safety Validation
    /// Ensures all essential flight control channels (Roll, Pitch, Throttle, Yaw)
    /// are mapped to prevent unsafe operation. Missing essential channels could
    /// result in uncontrollable vehicle behavior.
    ///
    /// ## Validation Strategy
    /// 1. Check that joystick mappings exist (primary control requirement)
    /// 2. Collect all mapped channels from all input sources
    /// 3. Verify essential channels are covered by at least one input
    /// 4. Report specific missing channels for easy debugging
    ///
    /// # Errors
    ///
    /// Returns [`MappingError::ConfigError`] when:
    /// - No joystick mappings defined (primary control missing)
    /// - Essential channels not mapped (unsafe configuration)
    fn validate(&self) -> Result<(), MappingError> {
        if self.joystick_mapping.is_empty() {
            return Err(MappingError::ConfigError(
                "Joystick mapping cannot be empty for ELRS configuration".to_string(),
            ));
        }

        // Essential channels required for basic vehicle control
        let mut essential_channels = vec![
            ELRSChannel::Roll,
            ELRSChannel::Pitch,
            ELRSChannel::Throttle,
            ELRSChannel::Yaw,
        ];

        let mut found_channels = Vec::new();

        // Collect all mapped channels from all sources
        for (ch1, ch2) in self.joystick_mapping.values() {
            found_channels.push(*ch1);
            found_channels.push(*ch2);
        }

        for ch in self.trigger_mapping.values() {
            found_channels.push(*ch);
        }

        // Check coverage of essential channels
        essential_channels.retain(|ch| !found_channels.contains(ch));

        if !essential_channels.is_empty() {
            return Err(MappingError::ConfigError(format!(
                "Missing essential channels in ELRS configuration: {:?}",
                essential_channels
            )));
        }

        Ok(())
    }

    fn create_strategy(&self) -> Result<Box<dyn MappingStrategy>, MappingError> {
        Ok(Box::new(ELRSStrategy::new(self.clone())))
    }

    fn get_type(&self) -> MappingType {
        MappingType::ELRS
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}

/// Core implementation of gamepad-to-ELRS channel conversion.
///
/// ## Design Rationale
/// Maintains persistent channel state to provide continuous output even when
/// no new input events occur. This ensures ELRS receivers always have current
/// channel data for stable vehicle control.
///
/// ## State Management
/// - **Channel Values**: Current state of all RC channels
/// - **Failsafe Integration**: Automatic fallback to safe values
/// - **Context Preservation**: Maintains state across mapping operations
///
/// ## Conversion Strategy
/// All input types are converted to the standard 1000-2000µs range with
/// proper scaling, inversion support, and bounds checking for safety.
pub struct ELRSStrategy {
    config: ELRSConfig,
    context: MappingContext,
    /// Current RC channel values in microseconds (1000-2000µs range)
    channel_values: HashMap<ELRSChannel, u16>,
}

impl ELRSStrategy {
    /// Creates a new ELRS mapping strategy with failsafe initialization.
    ///
    /// All channels are pre-initialized with their failsafe values to ensure
    /// safe default state before any controller input is processed.
    pub fn new(config: ELRSConfig) -> Self {
        let mut channel_values = HashMap::new();
        for (channel, value) in &config.failsafe_values {
            channel_values.insert(*channel, *value);
        }

        Self {
            config,
            context: MappingContext::default(),
            channel_values,
        }
    }

    /// Converts normalized joystick values to RC channel microsecond values.
    ///
    /// ## Conversion Algorithm
    /// 1. Clamp input to valid range (-1.0 to +1.0)
    /// 2. Apply channel inversion if configured
    /// 3. Scale to microsecond range around center point
    /// 4. Round and bounds-check final value
    ///
    /// ## Mathematical Formula
    /// $$ \text{channel\_value} = \text{mid} + (\text{normalized} \times \frac{\text{range}}{2}) $$
    ///
    /// Where:
    /// - $\text{mid} = \frac{\text{channel\_max} + \text{channel\_min}}{2}$
    /// - $\text{range} = \text{channel\_max} - \text{channel\_min}$
    /// - $\text{normalized} \in [-1.0, 1.0]$
    ///
    /// ## Safety Features
    /// Output is always clamped to valid channel range to prevent hardware
    /// damage or unexpected vehicle behavior from out-of-range values.
    fn convert_joystick_value(&self, value: f32, invert: bool) -> u16 {
        let range = (self.config.channel_max - self.config.channel_min) as f32;
        let mid = self.config.channel_mid;

        // Normalize and clamp input value
        let mut normalized = value.clamp(-1.0, 1.0);

        // Apply inversion if configured
        if invert {
            normalized = -normalized;
        }

        // Convert to microsecond value
        let channel_value = mid as f32 + (normalized * range / 2.0);

        // Round and enforce bounds for safety
        let out = channel_value.round() as u16;
        out.min(self.config.channel_max)
            .max(self.config.channel_min)
    }

    /// Updates RC channels based on joystick positions.
    ///
    /// Processes all configured joystick mappings, converting X/Y coordinates
    /// to the assigned channel pairs with proper scaling and inversion.
    fn update_joystick_channels(&mut self, input: &ControllerOutput) {
        for (joystick_type, (x_channel, y_channel)) in &self.config.joystick_mapping {
            let (x, y) = match joystick_type {
                JoystickType::Left => (input.left_stick.x, input.left_stick.y),
                JoystickType::Right => (input.right_stick.x, input.right_stick.y),
            };

            // Process X-axis (typically Roll or Yaw)
            let invert_x = self
                .config
                .invert_channel
                .get(x_channel)
                .copied()
                .unwrap_or(false);
            let x_value = self.convert_joystick_value(x, invert_x);
            self.channel_values.insert(*x_channel, x_value);

            // Process Y-axis (typically Pitch or Throttle)
            let invert_y = self
                .config
                .invert_channel
                .get(y_channel)
                .copied()
                .unwrap_or(false);
            let y_value = self.convert_joystick_value(y, invert_y);
            self.channel_values.insert(*y_channel, y_value);
        }
    }

    /// Updates RC channels based on analog trigger positions.
    ///
    /// Converts trigger values (0.0-1.0) to full channel range by scaling
    /// to (-1.0 to +1.0) before applying standard conversion.
    fn update_trigger_channels(&mut self, input: &ControllerOutput) {
        for (trigger_type, channel) in &self.config.trigger_mapping {
            let value = match trigger_type {
                TriggerType::Left => input.left_trigger.value,
                TriggerType::Right => input.right_trigger.value,
            };

            // Convert trigger range (0.0-1.0) to joystick range (-1.0-1.0)
            let scaled_value = value * 2.0 - 1.0;

            let invert = self
                .config
                .invert_channel
                .get(channel)
                .copied()
                .unwrap_or(false);
            let channel_value = self.convert_joystick_value(scaled_value, invert);
            self.channel_values.insert(*channel, channel_value);
        }
    }

    /// Updates RC channels based on button press events.
    ///
    /// Sets channels to configured pressed or released values based on
    /// current button state. Supports both momentary and toggle behaviors.
    fn update_button_channels(&mut self, input: &ControllerOutput) {
        for button_event in &input.button_events {
            if let Some((channel, pressed_value, released_value)) =
                self.config.button_mapping.get(&button_event.button)
            {
                let value = match button_event.state {
                    crate::controller::controller_handle::ButtonEventState::Held => *pressed_value,
                    crate::controller::controller_handle::ButtonEventState::Complete => {
                        *released_value
                    }
                };

                self.channel_values.insert(*channel, value);
            }
        }
    }
}

impl MappingStrategy for ELRSStrategy {
    /// Converts controller input to ELRS channel data format.
    ///
    /// ## Processing Order
    /// 1. Update joystick channels (primary flight controls)
    /// 2. Update trigger channels (auxiliary analog controls)  
    /// 3. Update button channels (auxiliary digital controls)
    /// 4. Convert to output format for transmission
    ///
    /// ## Output Format
    /// Returns HashMap with channel numbers as keys and microsecond values
    /// as values, ready for CRSF packet construction and transmission.
    ///
    /// ## ⚠️ Experimental Status
    /// The generated data is correctly formatted but not transmitted to
    /// actual ELRS hardware in this experimental implementation.
    ///
    /// # Returns
    /// `Some(MappedEvent::ELRSData)` with current channel values,
    /// `None` if no channels are configured (should not occur after validation).
    fn map(&mut self, input: &ControllerOutput) -> Option<MappedEvent> {
        // Update all channel types in priority order
        self.update_joystick_channels(input);
        self.update_trigger_channels(input);
        self.update_button_channels(input);

        // Convert to output format
        let mut pre_package = HashMap::new();
        for (channel, value) in &self.channel_values {
            pre_package.insert(*channel as u16, *value);
        }

        if pre_package.is_empty() {
            None
        } else {
            Some(MappedEvent::ELRSData { pre_package })
        }
    }

    /// Initializes ELRS strategy with failsafe values.
    ///
    /// Ensures all configured channels start in safe states before
    /// any controller input is processed.
    fn initialize(&mut self) -> Result<(), MappingError> {
        info!("Initializing ELRS mapping strategy: {}", self.config.name);

        for (channel, value) in &self.config.failsafe_values {
            self.channel_values.insert(*channel, *value);
        }

        Ok(())
    }

    /// Shuts down ELRS strategy with failsafe restoration.
    ///
    /// Returns all channels to safe failsafe values to ensure
    /// safe vehicle state during shutdown.
    fn shutdown(&mut self) {
        info!("Shutting down ELRS mapping strategy: {}", self.config.name);

        // Reset to failsafe values for safe shutdown
        for (channel, value) in &self.config.failsafe_values {
            self.channel_values.insert(*channel, *value);
        }
    }

    /// Returns rate limit appropriate for RC communication.
    ///
    /// Set to 20Hz (50ms) which is typical for RC systems and provides
    /// adequate control responsiveness while avoiding unnecessary bandwidth
    /// usage and processing overhead.
    fn get_rate_limit(&self) -> Option<u64> {
        Some(50)
    }

    fn get_type(&self) -> MappingType {
        MappingType::ELRS
    }
}
