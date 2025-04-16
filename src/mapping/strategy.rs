//! Trait-Definitionen und allgemeine Strategien für das Mapping von Controller-Events.

use crate::controller;
use crate::controller::controller::ControllerOutput;
use crate::mapping::{MappedEvent, MappingError};
use std::fmt::{Debug, Display};

/// Enum für die verschiedenen Typen von Mapping-Strategien
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MappingType {
    /// Mapping für Keyboard-Events
    Keyboard,
    
    /// Mapping für ELRS (ExpressLRS) Protokoll
    ELRS,
    
    /// Mapping für benutzerdefinierte Ereignisse
    Custom,
}

impl Display for MappingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MappingType::Keyboard => write!(f, "Keyboard"),
            MappingType::ELRS => write!(f, "ELRS"),
            MappingType::Custom => write!(f, "Custom"),
        }
    }
}

/// Trait für Mapping-Konfigurationen
///
/// Dieses Trait definiert die Schnittstelle für Konfigurationen, die von Mapping-Strategien
/// verwendet werden. Es ermöglicht die Validierung und Erstellung von Strategien.
pub trait MappingConfig: Send + Sync + 'static {
    /// Validiert die Konfiguration
    fn validate(&self) -> Result<(), MappingError>;
    
    /// Erstellt eine Strategie aus dieser Konfiguration
    fn create_strategy(&self) -> Result<Box<dyn MappingStrategy>, MappingError>;
    
    /// Gibt den Typ der Mapping-Strategie zurück
    fn get_type(&self) -> MappingType;
    
    /// Gibt den Namen der Konfiguration zurück
    fn get_name(&self) -> String {
        format!("{} Mapping Configuration", self.get_type())
    }
    
    /// Gibt eine Beschreibung der Konfiguration zurück
    fn get_description(&self) -> String {
        format!("Configuration for {} mapping", self.get_type())
    }
}

/// Trait für Mapping-Strategien
///
/// Dieses Trait definiert die Schnittstelle für Strategien, die Controller-Events
/// in verschiedene Ausgabeformate umwandeln.
pub trait MappingStrategy: Send + Sync + 'static {
    /// Wandelt ein Controller-Event in ein gemapptes Event um
    fn map(&mut self, input: &ControllerOutput) -> Option<MappedEvent>;
    
    /// Initialisiert die Strategie
    fn initialize(&mut self) -> Result<(), MappingError>;
    
    /// Fährt die Strategie sauber herunter
    fn shutdown(&mut self);
    
    /// Gibt die gewünschte Rate-Limiting-Konfiguration zurück
    fn get_rate_limit(&self) -> Option<u64> {
        None // Standardimplementierung: kein Rate Limiting
    }
    
    /// Gibt den Typ der Mapping-Strategie zurück
    fn get_type(&self) -> MappingType;
}

/// Hilfsstruktur für den Mapping-Kontext, der zwischen mehreren Aufrufen bestehen bleibt
#[derive(Debug, Default, Clone)]
pub struct MappingContext {
    /// Speichert den letzten Zustand von Buttons 
    pub last_button_states: std::collections::HashMap<crate::controller::controller::ButtonType, controller::controller::ButtonEventState>,
    
    /// Speichert aggregierte Daten für komplexere Mappings
    pub accumulated_data: std::collections::HashMap<String, Vec<u8>>,
    
    /// Speichert den letzten Timestamp für zeitbasierte Mappings
    pub last_timestamp: Option<std::time::SystemTime>,
}