//! Fehlerdefinitionen für das Mapping-Modul

use thiserror::Error;

/// Fehlertypen für die Mapping-Engine
#[derive(Debug, Error)]
pub enum MappingError {
    /// Fehler bei der Konfiguration einer Mapping-Strategie
    #[error("Konfigurationsfehler: {0}")]
    ConfigError(String),
    
    /// Fehler bei der Initialisierung einer Mapping-Engine
    #[error("Initialisierungsfehler: {0}")]
    InitializationError(String),
    
    /// Fehler bei der Kommunikation über Kanäle
    #[error("Kanalfehler: {0}")]
    ChannelError(String),
    
    /// Fehler bei der Thread-Verwaltung
    #[error("Thread-Fehler: {0}")]
    ThreadError(String),
    
    /// Fehler bei der Verarbeitung von Ereignissen
    #[error("Verarbeitungsfehler: {0}")]
    ProcessingError(String),
    
    /// Fehler bei der Strategie-Anwendung
    #[error("Strategiefehler: {0}")]
    StrategyError(String),
    
    /// Ungültiger Zustandsübergang
    #[error("Ungültiger Zustandsübergang: {0}")]
    InvalidStateTransition(String),
    
    /// Die angeforderte Mapping-Strategie existiert nicht
    #[error("Unbekannter Mapping-Typ: {0}")]
    UnknownMappingType(String),
    
    /// Allgemeiner Fehler
    #[error("Allgemeiner Fehler: {0}")]
    General(String),
}