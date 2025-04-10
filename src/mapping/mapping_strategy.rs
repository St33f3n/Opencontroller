//! Mapping-Strategien für die Mapping Engine
//!
//! Dieses Modul definiert verschiedene Strategien zur Transformation von Controller-Inputs
//! in gemappte Events. Es implementiert das Strategie-Designmuster für verschiedene
//! Mapping-Ansätze.

use crate::mapping::mapping_types::{InputComponent, MappedEvent, KeyState, Region, KeyCode};
use crate::controller::controller::{JoystickType, TriggerType, ButtonType, ButtonEventState};

/// Mapping-Strategieschnittstelle - alle konkreten Strategien implementieren diesen Trait
pub trait MappingStrategy: Send + Sync + 'static {
    /// Wendet die Mapping-Strategie auf eine Eingabekomponente an
    fn apply(&self, input: &InputComponent) -> Option<MappedEvent>;
    
    /// Name der Strategie für Debugging
    fn name(&self) -> &str;
}

/// Direkte Mapping-Strategie (1:1 Transformation mit Skalierung)
/// 
/// Diese Strategie transformiert einen Eingabewert direkt in einen Ausgabewert
/// basierend auf einem Eingabe- und Ausgabebereich mit optionaler Invertierung.
pub struct DirectMappingStrategy {
    name: String,
    input_range: (f32, f32),   // Eingabebereich (min, max)
    output_range: (u16, u16),  // Ausgabebereich (min, max)
    invert: bool,              // Ob die Ausgabe invertiert werden soll
    channel: u8,               // Zielkanal für ELRS-Daten
}

impl DirectMappingStrategy {
    /// Erstellt eine neue DirectMappingStrategy
    pub fn new(
        name: impl Into<String>,
        input_range: (f32, f32),
        output_range: (u16, u16),
        invert: bool,
        channel: u8,
    ) -> Self {
        Self {
            name: name.into(),
            input_range,
            output_range,
            invert,
            channel,
        }
    }
    
    /// Skaliert einen Wert von einem Bereich auf einen anderen
    fn scale_value(&self, value: f32) -> u16 {
        let (in_min, in_max) = self.input_range;
        let (out_min, out_max) = self.output_range;
        
        // Wert auf den Eingabebereich begrenzen
        let clamped = value.max(in_min).min(in_max);
        
        // Normalisieren auf 0.0 - 1.0
        let normalized = (clamped - in_min) / (in_max - in_min);
        
        // Invertieren, falls erforderlich
        let adjusted = if self.invert { 1.0 - normalized } else { normalized };
        
        // Auf Ausgabebereich skalieren
        let scaled = out_min as f32 + adjusted * (out_max - out_min) as f32;
        
        // Zu u16 konvertieren
        scaled.round() as u16
    }
}

impl MappingStrategy for DirectMappingStrategy {
    fn apply(&self, input: &InputComponent) -> Option<MappedEvent> {
        match input {
            InputComponent::Joystick(joy_type, x, y) => {
                // Für Joystick verwenden wir je nach Strategie X oder Y
                // Hier beispielhaft für X-Achse
                let value = *x;
                let scaled_value = self.scale_value(value);
                
                Some(MappedEvent::ELRSData { 
                    channel: self.channel, 
                    value: scaled_value
                })
            },
            InputComponent::Trigger(trigger_type, value) => {
                // Trigger-Wert direkt skalieren
                let scaled_value = self.scale_value(*value);
                
                Some(MappedEvent::ELRSData { 
                    channel: self.channel, 
                    value: scaled_value
                })
            },
            InputComponent::Button(button_type, button_state) => {
                // Button auf On/Off-Wert im Ausgabebereich abbilden
                let (_, out_max) = self.output_range;
                let value = match button_state {
                    ButtonEventState::Held => out_max,
                    ButtonEventState::Complete => out_max,
                    _ => self.output_range.0,
                };
                
                Some(MappedEvent::ELRSData { 
                    channel: self.channel, 
                    value
                })
            },
        }
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Regionbasierte Mapping-Strategie für Joysticks
///
/// Diese Strategie definiert Regionen auf einem 2D-Joystick und ordnet
/// jeder Region einen bestimmten Ausgabewert zu.
pub struct RegionMappingStrategy {
    name: String,
    regions: Vec<Region>,
    default_key: Option<KeyCode>,
}

impl RegionMappingStrategy {
    /// Erstellt eine neue RegionMappingStrategy
    pub fn new(
        name: impl Into<String>,
        regions: Vec<Region>,
        default_key: Option<KeyCode>,
    ) -> Self {
        Self {
            name: name.into(),
            regions,
            default_key,
        }
    }
}

impl MappingStrategy for RegionMappingStrategy {
    fn apply(&self, input: &InputComponent) -> Option<MappedEvent> {
        match input {
            InputComponent::Joystick(joy_type, x, y) => {
                // Prüfen, in welcher Region sich der Joystick befindet
                for region in &self.regions {
                    if region.contains(*x, *y) {
                        return Some(MappedEvent::KeyboardEvent { 
                            key_code: region.output_key, 
                            state: KeyState::Pressed 
                        });
                    }
                }
                
                // Wenn keine Region gefunden wurde und ein Default-Key definiert ist
                if let Some(key) = self.default_key {
                    return Some(MappedEvent::KeyboardEvent {
                        key_code: key,
                        state: KeyState::Released,
                    });
                }
                
                None
            },
            _ => None, // Andere Komponententypen werden nicht unterstützt
        }
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Schwellenwert-Mapping-Strategie für Trigger
///
/// Diese Strategie bildet Trigger-Werte auf Keyboard-Events ab,
/// wenn ein bestimmter Schwellenwert überschritten wird.
pub struct ThresholdMappingStrategy {
    name: String,
    threshold: f32,
    output_key: KeyCode,
}

impl ThresholdMappingStrategy {
    /// Erstellt eine neue ThresholdMappingStrategy
    pub fn new(
        name: impl Into<String>,
        threshold: f32,
        output_key: KeyCode,
    ) -> Self {
        Self {
            name: name.into(),
            threshold,
            output_key,
        }
    }
}

impl MappingStrategy for ThresholdMappingStrategy {
    fn apply(&self, input: &InputComponent) -> Option<MappedEvent> {
        match input {
            InputComponent::Trigger(trigger_type, value) => {
                let state = if *value >= self.threshold {
                    KeyState::Pressed
                } else {
                    KeyState::Released
                };
                
                Some(MappedEvent::KeyboardEvent { 
                    key_code: self.output_key, 
                    state
                })
            },
            _ => None, // Andere Komponententypen werden nicht unterstützt
        }
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Direkte Button-zu-Taste-Mapping-Strategie
///
/// Diese Strategie bildet Controller-Buttons direkt auf Tastaturevents ab.
pub struct ButtonMappingStrategy {
    name: String,
    button_type: ButtonType,
    output_key: KeyCode,
}

impl ButtonMappingStrategy {
    /// Erstellt eine neue ButtonMappingStrategy
    pub fn new(
        name: impl Into<String>,
        button_type: ButtonType,
        output_key: KeyCode,
    ) -> Self {
        Self {
            name: name.into(),
            button_type,
            output_key,
        }
    }
}

impl MappingStrategy for ButtonMappingStrategy {
    fn apply(&self, input: &InputComponent) -> Option<MappedEvent> {
        match input {
            InputComponent::Button(button_type, button_state) => {
                if *button_type == self.button_type {
                    let state = match button_state {
                        ButtonEventState::Held | ButtonEventState::Complete => KeyState::Pressed,
                        _ => KeyState::Released,
                    };
                    
                    Some(MappedEvent::KeyboardEvent { 
                        key_code: self.output_key, 
                        state
                    })
                } else {
                    None
                }
            },
            _ => None, // Andere Komponententypen werden nicht unterstützt
        }
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}