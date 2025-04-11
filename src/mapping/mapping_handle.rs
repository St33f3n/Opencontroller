//! Öffentliches Interface für die Mapping Engine
//!
//! Dieses Modul stellt das öffentliche Interface für die Interaction mit der
//! Mapping Engine bereit, einschließlich Engine-Erzeugung, Konfigurationswechsel
//! und Event-Abfrage.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::{
    spawn,
    sync::{mpsc, watch},
    time::sleep,
};
use tracing::{debug, error, info, warn};

use crate::controller::controller::ControllerOutput;
use crate::mapping::mapping_config::{ELRSMappingConfig, KeyboardMappingConfig, MappingConfig};
use crate::mapping::mapping_engine::{run_mapping_engine, MappingEngine};
use crate::mapping::mapping_types::{KeyCode, KeyState, MappedEvent, MappingError};

/// Typ-Alias für einen Event-Receiver
pub type EventReceiver = mpsc::Receiver<MappedEvent>;

/// Handle für die Mapping Engine
///
/// Bietet ein öffentliches Interface für die Interaktion mit der Mapping Engine,
/// einschließlich Event-Abfrage und Konfigurationswechsel.
pub struct MappingEngineHandle {
    /// Empfänger für gemappte Events
    output_receiver: mpsc::Receiver<MappedEvent>,

    /// Sender für Konfigurationsänderungen
    config_sender: mpsc::Sender<Box<dyn MappingConfig>>,

    /// Name der aktuellen Konfiguration
    current_config_name: String,
}

impl MappingEngineHandle {
    /// Startet die Mapping Engine mit einer initialen Konfiguration
    pub fn spawn(
        controller_output_receiver: watch::Receiver<ControllerOutput>,
        initial_config: Box<dyn MappingConfig>,
    ) -> Result<Self, MappingError> {
        info!(
            "Spawning Mapping Engine with config: {}",
            initial_config.name()
        );

        // Kanäle für die Kommunikation erstellen
        let (output_sender, output_receiver) = mpsc::channel(100);
        let (config_sender, config_receiver) = mpsc::channel(10);

        // MPSC-Receiver für ControllerOutput erstellen
        let (input_sender, input_receiver) = mpsc::channel(100);

        let config_name = initial_config.name().to_string();

        // Tokio-Task starten, der watch::Receiver abonniert und in MPSC umwandelt
        spawn(async move {
            let mut controller_rx = controller_output_receiver;

            debug!("Starting controller output watch-to-mpsc converter task");

            loop {
                // Auf Änderungen am Controller-State warten
                match controller_rx.changed().await {
                    Ok(_) => {
                        // Neuen State lesen und senden
                        let controller_output = controller_rx.borrow().clone();

                        match input_sender.send(controller_output).await {
                            Ok(_) => debug!("Sent controller output to mapping engine"),
                            Err(e) => {
                                error!("Failed to send controller output: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        // Kanal geschlossen
                        error!("Controller watch channel closed: {}", e);
                        break;
                    }
                }
            }

            warn!("Controller output converter task terminated");
        });

        // Mapping Engine im Initializing-Zustand erstellen
        let engine = MappingEngine::create(
            // Statt MappingEngine::new
            input_receiver,
            output_sender,
            initial_config,
        );

        // Haupttask für die Mapping Engine starten
        spawn(async move {
            debug!("Starting mapping engine main task");

            // Initialisieren und in Ready-Zustand überführen
            let engine = engine.initialize();

            // Hauptschleife starten
            if let Err(e) = run_mapping_engine(engine, config_receiver).await {
                error!("Mapping engine task terminated with error: {}", e);
            } else {
                info!("Mapping engine task terminated");
            }
        });

        Ok(MappingEngineHandle {
            output_receiver,
            config_sender,
            current_config_name: config_name,
        })
    }

    /// Gibt den Namen der aktuellen Konfiguration zurück
    pub fn current_config_name(&self) -> &str {
        &self.current_config_name
    }

    /// Ändert die Konfiguration der Mapping Engine
    pub async fn change_config(
        &mut self,
        new_config: Box<dyn MappingConfig>,
    ) -> Result<(), MappingError> {
        let config_name = new_config.name().to_string();
        info!("Changing mapping configuration to: {}", config_name);

        match self.config_sender.send(new_config).await {
            Ok(_) => {
                debug!("Sent new configuration to mapping engine");
                self.current_config_name = config_name;
                Ok(())
            }
            Err(_) => {
                error!("Failed to send configuration change request");
                Err(MappingError::ConfigChangeError)
            }
        }
    }

    /// Erstellt eine spezifische Tastatur-Konfiguration und wendet sie an
    pub async fn use_keyboard_config(
        &mut self,
        config: KeyboardMappingConfig,
    ) -> Result<(), MappingError> {
        self.change_config(Box::new(config)).await
    }

    /// Erstellt eine spezifische ELRS-Konfiguration und wendet sie an
    pub async fn use_elrs_config(&mut self, config: ELRSMappingConfig) -> Result<(), MappingError> {
        self.change_config(Box::new(config)).await
    }

    /// Wechselt zur Standard-WASD-Konfiguration
    pub async fn use_wasd_config(&mut self) -> Result<(), MappingError> {
        let config = KeyboardMappingConfig::default_wasd();
        self.change_config(Box::new(config)).await
    }

    /// Wechselt zur Standard-Quadcopter-Konfiguration
    pub async fn use_quadcopter_config(&mut self) -> Result<(), MappingError> {
        let config = ELRSMappingConfig::default_quadcopter();
        self.change_config(Box::new(config)).await
    }
}

/// Dienstprogramm zur Verarbeitung von Tastaturevents in einem egui-Kontext
pub struct KeyboardEventProcessor {
    event_receiver: mpsc::Receiver<MappedEvent>,
    key_states: HashMap<KeyCode, bool>, // Korrigiert!
}

impl KeyboardEventProcessor {
    /// Erstellt einen neuen KeyboardEventProcessor
    pub fn new(event_receiver: mpsc::Receiver<MappedEvent>) -> Self {
        Self {
            event_receiver,
            key_states: HashMap::new(),
        }
    }

    /// Verarbeitet alle verfügbaren Events und aktualisiert die Tastaturtabelle
    pub fn process_events(&mut self) {
        // Alle verfügbaren Events empfangen und verarbeiten
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                MappedEvent::KeyboardEvent { key_code, state } => {
                    // Zustand in der Tabelle aktualisieren
                    let is_pressed = match state {
                        KeyState::Pressed => true,
                        KeyState::Released => false,
                    };

                    self.key_states.insert(key_code, is_pressed);
                }
                _ => {
                    // Andere Event-Typen ignorieren
                }
            }
        }
    }

    /// Prüft, ob eine bestimmte Taste gedrückt ist
    pub fn is_key_pressed(&self, key_code: KeyCode) -> bool {
        *self.key_states.get(&key_code).unwrap_or(&false)
    }

    /// Gibt alle aktuell gedrückten Tasten zurück
    pub fn get_pressed_keys(&self) -> Vec<KeyCode> {
        self.key_states
            .iter()
            .filter_map(|(key, pressed)| if *pressed { Some(*key) } else { None })
            .collect()
    }
}

/// Dienstprogramm zur Verarbeitung von ELRS-Events
pub struct ELRSEventProcessor {
    event_receiver: mpsc::Receiver<MappedEvent>,
    channel_values: Vec<u16>,
}

impl ELRSEventProcessor {
    /// Erstellt einen neuen ELRSEventProcessor
    pub fn new(event_receiver: mpsc::Receiver<MappedEvent>, num_channels: usize) -> Self {
        // Standardwerte für alle Kanäle (Mittelstellung)
        let default_value = 1500;
        let mut channel_values = Vec::with_capacity(num_channels);
        for _ in 0..num_channels {
            channel_values.push(default_value);
        }

        Self {
            event_receiver,
            channel_values,
        }
    }

    /// Verarbeitet alle verfügbaren Events und aktualisiert die Kanalwerte
    pub fn process_events(&mut self) {
        // Alle verfügbaren Events empfangen und verarbeiten
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                MappedEvent::ELRSData { channel, value } => {
                    // Kanalwert aktualisieren
                    if (channel as usize) < self.channel_values.len() {
                        self.channel_values[channel as usize] = value;
                    }
                }
                _ => {
                    // Andere Event-Typen ignorieren
                }
            }
        }
    }

    /// Gibt den Wert eines bestimmten Kanals zurück
    pub fn get_channel_value(&self, channel: usize) -> Option<u16> {
        if channel < self.channel_values.len() {
            Some(self.channel_values[channel])
        } else {
            None
        }
    }

    /// Gibt alle Kanalwerte zurück
    pub fn get_all_channel_values(&self) -> &[u16] {
        &self.channel_values
    }
}

/// Hilfsfunktion zum Starten der Mapping Engine mit Standardkonfigurationen
pub async fn setup_mapping_engine(
    controller_output_receiver: watch::Receiver<ControllerOutput>,
) -> Result<MappingEngineHandle, MappingError> {
    // Standard-Konfiguration für Tastatur-Mapping
    let keyboard_config = KeyboardMappingConfig::default_wasd();

    // Mapping Engine starten
    let handle = MappingEngineHandle::spawn(controller_output_receiver, Box::new(keyboard_config))?;

    // Kurze Pause, um sicherzustellen, dass die Engine initialisiert ist
    sleep(Duration::from_millis(100)).await;

    Ok(handle)
}
