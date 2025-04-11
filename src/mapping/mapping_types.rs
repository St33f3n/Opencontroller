//! Gemeinsame Datentypen für die Mapping Engine
//!
//! Dieses Modul definiert die grundlegenden Datentypen für die Mapping Engine,
//! inklusive der Mapping-Events, Fehlertypen und Hilfsfunktionen.

use crate::controller::controller::{ButtonEventState, ButtonType, JoystickType, TriggerType};
use eframe::egui::Event;
use std::fmt;
use thiserror::Error;

/// Repräsentiert ein gemapptes Event, das von der Mapping Engine erzeugt wird
#[derive(Clone, Debug)]
pub enum MappedEvent {
    /// Tastatur-Event für UI-Integration
    KeyboardEvent { key_code: Event},
    /// ELRS-Daten für Drohnensteuerung
    ELRSData { channel: u8, value: u16 },
    /// Custom Event für zukünftige Erweiterungen
    CustomEvent { event_type: String, data: Vec<u8> },
}

/// Input-Komponententypen für die Mapping-Strategien
#[derive(Clone, Debug)]
pub enum InputComponent {
    Button(ButtonType, ButtonEventState),
    Joystick(JoystickType, f32, f32),
    Trigger(TriggerType, f32),
}

#[derive(Clone, Copy, Debug)]
pub enum Section {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

/// Region-Definition für Joystick-Zonen
#[derive(Clone, Debug)]
pub struct Region {
    pub x_min: f32,
    pub x_max: f32,
    pub y_min: f32,
    pub y_max: f32,
    pub section: Section,
}

impl Region {
    /// Erstellt eine neue Region mit den angegebenen Grenzen und der zugehörigen Section
    pub fn new(x_min: f32, x_max: f32, y_min: f32, y_max: f32, section: Section) -> Self {
        Self {
            x_min,
            x_max,
            y_min,
            y_max,
            section,
        }
    }

    /// Prüft, ob ein Punkt (x, y) innerhalb der Region liegt
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x_min && x <= self.x_max && y >= self.y_min && y <= self.y_max
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
