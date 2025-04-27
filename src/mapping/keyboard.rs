//! Implementierung der Keyboard-Mapping-Strategie

use crate::controller::controller_handle::{ButtonType, ControllerOutput};
use crate::mapping::{
    strategy::MappingContext, MappedEvent, MappingError, MappingStrategy, MappingType,
};
use eframe::egui::{self, Event, Key, Modifiers};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use tracing::{debug, error, info, warn};

macro_rules! map_insert {
    ($map:expr, $regions:expr, $key:expr, $upper:expr, $lower:expr) => {
        $map.insert($regions, ($key, $upper.to_string(), $lower.to_string()));
    };
}

/// Hysterese-Wert für die Region-Erkennung (in Einheitenbereichen, z.B. 0-1.0)
pub const REGION_HYSTERESIS: f32 = 0.08;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
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

/// Konstante Standard-Regionen für Joystick-Mappings
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

/// Liefert alle Standardregionen als Array
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

/// Region-Definition für Joystick-Zonen mit Hysterese
#[derive(Clone, Debug, Default)]
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
        // Nur die Section wird gehasht, alle anderen Felder werden ignoriert
        self.section.hash(state);
    }
}

impl PartialEq for Region {
    fn eq(&self, other: &Self) -> bool {
        self.section == other.section
    }
}

impl Eq for Region {}

impl Region {
    fn region_from_pos(x: f32, y: f32, old_section: Option<Section>) -> Option<Region> {
        for region in ALL_REGIONS {
            if region.contains(x, y, old_section) {
                info!("New Region: {:?}", region.section);
                return Some(region);
            }
        }
        Some(REGION_CENTER)
    }

    fn to_polar(x: f32, y: f32) -> (f32, f32) {
        let angle_rad = y.atan2(x);
        let mut angle_deg = angle_rad.to_degrees();

        // Konvertieren zu 0-360°, wobei 0° nach Osten zeigt (Standardverhalten von atan2)
        if angle_deg < 0.0 {
            angle_deg += 360.0;
        }
        let magnitude = (x.powi(2) + y.powi(2)).sqrt().min(1.0);
        // Rotieren, damit 0° an den anfang von Norden zeigt (112.5° gegen den Uhrzeigersinn)
        let north_oriented = (360.0 + 112.5 - angle_deg) % 360.0;

        (north_oriented, magnitude)
    }
    /// Erstellt eine neue Region mit den angegebenen Grenzen und der zugehörigen Section
    pub const fn new(
        angle_min: f32,
        angle_max: f32,
        mag_min: f32,
        mag_max: f32,
        section: Section,
    ) -> Self {
        // Innere Grenzen für Hysterese berechnen
        let hysteresis = REGION_HYSTERESIS;
        let angle_span = angle_max - angle_min;
        let mag_span = mag_max - mag_min;
        // Hysterese proportional zur Größe der Region
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

    /// Prüft, ob ein Punkt (x, y) innerhalb der äußeren Region liegt (zum Verlassen)
    pub fn contains_outer(&self, x: f32, y: f32) -> bool {
        let (angle, magnitute) = Region::to_polar(x, y);
        angle >= self.min_angle
            && angle <= self.max_angle
            && magnitute >= self.min_magnitute
            && magnitute <= self.max_magnitute
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
    joystick_mapping: HashMap<(Region, Region), (Key, String, String)>,

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
        (button_mapping).insert(ButtonType::RightStick, Key::Backspace);
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
        // Basis-Alphabet (A-I) mit rechtem Joystick auf CENTER
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

        // J-R mit rechtem Joystick auf NORTH
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

        // S-Z mit rechtem Joystick auf SOUTH
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
            name: "Keyboard-Config".to_string(),
        }
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

        self.context.last_sections = (left_region.section, right_region.section);

        let map = self
            .config
            .joystick_mapping
            .get(&(left_region, right_region));

        let modifier = self.map_modifiers(&controller_state.button_events);

        let mut events = vec![];
        if let Some((key, upper, lower)) = map {
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
            if modifier.shift {
                events.push(Event::Text(upper.clone()));
            } else {
                events.push(Event::Text(lower.clone()));
            }
        }
        if !events.is_empty() {
            info!("Joysticks successfully maped: {:?}", events);
        }
        events
    }

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

    /// Mappt Button-Events zu Keyboard-Events
    fn map_buttons(
        &mut self,
        button_events: &[crate::controller::controller_handle::ButtonEvent],
    ) -> Vec<egui::Event> {
        let mut events = Vec::new();
        let mut buttons: Vec<crate::controller::controller_handle::ButtonEvent> = vec![];
        buttons.extend_from_slice(button_events);
        let mut button_events = buttons;

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
                    crate::controller::controller_handle::ButtonEventState::Held => {
                        events.push(Event::Key {
                            key: *key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: modifier,
                        });
                        match key {
                            Key::Enter => {
                                events.push(Event::Text("\n".to_string()));
                            }
                            Key::Tab => {
                                events.push(Event::Text("\t".to_string()));
                            }
                            Key::Space => {
                                events.push(Event::Text(" ".to_string()));
                            }
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

                        match key {
                            Key::Enter => {
                                events.push(Event::Text("\n".to_string()));
                            }
                            Key::Tab => {
                                events.push(Event::Text("\t".to_string()));
                            }
                            Key::Space => {
                                events.push(Event::Text(" ".to_string()));
                            }
                            _ => {}
                        };
                    }
                };

                // Status im Kontext speichern
                self.context
                    .last_button_states
                    .insert(button_event.button.clone(), button_event.state);
            }
        }
        if !events.is_empty() {
            info!("Buttons successfully maped: {:?}", events);
        }
        events
    }
}

impl MappingStrategy for KeyboardStrategy {
    fn map(&mut self, input: &ControllerOutput) -> Option<MappedEvent> {
        let mut events = Vec::new();

        // Button-Events mappen
        events.extend(self.map_buttons(&input.button_events));
        events.extend(self.map_joystick(input));
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
        Some(45)
    }

    fn get_type(&self) -> MappingType {
        MappingType::Keyboard
    }
}
