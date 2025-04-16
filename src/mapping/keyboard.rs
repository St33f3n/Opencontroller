//! Implementierung der Keyboard-Mapping-Strategie

use crate::controller::controller::{ButtonType, ControllerOutput, JoystickType};
use crate::mapping::{
    strategy::MappingContext, MappedEvent, MappingError, MappingStrategy, MappingType,
};
use eframe::egui::{self, Event, Key, Modifiers};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// Hysterese-Wert für die Region-Erkennung (in Einheitenbereichen, z.B. 0-1.0)
pub const REGION_HYSTERESIS: f32 = 0.08;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Section {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
    Center,
}

/// Region-Definition für Joystick-Zonen mit Hysterese
#[derive(Clone, Debug)]
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

impl Region {
    fn to_polar(x: f32, y: f32) -> (f32, f32) {
        let angle_rad = y.atan2(x);
        let mut angle_deg = angle_rad.to_degrees();
        
        // Konvertieren zu 0-360°, wobei 0° nach Osten zeigt (Standardverhalten von atan2)
        if angle_deg < 0.0 {
            angle_deg += 360.0;
        }
        let magnitude = (x.powi(2) + y.powi(2)).sqrt().min(1.0);
        // Rotieren, damit 0° an den anfang von Norden zeigt (90° gegen den Uhrzeigersinn)
        let north_oriented = (360.0 + 112.5 - angle_deg) % 360.0;
        
        (north_oriented, magnitude)
    }
        /// Erstellt eine neue Region mit den angegebenen Grenzen und der zugehörigen Section
    pub fn new(angle_min: f32, angle_max: f32, mag_min: f32, mag_max: f32, section: Section) -> Self {
        // Innere Grenzen für Hysterese berechnen
        let hysteresis = REGION_HYSTERESIS;
        let angle_span = angle_max-angle_min;
        let mag_span = mag_max-mag_min;
        // Hysterese proportional zur Größe der Region
        let angle_hysteresis = angle_span * hysteresis;
        let mag_hysteresis = mag_span * hysteresis;

        let inner_min_angle = angle_min + angle_hysteresis;
        let inner_max_angle= angle_max - angle_hysteresis;
        let inner_min_magnitute = mag_min + mag_hysteresis;
        let inner_max_magnitute= mag_max - mag_hysteresis;

        Self {
            min_angle: angle_min,
            max_angle: angle_max,
            inner_min_angle,
            inner_max_angle,
            min_magnitute: mag_min,
            max_magnitute: mag_max,
            inner_min_magnitute,
            inner_max_magnitute,
            section
        }
    }

    /// Prüft, ob ein Punkt (x, y) innerhalb der äußeren Region liegt (zum Verlassen)
    pub fn contains_outer(&self, x: f32, y: f32) -> bool {
        let (angle, magnitute) = Region::to_polar(x, y);
        angle >= self.min_angle && angle <= self.max_angle && magnitute >= self.min_magnitute && magnitute <= self.max_magnitute
    }

    /// Prüft, ob ein Punkt (x, y) innerhalb der inneren Region liegt (zum Betreten)
    pub fn contains_inner(&self, x: f32, y: f32) -> bool {
        let (angle, magnitute) = Region::to_polar(x, y);
        angle >= self.inner_min_angle
            && angle <= self.inner_max_angle
            && magnitute >= self.inner_min_magnitute
            && magnitute <= self.inner_max_magnitute
    }

    /// Prüft, ob ein Punkt (x, y) innerhalb der Region liegt, mit Hysterese-Unterstützung
    /// previous_section gibt an, in welcher Section der Punkt zuvor war
    pub fn contains(&self, x: f32, y: f32, previous_section: Option<Section>) -> bool {
        // Wenn der Punkt in der vorherigen Berechnung in dieser Region war,
        // dann verlässt er die Region erst, wenn er die äußeren Grenzen überschreitet
        if previous_section == Some(self.section) {
            return self.contains_outer(x, y);
        }

        // Ansonsten muss der Punkt die inneren Grenzen überschreiten, um als "in dieser Region" zu gelten
        self.contains_inner(x, y)
    }
}

/// Konfiguration für Keyboard-Mapping
#[derive(Debug, Clone)]
pub struct KeyboardConfig {
    /// Zuordnung von Controller-Buttons zu Keyboard-Keys
    button_mapping: HashMap<ButtonType, Key>,

    /// Zuordnung von JoyStick-Left-Regions
    left_joystick_mapping: HashMap<Region, Key>,

    /// Zuordnung von JoyStick-Right-Regions
    right_joystick_mapping: HashMap<Region, Key>,

    /// Zuordnung der Modifier
    modifier_mapping: HashMap<ButtonType, Modifiers>,

    /// Name der Konfiguration
    name: String,
}

impl KeyboardConfig {
    /// Erstellt eine Standard-Konfiguration
    pub fn default_config() -> Self {
        let mut button_mapping = HashMap::new();
        button_mapping.insert(ButtonType::A, Key::Space);
        button_mapping.insert(ButtonType::B, Key::Enter);
        button_mapping.insert(ButtonType::X, Key::Escape);
        button_mapping.insert(ButtonType::Y, Key::Tab);
        (button_mapping).insert(ButtonType::LeftStick, Key::Semicolon);
        (button_mapping).insert(ButtonType::RightStick, Key::Period);
        button_mapping.insert(ButtonType::DPadUp, Key::ArrowUp);
        button_mapping.insert(ButtonType::DPadRight, Key::ArrowRight);
        button_mapping.insert(ButtonType::DPadLeft, Key::ArrowLeft);
        button_mapping.insert(ButtonType::DPadDown, Key::ArrowDown);

        let mut modifier_mapping = HashMap::new();
        modifier_mapping.insert(ButtonType::RightBumper, Modifiers::SHIFT);
        modifier_mapping.insert(ButtonType::LeftBumper, Modifiers::CTRL);
        modifier_mapping.insert(ButtonType::Select, Modifiers::ALT);
        modifier_mapping.insert(ButtonType::Start, Modifiers::COMMAND);
    }
}

impl crate::mapping::MappingConfig for KeyboardConfig {
    fn validate(&self) -> Result<(), MappingError> {
        // Mindestanforderungen prüfen (z.B. essentielle Buttons)
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

/// Implementierung der Keyboard-Mapping-Strategie
pub struct KeyboardStrategy {
    /// Konfiguration für das Mapping
    config: KeyboardConfig,

    /// Zustandskontext
    context: MappingContext,
}

impl KeyboardStrategy {
    /// Erstellt eine neue Keyboard-Mapping-Strategie
    pub fn new(config: KeyboardConfig) -> Self {
        Self {
            config,
            context: MappingContext::default(),
        }
    }

    /// Mappt Joystick-Bewegungen zu Regions
    fn map_joystick_region(&mut self, controller_state: ControllerOutput) -> (Region, Region){
        
    }

    fn map_modifiers(
        &self,
        raw_modifiers: &[crate::controller::controller::ButtonEvent],
    ) -> egui::Modifiers {
        let mut mods: egui::Modifiers = Modifiers::NONE;
        for raw in raw_modifiers {
            if let Some(key) = self.config.modifier_mapping.get(&raw.button) {
                mods = mods.plus(*key);
            }
        }
        mods
    }

    /// Mappt Button-Events zu Keyboard-Events
    fn map_buttons(
        &mut self,
        button_events: &[crate::controller::controller::ButtonEvent],
    ) -> Vec<egui::Event> {
        let mut events = Vec::new();
        let mut buttons: Vec<crate::controller::controller::ButtonEvent> = vec![];
        buttons.extend_from_slice(button_events);
        let mut button_events = buttons;

        let raw_modifiers: Vec<crate::controller::controller::ButtonEvent> = button_events
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
        button_events.retain(|x| {
            !x.button.eq(&ButtonType::LeftBumper)
                || !x.button.eq(&ButtonType::RightBumper)
                || !x.button.eq(&ButtonType::Start)
                || !x.button.eq(&ButtonType::Select)
        });

        for button_event in button_events {
            // Nur mappable Buttons verarbeiten
            if let Some(key) = self.config.button_mapping.get(&button_event.button) {
                // Button-Zustand prüfen
                match button_event.state {
                    crate::controller::controller::ButtonEventState::Held => {
                        events.push(Event::Key {
                            key: *key,
                            physical_key: None,
                            pressed: true,
                            repeat: true,
                            modifiers: modifier.clone(),
                        });
                        events.push(Event::Key {
                            key: *key,
                            physical_key: None,
                            pressed: false,
                            repeat: true,
                            modifiers: modifier.clone(),
                        })
                    },
                    crate::controller::controller::ButtonEventState::Complete => {
                        events.push(Event::Key {
                            key: *key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: modifier.clone(),
                        });
                        events.push(Event::Key {
                            key: *key,
                            physical_key: None,
                            pressed: false,
                            repeat: false,
                            modifiers: modifier.clone(),
                        })
                    },
                };

                // Status im Kontext speichern
                self.context
                    .last_button_states
                    .insert(button_event.button.clone(), button_event.state);
            }
        }

        events
    }
}

impl MappingStrategy for KeyboardStrategy {
    fn map(&mut self, input: &ControllerOutput) -> Option<MappedEvent> {
        let mut events = Vec::new();

        // Joystick-Bewegungen mappen
        events.extend(self.map_joystick(
            JoystickType::Left,
            input.left_stick.x,
            input.left_stick.y,
        ));

        events.extend(self.map_joystick(
            JoystickType::Right,
            input.right_stick.x,
            input.right_stick.y,
        ));

        // Button-Events mappen
        events.extend(self.map_buttons(&input.button_events));

        // Nur ein Event zurückgeben, wenn tatsächlich Events vorhanden sind
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

    fn get_rate_limit(&self) -> Option<u64> {
        Some(16) // ~60 FPS für UI-Events
    }

    fn get_type(&self) -> MappingType {
        MappingType::Keyboard
    }
}
