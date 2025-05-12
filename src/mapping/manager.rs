//! Manager für Mapping-Engines zur Verwaltung verschiedener Mapping-Strategien

use crate::controller::controller_handle::ControllerOutput;
use crate::mapping::custom::CustomConfig;
use crate::mapping::elrs::ELRSConfig;
use crate::mapping::keyboard::KeyboardConfig;
use crate::mapping::MappingStrategy;
use crate::mapping::{
    engine::MappingEngineHandle, MappedEvent, MappingConfig, MappingError, MappingType,
};
use crate::persistence::config_portal::{ConfigPortal, ConfigResult, PortalAction};
use color_eyre::{eyre::Report, Result};
use eframe::egui;
use rumqttc::tokio_rustls::rustls::KeyLog;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

/// Manager für Mapping-Engines zur Verwaltung mehrerer paralleler Mapping-Strategien
pub struct MappingEngineManager {
    /// Aktive Mapping-Engines, indexiert nach Typ
    active_engines: HashMap<
        MappingType,
        (
            MappingEngineHandle,
            mpsc::Receiver<MappedEvent>,
            mpsc::Sender<ControllerOutput>,
        ),
    >,

    /// Old Events from last cycle
    old_events: Vec<egui::Event>,
    /// Receiver für Controller-Events
    controller_rx: mpsc::Receiver<ControllerOutput>,

    ///Output Channels
    ui_tx: mpsc::Sender<Vec<egui::Event>>,
    elrs_tx: mpsc::Sender<HashMap<u16, u16>>,
    custom_tx: mpsc::Sender<HashMap<String, Vec<u8>>>,

    config_portal: Arc<ConfigPortal>,
}

impl MappingEngineManager {
    /// Erstellt einen neuen Mapping-Engine-Manager
    pub fn new(
        controller_rx: mpsc::Receiver<ControllerOutput>,
        ui_tx: mpsc::Sender<Vec<egui::Event>>,
        elrs_tx: mpsc::Sender<HashMap<u16, u16>>,
        custom_tx: mpsc::Sender<HashMap<String, Vec<u8>>>,
        config_portal: Arc<ConfigPortal>,
    ) -> Self {
        info!("Creating new MappingEngineManager");

        Self {
            active_engines: HashMap::new(),
            old_events: Vec::new(),
            controller_rx,
            ui_tx,
            elrs_tx,
            custom_tx,
            config_portal,
        }
    }

    /// Aktiviert eine Mapping-Strategie mit der angegebenen Konfiguration
    pub async fn activate_mapping(
        &mut self,
        mapping_type: MappingType,
    ) -> Result<(), MappingError> {
        let keyboard_config: KeyboardConfig = if let ConfigResult::KeyboardConfig(config) = self
            .config_portal
            .execute_potal_action(PortalAction::GetKeyboardConfig)
        {
            if config.button_mapping.is_empty() {
                KeyboardConfig::default_config()
            } else {
                config
            }
        } else {
            KeyboardConfig::default_config()
        };

        let elrs_config: ELRSConfig = if let ConfigResult::ElrsConfig(config) = self
            .config_portal
            .execute_potal_action(PortalAction::GetElrsConfig)
        {
            if config.joystick_mapping.is_empty() {
                ELRSConfig::default_config()
            } else {
                config
            }
        } else {
            ELRSConfig::default_config()
        };

        // Konfiguration validieren
        if let Err(e) = elrs_config.validate() {
            error!("Invalid configuration: {}", e);
            return Err(MappingError::ConfigError(format!(
                "Invalid configuration: {}",
                e
            )));
        }
        if let Err(e) = keyboard_config.validate() {
            error!("Invalid configuration: {}", e);
            return Err(MappingError::ConfigError(format!(
                "Invalid configuration: {}",
                e
            )));
        }

        // Prüfen, ob bereits eine Engine dieses Typs aktiv ist
        if let Some(mut engine) = self.active_engines.remove(&mapping_type) {
            info!("Deactivating existing mapping engine: {}", mapping_type);

            // Bestehende Engine herunterfahren
            if let Err(e) = engine.0.shutdown().await {
                warn!("Error shutting down existing engine: {}", e);
                // Weitermachen trotz Fehler
            }
        }

        match mapping_type {
            MappingType::Keyboard => {
                info!("Activating mapping: Keyboard ({})", mapping_type);

                // Strategie aus Konfiguration erstellen
                let strategy = keyboard_config.create_strategy()?;

                let mut mapping_engine_handle =
                    MappingEngineHandle::new(mapping_type, mapping_type.to_string());

                let (mapped_event_receiver, controller_state_sender) =
                    mapping_engine_handle.start(strategy)?;

                self.active_engines.insert(
                    mapping_type,
                    (
                        mapping_engine_handle,
                        mapped_event_receiver,
                        controller_state_sender,
                    ),
                );
            }
            MappingType::ELRS => {
                info!("Activating mapping: ELRS ({})", mapping_type);

                // Strategie aus Konfiguration erstellen
                let strategy = elrs_config.create_strategy()?;

                let mut mapping_engine_handle =
                    MappingEngineHandle::new(mapping_type, mapping_type.to_string());

                let (mapped_event_receiver, controller_state_sender) =
                    mapping_engine_handle.start(strategy)?;

                self.active_engines.insert(
                    mapping_type,
                    (
                        mapping_engine_handle,
                        mapped_event_receiver,
                        controller_state_sender,
                    ),
                );
            }
            _ => {}
        }

        Ok(())
    }

    pub async fn run_mapping(&mut self) -> Result<(), Report> {
        info!("Start Mapping");
        loop {
            tokio::time::sleep(Duration::from_millis(20)).await;
            if let Ok(controller_output) = self.controller_rx.try_recv() {
                for (_mapping_type, (_engine, receiver, sender)) in &mut self.active_engines {
                    let sending_result = sender.try_send(controller_output.clone());
                    if let Err(e) = sending_result {
                        warn!("{}", e);
                    }
                    let mapped_events = receiver.try_recv();
                    if let Ok(events) = mapped_events {
                        match events {
                            MappedEvent::KeyboardEvent { key_code } => {
                                info!("Message to send: {:?}", key_code);
                                if key_code != self.old_events {
                                    self.old_events = key_code.clone();
                                    self.ui_tx.try_send(key_code)?;
                                } else {
                                    self.old_events = Vec::new();
                                }
                            }
                            MappedEvent::ELRSData { pre_package } => {
                                self.elrs_tx.try_send(pre_package)?;
                            }
                            MappedEvent::CustomEvent { event_type } => {
                                self.custom_tx.try_send(event_type)?;
                            }
                        }
                    }
                }
            }
        }
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
            if let Err(e) = engine.0.shutdown().await {
                error!("Error shutting down engine: {}", e);
                return Err(e);
            }

            info!("Mapping engine deactivated: {}", engine.0.name);
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
            .map(|(t, h)| (*t, h.0.name.clone()))
            .collect()
    }
}
