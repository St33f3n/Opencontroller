use crate::mapping;
use crate::mqtt;
use crate::try_lock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

use super::{ConnectionConfig, ControllerConfig, SavedMessages, SessionConfig, Theme, UIConfig};

#[derive(Default, Debug)]
pub struct ConfigPortal {
    pub session: Arc<RwLock<SessionConfig>>,
    pub ui_config: Arc<RwLock<UIConfig>>,
    pub controller_config: Arc<RwLock<ControllerConfig>>,
    pub connection_config: Arc<RwLock<ConnectionConfig>>,
    pub msg_save: Arc<RwLock<SavedMessages>>,
}

impl ConfigPortal {
    pub fn new(
        session_config: SessionConfig,
        ui_config: UIConfig,
        controller_config: ControllerConfig,
        connection_config: ConnectionConfig,
        msg_save: SavedMessages,
    ) -> Self {
        Self {
            session: Arc::new(RwLock::new(session_config)),
            ui_config: Arc::new(RwLock::new(ui_config)),
            controller_config: Arc::new(RwLock::new(controller_config)),
            connection_config: Arc::new(RwLock::new(connection_config)),
            msg_save: Arc::new(RwLock::new(msg_save)),
        }
    }

    pub fn write_into_session(&self, session_name: String) {
        loop {
            match self.session.try_write() {
                Ok(mut session_guard) => {
                    session_guard.session_name = session_name;
                    break;
                }
                Err(e) => {
                    warn!("Writing {} into session is blocked: {}", session_name, e);
                }
            }
        }
    }

    pub fn get_copy_of_session(&self) -> SessionConfig {
        loop {
            match self.session.try_read() {
                Ok(session_guard) => return session_guard.clone(),
                Err(e) => {
                    warn!("Cloning of session is blocked: {}", e);
                }
            }
        }
    }

    pub fn execute_potal_action(&self, action: PortalAction) -> ConfigResult {
        let result = match action {
            // Session-bezogene Aktionen
            PortalAction::GetSession => {
                try_lock!(@read_lock_retry, self.session.clone(), |guard: &SessionConfig| {
                    ConfigResult::SessionConfig(guard.clone())
                })
            }
            PortalAction::GetSessionName => {
                try_lock!(@read_lock_retry, self.session.clone(), |guard: &SessionConfig| {
                    ConfigResult::String(guard.session_name.clone())
                })
            }
            PortalAction::GetLastSession => {
                try_lock!(@read_lock_retry, self.session.clone(), |guard: &SessionConfig| {
                    ConfigResult::OptionString(guard.last_session.clone())
                })
            }
            PortalAction::GetSessionPath => {
                try_lock!(@read_lock_retry, self.session.clone(), |guard: &SessionConfig| {
                    ConfigResult::PathBuf(guard.path.clone())
                })
            }
            PortalAction::GetAvailableSessions => {
                try_lock!(@read_lock_retry, self.session.clone(), |guard: &SessionConfig| {
                    ConfigResult::AvailableSessions(guard.available_sessions.clone())
                })
            }
            PortalAction::WriteSession(session_config) => {
                try_lock!(@write_lock_retry, self.session.clone(), |guard: &mut SessionConfig| {
                    *guard = session_config;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
            PortalAction::WriteSessionName(name) => {
                try_lock!(@write_lock_retry, self.session.clone(), |guard: &mut SessionConfig| {
                    guard.session_name = name;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
            PortalAction::WriteLastSession(last_session) => {
                try_lock!(@write_lock_retry, self.session.clone(), |guard: &mut SessionConfig| {
                    guard.last_session = last_session;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
            PortalAction::WriteSessionPath(path) => {
                try_lock!(@write_lock_retry, self.session.clone(), |guard: &mut SessionConfig| {
                    guard.path = path;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
            PortalAction::WriteAvailableSessions(sessions) => {
                try_lock!(@write_lock_retry, self.session.clone(), |guard: &mut SessionConfig| {
                    guard.available_sessions = sessions;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }

            // UI-Config-bezogene Aktionen
            PortalAction::GetUIConfig => {
                try_lock!(@read_lock_retry, self.ui_config.clone(), |guard: &UIConfig| {
                    ConfigResult::UIConfig(guard.clone())
                })
            }
            PortalAction::GetTheme => {
                try_lock!(@read_lock_retry, self.ui_config.clone(), |guard: &UIConfig| {
                    ConfigResult::Theme(guard.theme.clone())
                })
            }
            PortalAction::GetFps => {
                try_lock!(@read_lock_retry, self.ui_config.clone(), |guard: &UIConfig| {
                    ConfigResult::Fps(guard.fps)
                })
            }
            PortalAction::WriteUIConfig(ui_config) => {
                try_lock!(@write_lock_retry, self.ui_config.clone(), |guard: &mut UIConfig| {
                    *guard = ui_config;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
            PortalAction::WriteTheme(theme) => {
                try_lock!(@write_lock_retry, self.ui_config.clone(), |guard: &mut UIConfig| {
                    guard.theme = theme;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
            PortalAction::WriteFps(fps) => {
                try_lock!(@write_lock_retry, self.ui_config.clone(), |guard: &mut UIConfig| {
                    guard.fps = fps;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }

            // Controller-Config-bezogene Aktionen
            PortalAction::GetElrsConfig => {
                try_lock!(@read_lock_retry, self.controller_config.clone(), |guard: &ControllerConfig| {
                    ConfigResult::ElrsConfig(guard.elrs_mapping.clone())
                })
            }
            PortalAction::GetKeyboardConfig => {
                try_lock!(@read_lock_retry, self.controller_config.clone(), |guard: &ControllerConfig| {
                    ConfigResult::KeyboardConfig(guard.keyboard_mapping.clone())
                })
            }
            PortalAction::GetControllerConfig => {
                try_lock!(@read_lock_retry, self.controller_config.clone(), |guard: &ControllerConfig| {
                    ConfigResult::ControllerConfig(guard.clone())
                })
            }
            PortalAction::WriteElrsConfig(elrs_config) => {
                try_lock!(@write_lock_retry, self.controller_config.clone(), |guard: &mut ControllerConfig| {
                    guard.elrs_mapping = elrs_config;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
            PortalAction::WriteKeyboardConfig(keyboard_config) => {
                try_lock!(@write_lock_retry, self.controller_config.clone(), |guard: &mut ControllerConfig| {
                    guard.keyboard_mapping = keyboard_config;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
            PortalAction::WriteControllerConfig(controller_config) => {
                try_lock!(@write_lock_retry, self.controller_config.clone(), |guard: &mut ControllerConfig| {
                    *guard = controller_config;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }

            // Connection-Config-bezogene Aktionen
            PortalAction::GetMqttConfig => {
                try_lock!(@read_lock_retry, self.connection_config.clone(), |guard: &ConnectionConfig| {
                    ConfigResult::MqttConfig(guard.mqtt_config.clone())
                })
            }
            PortalAction::GetConnectionConfig => {
                try_lock!(@read_lock_retry, self.connection_config.clone(), |guard: &ConnectionConfig| {
                    ConfigResult::ConnectionConfig(guard.clone())
                })
            }
            PortalAction::WriteMqttConfig(mqtt_config) => {
                try_lock!(@write_lock_retry, self.connection_config.clone(), |guard: &mut ConnectionConfig| {
                    guard.mqtt_config = mqtt_config;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
            PortalAction::WriteConnectionConfig(connection_config) => {
                try_lock!(@write_lock_retry, self.connection_config.clone(), |guard: &mut ConnectionConfig| {
                    *guard = connection_config;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }

            // SavedMessages-bezogene Aktionen
            PortalAction::GetSavedMessagesMsg => {
                try_lock!(@read_lock_retry, self.msg_save.clone(), |guard: &SavedMessages| {
                    ConfigResult::MqttMessages(guard.msg.clone())
                })
            }
            PortalAction::WriteSavedMessages(saved_messages) => {
                try_lock!(@write_lock_retry, self.msg_save.clone(), |guard: &mut SavedMessages| {
                    *guard = saved_messages;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
            PortalAction::WriteSavedMessagesMsg(messages) => {
                try_lock!(@write_lock_retry, self.msg_save.clone(), |guard: &mut SavedMessages| {
                    guard.msg = messages;
                    Ok::<ConfigResult, Error>(ConfigResult::Success)
                })
            }
        };

        match result {
            Ok(res) => res,
            Err(e) => ConfigResult::Failed(e),
        }
    }
}

#[derive(Debug)]
pub enum PortalAction {
    // Session-bezogene Aktionen
    GetSession,
    GetSessionName,
    GetLastSession,
    GetSessionPath,
    GetAvailableSessions,
    WriteSession(SessionConfig),
    WriteSessionName(String),
    WriteLastSession(Option<String>),
    WriteSessionPath(PathBuf),
    WriteAvailableSessions(HashMap<String, PathBuf>),

    // UI-Config-bezogene Aktionen
    GetUIConfig,
    GetTheme,
    GetFps,
    WriteUIConfig(UIConfig),
    WriteTheme(Theme),
    WriteFps(u8),

    // Controller-Config-bezogene Aktionen
    GetElrsConfig,
    GetKeyboardConfig,
    GetControllerConfig,
    WriteElrsConfig(mapping::elrs::ELRSConfig),
    WriteKeyboardConfig(mapping::keyboard::KeyboardConfig),
    WriteControllerConfig(ControllerConfig),

    // Connection-Config-bezogene Aktionen
    GetMqttConfig,
    GetConnectionConfig,
    WriteMqttConfig(mqtt::config::MqttConfig),
    WriteConnectionConfig(ConnectionConfig),

    // SavedMessages-bezogene Aktionen
    GetSavedMessagesMsg,
    WriteSavedMessages(SavedMessages),
    WriteSavedMessagesMsg(Vec<mqtt::message_manager::MQTTMessage>),
}

pub enum ConfigResult {
    Success,
    SessionConfig(SessionConfig),
    String(String),
    OptionString(Option<String>),
    PathBuf(std::path::PathBuf),
    AvailableSessions(std::collections::HashMap<String, std::path::PathBuf>),
    UIConfig(UIConfig),
    Theme(Theme),
    Fps(u8),
    ControllerConfig(ControllerConfig),
    ElrsConfig(mapping::elrs::ELRSConfig),
    KeyboardConfig(mapping::keyboard::KeyboardConfig),
    ConnectionConfig(ConnectionConfig),
    MqttConfig(mqtt::config::MqttConfig),
    MqttMessages(Vec<mqtt::message_manager::MQTTMessage>),
    Failed(Error),
}

#[macro_export]
macro_rules! try_lock {
    // Inneres Makro für Write-Lock-Operationen
    (@write_lock_retry, $accessor:expr, $operation:expr) => {{
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 5;

        loop {
            match $accessor.try_write() {
                Ok(mut guard) => {
                    $operation(&mut *guard);
                    break Ok($crate::persistence::config_portal::ConfigResult::Success);
                }
                Err(e) => {
                    attempts += 1;
                    tracing::warn!(
                        "Write lock blocked: {} (attempt {}/{})",
                        e,
                        attempts,
                        MAX_ATTEMPTS
                    );

                    if attempts >= MAX_ATTEMPTS {
                        break Err($crate::persistence::config_portal::Error::LockTimeout);
                    }

                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    }};

    // Inneres Makro für Read-Lock-Operationen
    (@read_lock_retry, $accessor:expr, $operation:expr) => {{
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 5;

        loop {
            match $accessor.try_read() {
                Ok(guard) => {
                    let result = $operation(&*guard);
                    break Ok(result);
                }
                Err(e) => {
                    attempts += 1;
                    tracing::warn!(
                        "Read lock blocked: {} (attempt {}/{})",
                        e,
                        attempts,
                        MAX_ATTEMPTS
                    );

                    if attempts >= MAX_ATTEMPTS {
                        break Err($crate::persistence::config_portal::Error::LockTimeout);
                    }

                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    }};
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Konnte Lock nach maximalen Versuchen nicht erhalten")]
    LockTimeout,

    #[error("Session nicht gefunden: {0}")]
    SessionNotFound(String),

    #[error("Ungültige Operation: {0}")]
    InvalidOperation(String),
    // Weitere Fehlertypen...
}
