//! Implementierung der Keyboard-Mapping-Strategie

use crate::controller::controller::{ButtonType, ControllerOutput, JoystickType};
use crate::mapping::{
    MappedEvent, MappingError, MappingStrategy, MappingType,
    strategy::MappingContext,
};
use eframe::egui::{self, Event, Key, Modifiers};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// Konfiguration für Keyboard-Mapping
#[derive(Debug, Clone)]
pub struct KeyboardConfig {
    /// Zuordnung von Controller-Buttons zu Keyboard-Keys
    button_mapping: HashMap<ButtonType, Key>,
    
    /// Zuordnung von Controller-Joysticks zu Keyboard-Cursor-Bewegungen
    joystick_mapping: HashMap<JoystickType, JoystickKeyMapping>,
    
    /// Name der Konfiguration
    name: String,
}

/// Zuordnung eines Joysticks zu Keyboard-Keys für Richtungen
#[derive(Debug, Clone)]
pub struct JoystickKeyMapping {
    /// Key für Aufwärtsbewegung
    up: Key,
    
    /// Key für Abwärtsbewegung
    down: Key,
    
    /// Key für Linksbewegung
    left: Key,
    
    /// Key für Rechtsbewegung
    right: Key,
    
    /// Schwellenwert für Joystick-Ausschlag
    threshold: f32,
}

impl JoystickKeyMapping {
    /// Erstellt eine neue Joystick-Key-Zuordnung
    pub fn new(up: Key, down: Key, left: Key, right: Key, threshold: f32) -> Self {
        Self {
            up,
            down,
            left,
            right,
            threshold,
        }
    }
    
    /// Standard-Zuordnung für linken Joystick (WASD)
    pub fn wasd() -> Self {
        Self::new(Key::W, Key::S, Key::A, Key::D, 0.5)
    }
    
    /// Standard-Zuordnung für rechten Joystick (Pfeiltasten)
    pub fn arrows() -> Self {
        Self::new(Key::ArrowUp, Key::ArrowDown, Key::ArrowLeft, Key::ArrowRight, 0.5)
    }
}

impl KeyboardConfig {
    /// Erstellt eine neue Keyboard-Mapping-Konfiguration
    pub fn new(
        button_mapping: HashMap<ButtonType, Key>,
        joystick_mapping: HashMap<JoystickType, JoystickKeyMapping>,
        name: String,
    ) -> Self {
        Self {
            button_mapping,
            joystick_mapping,
            name,
        }
    }
    
    /// Erstellt eine Standard-Konfiguration
    pub fn default_config() -> Self {
        let mut button_mapping = HashMap::new();
        button_mapping.insert(ButtonType::A, Key::Space);
        button_mapping.insert(ButtonType::B, Key::Escape);
        button_mapping.insert(ButtonType::X, Key::E);
        button_mapping.insert(ButtonType::Y, Key::Q);
        button_mapping.insert(ButtonType::Start, Key::Enter);
        button_mapping.insert(ButtonType::Select, Key::Tab);
        
        let mut joystick_mapping = HashMap::new();
        joystick_mapping.insert(JoystickType::Left, JoystickKeyMapping::wasd());
        joystick_mapping.insert(JoystickType::Right, JoystickKeyMapping::arrows());
        
        Self::new(button_mapping, joystick_mapping, "Default Keyboard Mapping".to_string())
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
    
    /// Mappt Joystick-Bewegungen zu Keyboard-Events
    fn map_joystick(
        &mut self,
        joystick_type: JoystickType,
        x: f32,
        y: f32,
    ) -> Vec<egui::Event> {
        let mut events = Vec::new();
        
        // Zuordnung für diesen Joystick finden
        if let Some(mapping) = self.config.joystick_mapping.get(&joystick_type) {
            let threshold = mapping.threshold;
            
            // X-Achse (links/rechts)
            if x.abs() >= threshold {
                let key = if x > 0.0 { mapping.right } else { mapping.left };
                events.push(Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                });
            }
            
            // Y-Achse (oben/unten)
            if y.abs() >= threshold {
                let key = if y > 0.0 { mapping.down } else { mapping.up };
                events.push(Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                });
            }
        }
        
        events
    }
    
    /// Mappt Button-Events zu Keyboard-Events
    fn map_buttons(&mut self, button_events: &[crate::controller::controller::ButtonEvent]) -> Vec<egui::Event> {
        let mut events = Vec::new();
        
        for button_event in button_events {
            // Nur mappable Buttons verarbeiten
            if let Some(key) = self.config.button_mapping.get(&button_event.button) {
                // Button-Zustand prüfen
                let pressed = match button_event.state {
                    crate::controller::controller::ButtonEventState::Held => true,
                    crate::controller::controller::ButtonEventState::Complete => false,
                };
                
                // Event erstellen
                events.push(Event::Key {
                    key: *key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Modifiers::default(),
                });
                
                // Status im Kontext speichern
                self.context.last_button_states.insert(button_event.button.clone(), pressed);
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
        info!("Initializing keyboard mapping strategy: {}", self.config.name);
        Ok(())
    }
    
    fn shutdown(&mut self) {
        info!("Shutting down keyboard mapping strategy: {}", self.config.name);
    }
    
    fn get_rate_limit(&self) -> Option<u64> {
        Some(16) // ~60 FPS für UI-Events
    }
    
    fn get_type(&self) -> MappingType {
        MappingType::Keyboard
    }
}