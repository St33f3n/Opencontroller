//! Implementierung der ELRS-Mapping-Strategie für Drohnensteuerung

use crate::controller::controller_handle::{ControllerOutput, JoystickType, TriggerType};
use crate::mapping::{
    strategy::MappingContext, MappedEvent, MappingError, MappingStrategy, MappingType,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// ELRS Kanaltypen für das Mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum ELRSChannel {
    /// Roll-Kanal (typischerweise Kanal 0)
    Roll = 0,

    /// Pitch-Kanal (typischerweise Kanal 1)
    Pitch = 1,

    /// Throttle-Kanal (typischerweise Kanal 2)
    Throttle = 2,

    /// Yaw-Kanal (typischerweise Kanal 3)
    Yaw = 3,

    /// AUX1-Kanal (typischerweise Kanal 4)
    Aux1 = 4,

    /// AUX2-Kanal (typischerweise Kanal 5)
    Aux2 = 5,

    /// AUX3-Kanal (typischerweise Kanal 6)
    Aux3 = 6,

    /// AUX4-Kanal (typischerweise Kanal 7)
    Aux4 = 7,

    /// AUX5-Kanal (typischerweise Kanal 8)
    Aux5 = 8,

    /// AUX6-Kanal (typischerweise Kanal 9)
    Aux6 = 9,

    /// AUX7-Kanal (typischerweise Kanal 10)
    Aux7 = 10,

    /// AUX8-Kanal (typischerweise Kanal 11)
    Aux8 = 11,
}

impl From<ELRSChannel> for u16 {
    fn from(channel: ELRSChannel) -> Self {
        channel as u16
    }
}

/// Konfiguration für ELRS-Mapping
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ELRSConfig {
    /// Zuordnung von Joysticks zu ELRS-Kanälen
    joystick_mapping: HashMap<JoystickType, (ELRSChannel, ELRSChannel)>,

    /// Zuordnung von Triggern zu ELRS-Kanälen
    trigger_mapping: HashMap<TriggerType, ELRSChannel>,

    /// Buttons zu ELRS AUX-Kanälen
    button_mapping:
        HashMap<crate::controller::controller_handle::ButtonType, (ELRSChannel, u16, u16)>,

    /// Invertierungsflag für Kanäle
    invert_channel: HashMap<ELRSChannel, bool>,

    /// Failsafe-Werte für alle Kanäle
    failsafe_values: HashMap<ELRSChannel, u16>,

    /// Name der Konfiguration
    name: String,

    /// Mindest- und Höchstwerte für ELRS-Kanäle
    channel_min: u16,
    channel_max: u16,
    channel_mid: u16,
}

impl ELRSConfig {
    /// Erstellt eine neue ELRS-Mapping-Konfiguration
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

    /// Erstellt eine Standardkonfiguration für ELRS-Mapping
    pub fn default_config() -> Self {
        // Standard-Werte für ELRS RC-Systeme
        let channel_min = 1000; // 1000µs entspricht -100%
        let channel_max = 2000; // 2000µs entspricht +100%

        // Joystick-Zuordnung
        let mut joystick_mapping = HashMap::new();
        joystick_mapping.insert(JoystickType::Right, (ELRSChannel::Roll, ELRSChannel::Pitch));
        joystick_mapping.insert(
            JoystickType::Left,
            (ELRSChannel::Yaw, ELRSChannel::Throttle),
        );

        // Trigger-Zuordnung (optional)
        let mut trigger_mapping = HashMap::new();
        trigger_mapping.insert(TriggerType::Left, ELRSChannel::Aux1);
        trigger_mapping.insert(TriggerType::Right, ELRSChannel::Aux2);

        // Button-Zuordnung (Button, Kanal, Wert bei Drücken, Wert bei Loslassen)
        let mut button_mapping = HashMap::new();
        button_mapping.insert(
            crate::controller::controller_handle::ButtonType::A,
            (ELRSChannel::Aux3, 2000, 1000), // Arm-Schalter
        );
        button_mapping.insert(
            crate::controller::controller_handle::ButtonType::B,
            (ELRSChannel::Aux4, 2000, 1000), // Flugmodus-Schalter
        );

        // Kanal-Invertierung
        let mut invert_channel = HashMap::new();
        invert_channel.insert(ELRSChannel::Throttle, true); // Throttle invertieren

        // Failsafe-Werte
        let mut failsafe_values = HashMap::new();
        failsafe_values.insert(ELRSChannel::Roll, 1500); // Mitte
        failsafe_values.insert(ELRSChannel::Pitch, 1500); // Mitte
        failsafe_values.insert(ELRSChannel::Throttle, 1000); // Minimal
        failsafe_values.insert(ELRSChannel::Yaw, 1500); // Mitte
        failsafe_values.insert(ELRSChannel::Aux1, 1000); // Aus
        failsafe_values.insert(ELRSChannel::Aux2, 1000); // Aus
        failsafe_values.insert(ELRSChannel::Aux3, 1000); // Disarm
        failsafe_values.insert(ELRSChannel::Aux4, 1000); // Standard-Flugmodus

        Self::new(
            joystick_mapping,
            trigger_mapping,
            button_mapping,
            invert_channel,
            failsafe_values,
            "Default ELRS Mapping".to_string(),
            channel_min,
            channel_max,
        )
    }
}

impl crate::mapping::MappingConfig for ELRSConfig {
    fn validate(&self) -> Result<(), MappingError> {
        // Mindestanforderungen prüfen
        if self.joystick_mapping.is_empty() {
            return Err(MappingError::ConfigError(
                "Joystick mapping cannot be empty for ELRS configuration".to_string(),
            ));
        }

        // Prüfen, ob die essentiellen Kanäle (0-3) zugeordnet sind
        let mut essential_channels = vec![
            ELRSChannel::Roll,
            ELRSChannel::Pitch,
            ELRSChannel::Throttle,
            ELRSChannel::Yaw,
        ];

        let mut found_channels = Vec::new();

        // Joystick-Mappings prüfen
        for (ch1, ch2) in self.joystick_mapping.values() {
            found_channels.push(*ch1);
            found_channels.push(*ch2);
        }

        // Trigger-Mappings prüfen
        for ch in self.trigger_mapping.values() {
            found_channels.push(*ch);
        }

        // Prüfen, ob alle essentiellen Kanäle zugeordnet sind
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

/// Implementierung der ELRS-Mapping-Strategie
pub struct ELRSStrategy {
    /// Konfiguration für das Mapping
    config: ELRSConfig,

    /// Zustandskontext
    context: MappingContext,

    /// Aktuelle Kanalwerte
    channel_values: HashMap<ELRSChannel, u16>,
}

impl ELRSStrategy {
    /// Erstellt eine neue ELRS-Mapping-Strategie
    pub fn new(config: ELRSConfig) -> Self {
        // Initialisiere Kanalwerte mit Failsafe-Werten
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

    /// Konvertiert einen Joystick-Wert in einen ELRS-Kanalwert
    fn convert_joystick_value(&self, value: f32, invert: bool) -> u16 {
        let range = (self.config.channel_max - self.config.channel_min) as f32;
        let mid = self.config.channel_mid;

        // Wert normalisieren (-1.0 bis 1.0)
        let mut normalized = value.clamp(-1.0, 1.0);

        // Bei Bedarf invertieren
        if invert {
            normalized = -normalized;
        }

        // Konvertieren in Kanalwert
        let channel_value = mid as f32 + (normalized * range / 2.0);

        // Auf u16 runden und begrenzen
        let out = channel_value.round() as u16;
        out.min(self.config.channel_max)
            .max(self.config.channel_min)
    }

    /// Aktualisiert die Kanalwerte basierend auf Joystick-Bewegungen
    fn update_joystick_channels(&mut self, input: &ControllerOutput) {
        for (joystick_type, (x_channel, y_channel)) in &self.config.joystick_mapping {
            let (x, y) = match joystick_type {
                JoystickType::Left => (input.left_stick.x, input.left_stick.y),
                JoystickType::Right => (input.right_stick.x, input.right_stick.y),
            };

            // X-Achse (normalerweise Roll oder Yaw)
            let invert_x = self
                .config
                .invert_channel
                .get(x_channel)
                .copied()
                .unwrap_or(false);
            let x_value = self.convert_joystick_value(x, invert_x);
            self.channel_values.insert(*x_channel, x_value);

            // Y-Achse (normalerweise Pitch oder Throttle)
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

    /// Aktualisiert die Kanalwerte basierend auf Trigger-Bewegungen
    fn update_trigger_channels(&mut self, input: &ControllerOutput) {
        for (trigger_type, channel) in &self.config.trigger_mapping {
            let value = match trigger_type {
                TriggerType::Left => input.left_trigger.value,
                TriggerType::Right => input.right_trigger.value,
            };

            // Trigger-Wert ist 0.0 bis 1.0, in ELRS-Wert umrechnen
            let invert = self
                .config
                .invert_channel
                .get(channel)
                .copied()
                .unwrap_or(false);
            let channel_value = self.convert_joystick_value(value * 2.0 - 1.0, invert);
            self.channel_values.insert(*channel, channel_value);
        }
    }

    /// Aktualisiert die Kanalwerte basierend auf Button-Events
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
    fn map(&mut self, input: &ControllerOutput) -> Option<MappedEvent> {
        // Kanalwerte basierend auf Controller-Eingaben aktualisieren
        self.update_joystick_channels(input);
        self.update_trigger_channels(input);
        self.update_button_channels(input);

        // Kanalwerte in das Ausgabeformat konvertieren
        let mut pre_package = HashMap::new();
        for (channel, value) in &self.channel_values {
            pre_package.insert(*channel as u16, *value);
        }

        // Nur ein Event zurückgeben, wenn tatsächlich Kanäle vorhanden sind
        if pre_package.is_empty() {
            None
        } else {
            Some(MappedEvent::ELRSData { pre_package })
        }
    }

    fn initialize(&mut self) -> Result<(), MappingError> {
        info!("Initializing ELRS mapping strategy: {}", self.config.name);

        // Kanalwerte mit Failsafe-Werten initialisieren
        for (channel, value) in &self.config.failsafe_values {
            self.channel_values.insert(*channel, *value);
        }

        Ok(())
    }

    fn shutdown(&mut self) {
        info!("Shutting down ELRS mapping strategy: {}", self.config.name);

        // Alle Kanäle auf Failsafe-Werte zurücksetzen
        for (channel, value) in &self.config.failsafe_values {
            self.channel_values.insert(*channel, *value);
        }
    }

    fn get_rate_limit(&self) -> Option<u64> {
        Some(50) // 20Hz für ELRS-Kommunikation, typisch für RC-Systeme
    }

    fn get_type(&self) -> MappingType {
        MappingType::ELRS
    }
}
