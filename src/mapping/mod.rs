//! Modul für die Umwandlung von Controller-Ereignissen in verschiedene Ausgabeformate.
//!
//! Dieses Modul enthält die Implementierung einer Mapping-Engine, die Controller-Ereignisse
//! (`ControllerOutput`) in verschiedene Zielformate (`MappedEvent`) umwandelt.
//! Die Engine basiert auf einer Statum State Machine und unterstützt mehrere Mapping-Strategien.

pub mod engine;
pub mod strategy;
pub mod manager;
pub mod keyboard;
pub mod elrs;
pub mod custom;
pub mod error;

// Re-exports für einfacheren Zugriff
pub use engine::{MappingEngine, MappingEngineState, MappingEngineHandle};
pub use strategy::{MappingStrategy, MappingConfig, MappingType};
pub use manager::MappingEngineManager;
pub use error::MappingError;

// Event-Typ für gemappte Ereignisse
use eframe::egui;
use std::collections::HashMap;

/// Ausgabe-Event-Typ der Mapping-Engine
#[derive(Debug, Clone)]
pub enum MappedEvent {
    /// Tastatur-Event für UI-Integration
    KeyboardEvent { key_code: Vec<egui::Event> },
    
    /// ELRS-Daten für Drohnensteuerung
    ELRSData { pre_package: HashMap<u16, u16> },
    
    /// Custom Event für zukünftige Erweiterungen
    CustomEvent { event_type: HashMap<String, Vec<u8>> },
}

/// Rate-Limiter für Event-Verarbeitung
#[derive(Debug, Clone)]
pub struct RateLimiter {
    /// Minimaler Zeitabstand zwischen Events (in Millisekunden)
    min_interval_ms: u64,
    
    /// Zeitpunkt des letzten verarbeiteten Events
    last_event_time: std::time::Instant,
}

impl RateLimiter {
    /// Erstellt einen neuen Rate-Limiter mit dem angegebenen Mindestintervall
    pub fn new(min_interval_ms: u64) -> Self {
        Self {
            min_interval_ms,
            last_event_time: std::time::Instant::now() - std::time::Duration::from_secs(1),
        }
    }
    
    /// Prüft, ob ein neues Event verarbeitet werden sollte basierend auf dem Intervall
    pub fn should_process(&mut self) -> bool {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_event_time);
        
        if elapsed.as_millis() as u64 >= self.min_interval_ms {
            self.last_event_time = now;
            true
        } else {
            false
        }
    }
}