//! Mapping Engine Implementierung
//!
//! Dieses Modul enthält die Kernimplementierung der Mapping Engine
//! mit allen Zustandsübergängen und der Verarbeitungslogik.

use std::time::Duration;
use statum::{machine, state};
use tokio::{
    sync::{mpsc, watch},
    select, time::{self, Instant},
};
use tracing::{debug, error, info, warn};

use crate::controller::controller::ControllerOutput;
use crate::mapping::mapping_config::MappingConfig;
use crate::mapping::mapping_types::{
    MappedEvent, MappingError, has_significant_changes, ProcessingData
};

/// Tracking-Informationen für Controller-Updates
struct UpdateMetrics {
    total_updates: usize,
    filtered_updates: usize,
    events_produced: usize,
    last_processing_time: Duration,
}

impl Default for UpdateMetrics {
    fn default() -> Self {
        Self {
            total_updates: 0,
            filtered_updates: 0,
            events_produced: 0,
            last_processing_time: Duration::from_millis(0),
        }
    }
}

/// Zustände der Mapping Engine
#[state]
#[derive(Debug, Clone)]
pub enum MappingEngineState {
    Initializing,
    Ready,
    Processing(ProcessingData),
    ConfigChanging(Box<dyn MappingConfig>),
}

/// Zustandsmaschine für die Mapping Engine
#[machine]
pub struct MappingEngine<S: MappingEngineState> {
    /// Empfänger für Controller-Output
    input_receiver: mpsc::Receiver<ControllerOutput>,
    
    /// Sender für gemappte Events
    output_sender: mpsc::Sender<MappedEvent>,
    
    /// Letzter verarbeiteter Controller-Output (für Änderungserkennung)
    previous_output: Option<ControllerOutput>,
    
    /// Aktuelle Mapping-Konfiguration
    config: Box<dyn MappingConfig>,
    
    /// Metriken für die Verarbeitung
    metrics: UpdateMetrics,
    
    /// Zeitintervall zwischen Status-Logs (in Sekunden)
    log_interval_seconds: u64,
    
    /// Zeitpunkt des letzten Status-Logs
    last_log_time: Instant,
}

/// Implementierung für alle Zustände
impl<S: MappingEngineState> MappingEngine<S> {
    /// Gibt den Namen der aktuellen Konfiguration zurück
    pub fn config_name(&self) -> &str {
        self.config.name()
    }
    
    /// Protokolliert Metriken, wenn das Log-Intervall abgelaufen ist
    fn log_metrics_if_due(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_log_time).as_secs() >= self.log_interval_seconds {
            let efficiency = if self.metrics.total_updates > 0 {
                100.0 * (self.metrics.total_updates - self.metrics.filtered_updates) as f64 
                    / self.metrics.total_updates as f64
            } else {
                0.0
            };
            
            info!(
                "Mapping Engine stats: {} updates processed, {} filtered ({:.1}% efficiency), {} events produced",
                self.metrics.total_updates,
                self.metrics.filtered_updates,
                efficiency,
                self.metrics.events_produced
            );
            
            debug!(
                "Last processing time: {:.2}ms",
                self.metrics.last_processing_time.as_micros() as f64 / 1000.0
            );
            
            // Metriken zurücksetzen
            self.metrics = UpdateMetrics::default();
            self.last_log_time = now;
        }
    }
}

/// Implementierung für Initializing-Zustand
impl MappingEngine<Initializing> {
    /// Erstellt eine neue Mapping Engine im Initializing-Zustand
    pub fn new(
        input_receiver: mpsc::Receiver<ControllerOutput>,
        output_sender: mpsc::Sender<MappedEvent>,
        config: Box<dyn MappingConfig>,
    ) -> Self {
        Self::new(
            input_receiver,
            output_sender,
            None,
            config,
            UpdateMetrics::default(),
            30, // Log-Intervall in Sekunden
            Instant::now(),
        )
    }
    
    /// Initialisiert die Mapping Engine und wechselt in den Ready-Zustand
    pub fn initialize(self) -> MappingEngine<Ready> {
        info!("Initializing Mapping Engine with config: {}", self.config.name());
        debug!("Config type: {:?}", self.config.config_type());
        
        self.transition()
    }
}

/// Implementierung für Ready-Zustand
impl MappingEngine<Ready> {
    /// Wartet auf Controller-Output und wechselt in den Processing-Zustand
    pub async fn wait_for_input(mut self) -> Result<MappingEngine<Processing>, MappingError> {
        self.log_metrics_if_due();
        
        // Auf Controller-Output warten
        match self.input_receiver.recv().await {
            Some(output) => {
                debug!("Received controller output");
                // In den Processing-Zustand wechseln
                let processing_data = ProcessingData { output };
                Ok(self.transition_with(processing_data))
            },
            None => {
                // Kanal wurde geschlossen
                error!("Controller output channel closed");
                Err(MappingError::ChannelClosed)
            }
        }
    }
    
    /// Wechselt in den ConfigChanging-Zustand
    pub fn change_config(self, new_config: Box<dyn MappingConfig>) -> MappingEngine<ConfigChanging> {
        info!("Changing mapping configuration to: {}", new_config.name());
        debug!("New config type: {:?}", new_config.config_type());
        
        self.transition_with(new_config)
    }
}

/// Implementierung für Processing-Zustand
impl MappingEngine<Processing> {
    /// Verarbeitet Controller-Output und erzeugt gemappte Events
    pub fn process_output(mut self) -> MappingEngine<Ready> {
        let processing_start = Instant::now();
        
        // Controller-Output aus dem State-Data extrahieren
        let current_output = if let Some(data) = self.get_state_data() {
            data.output.clone()
        } else {
            // Sollte nicht vorkommen, da wir im Processing-Zustand sind
            warn!("No output data in Processing state");
            return self.transition();
        };
        
        // Änderungserkennung
        let mut events_to_send = Vec::new();
        let output_changed = match &self.previous_output {
            Some(prev) => has_significant_changes(prev, &current_output),
            None => true, // Erste Verarbeitung
        };
        
        self.metrics.total_updates += 1;
        
        if output_changed {
            debug!("Significant changes detected, applying mapping");
            
            // Mapping anwenden
            events_to_send = self.config.map(&current_output);
            debug!("Produced {} mapped events", events_to_send.len());
            
            // Events senden
            for event in events_to_send.iter() {
                match self.output_sender.try_send(event.clone()) {
                    Ok(_) => self.metrics.events_produced += 1,
                    Err(e) => {
                        if e.is_full() {
                            warn!("Output channel is full, event dropped");
                        } else {
                            error!("Failed to send mapped event: {}", e);
                        }
                    }
                }
            }
        } else {
            self.metrics.filtered_updates += 1;
            debug!("No significant changes, skipping mapping");
        }
        
        // Metriken aktualisieren
        self.metrics.last_processing_time = processing_start.elapsed();
        
        // Aktuellen Output als vorherigen speichern
        self.previous_output = Some(current_output);
        
        // Zurück zum Ready-Zustand
        self.transition()
    }
}

/// Implementierung für ConfigChanging-Zustand
impl MappingEngine<ConfigChanging> {
    /// Wendet die neue Konfiguration an und wechselt in den Ready-Zustand
    pub fn apply_new_config(self) -> MappingEngine<Ready> {
        info!("Applied new mapping configuration: {}", self.get_state_data().unwrap().name());
        
        // Wir nehmen die neue Konfiguration aus dem State-Data und wenden sie an
        if let Some(new_config) = self.get_state_data() {
            let mut engine = self.transition();
            engine.config = new_config;
            
            // Vorherigen Output löschen, um bei der nächsten Verarbeitung
            // eine vollständige Neubewertung zu erzwingen
            engine.previous_output = None;
            
            engine
        } else {
            // Sollte nicht vorkommen, da wir im ConfigChanging-Zustand sind
            warn!("No config data in ConfigChanging state");
            self.transition()
        }
    }
}

/// Hauptfunktion für den Engine-Loop
///
/// Startet die Hauptschleife der Mapping Engine und verarbeitet kontinuierlich Events.
pub async fn run_mapping_engine(
    mut engine: MappingEngine<Ready>,
    mut config_receiver: mpsc::Receiver<Box<dyn MappingConfig>>,
) -> Result<(), MappingError> {
    info!("Starting mapping engine main loop with config: {}", engine.config_name());
    
    loop {
        // Warten auf ControllerOutput oder Konfigurationsänderung
        select! {
            result = engine.wait_for_input() => {
                match result {
                    Ok(processing_engine) => {
                        // Output verarbeiten und wieder in Ready-Zustand überführen
                        engine = processing_engine.process_output();
                    },
                    Err(err) => {
                        error!("Error waiting for input: {}", err);
                        return Err(err);
                    }
                }
            },
            new_config = config_receiver.recv() => {
                match new_config {
                    Some(config) => {
                        // Konfiguration ändern
                        let changing_engine = engine.change_config(config);
                        engine = changing_engine.apply_new_config();
                    },
                    None => {
                        // Kanal wurde geschlossen
                        error!("Config channel closed");
                        return Err(MappingError::ChannelClosed);
                    }
                }
            }
        }
    }
}