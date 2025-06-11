//! # Keyboard Mapping Strategy
//!
//! Converts gamepad input into keyboard events for text input and UI navigation.
//! This module implements a sophisticated region-based joystick mapping system
//! that allows typing the entire alphabet using dual-joystick combinations.
//!
//! ## Why This Module Exists
//!
//! The keyboard mapping enables text input using only a gamepad, which is essential
//! for the OpenController's vision of unified device control. Instead of requiring
//! a physical keyboard, users can type using joystick combinations and button presses.
//!
//! ## Key Design Decisions
//!
//! - **Region-based Joystick Mapping**: Divides joystick movement space into 8 directional
//!   regions plus center, allowing 9x9=81 possible combinations (26 letters + extras)
//! - **Hysteresis Implementation**: Prevents flickering between regions when joystick
//!   position is near boundaries by using different thresholds for entering vs. leaving
//! - **Polar Coordinate System**: Converts cartesian joystick coordinates to polar
//!   (angle + magnitude) for more intuitive directional mapping
//! - **Dual-Joystick Alphabet**: Left joystick + right joystick combinations map to
//!   specific letters, providing systematic text input method
//!
//! ## Region Layout Strategy
//!
//! The alphabet mapping follows a logical pattern:
//! - A-H: Left joystick directions + right joystick center
//! - I-P: Left joystick center + right joystick directions  
//! - Q-Z: Symmetric combinations for remaining letters
//!
//! ## Hysteresis Rationale
//!
//! Without hysteresis, small joystick movements near region boundaries would cause
//! rapid region switching, making precise text input impossible. The implementation
//! uses 8% hysteresis factor to create stable region detection.
//!
//! ## Error Handling Strategy
//!
//! Input validation occurs early - invalid configurations are rejected during setup.
//! Runtime errors (e.g., unmapped combinations) simply produce no output rather than
//! failing, maintaining system stability during user interaction.

use crate::controller::controller_handle::{ButtonType, ControllerOutput};
use crate::mapping::{
    strategy::MappingContext, MappedEvent, MappingError, MappingStrategy, MappingType,
};
use eframe::egui::{self, Event, Key, Modifiers};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use tracing::{debug, error, info, warn};

macro_rules! map_insert {
    ($map:expr, $regions:expr, $key:expr, $upper:expr, $lower:expr) => {
        $map.insert($regions, ($key, $upper.to_string(), $lower.to_string()));
    };
}

/// Hysteresis factor for region detection to prevent boundary flickering.
///
/// Set to 8% of region size to provide stable region transitions while
/// maintaining responsive input. Higher values increase stability but
/// reduce precision; lower values increase sensitivity but may cause flickering.
pub const REGION_HYSTERESIS: f32 = 0.08;

/// Represents the 8 cardinal and intercardinal directions plus center position.
///
/// ## Design Rationale
/// Uses 8-direction system (N, NE, E, SE, S, SW, W, NW) plus center to provide
/// 9 distinct positions per joystick. This gives 9x9=81 possible combinations,
/// more than sufficient for alphabet (26) plus numbers and symbols.
///
/// ## Usage Context
/// Each section corresponds to a specific angular range in polar coordinates.
/// Center section is determined by magnitude threshold rather than angle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Section {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
    #[default]
    Center,
}

/// Pre-defined region constants for consistent joystick area definitions.
pub const REGION_CENTER: Region = Region {
    min_angle: 0.0,
    max_angle: 360.0,
    inner_max_angle: 360.0,
    inner_min_angle: 0.0,
    min_magnitute: 0.0,
    max_magnitute: 0.25,
    inner_min_magnitute: 0.0,
    inner_max_magnitute: 0.25,
    section: Section::Center,
};

pub const REGION_NORTH: Region = Region::new(0.0, 45.0, 0.3, 1.0, Section::North);
pub const REGION_NORTHEAST: Region = Region::new(45.0, 90.0, 0.3, 1.0, Section::NorthEast);
pub const REGION_EAST: Region = Region::new(90.0, 135.0, 0.3, 1.0, Section::East);
pub const REGION_SOUTHEAST: Region = Region::new(135.0, 180.0, 0.3, 1.0, Section::SouthEast);
pub const REGION_SOUTH: Region = Region::new(180.0, 225.0, 0.3, 1.0, Section::South);
pub const REGION_SOUTHWEST: Region = Region::new(225.0, 270.0, 0.3, 1.0, Section::SouthWest);
pub const REGION_WEST: Region = Region::new(270.0, 315.0, 0.3, 1.0, Section::West);
pub const REGION_NORTHWEST: Region = Region::new(315.0, 360.0, 0.3, 1.0, Section::NorthWest);

pub const ALL_REGIONS: [Region; 8] = standard_regions();

/// Returns all standard directional regions as a compile-time constant array.
pub const fn standard_regions() -> [Region; 8] {
    [
        REGION_NORTH,
        REGION_NORTHEAST,
        REGION_EAST,
        REGION_SOUTHEAST,
        REGION_SOUTH,
        REGION_SOUTHWEST,
        REGION_WEST,
        REGION_NORTHWEST,
    ]
}

/// Defines a joystick region with hysteresis for stable boundary detection.
///
/// ## Design Rationale
/// Uses dual-threshold system (inner/outer boundaries) to implement hysteresis.
/// This prevents rapid switching between regions when joystick position oscillates
/// near a boundary, which would make text input unusable.
///
/// ## Coordinate System
/// - Angles: 0° = North, increasing clockwise (0-360°)
/// - Magnitude: 0.0 = center, 1.0 = maximum deflection
/// - Hysteresis: Inner boundaries for entering, outer boundaries for leaving
///
/// ## Usage Context
/// Regions are used as HashMap keys for joystick-to-letter mapping.
/// The Hash and PartialEq implementations only consider the section field,
/// allowing regions with different boundaries to be treated as equivalent.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Region {
    pub min_angle: f32,
    pub max_angle: f32,
    pub inner_min_angle: f32,
    pub inner_max_angle: f32,
    pub min_magnitute: f32,
    pub max_magnitute: f32,
    pub inner_min_magnitute: f32,
    pub inner_max_magnitute: f32,
    pub section: Section,
}

impl Hash for Region {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Only hash the section field to allow regions with different boundaries
        // but same section to be treated as equivalent in HashMap lookups
        self.section.hash(state);
    }
}

impl PartialEq for Region {
    fn eq(&self, other: &Self) -> bool {
        // Regions are equal if they represent the same section,
        // regardless of boundary differences
        self.section == other.section
    }
}

impl Eq for Region {}

impl Region {
    /// Determines which region contains the given joystick position.
    ///
    /// Uses hysteresis-aware detection to prevent boundary flickering.
    /// If the previous position was in a specific region, requires movement
    /// beyond outer boundaries to exit that region.
    ///
    /// ## Algorithm
    /// 1. Check each directional region using hysteresis logic
    /// 2. If no directional region matches, default to center
    /// 3. Previous section influences which boundaries are used for detection
    ///
    /// # Performance Notes
    /// Iterates through all 8 regions on each call. Could be optimized with
    /// angle-based lookup if this becomes a bottleneck, but current performance
    /// is acceptable for typical input rates.
    fn region_from_pos(x: f32, y: f32, old_section: Option<Section>) -> Option<Region> {
        for region in ALL_REGIONS {
            if region.contains(x, y, old_section) {
                info!("New Region: {:?}", region.section);
                return Some(region);
            }
        }
        Some(REGION_CENTER)
    }

    /// Converts cartesian coordinates to polar coordinates with North orientation.
    ///
    /// ## Coordinate System Transformation
    /// 1. Standard atan2 gives angle with 0° = East, increasing counter-clockwise
    /// 2. Convert negative angles to 0-360° range
    /// 3. Rotate by 112.5° to align 0° with North direction
    /// 4. Magnitude is clamped to [0.0, 1.0] range
    ///
    /// ## Why This Transformation
    /// The standard mathematical coordinate system doesn't match intuitive
    /// directional input. Users expect "up" on joystick to be North (0°),
    /// but atan2 considers "right" as 0°. The 112.5° rotation aligns the
    /// coordinate system with user expectations.
    ///
    /// # Mathematical Formula
    /// $$ \theta_{north} = (360° + 112.5° - \theta_{atan2}) \bmod 360° $$
    fn to_polar(x: f32, y: f32) -> (f32, f32) {
        let angle_rad = y.atan2(x);
        let mut angle_deg = angle_rad.to_degrees();

        // Convert to 0-360° range
        if angle_deg < 0.0 {
            angle_deg += 360.0;
        }
        let magnitude = (x.powi(2) + y.powi(2)).sqrt().min(1.0);

        // Rotate coordinate system so 0° points North
        let north_oriented = (360.0 + 112.5 - angle_deg) % 360.0;

        (north_oriented, magnitude)
    }

    /// Creates a new region with hysteresis boundaries automatically calculated.
    ///
    /// ## Hysteresis Calculation
    /// Inner boundaries are computed by shrinking the region by the hysteresis
    /// factor proportional to region size. This ensures consistent hysteresis
    /// behavior regardless of region dimensions.
    ///
    /// ## Design Decision
    /// Hysteresis is applied to both angle and magnitude dimensions to provide
    /// uniform stability across all region boundaries.
    pub const fn new(
        angle_min: f32,
        angle_max: f32,
        mag_min: f32,
        mag_max: f32,
        section: Section,
    ) -> Self {
        let hysteresis = REGION_HYSTERESIS;
        let angle_span = angle_max - angle_min;
        let mag_span = mag_max - mag_min;

        // Calculate hysteresis proportional to region size
        let angle_hysteresis = angle_span * hysteresis;
        let mag_hysteresis = mag_span * hysteresis;

        let inner_min_angle = angle_min + angle_hysteresis;
        let inner_max_angle = angle_max - angle_hysteresis;
        let inner_min_magnitute = mag_min + mag_hysteresis;
        let inner_max_magnitute = mag_max;

        Self {
            min_angle: angle_min,
            max_angle: angle_max,
            inner_min_angle,
            inner_max_angle,
            min_magnitute: mag_min,
            max_magnitute: mag_max,
            inner_min_magnitute,
            inner_max_magnitute,
            section,
        }
    }

    /// Checks if position is within outer region boundaries (for exiting region).
    pub fn contains_outer(&self, x: f32, y: f32) -> bool {
        let (angle, magnitute) = Region::to_polar(x, y);
        angle >= self.min_angle
            && angle <= self.max_angle
            && magnitute >= self.min_magnitute
            && magnitute <= self.max_magnitute
    }

    /// Checks if position is within inner region boundaries (for entering region).
    pub fn contains_inner(&self, x: f32, y: f32) -> bool {
        let (angle, magnitute) = Region::to_polar(x, y);
        angle >= self.inner_min_angle
            && angle <= self.inner_max_angle
            && magnitute >= self.inner_min_magnitute
            && magnitute <= self.inner_max_magnitute
    }

    /// Implements hysteresis-aware region containment check.
    ///
    /// ## Hysteresis Logic
    /// - If previously in this region: use outer boundaries (harder to exit)
    /// - If coming from different region: use inner boundaries (must clearly enter)
    ///
    /// This prevents rapid oscillation between regions when joystick position
    /// is near a boundary, which would cause unusable text input behavior.
    ///
    /// ## Performance Notes
    /// The polar coordinate conversion happens for every region check.
    /// Could be optimized by converting once per frame, but current performance
    /// is adequate for real-time input processing.
    pub fn contains(&self, x: f32, y: f32, previous_section: Option<Section>) -> bool {
        if previous_section == Some(self.section) {
            // Use outer boundaries when staying in same region
            return self.contains_outer(x, y);
        }

        // Use inner boundaries when entering from different region
        self.contains_inner(x, y)
    }
}

/// Configuration for gamepad-to-keyboard mapping behavior.
///
/// ## Design Rationale
/// Separates configuration from implementation to allow runtime customization
/// without code changes. The mapping tables define the complete input transformation
/// behavior and can be saved/loaded as user preferences.
///
/// ## Mapping Strategy
/// Three independent mapping tables handle different input types:
/// - Buttons → Individual keys (simple 1:1 mapping)
/// - Joystick combinations → Letters (complex 2D mapping)  
/// - Modifier buttons → Key modifiers (affects other mappings)
///
/// ## Serialization Support
/// Implements Serde for configuration persistence. Users can save custom
/// mappings and reload them across application sessions.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct KeyboardConfig {
    /// Maps individual buttons to specific keyboard keys.
    pub button_mapping: HashMap<ButtonType, Key>,

    /// Maps joystick region combinations to letters with case variants.
    /// Key: (left_region, right_region), Value: (key, uppercase, lowercase)
    joystick_mapping: HashMap<(Region, Region), (Key, String, String)>,

    /// Maps buttons to keyboard modifiers (Shift, Ctrl, Alt, etc.).
    modifier_mapping: HashMap<ButtonType, Modifiers>,

    /// Human-readable name for this configuration.
    name: String,
}

impl KeyboardConfig {
    /// Creates the default keyboard mapping configuration.
    ///
    /// ## Default Button Layout
    /// Based on common gamepad conventions:
    /// - A/B/X/Y → Space/Enter/Escape/Tab (common UI actions)
    /// - D-Pad → Arrow keys (navigation)
    /// - Bumpers → Ctrl/Shift (modifiers)
    /// - Start/Select → Command/Alt (system actions)
    ///
    /// ## Default Alphabet Layout  
    /// Systematic assignment using dual-joystick combinations:
    /// - A-H: Left directions + right center
    /// - I-P: Left center + right directions
    /// - Q-Z: Symmetric directional combinations
    ///
    /// This layout prioritizes learnability over frequency optimization.
    /// Future versions could implement QWERTY-based or frequency-optimized layouts.
    ///
    /// ## Performance Notes
    /// HashMap creation happens once during initialization. The large number
    /// of map_insert! calls is compile-time overhead only.
    pub fn default_config() -> Self {
        let mut button_mapping = HashMap::new();
        button_mapping.insert(ButtonType::A, Key::Space);
        button_mapping.insert(ButtonType::B, Key::Enter);
        button_mapping.insert(ButtonType::X, Key::Escape);
        button_mapping.insert(ButtonType::Y, Key::Tab);
        button_mapping.insert(ButtonType::LeftStick, Key::Semicolon);
        button_mapping.insert(ButtonType::RightStick, Key::Backspace);
        button_mapping.insert(ButtonType::DPadUp, Key::ArrowUp);
        button_mapping.insert(ButtonType::DPadRight, Key::ArrowRight);
        button_mapping.insert(ButtonType::DPadLeft, Key::ArrowLeft);
        button_mapping.insert(ButtonType::DPadDown, Key::ArrowDown);

        let mut modifier_mapping = HashMap::new();
        modifier_mapping.insert(ButtonType::RightBumper, Modifiers::SHIFT);
        modifier_mapping.insert(ButtonType::LeftBumper, Modifiers::CTRL);
        modifier_mapping.insert(ButtonType::Select, Modifiers::ALT);
        modifier_mapping.insert(ButtonType::Start, Modifiers::COMMAND);

        let mut joystick_mapping = HashMap::new();

        // Alphabet mapping: A-I with left joystick + right center
        map_insert!(
            joystick_mapping,
            (REGION_NORTH, REGION_CENTER),
            Key::A,
            "A",
            "a"
        );
        map_insert!(
            joystick_mapping,
            (REGION_NORTHEAST, REGION_CENTER),
            Key::B,
            "B",
            "b"
        );
        map_insert!(
            joystick_mapping,
            (REGION_EAST, REGION_CENTER),
            Key::C,
            "C",
            "c"
        );
        map_insert!(
            joystick_mapping,
            (REGION_SOUTHEAST, REGION_CENTER),
            Key::D,
            "D",
            "d"
        );
        map_insert!(
            joystick_mapping,
            (REGION_SOUTH, REGION_CENTER),
            Key::E,
            "E",
            "e"
        );
        map_insert!(
            joystick_mapping,
            (REGION_SOUTHWEST, REGION_CENTER),
            Key::F,
            "F",
            "f"
        );
        map_insert!(
            joystick_mapping,
            (REGION_WEST, REGION_CENTER),
            Key::G,
            "G",
            "g"
        );
        map_insert!(
            joystick_mapping,
            (REGION_NORTHWEST, REGION_CENTER),
            Key::H,
            "H",
            "h"
        );
        map_insert!(
            joystick_mapping,
            (REGION_CENTER, REGION_NORTH),
            Key::I,
            "I",
            "i"
        );

        // J-P with left center + right joystick directions
        map_insert!(
            joystick_mapping,
            (REGION_CENTER, REGION_NORTHEAST),
            Key::J,
            "J",
            "j"
        );
        map_insert!(
            joystick_mapping,
            (REGION_CENTER, REGION_EAST),
            Key::K,
            "K",
            "k"
        );
        map_insert!(
            joystick_mapping,
            (REGION_CENTER, REGION_SOUTHEAST),
            Key::L,
            "L",
            "l"
        );
        map_insert!(
            joystick_mapping,
            (REGION_CENTER, REGION_SOUTH),
            Key::M,
            "M",
            "m"
        );
        map_insert!(
            joystick_mapping,
            (REGION_CENTER, REGION_SOUTHWEST),
            Key::N,
            "N",
            "n"
        );
        map_insert!(
            joystick_mapping,
            (REGION_CENTER, REGION_WEST),
            Key::O,
            "O",
            "o"
        );
        map_insert!(
            joystick_mapping,
            (REGION_CENTER, REGION_NORTHWEST),
            Key::P,
            "P",
            "p"
        );
        map_insert!(
            joystick_mapping,
            (REGION_NORTH, REGION_NORTH),
            Key::Q,
            "Q",
            "q"
        );
        map_insert!(
            joystick_mapping,
            (REGION_NORTHEAST, REGION_NORTHEAST),
            Key::R,
            "R",
            "r"
        );

        // S-Z with symmetric directional combinations
        map_insert!(
            joystick_mapping,
            (REGION_EAST, REGION_EAST),
            Key::S,
            "S",
            "s"
        );
        map_insert!(
            joystick_mapping,
            (REGION_SOUTHEAST, REGION_SOUTHEAST),
            Key::T,
            "T",
            "t"
        );
        map_insert!(
            joystick_mapping,
            (REGION_SOUTH, REGION_SOUTH),
            Key::U,
            "U",
            "u"
        );
        map_insert!(
            joystick_mapping,
            (REGION_SOUTHWEST, REGION_SOUTHWEST),
            Key::V,
            "V",
            "v"
        );
        map_insert!(
            joystick_mapping,
            (REGION_WEST, REGION_WEST),
            Key::W,
            "W",
            "w"
        );
        map_insert!(
            joystick_mapping,
            (REGION_NORTHWEST, REGION_NORTHWEST),
            Key::X,
            "X",
            "x"
        );
        map_insert!(
            joystick_mapping,
            (REGION_NORTH, REGION_SOUTH),
            Key::Y,
            "Y",
            "y"
        );
        map_insert!(
            joystick_mapping,
            (REGION_SOUTH, REGION_NORTH),
            Key::Z,
            "Z",
            "z"
        );

        KeyboardConfig {
            button_mapping,
            joystick_mapping,
            modifier_mapping,
            name: "Default Keyboard Configuration".to_string(),
        }
    }
}

impl crate::mapping::MappingConfig for KeyboardConfig {
    fn validate(&self) -> Result<(), MappingError> {
        if self.button_mapping.is_empty() {
            return Err(MappingError::ConfigError(
                "Button mapping cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    fn create_strategy(&self) -> Result<Box<dyn MappingStrategy>, MappingError> {
        Ok(Box::new(KeyboardStrategy::new(self.clone())))
    }

    fn get_type(&self) -> MappingType {
        MappingType::Keyboard
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}

/// Core implementation of gamepad-to-keyboard event conversion.
///
/// ## Design Rationale
/// Stateful converter that maintains context between mapping calls to implement
/// hysteresis and modifier state tracking. The conversion happens in real-time
/// as controller input arrives, generating egui Events for the UI system.
///
/// ## State Management
/// - Region history: Tracks previous joystick regions for hysteresis
/// - Button states: Maintains modifier button state across frames
/// - Context persistence: Preserves state between mapping operations
pub struct KeyboardStrategy {
    config: KeyboardConfig,
    context: MappingContext,
}

impl KeyboardStrategy {
    /// Creates a new keyboard mapping strategy with the given configuration.
    pub fn new(config: KeyboardConfig) -> Self {
        Self {
            config,
            context: MappingContext::default(),
        }
    }

    /// Converts joystick positions to keyboard events using region-based mapping.
    ///
    /// ## Algorithm Overview
    /// 1. Get current joystick positions from controller state
    /// 2. Convert positions to regions using hysteresis-aware detection
    /// 3. Look up letter mapping for (left_region, right_region) combination
    /// 4. Generate Key events and Text events with appropriate modifiers
    /// 5. Update context for next frame's hysteresis calculation
    ///
    /// ## Event Generation
    /// For each mapped combination, generates:
    /// - KeyDown event (pressed: true)
    /// - KeyUp event (pressed: false)
    /// - Text event (with case determined by Shift modifier)
    ///
    /// ## Performance Notes
    /// Region detection happens on every call. Could be optimized by caching
    /// when joystick positions haven't changed significantly, but current
    /// performance is acceptable for 60fps input processing.
    ///
    /// # Returns
    /// Vector of egui Events ready for injection into the UI event stream.
    /// Empty vector if no mapping exists for current joystick combination.
    fn map_joystick(&mut self, controller_state: &ControllerOutput) -> Vec<Event> {
        let (prev_left_section, prev_right_section) = self.context.last_sections;

        let left_x = controller_state.left_stick.x;
        let left_y = controller_state.left_stick.y;
        let right_x = controller_state.right_stick.x;
        let right_y = controller_state.right_stick.y;

        let left_region =
            Region::region_from_pos(left_x, left_y, Some(prev_left_section)).unwrap_or_default();
        let right_region =
            Region::region_from_pos(right_x, right_y, Some(prev_right_section)).unwrap_or_default();

        // Update context for next frame's hysteresis
        self.context.last_sections = (left_region.section, right_region.section);

        let map = self
            .config
            .joystick_mapping
            .get(&(left_region, right_region));
        let modifier = self.map_modifiers(&controller_state.button_events);

        let mut events = vec![];
        if let Some((key, upper, lower)) = map {
            // Generate key press and release events
            events.push(Event::Key {
                key: *key,
                physical_key: Some(*key),
                pressed: true,
                repeat: false,
                modifiers: modifier,
            });
            events.push(Event::Key {
                key: *key,
                physical_key: Some(*key),
                pressed: false,
                repeat: false,
                modifiers: modifier,
            });

            // Generate text event with appropriate case
            if modifier.shift {
                events.push(Event::Text(upper.clone()));
            } else {
                events.push(Event::Text(lower.clone()));
            }
        }

        if !events.is_empty() {
            info!("Joysticks successfully mapped: {:?}", events);
        }
        events
    }

    /// Converts button events to modifier flags for use with other mappings.
    ///
    /// Scans active button events for modifier buttons (Shift, Ctrl, Alt, etc.)
    /// and combines them into an egui Modifiers bitfield. This modifier state
    /// affects both button mappings and joystick mappings.
    fn map_modifiers(
        &self,
        raw_modifiers: &[crate::controller::controller_handle::ButtonEvent],
    ) -> egui::Modifiers {
        let mut mods: egui::Modifiers = Modifiers::NONE;
        for raw in raw_modifiers {
            if let Some(key) = self.config.modifier_mapping.get(&raw.button) {
                mods = mods.plus(*key);
            }
        }
        mods
    }
    /// Converts button presses to keyboard events and special key actions.
    ///
    /// ## Processing Strategy
    /// 1. Separate modifier buttons from regular buttons to avoid conflicts
    /// 2. Apply current modifier state to all generated key events
    /// 3. Generate special text events for keys like Enter, Tab, Space
    /// 4. Handle both held and completed button states appropriately
    ///
    /// ## Event Generation Logic
    /// - Held buttons: Generate press events continuously (for key repeat)
    /// - Completed buttons: Generate single press event (for one-shot actions)
    /// - Special keys: Generate both key events and corresponding text
    ///
    /// ## Modifier Button Separation
    /// Modifier buttons are extracted for modifier state calculation, then
    /// filtered out from regular button processing to prevent duplicate events.
    /// Uses pattern matching for clean, readable filtering logic.
    fn map_buttons(
        &mut self,
        button_events: &[crate::controller::controller_handle::ButtonEvent],
    ) -> Vec<egui::Event> {
        let mut events = Vec::new();
        let mut buttons: Vec<crate::controller::controller_handle::ButtonEvent> = vec![];
        buttons.extend_from_slice(button_events);
        let mut button_events = buttons;

        // Extract modifier buttons for modifier state calculation
        let raw_modifiers: Vec<crate::controller::controller_handle::ButtonEvent> = button_events
            .iter()
            .filter(|&x| {
                x.button.eq(&ButtonType::LeftBumper)
                    || x.button.eq(&ButtonType::RightBumper)
                    || x.button.eq(&ButtonType::Start)
                    || x.button.eq(&ButtonType::Select)
            })
            .cloned()
            .collect();

        let modifier = self.map_modifiers(raw_modifiers.as_slice());

        // Filter out modifier buttons from regular processing
        button_events.retain(|x| {
            !matches!(
                x.button,
                ButtonType::LeftBumper
                    | ButtonType::RightBumper
                    | ButtonType::Start
                    | ButtonType::Select
            )
        });

        for button_event in button_events {
            if let Some(key) = self.config.button_mapping.get(&button_event.button) {
                match button_event.state {
                    crate::controller::controller_handle::ButtonEventState::Held => {
                        events.push(Event::Key {
                            key: *key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: modifier,
                        });

                        // Generate text for special keys
                        match key {
                            Key::Enter => events.push(Event::Text("\n".to_string())),
                            Key::Tab => events.push(Event::Text("\t".to_string())),
                            Key::Space => events.push(Event::Text(" ".to_string())),
                            _ => {}
                        };
                    }
                    crate::controller::controller_handle::ButtonEventState::Complete => {
                        events.push(Event::Key {
                            key: *key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: modifier,
                        });

                        // Generate text for special keys
                        match key {
                            Key::Enter => events.push(Event::Text("\n".to_string())),
                            Key::Tab => events.push(Event::Text("\t".to_string())),
                            Key::Space => events.push(Event::Text(" ".to_string())),
                            _ => {}
                        };
                    }
                };

                // Update context state tracking
                self.context
                    .last_button_states
                    .insert(button_event.button.clone(), button_event.state);
            }
        }

        if !events.is_empty() {
            info!("Buttons successfully mapped: {:?}", events);
        }
        events
    }
}

impl MappingStrategy for KeyboardStrategy {
    /// Main entry point for converting controller input to keyboard events.
    ///
    /// Combines button and joystick mapping results into a single event collection.
    /// The order of processing (buttons first, then joysticks) ensures modifier
    /// state is correctly applied to joystick-generated events.
    ///
    /// # Returns
    /// `Some(MappedEvent::KeyboardEvent)` if any events were generated,
    /// `None` if no input mappings were active this frame.
    fn map(&mut self, input: &ControllerOutput) -> Option<MappedEvent> {
        let mut events = Vec::new();

        // Process button events first to establish modifier state
        events.extend(self.map_buttons(&input.button_events));
        events.extend(self.map_joystick(input));

        if events.is_empty() {
            None
        } else {
            Some(MappedEvent::KeyboardEvent { key_code: events })
        }
    }

    fn initialize(&mut self) -> Result<(), MappingError> {
        info!(
            "Initializing keyboard mapping strategy: {}",
            self.config.name
        );
        Ok(())
    }

    fn shutdown(&mut self) {
        info!(
            "Shutting down keyboard mapping strategy: {}",
            self.config.name
        );
    }

    /// Returns rate limit for keyboard event generation.
    ///
    /// Set to ~22Hz (45ms) to balance responsiveness with system load.
    /// Faster rates don't improve user experience for text input but
    /// increase CPU usage and event queue pressure.
    fn get_rate_limit(&self) -> Option<u64> {
        Some(45)
    }

    fn get_type(&self) -> MappingType {
        MappingType::Keyboard
    }
}
