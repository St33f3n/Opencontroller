use super::config_portal::{ConfigPortal, ConfigResult, PortalAction};
use super::{ConnectionConfig, ControllerConfig, SavedMessages, SessionConfig, UIConfig};
use color_eyre::{eyre::eyre, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::{read, read_dir, read_to_string, try_exists, write};
use tracing::{debug, error, info, warn};

const CONFIG_DIR: &str = ".config/opencontroller/config";
const MAIN_CONFIG_FILE: &str = "main_config.toml";
const UI_CONFIG_FILE: &str = "ui_config.toml";
const CONNECTION_CONFIG_FILE: &str = "connection_config.toml";
const CONTROLLER_CONFIG_FILE: &str = "controller_config.toml";
const MESSAGES_FILE: &str = "saved_messages.toml";

pub struct SessionClient {
    current_session: String,
    last_session: Option<String>,
    config_portal: Arc<ConfigPortal>,
}

impl SessionClient {
    async fn save_current_session(&self) -> Result<()> {
        self.save_session(self.current_session.clone()).await
    }

    async fn save_session(&self, name: String) -> Result<()> {
        let mut base_path: PathBuf = get_home_dir();
        base_path.push(CONFIG_DIR);
        base_path.push(&name);

        if !tokio::fs::try_exists(&base_path)
            .await
            .map_err(|e| eyre!("Failed to check if session directory exists: {}", e))?
        {
            tokio::fs::create_dir_all(&base_path)
                .await
                .map_err(|e| eyre!("Failed to create session directory: {}", e))?;
        }

        let mut ui_path = base_path.clone();
        ui_path.push(UI_CONFIG_FILE);

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

        tokio::fs::write(&ui_path, ui_content)
            .await
            .map_err(|e| eyre!("Failed to write UI config file: {}", e))?;

        let connection_content = toml::to_string_pretty(&connection_config)
            .map_err(|e| eyre!("Failed to serialize connection config: {}", e))?;

        tokio::fs::write(&connection_path, connection_content)
            .await
            .map_err(|e| eyre!("Failed to write connection config file: {}", e))?;

        let controller_content = toml::to_string_pretty(&controller_config)
            .map_err(|e| eyre!("Failed to serialize controller config: {}", e))?;

        tokio::fs::write(&controller_path, controller_content)
            .await
            .map_err(|e| eyre!("Failed to write controller config file: {}", e))?;

        let messages_content = toml::to_string_pretty(&saved_msg)
            .map_err(|e| eyre!("Failed to serialize messages: {}", e))?;

        tokio::fs::write(&messages_path, messages_content)
            .await
            .map_err(|e| eyre!("Failed to write messages file: {}", e))?;

        info!("Session {} saved successfully", name);
        Ok(())
    }

    async fn load_session(session_name: &str) -> Result<Self> {
        let mut base_path = Self::get_home_dir();
        base_path.push(CONFIG_DIR);
        base_path.push(session_name);

        if !tokio::fs::try_exists(&base_path)
            .await
            .map_err(|e| eyre!("Failed to check if session directory exists: {}", e))?
        {
            return Err(eyre!("Session directory does not exist: {}", session_name));
        }

        let mut ui_path = base_path.clone();
        ui_path.push(UI_CONFIG_FILE);

        let mut connection_path = base_path.clone();
        connection_path.push(CONNECTION_CONFIG_FILE);

        let mut controller_path = base_path.clone();
        controller_path.push(CONTROLLER_CONFIG_FILE);

        let mut messages_path = base_path.clone();
        messages_path.push(MESSAGES_FILE);

        let ui_config = if tokio::fs::try_exists(&ui_path)
            .await
            .map_err(|e| eyre!("Failed to check if UI config file exists: {}", e))?
        {
            let content = tokio::fs::read_to_string(&ui_path)
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

        let connection_config = if tokio::fs::try_exists(&connection_path)
            .await
            .map_err(|e| eyre!("Failed to check if connection config file exists: {}", e))?
        {
            let content = tokio::fs::read_to_string(&connection_path)
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

        let controller_config = if tokio::fs::try_exists(&controller_path)
            .await
            .map_err(|e| eyre!("Failed to check if controller config file exists: {}", e))?
        {
            let content = tokio::fs::read_to_string(&controller_path)
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

        let saved_msg = if tokio::fs::try_exists(&messages_path)
            .await
            .map_err(|e| eyre!("Failed to check if messages file exists: {}", e))?
        {
            let content = tokio::fs::read_to_string(&messages_path)
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

        Ok(Self {
            session_name: session_name.to_string(),
            last_session: Some(session_name.to_string()),
            path: base_path,
            available_sessions: HashMap::new(), // Diese werden separat geladen
            ui_config,
            connection_config,
            controller_config,
            saved_msg,
        })
    }
}

fn get_home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| {
        warn!("Could not determine home directory, using current directory");
        PathBuf::from(".")
    })
}

// Scannt den Konfigurationsordner nach verfügbaren Sessions (mit tokio::fs)
async fn scan_available_sessions() -> Result<HashMap<String, PathBuf>> {
    let mut base_path = Self::get_home_dir();
    base_path.push(CONFIG_DIR);

    if !tokio::fs::try_exists(&base_path)
        .await
        .map_err(|e| eyre!("Failed to check if config directory exists: {}", e))?
    {
        debug!("Config directory does not exist, no sessions available");
        return Ok(HashMap::new());
    }

    let mut available_sessions = HashMap::new();

    let mut read_dir = tokio::fs::read_dir(&base_path)
        .await
        .map_err(|e| eyre!("Failed to read config directory: {}", e))?;

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| eyre!("Failed to read directory entry: {}", e))?
    {
        let path = entry.path();

        if tokio::fs::metadata(&path)
            .await
            .map_err(|e| eyre!("Failed to get metadata for {}: {}", path.display(), e))?
            .is_dir()
        {
            if let Some(session_name) = path.file_name().and_then(|n| n.to_str()) {
                match Self::load_session(session_name).await {
                    Ok(config) => {
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

// Löscht eine Session (mit tokio::fs)
async fn delete_session(session_name: &str) -> Result<()> {
    let mut base_path = Self::get_home_dir();
    base_path.push(CONFIG_DIR);
    base_path.push(session_name);

    if tokio::fs::try_exists(&base_path)
        .await
        .map_err(|e| eyre!("Failed to check if session directory exists: {}", e))?
    {
        tokio::fs::remove_dir_all(&base_path)
            .await
            .map_err(|e| eyre!("Failed to delete session directory: {}", e))?;

        info!("Session {} deleted successfully", session_name);
        Ok(())
    } else {
        return Err(eyre!("Session directory does not exist: {}", session_name));
    }
}

pub async fn start_autosave_task(
    config_portal: Arc<Self>,
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

            if let Err(e) = config_portal.save_current_session().await {
                error!("Failed to autosave configuration: {}", e);
            } else {
                debug!("Configuration autosaved successfully");
            }
        }
    })
}
pub async fn ensure_default_config() -> Result<()> {
    let mut base_path = Config::get_home_dir();
    base_path.push(CONFIG_DIR);

    if !base_path.exists() {
        info!("Creating default configuration");

        fs::create_dir_all(&base_path)
            .map_err(|e| eyre!("Failed to create config directory: {}", e))?;

        let config = Config::default();
        config.save_main_config().await?;

        // Erstelle eine Standardsession
        let default_session = "default";
        let mut session_path = base_path.clone();
        session_path.push(default_session);

        if !session_path.exists() {
            fs::create_dir_all(&session_path)
                .map_err(|e| eyre!("Failed to create default session directory: {}", e))?;

            let mut default_config = Config::new(default_session.to_string());
            default_config.last_session = Some(default_session.to_string());
            default_config.save_session().await?;

            // Aktualisiere die Hauptkonfiguration
            let mut main_config = Config::load_main_config().await?;
            main_config.last_session = Some(default_session.to_string());
            main_config.available_sessions = Config::scan_available_sessions().await?;
            main_config.save_main_config().await?;
        }
    }

    Ok(())
}
