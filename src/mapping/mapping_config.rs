//! Mapping-Konfigurationen für die Mapping Engine
//!
//! Dieses Modul definiert das Konfigurations-Interface und konkrete Implementierungen
//! für verschiedene Mapping-Szenarien wie Tastatur- und ELRS-Mapping.

use std::collections::HashMap;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use tracing::{debug, info, warn};

use crate::controller::controller::{
    ControllerOutput, ButtonType, ButtonEventState, 
    JoystickType, TriggerType, 
};
use crate::mapping::mapping_types::{
    MappedEvent, KeyCode, KeyState, Region, InputComponent
};
use crate::mapping::mapping_strategy::{
    MappingStrategy, DirectMappingStrategy, RegionMappingStrategy,
    ThresholdMappingStrategy, ButtonMappingStrategy
};

/// Mapping-Konfigurationsschnittstelle
///
/// Dieser Trait definiert, wie eine Konfiguration einen ControllerOutput
/// in gemappte Events umwandelt.
pub trait MappingConfig: Send + Sync + 'static {
    /// Wendet das Mapping auf einen Controller-Output an
    fn map(&self, input: &ControllerOutput) -> Vec<MappedEvent>;
    
    /// Gibt den Namen der Konfiguration zurück
    fn name(&self) -> &str;
    
    /// Gibt den Typ der Konfiguration zurück
    fn config_type(&self) -> MappingConfigType;
}

/// Typen von Mapping-Konfigurationen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MappingConfigType {
    Keyboard,
    ELRS,
    Custom,
}

/// Tastatur-Mapping-Konfiguration
///
/// Bildet Controller-Eingaben auf Tastaturevents ab
pub struct KeyboardMappingConfig {
    name: String,
    button_mappings: HashMap<ButtonType, KeyCode>,
    joystick_region_mappings: Vec<(JoystickType, Arc<dyn MappingStrategy>)>,
    trigger_mappings: Vec<(TriggerType, Arc<dyn MappingStrategy>)>,
}

impl KeyboardMappingConfig {
    /// Erstellt eine neue KeyboardMappingConfig
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            button_mappings: HashMap::new(),
            joystick_region_mappings: Vec::new(),
            trigger_mappings: Vec::new(),
        }
    }
    
    /// Fügt ein Button-zu-Taste-Mapping hinzu
    pub fn map_button(&mut self, button: ButtonType, key: KeyCode) -> &mut Self {
        self.button_mappings.insert(button, key);
        self
    }
    
    /// Fügt eine Region für einen Joystick hinzu
    pub fn add_joystick_mapping(
        &mut self, 
        joystick: JoystickType, 
        strategy: Arc<dyn MappingStrategy>
    ) -> &mut Self {
        self.joystick_region_mappings.push((joystick, strategy));
        self
    }
    
    /// Fügt ein Trigger-Mapping hinzu
    pub fn add_trigger_mapping(
        &mut self, 
        trigger: TriggerType, 
        strategy: Arc<dyn MappingStrategy>
    ) -> &mut Self {
        self.trigger_mappings.push((trigger, strategy));
        self
    }
    
    /// Erstellt eine voreingestellte WASD-Konfiguration für den linken Joystick
    pub fn default_wasd() -> Self {
        let mut config = Self::new("Standard WASD");
        
        // Button-Mappings
        config.map_button(ButtonType::A, KeyCode::Space)
              .map_button(ButtonType::B, KeyCode::Escape)
              .map_button(ButtonType::X, KeyCode::E)
              .map_button(ButtonType::Y, KeyCode::Q);
        
        // Joystick-Region für WASD
        let regions = vec![
            Region::new(0.3, 0.7, 0.7, 1.0, KeyCode::W),  // Oben
            Region::new(0.3, 0.7, 0.0, 0.3, KeyCode::S),  // Unten
            Region::new(0.0, 0.3, 0.3, 0.7, KeyCode::A),  // Links
            Region::new(0.7, 1.0, 0.3, 0.7, KeyCode::D),  // Rechts
        ];
        
        let region_strategy = Arc::new(RegionMappingStrategy::new(
            "WASD Regions",
            regions,
            None,
        ));
        
        config.add_joystick_mapping(JoystickType::Left, region_strategy);
        
        // Trigger für Shift und Ctrl
        let left_trigger = Arc::new(ThresholdMappingStrategy::new(
            "Left Trigger to Shift",
            0.7,
            KeyCode::Shift,
        ));
        
        let right_trigger = Arc::new(ThresholdMappingStrategy::new(
            "Right Trigger to Ctrl",
            0.7,
            KeyCode::Ctrl,
        ));
        
        config.add_trigger_mapping(TriggerType::Left, left_trigger)
              .add_trigger_mapping(TriggerType::Right, right_trigger);
        
        config
    }
}

impl MappingConfig for KeyboardMappingConfig {
    fn map(&self, input: &ControllerOutput) -> Vec<MappedEvent> {
        let mut events = Vec::new();
        
        // Button-Mappings verarbeiten
        for button_event in &input.button_events {
            if let Some(key_code) = self.button_mappings.get(&button_event.button) {
                let key_state = match button_event.state {
                    ButtonEventState::Held | ButtonEventState::Complete => KeyState::Pressed,
                    _ => KeyState::Released,
                };
                
                events.push(MappedEvent::KeyboardEvent {
                    key_code: *key_code,
                    state: key_state,
                });
            }
        }
        
        // Joystick-Mappings verarbeiten
        for (joystick_type, strategy) in &self.joystick_region_mappings {
            match joystick_type {
                JoystickType::Left => {
                    let component = InputComponent::Joystick(
                        *joystick_type, 
                        input.left_stick.x, 
                        input.left_stick.y
                    );
                    
                    if let Some(event) = strategy.apply(&component) {
                        events.push(event);
                    }
                },
                JoystickType::Right => {
                    let component = InputComponent::Joystick(
                        *joystick_type, 
                        input.right_stick.x, 
                        input.right_stick.y
                    );
                    
                    if let Some(event) = strategy.apply(&component) {
                        events.push(event);
                    }
                },
            }
        }
        
        // Trigger-Mappings verarbeiten
        for (trigger_type, strategy) in &self.trigger_mappings {
            match trigger_type {
                TriggerType::Left => {
                    let component = InputComponent::Trigger(
                        *trigger_type, 
                        input.left_trigger.value
                    );
                    
                    if let Some(event) = strategy.apply(&component) {
                        events.push(event);
                    }
                },
                TriggerType::Right => {
                    let component = InputComponent::Trigger(
                        *trigger_type, 
                        input.right_trigger.value
                    );
                    
                    if let Some(event) = strategy.apply(&component) {
                        events.push(event);
                    }
                },
            }
        }
        
        events
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn config_type(&self) -> MappingConfigType {
        MappingConfigType::Keyboard
    }
}

/// ELRS-Mapping-Konfiguration
///
/// Bildet Controller-Eingaben auf ELRS-Kanaldaten für Drohnensteuerung ab
pub struct ELRSMappingConfig {
    name: String,
    strategies: Vec<(InputComponent, Arc<dyn MappingStrategy>)>,
    num_channels: u8,
}

impl ELRSMappingConfig {
    /// Erstellt eine neue ELRSMappingConfig
    pub fn new(name: impl Into<String>, num_channels: u8) -> Self {
        Self {
            name: name.into(),
            strategies: Vec::new(),
            num_channels,
        }
    }
    
    /// Fügt ein Mapping für einen Joystick hinzu
    pub fn add_joystick_mapping(
        &mut self,
        joystick: JoystickType,
        axis: &str, // "x" oder "y"
        channel: u8,
        input_range: (f32, f32),
        output_range: (u16, u16),
        invert: bool,
    ) -> &mut Self {
        if channel >= self.num_channels {
            warn!("Channel {} exceeds configured channel count {}", channel, self.num_channels);
            return self;
        }
        
        let strategy_name = format!("{:?} {} Axis to Channel {}", joystick, axis, channel);
        let strategy = Arc::new(DirectMappingStrategy::new(
            strategy_name,
            input_range,
            output_range,
            invert,
            channel,
        ));
        
        // Create a dummy component for storing the mapping type
        let component = match joystick {
            JoystickType::Left => InputComponent::Joystick(JoystickType::Left, 0.0, 0.0),
            JoystickType::Right => InputComponent::Joystick(JoystickType::Right, 0.0, 0.0),
        };
        
        self.strategies.push((component, strategy));
        self
    }
    
    /// Fügt ein Mapping für einen Trigger hinzu
    pub fn add_trigger_mapping(
        &mut self,
        trigger: TriggerType,
        channel: u8,
        input_range: (f32, f32),
        output_range: (u16, u16),
        invert: bool,
    ) -> &mut Self {
        if channel >= self.num_channels {
            warn!("Channel {} exceeds configured channel count {}", channel, self.num_channels);
            return self;
        }
        
        let strategy_name = format!("{:?} Trigger to Channel {}", trigger, channel);
        let strategy = Arc::new(DirectMappingStrategy::new(
            strategy_name,
            input_range,
            output_range,
            invert,
            channel,
        ));
        
        // Create a dummy component for storing the mapping type
        let component = InputComponent::Trigger(trigger, 0.0);
        
        self.strategies.push((component, strategy));
        self
    }
    
    /// Fügt ein Mapping für einen Button hinzu
    pub fn add_button_mapping(
        &mut self,
        button: ButtonType,
        channel: u8,
        output_range: (u16, u16),
    ) -> &mut Self {
        if channel >= self.num_channels {
            warn!("Channel {} exceeds configured channel count {}", channel, self.num_channels);
            return self;
        }
        
        let strategy_name = format!("{:?} Button to Channel {}", button, channel);
        let strategy = Arc::new(DirectMappingStrategy::new(
            strategy_name,
            (0.0, 1.0), // Dummy input range für Buttons
            output_range,
            false,
            channel,
        ));
        
        // Create a dummy component for storing the mapping type
        let component = InputComponent::Button(button, ButtonEventState::Held);
        
        self.strategies.push((component, strategy));
        self
    }
    
    /// Erstellt eine Standard-ELRS-Konfiguration für Quadcopter
    pub fn default_quadcopter() -> Self {
        let mut config = Self::new("Standard Quadcopter", 16);
        
        // Roll (rechter Stick X) -> Kanal 1
        config.add_joystick_mapping(
            JoystickType::Right, "x", 0, 
            (-1.0, 1.0), (1000, 2000), false
        );
        
        // Pitch (rechter Stick Y) -> Kanal 2
        config.add_joystick_mapping(
            JoystickType::Right, "y", 1, 
            (-1.0, 1.0), (1000, 2000), true
        );
        
        // Throttle (linker Stick Y) -> Kanal 3
        config.add_joystick_mapping(
            JoystickType::Left, "y", 2, 
            (-1.0, 1.0), (1000, 2000), true
        );
        
        // Yaw (linker Stick X) -> Kanal 4
        config.add_joystick_mapping(
            JoystickType::Left, "x", 3, 
            (-1.0, 1.0), (1000, 2000), false
        );
        
        // Arming (Button A) -> Kanal 5
        config.add_button_mapping(
            ButtonType::A, 4, (1000, 2000)
        );
        
        // Flugmodus (Button B) -> Kanal 6
        config.add_button_mapping(
            ButtonType::B, 5, (1000, 2000)
        );
        
        config
    }
}

impl MappingConfig for ELRSMappingConfig {
    fn map(&self, input: &ControllerOutput) -> Vec<MappedEvent> {
        let mut events = Vec::new();
        
        // Alle Strategien durchlaufen und anhand der Komponententypen anwenden
        for (component_type, strategy) in &self.strategies {
            match component_type {
                InputComponent::Joystick(joy_type, _, _) => {
                    // Konkrete Joystick-Werte abrufen
                    let (x, y) = match joy_type {
                        JoystickType::Left => (input.left_stick.x, input.left_stick.y),
                        JoystickType::Right => (input.right_stick.x, input.right_stick.y),
                    };
                    
                    // Komponente mit aktuellen Werten erstellen
                    let component = InputComponent::Joystick(*joy_type, x, y);
                    
                    // Strategie anwenden
                    if let Some(event) = strategy.apply(&component) {
                        events.push(event);
                    }
                },
                InputComponent::Trigger(trigger_type, _) => {
                    // Konkreten Trigger-Wert abrufen
                    let value = match trigger_type {
                        TriggerType::Left => input.left_trigger.value,
                        TriggerType::Right => input.right_trigger.value,
                    };
                    
                    // Komponente mit aktuellem Wert erstellen
                    let component = InputComponent::Trigger(*trigger_type, value);
                    
                    // Strategie anwenden
                    if let Some(event) = strategy.apply(&component) {
                        events.push(event);
                    }
                },
                InputComponent::Button(button_type, _) => {
                    // Prüfen, ob der Button gedrückt ist
                    let is_pressed = input.button_events.iter().any(|event| 
                        event.button == *button_type && 
                        (event.state == ButtonEventState::Held || event.state == ButtonEventState::Complete)
                    );
                    
                    if is_pressed {
                        // Komponente mit aktuellem Zustand erstellen
                        let component = InputComponent::Button(*button_type, ButtonEventState::Held);
                        
                        // Strategie anwenden
                        if let Some(event) = strategy.apply(&component) {
                            events.push(event);
                        }
                    }
                },
            }
        }
        
        events
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn config_type(&self) -> MappingConfigType {
        MappingConfigType::ELRS
    }
}

/// Lädt Mapping-Konfigurationen aus einer Datei
pub fn load_configs_from_file(path: &str) -> Result<Vec<Box<dyn MappingConfig>>, std::io::Error> {
    // Hier könnte Code zum Laden von Konfigurationen stehen
    // Als Beispiel erstellen wir einfach die Standardkonfigurationen
    let mut configs: Vec<Box<dyn MappingConfig>> = Vec::new();
    
    configs.push(Box::new(KeyboardMappingConfig::default_wasd()));
    configs.push(Box::new(ELRSMappingConfig::default_quadcopter()));
    
    Ok(configs)
}