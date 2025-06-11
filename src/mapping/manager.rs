//! Mapping engine manager for parallel strategy execution
//!
//! Orchestrates multiple mapping engines running simultaneously. Distributes controller
//! input to all active engines and routes their outputs to appropriate subsystems.
//!
//! # Architecture
//!
//! ```text
//! ControllerOutput ──┬─► KeyboardEngine ──► UI Events
//!                    ├─► ELRSEngine ────────► RC Data  
//!                    └─► CustomEngine ──────► Protocol Data
//! ```
//!
//! Engines run independently with their own rate limiting and state machines.
//! Manager handles lifecycle, configuration loading, and output routing.
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

/// Manager for parallel mapping engine execution
///
/// Handles the lifecycle of multiple mapping engines and routes their outputs.
/// Each engine runs in its own thread with independent rate limiting.
pub struct MappingEngineManager {
    /// Active engines with their communication channels
    ///
    /// Tuple contains: (engine handle, output receiver, input sender)
    active_engines: HashMap<
        MappingType,
        (
            MappingEngineHandle,
            mpsc::Receiver<MappedEvent>,
            mpsc::Sender<ControllerOutput>,
        ),
    >,

    /// Event deduplication for keyboard mapping
    ///
    /// Prevents sending identical consecutive keyboard events to UI.
    old_events: Vec<egui::Event>,
    /// Input and output channels
    controller_rx: mpsc::Receiver<ControllerOutput>,
    ui_tx: mpsc::Sender<Vec<egui::Event>>,
    elrs_tx: mpsc::Sender<HashMap<u16, u16>>,
    custom_tx: mpsc::Sender<HashMap<String, Vec<u8>>>,

    config_portal: Arc<ConfigPortal>,
}

impl MappingEngineManager {
    /// Creates new manager with output channels for each mapping type
    pub fn new(
        controller_rx: mpsc::Receiver<ControllerOutput>,
        ui_tx: mpsc::Sender<Vec<egui::Event>>,
        elrs_tx: mpsc::Sender<HashMap<u16, u16>>,
        custom_tx: mpsc::Sender<HashMap<String, Vec<u8>>>,
        config_portal: Arc<ConfigPortal>,
    ) -> Self {
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

    /// Activates a mapping engine with configuration from ConfigPortal
    ///
    /// Loads configuration, validates it, and spawns the engine. If an engine
    /// of the same type is already active, it will be shut down first.
    pub async fn activate_mapping(
        &mut self,
        mapping_type: MappingType,
    ) -> Result<(), MappingError> {
        // Load configurations from ConfigPortal
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

        // Validate configurations
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

        // Shutdown existing engine of same type if present
        if let Some(mut engine) = self.active_engines.remove(&mapping_type) {
            debug!("Deactivating existing mapping engine: {}", mapping_type);

            // Bestehende Engine herunterfahren
            if let Err(e) = engine.0.shutdown().await {
                warn!("Error shutting down existing engine: {}", e);
                // Weitermachen trotz Fehler
            }
        }

        // Create and start new engine based on type
        match mapping_type {
            MappingType::Keyboard => {
                debug!("Activating mapping: Keyboard ({})", mapping_type);

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
                debug!("Activating mapping: ELRS ({})", mapping_type);

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
            MappingType::Custom => {
                // TODO: Implement custom mapping activation
            }
        }

        Ok(())
    }

    /// Main processing loop - distributes input and routes output
    ///
    /// Runs continuously with 20ms intervals. For each controller input:
    /// 1. Sends input to all active engines
    /// 2. Collects outputs from engines  
    /// 3. Routes outputs to appropriate channels
    /// 4. Handles event deduplication for keyboard events
    pub async fn run_mapping(&mut self) -> Result<(), Report> {
        debug!("Start Mapping");
        loop {
            tokio::time::sleep(Duration::from_millis(20)).await;
            // Process controller input if available
            if let Ok(controller_output) = self.controller_rx.try_recv() {
                for (_mapping_type, (_engine, receiver, sender)) in &mut self.active_engines {
                    // Send input to engine (non_blocking)
                    let sending_result = sender.try_send(controller_output.clone());
                    if let Err(e) = sending_result {
                        warn!("{}", e);
                    }
                    //Collect engine output and route to appropriate channel
                    let mapped_events = receiver.try_recv();
                    if let Ok(events) = mapped_events {
                        match events {
                            MappedEvent::KeyboardEvent { key_code } => {
                                debug!("Message to send: {:?}", key_code);
                                //Deduplicate consecutive identical keyboard events
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
    /// Deactivates a specific mapping engine
    pub async fn deactivate_mapping(
        &mut self,
        mapping_type: MappingType,
    ) -> Result<(), MappingError> {
        debug!("Deactivating mapping of type: {}", mapping_type);

        if let Some(mut engine) = self.active_engines.remove(&mapping_type) {
            if let Err(e) = engine.0.shutdown().await {
                error!("Error shutting down engine: {}", e);
                return Err(e);
            }

            debug!("Mapping engine deactivated: {}", engine.0.name);
            Ok(())
        } else {
            warn!("No active mapping of type: {}", mapping_type);
            Ok(()) // Kein Fehler, wenn keine Engine dieses Typs aktiv ist
        }
    }
    /// Shuts down all active mapping engines
    pub async fn deactivate_all(&mut self) -> Result<(), MappingError> {
        debug!("Deactivating all mapping engines");

        let engine_types: Vec<MappingType> = self.active_engines.keys().cloned().collect();

        for mapping_type in engine_types {
            if let Err(e) = self.deactivate_mapping(mapping_type).await {
                error!("Error deactivating mapping of type {}: {}", mapping_type, e);
                // Weitermachen mit anderen Engines
            }
        }

        debug!("All mapping engines deactivated");
        Ok(())
    }

    /// Checks if a mapping engine is currently active
    pub fn is_mapping_active(&self, mapping_type: MappingType) -> bool {
        self.active_engines.contains_key(&mapping_type)
    }

    /// Returns list of all active mapping engines
    pub fn get_active_mappings(&self) -> Vec<(MappingType, String)> {
        self.active_engines
            .iter()
            .map(|(t, h)| (*t, h.0.name.clone()))
            .collect()
    }
}
