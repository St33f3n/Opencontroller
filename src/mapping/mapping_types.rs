//! Gemeinsame Datentypen für die Mapping Engine
//!
//! Dieses Modul definiert die grundlegenden Datentypen für die Mapping Engine,
//! inklusive der Mapping-Events, Fehlertypen und Hilfsfunktionen.

use crate::controller::controller::{ButtonEventState, ButtonType, JoystickType, TriggerType};
use eframe::egui::Event;
use std::fmt;
use thiserror::Error;

/// Hysterese-Wert für die Region-Erkennung (in Einheitenbereichen, z.B. 0-1.0)
pub const REGION_HYSTERESIS: f32 = 0.08;

/// Repräsentiert ein gemapptes Event, das von der Mapping Engine erzeugt wird
#[derive(Clone, Debug)]
pub enum MappedEvent {
    /// Tastatur-Event für UI-Integration
    KeyboardEvent { key_code: Event, state: KeyState },
    /// ELRS-Daten für Drohnensteuerung
    ELRSData { channel: u8, value: u16 },
    /// Custom Event für zukünftige Erweiterungen
    CustomEvent { event_type: String, data: Vec<u8> },
}

/// Tastenzustand für Keyboard-Events
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyState {
    Pressed,
    Released,
}

/// Input-Komponententypen für die Mapping-Strategien
#[derive(Clone, Debug)]
pub enum InputComponent {
    Button(ButtonType, ButtonEventState),
    Joystick(JoystickType, f32, f32),
    Trigger(TriggerType, f32),
}

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
    Center, // Zentrum als Neutralposition hinzugefügt
}

/// Region-Definition für Joystick-Zonen mit Hysterese
#[derive(Clone, Debug)]
pub struct Region {
    pub x_min: f32,
    pub x_max: f32,
    pub y_min: f32,
    pub y_max: f32,
    pub section: Section,
    // Innere Grenzen für Hysterese
    pub inner_x_min: f32,
    pub inner_x_max: f32,
    pub inner_y_min: f32,
    pub inner_y_max: f32,
}

impl Region {
    /// Erstellt eine neue Region mit den angegebenen Grenzen und der zugehörigen Section
    pub fn new(x_min: f32, x_max: f32, y_min: f32, y_max: f32, section: Section) -> Self {
        // Innere Grenzen für Hysterese berechnen
        let hysteresis = REGION_HYSTERESIS;
        let width = x_max - x_min;
        let height = y_max - y_min;
        
        // Hysterese proportional zur Größe der Region
        let x_hysteresis = width * hysteresis;
        let y_hysteresis = height * hysteresis;
        
        let inner_x_min = x_min + x_hysteresis;
        let inner_x_max = x_max - x_hysteresis;
        let inner_y_min = y_min + y_hysteresis;
        let inner_y_max = y_max - y_hysteresis;

        Self {
            x_min,
            x_max,
            y_min,
            y_max,
            section,
            inner_x_min,
            inner_x_max,
            inner_y_min,
            inner_y_max,
        }
    }

    /// Prüft, ob ein Punkt (x, y) innerhalb der äußeren Region liegt (zum Verlassen)
    pub fn contains_outer(&self, x: f32, y: f32) -> bool {
        x >= self.x_min && x <= self.x_max && y >= self.y_min && y <= self.y_max
    }

    /// Prüft, ob ein Punkt (x, y) innerhalb der inneren Region liegt (zum Betreten)
    pub fn contains_inner(&self, x: f32, y: f32) -> bool {
        x >= self.inner_x_min && x <= self.inner_x_max && y >= self.inner_y_min && y <= self.inner_y_max
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

/// Fehler, die bei der Mapping-Engine auftreten können
#[derive(Error, Debug)]
pub enum MappingError {
    #[error("Engine konnte nicht initialisiert werden: {0}")]
    InitializationError(String),

    #[error("Kanal wurde geschlossen")]
    ChannelClosed,

    #[error("Fehler beim Ändern der Konfiguration")]
    ConfigChangeError,

    #[error("Fehler beim Verarbeiten des Controller-Outputs: {0}")]
    ProcessingError(String),

    #[error("Unbekannter Fehler: {0}")]
    Unknown(String),
}

/// Daten für den Processing-State der Zustandsmaschine
#[derive(Clone, Debug)]
pub struct ProcessingData {
    pub output: crate::controller::controller::ControllerOutput,
}

/// Hilfsfunktion zum Prüfen, ob sich der ControllerOutput signifikant geändert hat
pub fn has_significant_changes(
    prev: &crate::controller::controller::ControllerOutput,
    current: &crate::controller::controller::ControllerOutput,
) -> bool {
    // Joystick-Änderungen prüfen (mit Toleranz)
    let joystick_changed = (prev.left_stick.x - current.left_stick.x).abs() > 0.05
        || (prev.left_stick.y - current.left_stick.y).abs() > 0.05
        || (prev.right_stick.x - current.right_stick.x).abs() > 0.05
        || (prev.right_stick.y - current.right_stick.y).abs() > 0.05;

    // Trigger-Änderungen prüfen
    let trigger_changed = (prev.left_trigger.value - current.left_trigger.value).abs() > 0.05
        || (prev.right_trigger.value - current.right_trigger.value).abs() > 0.05;

    // Button-Änderungen prüfen
    let button_events_differ = !buttons_equivalent(&prev.button_events, &current.button_events);

    joystick_changed || trigger_changed || button_events_differ
}

/// Prüft, ob zwei Button-Event-Sets inhaltlich gleich sind
pub fn buttons_equivalent(
    prev: &[crate::controller::controller::ButtonEvent],
    current: &[crate::controller::controller::ButtonEvent],
) -> bool {
    if prev.len() != current.len() {
        return false;
    }

    // Alle Buttons im aktuellen Set müssen im vorherigen Set
    // mit gleichem Zustand existieren
    for curr_event in current {
        if !prev.iter().any(|prev_event| {
            prev_event.button == curr_event.button && prev_event.state == curr_event.state
        }) {
            return false;
        }
    }

    true
}