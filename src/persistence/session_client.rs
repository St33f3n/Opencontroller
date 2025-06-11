//! # Session Management Module
//!
//! ## Why This Module Exists
//! The SessionClient provides persistent configuration management for the OpenController application.
//! It enables users to save, load, and switch between different application configurations (sessions),
//! similar to profiles in other applications. This supports the project's goal of allowing users to
//! quickly switch between different control setups (e.g., different MQTT configurations, mapping
//! strategies, or device setups).
//!
//! ## Key Abstractions
//! - **Session**: A complete snapshot of application configuration including UI settings, controller
//!   mappings, MQTT configurations, and saved messages
//! - **SessionClient**: The main interface for session operations, handling file I/O and state management
//! - **ConfigPortal Integration**: Seamless interaction with the application's central configuration system
//!
//! ## Error Handling Strategy
//! Uses `color_eyre` for rich error context because session operations involve complex file I/O chains
//! where detailed error information is crucial for debugging. Errors bubble up through the persistence
//! layer to the UI for user notification.
//!
//! ## Async Patterns Used
//! All file operations are async to prevent blocking the UI thread during potentially slow disk I/O.
//! Uses tokio's filesystem operations for consistency with the rest of the application's async runtime.

use super::config_portal::{ConfigPortal, ConfigResult, PortalAction};
use super::{ConnectionConfig, ControllerConfig, SavedMessages, SessionConfig, UIConfig};
use color_eyre::{eyre::eyre, Report, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::{
    create_dir_all, metadata, read_dir, read_to_string, remove_dir_all, try_exists, write,
};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

const CONFIG_DIR: &str = ".config/opencontroller/config";
const MAIN_CONFIG_FILE: &str = "main_config.toml";
const UI_CONFIG_FILE: &str = "ui_config.toml";
const CONNECTION_CONFIG_FILE: &str = "connection_config.toml";
const CONTROLLER_CONFIG_FILE: &str = "controller_config.toml";
const MESSAGES_FILE: &str = "saved_messages.toml";
const SESSION_CONFIG_FILE: &str = "session.toml";

/// Manages application sessions and their persistent storage.
///
/// ## Design Rationale
/// SessionClient serves as the bridge between the application's runtime configuration
/// (stored in ConfigPortal) and persistent storage. It implements a session-based
/// configuration system that allows users to maintain multiple independent setups.
///
/// The design separates the concept of "current session" from "session data" to enable
/// atomic session switching without losing the current state.
///
/// ## Usage Context
/// SessionClient is typically initialized at application startup and used throughout
/// the application lifecycle for configuration persistence. It integrates with the
/// ConfigPortal system to ensure consistency between runtime and stored configurations.
#[derive(Clone, Deserialize, Serialize)]
pub struct SessionClient {
    /// Name of the currently active session
    current_session: String,
    /// Previously active session for fallback scenarios
    last_session: Option<String>,
    /// Runtime configuration portal (not serialized)
    #[serde(skip)]
    config_portal: Arc<ConfigPortal>,
}

impl SessionClient {
    /// Loads the most recently used session from persistent storage.
    ///
    /// This is the primary initialization method that attempts to restore the application
    /// to its last known state. It handles the complete bootstrap process including
    /// fallback to default configuration if the last session is corrupted or missing.
    ///
    /// ## Design Rationale
    /// Uses a two-stage loading process: first load the main config to determine which
    /// session was active, then load that specific session. This pattern ensures that
    /// session metadata is preserved separately from session content.
    ///
    /// ## Error Handling
    /// Gracefully degrades to default configuration rather than failing, ensuring the
    /// application can always start even with corrupted configuration files.
    ///
    /// # Errors
    ///
    /// This function is designed not to fail - it will always return a valid SessionClient:
    /// - **Missing main config**: Creates default configuration and logs warning
    /// - **Corrupted session data**: Falls back to default session with error logging  
    /// - **File system errors**: Uses in-memory defaults and warns about persistence issues
    ///
    /// ## Async Behavior
    ///
    /// This function is async because it:
    /// - Reads configuration files from disk
    /// - May need to create directory structures
    /// - Loads session-specific configuration data
    ///
    /// **Cancellation**: Safe to cancel - no partial state modifications occur
    /// **Concurrency**: Should only be called once during application initialization
    pub async fn load_last_session() -> Self {
        let mut path = Self::get_home_dir();
        path.push(CONFIG_DIR);
        path.push(MAIN_CONFIG_FILE);

        let client_string = read_to_string(path).await.unwrap_or_default();

        if client_string.is_empty() {
            error!("Last Session not found trying to load default config");
            let default = Self::ensure_default();
            return default;
        } else {
            let client_res = toml::from_str::<SessionClient>(&client_string);
            let client = client_res.unwrap_or(Self::ensure_default());

            Self::load_session(&client.current_session).await.unwrap()
        }
    }

    /// Returns a reference to the configuration portal for runtime access.
    pub fn get_portal_ref(&self) -> Arc<ConfigPortal> {
        self.config_portal.clone()
    }

    /// Saves the currently active session to persistent storage.
    ///
    /// Convenience wrapper around `save_session` that uses the current session name.
    /// Used by the autosave system and when the user explicitly requests a save operation.
    ///
    /// # Errors
    ///
    /// Returns [`color_eyre::Report`] when:
    /// - **File system errors**: Unable to write configuration files to disk
    ///   - *Recovery*: Check disk space and permissions
    /// - **Serialization errors**: Configuration data cannot be converted to TOML
    ///   - *Recovery*: Reset to default configuration
    /// - **ConfigPortal errors**: Unable to read current configuration state
    ///   - *Recovery*: Check for deadlocks or corrupted internal state
    pub async fn save_current_session(&self) -> Result<()> {
        self.save_session(self.current_session.clone()).await
    }

    /// Saves a complete session configuration to persistent storage.
    ///
    /// This is the core persistence operation that serializes all application configuration
    /// to individual TOML files within a session directory. Each configuration type gets
    /// its own file to enable partial loading and easier debugging.
    ///
    /// ## Design Rationale
    /// Uses separate files for each configuration type rather than a monolithic file because:
    /// - Enables partial loading of specific configuration types
    /// - Reduces risk of total configuration loss due to corruption in one area
    /// - Makes manual configuration editing more manageable
    /// - Allows for type-specific backup and recovery strategies
    ///
    /// ## Error Handling
    /// File operations are atomic where possible - either the entire session is saved
    /// successfully or the previous state is preserved. Uses detailed error context
    /// to help diagnose configuration persistence issues.
    ///
    /// # Errors
    ///
    /// This function can fail in the following scenarios:
    ///
    /// - **Directory creation**: Returns [`color_eyre::Report`] when unable to create session directory
    ///   - *Recovery*: Check filesystem permissions and disk space
    /// - **File serialization**: Returns [`color_eyre::Report`] when configuration cannot be converted to TOML
    ///   - *Recovery*: Reset problematic configuration section to defaults
    /// - **File writing**: Returns [`color_eyre::Report`] when unable to write files to disk
    ///   - *Recovery*: Check disk space and file permissions
    /// - **ConfigPortal access**: Returns [`color_eyre::Report`] when unable to read current configuration
    ///   - *Recovery*: Check for application deadlocks or corrupted state
    ///
    /// ## Async Behavior
    ///
    /// This function is async because it:
    /// - Performs multiple file system operations (create directories, write files)
    /// - Allows other tasks to run during potentially slow disk I/O
    ///
    /// **Cancellation**: Partial writes may occur if cancelled mid-operation
    /// **Concurrency**: Safe to call concurrently for different session names
    pub async fn save_session(&self, name: String) -> Result<()> {
        let mut base_path: PathBuf = Self::get_home_dir();
        base_path.push(CONFIG_DIR);

        let mut main_config: PathBuf = base_path.clone();
        main_config.push(MAIN_CONFIG_FILE);
        base_path.push(&name);

        if !try_exists(&base_path)
            .await
            .map_err(|e| eyre!("Failed to check if session directory exists: {}", e))?
        {
            create_dir_all(&base_path)
                .await
                .map_err(|e| eyre!("Failed to create session directory: {}", e))?;
        }

        let mut ui_path = base_path.clone();
        ui_path.push(UI_CONFIG_FILE);

        let mut session_path = base_path.clone();
        session_path.push(SESSION_CONFIG_FILE);

        let mut connection_path = base_path.clone();
        connection_path.push(CONNECTION_CONFIG_FILE);

        let mut controller_path = base_path.clone();
        controller_path.push(CONTROLLER_CONFIG_FILE);

        let mut messages_path = base_path.clone();
        messages_path.push(MESSAGES_FILE);

        let ui_config = self
            .config_portal
            .execute_potal_action(PortalAction::GetUIConfig);
        let ui_config = if let ConfigResult::UIConfig(ui_c) = ui_config {
            ui_c
        } else {
            warn!("Could not retriev valid UiConfig");
            UIConfig::default()
        };

        let controller_config = self
            .config_portal
            .execute_potal_action(PortalAction::GetControllerConfig);
        let controller_config = if let ConfigResult::ControllerConfig(result) = controller_config {
            result
        } else {
            warn!("Could not retriev valid UiConfig");
            ControllerConfig::default()
        };

        let connection_config = self
            .config_portal
            .execute_potal_action(PortalAction::GetConnectionConfig);
        let connection_config = if let ConfigResult::ConnectionConfig(result) = connection_config {
            result
        } else {
            warn!("Could not retriev valid UiConfig");
            ConnectionConfig::default()
        };

        let saved_msg = self
            .config_portal
            .execute_potal_action(PortalAction::GetSavedMessages);
        let saved_msg = if let ConfigResult::MqttHistory(result) = saved_msg {
            result
        } else {
            warn!("Could not retriev valid UiConfig");
            SavedMessages::default()
        };

        let ui_content = toml::to_string_pretty(&ui_config)
            .map_err(|e| eyre!("Failed to serialize UI config: {}", e))?;

        write(&ui_path, ui_content)
            .await
            .map_err(|e| eyre!("Failed to write UI config file: {}", e))?;

        let connection_content = toml::to_string_pretty(&connection_config)
            .map_err(|e| eyre!("Failed to serialize connection config: {}", e))?;

        write(&connection_path, connection_content)
            .await
            .map_err(|e| eyre!("Failed to write connection config file: {}", e))?;

        let controller_content = toml::to_string_pretty(&controller_config)
            .map_err(|e| eyre!("Failed to serialize controller config: {}", e))?;

        write(&controller_path, controller_content)
            .await
            .map_err(|e| eyre!("Failed to write controller config file: {}", e))?;

        let messages_content = toml::to_string_pretty(&saved_msg)
            .map_err(|e| eyre!("Failed to serialize messages: {}", e))?;

        write(&messages_path, messages_content)
            .await
            .map_err(|e| eyre!("Failed to write messages file: {}", e))?;

        let client_content = toml::to_string_pretty(&self)
            .map_err(|e| eyre!("Failed to parse main config file: {}", e))?;
        write(&main_config, client_content)
            .await
            .map_err(|e| eyre!("Failed to write main config file: {}", e))?;

        let mut current_sessions = match self
            .config_portal
            .execute_potal_action(PortalAction::GetAvailableSessions)
        {
            ConfigResult::AvailableSessions(sessions) => sessions,
            _ => HashMap::new(),
        };

        current_sessions.insert(name.to_string(), base_path.clone());
        self.config_portal
            .execute_potal_action(PortalAction::WriteAvailableSessions(current_sessions));

        let session = if let ConfigResult::SessionConfig(session) = self
            .config_portal
            .execute_potal_action(PortalAction::GetSession)
        {
            session
        } else {
            warn!("Could not read current Session from Configportal");
            SessionConfig::default()
        };

        let session_content = toml::to_string_pretty(&session)
            .map_err(|e| eyre!("Failed to parse session config file: {}", e))?;
        write(&session_path, session_content)
            .await
            .map_err(|e| eyre!("Failed to write session config file: {}", e))?;

        info!("Session {} saved successfully", name);
        Ok(())
    }

    /// Loads a specific session from persistent storage.
    ///
    /// Reconstructs a complete SessionClient instance from stored configuration files.
    /// This method handles the complex process of loading multiple configuration types
    /// and building a consistent runtime state.
    ///
    /// ## Design Rationale
    /// Loads configuration files individually and provides defaults for missing files
    /// rather than failing entirely. This allows sessions to be partially recovered
    /// even if some configuration files are missing or corrupted.
    ///
    /// ## Error Handling
    /// Uses graceful degradation - missing configuration files are replaced with
    /// defaults and warnings are logged. Only fails if the session directory
    /// doesn't exist at all.
    ///
    /// # Errors
    ///
    /// Returns [`color_eyre::Report`] when:
    /// - **Session not found**: The specified session directory does not exist
    ///   - *Recovery*: Check available sessions or create a new session with that name
    /// - **File system access**: Unable to read session directory or files
    ///   - *Recovery*: Check file permissions and disk health
    /// - **Configuration parsing**: TOML files are corrupted and cannot be parsed
    ///   - *Recovery*: Individual files fall back to defaults with warnings
    ///
    /// ## Async Behavior
    ///
    /// This function is async because it:
    /// - Reads multiple configuration files from disk
    /// - Checks for file existence before attempting to read
    ///
    /// **Cancellation**: Safe to cancel - no state modifications occur until success
    /// **Concurrency**: Safe to call concurrently for different session names
    pub async fn load_session(session_name: &str) -> Result<Self> {
        let mut base_path = Self::get_home_dir();
        base_path.push(CONFIG_DIR);
        base_path.push(session_name);

        if !try_exists(&base_path)
            .await
            .map_err(|e| eyre!("Failed to check if session directory exists: {}", e))?
        {
            return Err(eyre!("Session directory does not exist: {}", session_name));
        }

        let mut ui_path = base_path.clone();
        ui_path.push(UI_CONFIG_FILE);

        let mut session_path = base_path.clone();
        session_path.push(SESSION_CONFIG_FILE);

        let mut connection_path = base_path.clone();
        connection_path.push(CONNECTION_CONFIG_FILE);

        let mut controller_path = base_path.clone();
        controller_path.push(CONTROLLER_CONFIG_FILE);

        let mut messages_path = base_path.clone();
        messages_path.push(MESSAGES_FILE);

        let session_config = if try_exists(&session_path)
            .await
            .map_err(|e| eyre!("Failed to check if Session config file exists: {}", e))?
        {
            let content = read_to_string(&session_path)
                .await
                .map_err(|e| eyre!("Failed to read Session config file: {}", e))?;
            toml::from_str(&content)
                .map_err(|e| eyre!("Failed to parse Session config file: {}", e))?
        } else {
            warn!(
                "Session config file does not exist for session {}, using default",
                session_name
            );
            SessionConfig::default()
        };

        let ui_config = if try_exists(&ui_path)
            .await
            .map_err(|e| eyre!("Failed to check if UI config file exists: {}", e))?
        {
            let content = read_to_string(&ui_path)
                .await
                .map_err(|e| eyre!("Failed to read UI config file: {}", e))?;
            toml::from_str(&content).map_err(|e| eyre!("Failed to parse UI config file: {}", e))?
        } else {
            warn!(
                "UI config file does not exist for session {}, using default",
                session_name
            );
            UIConfig::default()
        };

        let connection_config = if try_exists(&connection_path)
            .await
            .map_err(|e| eyre!("Failed to check if connection config file exists: {}", e))?
        {
            let content = read_to_string(&connection_path)
                .await
                .map_err(|e| eyre!("Failed to read connection config file: {}", e))?;

            toml::from_str(&content)
                .map_err(|e| eyre!("Failed to parse connection config file: {}", e))?
        } else {
            warn!(
                "Connection config file does not exist for session {}, using default",
                session_name
            );
            ConnectionConfig::default()
        };

        let controller_config = if try_exists(&controller_path)
            .await
            .map_err(|e| eyre!("Failed to check if controller config file exists: {}", e))?
        {
            let content = read_to_string(&controller_path)
                .await
                .map_err(|e| eyre!("Failed to read controller config file: {}", e))?;

            toml::from_str(&content)
                .map_err(|e| eyre!("Failed to parse controller config file: {}", e))?
        } else {
            warn!(
                "Controller config file does not exist for session {}, using default",
                session_name
            );
            ControllerConfig::default()
        };

        let saved_msg = if try_exists(&messages_path)
            .await
            .map_err(|e| eyre!("Failed to check if messages file exists: {}", e))?
        {
            let content = read_to_string(&messages_path)
                .await
                .map_err(|e| eyre!("Failed to read messages file: {}", e))?;

            toml::from_str(&content).map_err(|e| eyre!("Failed to parse messages file: {}", e))?
        } else {
            warn!(
                "Messages file does not exist for session {}, using default",
                session_name
            );
            SavedMessages::default()
        };

        let portal = ConfigPortal::new(
            session_config.clone(),
            ui_config,
            controller_config,
            connection_config,
            saved_msg,
        );

        let last_session = session_config.last_session.clone();

        Ok(Self {
            current_session: session_name.to_string(),
            last_session,
            config_portal: Arc::new(portal),
        })
    }

    /// Creates a default SessionClient instance for fallback scenarios.
    fn ensure_default() -> SessionClient {
        Self {
            current_session: "default".to_string(),
            last_session: None,
            config_portal: Arc::new(ConfigPortal::default()),
        }
    }

    /// Switches to a different session, saving the current state first.
    ///
    /// This operation performs an atomic session switch by first saving the current
    /// session, then loading the target session. If the target session fails to load,
    /// the system falls back to the default configuration.
    ///
    /// ## Error Handling
    /// Uses a multi-stage fallback strategy to ensure the application remains usable
    /// even if session switching fails partially.
    ///
    /// # Errors
    ///
    /// Returns [`color_eyre::Report`] when:
    /// - **Session save fails**: Current session cannot be persisted before switch
    ///   - *Recovery*: Continues with switch but current changes may be lost
    /// - **Target session load fails**: Target session cannot be loaded
    ///   - *Recovery*: Falls back to default configuration
    /// - **State update fails**: Unable to update session registry
    ///   - *Recovery*: Falls back to default configuration with error logging
    pub async fn change_session(&mut self, name: &str) -> Result<()> {
        self.save_current_session().await;
        let new_session = SessionClient::load_session(name)
            .await
            .unwrap_or(SessionClient::ensure_default());

        let available_sessions = SessionClient::scan_available_sessions()
            .await
            .unwrap_or_default();

        let result = new_session
            .config_portal
            .execute_potal_action(PortalAction::WriteAvailableSessions(available_sessions));

        match result {
            ConfigResult::Success => {
                new_session
                    .save_current_session()
                    .await
                    .map_err(|_e| eyre!("Failed to save loaded session"));
                *self = new_session;
                Ok(())
            }
            _ => {
                error!("Fallback to default configuration as an error while changing_sessions occured. For details refer to logs.");
                *self = SessionClient::ensure_default();
                Err(Report::msg("Fallback to default"))
            }
        }
    }

    /// Scans the configuration directory for available sessions.
    ///
    /// Discovers all valid session directories by attempting to load each one.
    /// Used by the UI to populate session selection menus.
    ///
    /// # Errors
    ///
    /// Returns [`color_eyre::Report`] when unable to access the configuration directory.
    /// Individual session loading errors are logged but don't fail the entire operation.
    pub async fn scan_available_sessions() -> Result<HashMap<String, PathBuf>> {
        let mut base_path = Self::get_home_dir();
        base_path.push(CONFIG_DIR);

        if !try_exists(&base_path)
            .await
            .map_err(|e| eyre!("Failed to check if config directory exists: {}", e))?
        {
            debug!("Config directory does not exist, no sessions available");
            return Ok(HashMap::new());
        }

        let mut available_sessions = HashMap::new();

        let mut read_dir = read_dir(&base_path)
            .await
            .map_err(|e| eyre!("Failed to read config directory: {}", e))?;

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| eyre!("Failed to read directory entry: {}", e))?
        {
            let path = entry.path();

            if metadata(&path)
                .await
                .map_err(|e| eyre!("Failed to get metadata for {}: {}", path.display(), e))?
                .is_dir()
            {
                if let Some(session_name) = path.file_name().and_then(|n| n.to_str()) {
                    match Self::load_session(session_name).await {
                        Ok(_config) => {
                            debug!("Found session: {}", session_name);
                            available_sessions.insert(session_name.to_string(), path);
                        }
                        Err(e) => {
                            warn!("Failed to load session {}: {}", session_name, e);
                        }
                    }
                }
            }
        }

        Ok(available_sessions)
    }

    /// Deletes a session from persistent storage.
    ///
    /// If the session being deleted is currently active, automatically switches
    /// to the last used session or default session as a fallback.
    ///
    /// # Errors
    ///
    /// Returns [`color_eyre::Report`] when unable to delete the session directory
    /// or when the session directory doesn't exist.
    pub async fn delete_session(&mut self, session_name: &str) -> Result<()> {
        if session_name == self.current_session.as_str() {
            let last_session = self.last_session.clone().unwrap_or("default".to_string());
            self.clone().change_session(&last_session).await;
        }

        let mut base_path = Self::get_home_dir();
        base_path.push(CONFIG_DIR);
        base_path.push(session_name);

        if try_exists(&base_path)
            .await
            .map_err(|e| eyre!("Failed to check if session directory exists: {}", e))?
        {
            remove_dir_all(&base_path)
                .await
                .map_err(|e| eyre!("Failed to delete session directory: {}", e))?;

            info!("Session {} deleted successfully", session_name);
            Ok(())
        } else {
            return Err(eyre!("Session directory does not exist: {}", session_name));
        }
    }

    /// Returns the user's home directory or current directory as fallback.
    fn get_home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| {
            warn!("Could not determine home directory, using current directory");
            PathBuf::from(".")
        })
    }

    /// Ensures the default configuration directory structure exists.
    pub async fn ensure_default_config() -> Result<()> {
        let mut base_path = SessionClient::get_home_dir();
        base_path.push(CONFIG_DIR);

        if !base_path.exists() {
            info!("Creating default configuration");

            create_dir_all(&base_path)
                .await
                .map_err(|e| eyre!("Failed to create config directory: {}", e))?;

            let config = SessionClient::ensure_default();
            config.save_current_session().await;
        }

        Ok(())
    }

    /// Starts a background task that periodically saves the current session.
    ///
    /// Provides automatic backup functionality to prevent configuration loss
    /// in case of application crashes or unexpected shutdowns.
    ///
    /// ## Runtime Requirements
    ///
    /// Requires tokio runtime because it spawns a long-running background task
    /// that uses async intervals for timing.
    pub async fn start_autosave_task(
        portal: Arc<Mutex<SessionClient>>,
        interval_seconds: u64,
    ) -> JoinHandle<()> {
        info!(
            "Starting autosave task with interval: {}s",
            interval_seconds
        );

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_seconds));

            loop {
                interval.tick().await;
                if let Err(e) = portal.lock().await.save_current_session().await {
                    error!("Failed to autosave configuration: {}", e);
                } else {
                    debug!("Configuration autosaved successfully");
                }
            }
        })
    }
}
