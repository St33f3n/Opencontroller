//! Manager für Mapping-Engines zur Verwaltung verschiedener Mapping-Strategien

use crate::controller::controller::ControllerOutput;
use crate::mapping::{
    engine::{MappingEngine, MappingEngineHandle},
    MappedEvent, MappingConfig, MappingError, MappingStrategy, MappingType,
};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{debug, error, info, warn};

/// Manager für Mapping-Engines zur Verwaltung mehrerer paralleler Mapping-Strategien
pub struct MappingEngineManager {
    /// Aktive Mapping-Engines, indexiert nach Typ
    active_engines: HashMap<MappingType, MappingEngineHandle>,

    /// Receiver für Controller-Events
    controller_rx: watch::Receiver<ControllerOutput>,

    /// Sender und Receiver für verschiedene Ausgabetypen
    keyboard_tx: mpsc::Sender<MappedEvent>,
    elrs_tx: mpsc::Sender<MappedEvent>,
    custom_tx: mpsc::Sender<MappedEvent>,
}

impl MappingEngineManager {
    /// Erstellt einen neuen Mapping-Engine-Manager
    pub fn new(
        controller_rx: watch::Receiver<ControllerOutput>,
        keyboard_tx: mpsc::Sender<MappedEvent>,
        elrs_tx: mpsc::Sender<MappedEvent>,
        custom_tx: mpsc::Sender<MappedEvent>,
    ) -> Self {
        info!("Creating new MappingEngineManager");

        Self {
            active_engines: HashMap::new(),
            controller_rx,
            keyboard_tx,
            elrs_tx,
            custom_tx,
        }
    }

    /// Aktiviert eine Mapping-Strategie mit der angegebenen Konfiguration
    pub async fn activate_mapping(
        &mut self,
        config: Box<dyn MappingConfig>,
    ) -> Result<(), MappingError> {
        let mapping_type = config.get_type();
        let config_name = config.get_name();

        info!("Activating mapping: {} ({})", config_name, mapping_type);

        // Konfiguration validieren
        if let Err(e) = config.validate() {
            error!("Invalid configuration: {}", e);
            return Err(MappingError::ConfigError(format!(
                "Invalid configuration: {}",
                e
            )));
        }

        // Prüfen, ob bereits eine Engine dieses Typs aktiv ist
        if let Some(mut engine) = self.active_engines.remove(&mapping_type) {
            info!(
                "Deactivating existing mapping engine: {} ({})",
                engine.name, mapping_type
            );

            // Bestehende Engine herunterfahren
            if let Err(e) = engine.shutdown().await {
                warn!("Error shutting down existing engine: {}", e);
                // Weitermachen trotz Fehler
            }
        }

        // Strategie aus Konfiguration erstellen
        let strategy = config.create_strategy()?;

        // Ausgabekanal basierend auf Mapping-Typ wählen
        let output_sender = match mapping_type {
            MappingType::Keyboard => self.keyboard_tx.clone(),
            MappingType::ELRS => self.elrs_tx.clone(),
            MappingType::Custom => self.custom_tx.clone(),
        };

        // Mapping-Engine erstellen und konfigurieren
        // Hier create() statt new() verwenden
        let engine = MappingEngine::create(
            self.controller_rx.clone(),
            output_sender,
            mapping_type,
            config_name.clone(),
        )
        .configure(strategy)?;

        // Engine aktivieren
        let active_engine = engine.activate();

        // Shutdown-Kanal erstellen
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // Task für die Engine starten
        let engine_name = config_name.clone();
        let task_handle = tokio::spawn(async move {
            match active_engine.run_until_shutdown(shutdown_rx).await {
                Ok(deactivating_engine) => {
                    info!("Engine entering deactivating state: {}", engine_name);
                    let _ = deactivating_engine.shutdown().await;
                    Ok(())
                }
                Err(e) => {
                    error!("Error running engine: {} - {}", engine_name, e);
                    Err(e)
                }
            }
        });

        // Handle für die Engine erstellen und speichern
        let handle =
            MappingEngineHandle::new(mapping_type, config_name.clone(), task_handle, shutdown_tx);

        self.active_engines.insert(mapping_type, handle);

        info!(
            "Mapping engine activated: {} ({})",
            config_name, mapping_type
        );
        Ok(())
    }

    /// Deaktiviert eine Mapping-Strategie des angegebenen Typs
    pub async fn deactivate_mapping(
        &mut self,
        mapping_type: MappingType,
    ) -> Result<(), MappingError> {
        info!("Deactivating mapping of type: {}", mapping_type);

        // Prüfen, ob eine Engine dieses Typs aktiv ist
        if let Some(mut engine) = self.active_engines.remove(&mapping_type) {
            // Engine herunterfahren
            if let Err(e) = engine.shutdown().await {
                error!("Error shutting down engine: {}", e);
                return Err(e);
            }

            info!("Mapping engine deactivated: {}", engine.name);
            Ok(())
        } else {
            warn!("No active mapping of type: {}", mapping_type);
            Ok(()) // Kein Fehler, wenn keine Engine dieses Typs aktiv ist
        }
    }

    /// Deaktiviert alle aktiven Mapping-Strategien
    pub async fn deactivate_all(&mut self) -> Result<(), MappingError> {
        info!("Deactivating all mapping engines");

        let engine_types: Vec<MappingType> = self.active_engines.keys().cloned().collect();

        for mapping_type in engine_types {
            if let Err(e) = self.deactivate_mapping(mapping_type).await {
                error!("Error deactivating mapping of type {}: {}", mapping_type, e);
                // Weitermachen mit anderen Engines
            }
        }

        info!("All mapping engines deactivated");
        Ok(())
    }

    /// Prüft, ob eine Mapping-Strategie des angegebenen Typs aktiv ist
    pub fn is_mapping_active(&self, mapping_type: MappingType) -> bool {
        self.active_engines.contains_key(&mapping_type)
    }

    /// Gibt die Namen aller aktiven Mapping-Strategien zurück
    pub fn get_active_mappings(&self) -> Vec<(MappingType, String)> {
        self.active_engines
            .iter()
            .map(|(t, h)| (*t, h.name.clone()))
            .collect()
    }
}
