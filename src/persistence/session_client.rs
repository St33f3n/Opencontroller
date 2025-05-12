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

#[derive(Clone, Deserialize, Serialize)]
pub struct SessionClient {
    current_session: String,
    last_session: Option<String>,
    #[serde(skip)]
    config_portal: Arc<ConfigPortal>,
}

impl SessionClient {
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
    pub fn get_portal_ref(&self) -> Arc<ConfigPortal> {
        self.config_portal.clone()
    }

    pub async fn save_current_session(&self) -> Result<()> {
        self.save_session(self.current_session.clone()).await
    }

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
            .execute_potal_action(PortalAction::GetSavedMessagesMsg);
        let saved_msg = if let ConfigResult::MqttMessages(result) = saved_msg {
            result
        } else {
            warn!("Could not retriev valid UiConfig");
            Vec::new()
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
        info!("Session {} saved successfully", name);
        Ok(())
    }

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
            .map_err(|e| eyre!("Failed to check if UI config file exists: {}", e))?
        {
            let content = read_to_string(&session_path)
                .await
                .map_err(|e| eyre!("Failed to read UI config file: {}", e))?;
            toml::from_str(&content).map_err(|e| eyre!("Failed to parse UI config file: {}", e))?
        } else {
            warn!(
                "UI config file does not exist for session {}, using default",
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

    fn ensure_default() -> SessionClient {
        Self {
            current_session: "default".to_string(),
            last_session: None,
            config_portal: Arc::new(ConfigPortal::default()),
        }
    }

    pub async fn change_session(&mut self, name: &str) -> Result<()> {
        self.save_current_session();
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
    fn get_home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| {
            warn!("Could not determine home directory, using current directory");
            PathBuf::from(".")
        })
    }
    pub async fn ensure_default_config() -> Result<()> {
        let mut base_path = SessionClient::get_home_dir();
        base_path.push(CONFIG_DIR);

        if !base_path.exists() {
            info!("Creating default configuration");

            create_dir_all(&base_path)
                .await
                .map_err(|e| eyre!("Failed to create config directory: {}", e))?;

            let config = SessionClient::ensure_default();
            config.save_current_session();
        }

        Ok(())
    }
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
