//! Implementierung der Custom-Mapping-Strategie für benutzerdefinierte Protokolle

use crate::controller::controller::{ButtonType, ControllerOutput, JoystickType, TriggerType};
use crate::mapping::{
    strategy::MappingContext, MappedEvent, MappingError, MappingStrategy, MappingType,
};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// Konfiguration für Custom-Mapping
#[derive(Debug, Clone)]
pub struct CustomConfig {
    /// Benutzerdefinierte Mapping-Funktionen für Joysticks
    joystick_handlers: HashMap<JoystickType, JoystickHandler>,

    /// Benutzerdefinierte Mapping-Funktionen für Trigger
    trigger_handlers: HashMap<TriggerType, TriggerHandler>,

    /// Benutzerdefinierte Mapping-Funktionen für Buttons
    button_handlers: HashMap<ButtonType, ButtonHandler>,

    /// Protokoll-Spezifische Konfiguration
    protocol_config: ProtocolConfig,

    /// Name der Konfiguration
    name: String,
}

/// Protokoll-Spezifische Konfiguration
#[derive(Debug, Clone)]
pub struct ProtocolConfig {
    /// Name des Protokolls
    pub protocol_name: String,

    /// Protokoll-Version
    pub protocol_version: String,

    /// Benutzerdefinierte Protokoll-Parameter
    pub parameters: HashMap<String, Vec<u8>>,
}

/// Typ für Joystick-Handler-Funktionen
type JoystickHandler = fn(x: f32, y: f32) -> HashMap<String, Vec<u8>>;

/// Typ für Trigger-Handler-Funktionen
type TriggerHandler = fn(value: f32) -> HashMap<String, Vec<u8>>;

/// Typ für Button-Handler-Funktionen
type ButtonHandler = fn(pressed: bool, duration_ms: f64) -> HashMap<String, Vec<u8>>;

impl CustomConfig {
    /// Erstellt eine neue Custom-Mapping-Konfiguration
    pub fn new(
        joystick_handlers: HashMap<JoystickType, JoystickHandler>,
        trigger_handlers: HashMap<TriggerType, TriggerHandler>,
        button_handlers: HashMap<ButtonType, ButtonHandler>,
        protocol_config: ProtocolConfig,
        name: String,
    ) -> Self {
        Self {
            joystick_handlers,
            trigger_handlers,
            button_handlers,
            protocol_config,
            name,
        }
    }

    /// Erstellt eine minimale Demo-Konfiguration
    pub fn demo_config() -> Self {
        // Demo-Joystick-Handler
        let joystick_handler: JoystickHandler = |x, y| {
            let mut result = HashMap::new();
            // X-Wert als 16-Bit-Integer (Little-Endian) kodieren
            let x_scaled = (x * 32767.0) as i16;
            result.insert("x_pos".to_string(), x_scaled.to_le_bytes().to_vec());

            // Y-Wert als 16-Bit-Integer (Little-Endian) kodieren
            let y_scaled = (y * 32767.0) as i16;
            result.insert("y_pos".to_string(), y_scaled.to_le_bytes().to_vec());

            result
        };

        // Demo-Trigger-Handler
        let trigger_handler: TriggerHandler = |value| {
            let mut result = HashMap::new();
            // Wert als 8-Bit-Integer kodieren
            let byte_value = (value * 255.0) as u8;
            result.insert("trigger".to_string(), vec![byte_value]);
            result
        };

        // Demo-Button-Handler
        let button_handler: ButtonHandler = |pressed, _| {
            let mut result = HashMap::new();
            // Button-Status als einzelnes Byte kodieren
            result.insert("button".to_string(), vec![if pressed { 1 } else { 0 }]);
            result
        };

        // Handler-Maps erstellen
        let mut joystick_handlers = HashMap::new();
        joystick_handlers.insert(JoystickType::Left, joystick_handler);
        joystick_handlers.insert(JoystickType::Right, joystick_handler);

        let mut trigger_handlers = HashMap::new();
        trigger_handlers.insert(TriggerType::Left, trigger_handler);
        trigger_handlers.insert(TriggerType::Right, trigger_handler);

        let mut button_handlers = HashMap::new();
        button_handlers.insert(ButtonType::A, button_handler);
        button_handlers.insert(ButtonType::B, button_handler);
        button_handlers.insert(ButtonType::X, button_handler);
        button_handlers.insert(ButtonType::Y, button_handler);

        // Protokoll-Konfiguration erstellen
        let mut parameters = HashMap::new();
        parameters.insert("device_id".to_string(), vec![0x01, 0x02, 0x03, 0x04]);

        let protocol_config = ProtocolConfig {
            protocol_name: "Demo Protocol".to_string(),
            protocol_version: "1.0".to_string(),
            parameters,
        };

        Self::new(
            joystick_handlers,
            trigger_handlers,
            button_handlers,
            protocol_config,
            "Demo Custom Mapping".to_string(),
        )
    }
}

impl crate::mapping::MappingConfig for CustomConfig {
    fn validate(&self) -> Result<(), MappingError> {
        // Mindestanforderungen prüfen
        if self.joystick_handlers.is_empty()
            && self.trigger_handlers.is_empty()
            && self.button_handlers.is_empty()
        {
            return Err(MappingError::ConfigError(
                "Custom configuration must define at least one handler".to_string(),
            ));
        }

        Ok(())
    }

    fn create_strategy(&self) -> Result<Box<dyn MappingStrategy>, MappingError> {
        Ok(Box::new(CustomStrategy::new(self.clone())))
    }

    fn get_type(&self) -> MappingType {
        MappingType::Custom
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_description(&self) -> String {
        format!(
            "Custom mapping for {} protocol v{}",
            self.protocol_config.protocol_name, self.protocol_config.protocol_version
        )
    }
}

/// Implementierung der Custom-Mapping-Strategie
pub struct CustomStrategy {
    /// Konfiguration für das Mapping
    config: CustomConfig,

    /// Zustandskontext
    context: MappingContext,

    /// Gesammelte Ausgabedaten
    output_data: HashMap<String, Vec<u8>>,
}

impl CustomStrategy {
    /// Erstellt eine neue Custom-Mapping-Strategie
    pub fn new(config: CustomConfig) -> Self {
        // Protokoll-Parameter als Basis-Ausgabedaten verwenden
        let output_data = config.protocol_config.parameters.clone();

        Self {
            config,
            context: MappingContext::default(),
            output_data,
        }
    }

    /// Verarbeitet Joystick-Eingaben
    fn process_joysticks(&mut self, input: &ControllerOutput) {
        for (joystick_type, handler) in &self.config.joystick_handlers {
            let (x, y) = match joystick_type {
                JoystickType::Left => (input.left_stick.x, input.left_stick.y),
                JoystickType::Right => (input.right_stick.x, input.right_stick.y),
            };

            // Handler aufrufen und Ergebnis in Ausgabedaten integrieren
            let data = handler(x, y);
            for (key, value) in data {
                self.output_data.insert(key, value);
            }
        }
    }

    /// Verarbeitet Trigger-Eingaben
    fn process_triggers(&mut self, input: &ControllerOutput) {
        for (trigger_type, handler) in &self.config.trigger_handlers {
            let value = match trigger_type {
                TriggerType::Left => input.left_trigger.value,
                TriggerType::Right => input.right_trigger.value,
            };

            // Handler aufrufen und Ergebnis in Ausgabedaten integrieren
            let data = handler(value);
            for (key, value) in data {
                self.output_data.insert(key, value);
            }
        }
    }

    /// Verarbeitet Button-Eingaben
    fn process_buttons(&mut self, input: &ControllerOutput) {
        for button_event in &input.button_events {
            if let Some(handler) = self.config.button_handlers.get(&button_event.button) {
                let pressed = match button_event.state {
                    crate::controller::controller::ButtonEventState::Held => true,
                    crate::controller::controller::ButtonEventState::Complete => false,
                };

                // Handler aufrufen und Ergebnis in Ausgabedaten integrieren
                let data = handler(pressed, button_event.duration_ms);
                for (key, value) in data {
                    self.output_data.insert(key, value);
                }
            }
        }
    }
}

impl MappingStrategy for CustomStrategy {
    fn map(&mut self, input: &ControllerOutput) -> Option<MappedEvent> {
        // Benutzerdefinierte Verarbeitung aller Eingaben durchführen
        self.process_joysticks(input);
        self.process_triggers(input);
        self.process_buttons(input);

        // Ausgabedaten in ein Event kopieren
        if self.output_data.is_empty() {
            None
        } else {
            Some(MappedEvent::CustomEvent {
                event_type: self.output_data.clone(),
            })
        }
    }

    fn initialize(&mut self) -> Result<(), MappingError> {
        info!("Initializing custom mapping strategy: {}", self.config.name);
        info!(
            "Protocol: {} v{}",
            self.config.protocol_config.protocol_name, self.config.protocol_config.protocol_version
        );

        // Protokoll-Parameter kopieren
        self.output_data = self.config.protocol_config.parameters.clone();

        Ok(())
    }

    fn shutdown(&mut self) {
        info!(
            "Shutting down custom mapping strategy: {}",
            self.config.name
        );
    }

    fn get_rate_limit(&self) -> Option<u64> {
        // Standardmäßig 100Hz für benutzerdefinierte Protokolle
        Some(10)
    }

    fn get_type(&self) -> MappingType {
        MappingType::Custom
    }
}
