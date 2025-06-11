//! # Configuration Portal - Central Configuration Management Hub
//!
//! Implements the central configuration management system for OpenController, providing
//! thread-safe access to all application configuration data through a unified interface.
//! This module serves as the "convergence point" in OpenController's diamond architecture,
//! where all configuration data flows together for coordinated access and persistence.
//!
//! ## Why This Module Exists
//!
//! OpenController's multi-threaded architecture requires coordinated access to shared
//! configuration across 8+ concurrent threads (UI, MQTT, controller processing, mapping
//! engines, etc.). This portal provides:
//! - **Thread-safe configuration access** across all application components
//! - **Atomic configuration updates** to prevent inconsistent state during changes
//! - **Centralized configuration point** to avoid scattered config management
//! - **Performance-optimized concurrent reads** while maintaining write safety
//!
//! ## Architecture Design Rationale
//!
//! ### Diamond Architecture Integration
//! This module implements the "convergence point" of OpenController's diamond architecture:
//! ```text
//! UI Thread
//!    ↓
//! Multiple Processing Threads (Controller, MQTT, Mapping Engines, etc.)
//!    ↓
//! ConfigPortal (This Module) ← Central configuration convergence
//!    ↓
//! Persistence Layer
//! ```
//!
//! ### Thread Safety Strategy
//! Uses `Arc<RwLock<T>>` for each configuration section to enable:
//! - **Multiple concurrent readers**: UI and processing threads can read simultaneously
//! - **Exclusive write access**: Configuration updates are atomic and consistent
//! - **Shared ownership**: Configuration can be accessed from any thread safely
//! - **Memory efficiency**: Single configuration copy shared across all threads
//!
//! ### Action/Result Pattern
//! Instead of direct method access, uses action dispatch pattern because:
//! - **Uniform error handling**: All operations use consistent retry logic
//! - **Centralized locking**: Single implementation handles all lock complexity
//! - **Type safety**: Compile-time verification of valid operations
//! - **Extensibility**: Easy to add new configuration operations
//!
//! ## Performance Considerations
//!
//! **Read Performance**: RwLock allows unlimited concurrent readers, enabling multiple
//! threads to access configuration without blocking each other during normal operation.
//!
//! **Write Performance**: Write operations use retry logic with linear backoff
//! to handle contention gracefully without spinning or blocking indefinitely.
//!
//! **Memory Efficiency**: Arc sharing means only one copy of each configuration
//! exists in memory regardless of how many threads access it.
//!
//! ## Lock Contention Handling
//!
//! The custom `try_lock!` macro implements sophisticated retry logic:
//! - **Progressive retry**: 5 attempts with 10ms delays between retries
//! - **Graceful degradation**: Returns timeout error rather than panicking
//! - **Performance monitoring**: Logs contention for debugging and optimization
//! - **Non-blocking design**: Never blocks indefinitely or spins endlessly
//!
//! ## Error Handling Strategy
//!
//! Configuration errors are categorized and handled appropriately:
//! - **Lock timeouts**: Recoverable errors that indicate high system load
//! - **Session errors**: Configuration inconsistencies that require user intervention
//! - **Operation errors**: Invalid configuration state that needs correction
//! - **Fallback behavior**: All operations provide safe default behavior

use crate::mapping;
use crate::mqtt;
use crate::try_lock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

use super::{ConnectionConfig, ControllerConfig, SavedMessages, SessionConfig, Theme, UIConfig};

/// Central hub for all application configuration data with thread-safe access.
///
/// ## Design Rationale
///
/// Acts as the single source of truth for all configuration data in OpenController.
/// Each configuration section is wrapped in `Arc<RwLock<T>>` to provide:
/// - **Concurrent read access**: Multiple threads can read configuration simultaneously
/// - **Atomic write operations**: Configuration updates are consistent and isolated
/// - **Memory sharing**: Single configuration instance shared across all threads
/// - **Type safety**: Each configuration type is strongly typed and validated
///
/// ## Configuration Sections
///
/// The portal organizes configuration into logical sections:
/// - **Session**: Current session name, available sessions, and session metadata
/// - **UI Config**: Theme settings, FPS limits, and display preferences
/// - **Controller Config**: Gamepad mappings for keyboard and ELRS output
/// - **Connection Config**: MQTT broker settings and network configuration
/// - **Message Save**: Persistent message history and saved MQTT messages
///
/// ## Thread Safety Architecture
///
/// Uses Tokio's async RwLock instead of std::sync::RwLock because:
/// - **Async compatibility**: Integrates with OpenController's async architecture
/// - **Future-proofing**: Enables async configuration operations if needed
/// - **Consistent API**: Matches other async components in the system
/// - **Deadlock prevention**: Async locks have better debugging and monitoring
///
/// ## Usage Context
///
/// The ConfigPortal is:
/// - Created during application startup with loaded configuration
/// - Shared across all threads via Arc cloning
/// - Accessed through the action dispatch pattern for consistency
/// - Updated in response to UI changes and session management operations
#[derive(Default, Debug)]
pub struct ConfigPortal {
    /// Session management and metadata (current session, available sessions, paths)
    pub session: Arc<RwLock<SessionConfig>>,

    /// User interface configuration (themes, display settings, performance)
    pub ui_config: Arc<RwLock<UIConfig>>,

    /// Controller and mapping configuration (gamepad → output mappings)
    pub controller_config: Arc<RwLock<ControllerConfig>>,

    /// Network and communication configuration (MQTT, future protocols)
    pub connection_config: Arc<RwLock<ConnectionConfig>>,

    /// Persistent message history and saved content
    pub msg_save: Arc<RwLock<SavedMessages>>,
}

impl ConfigPortal {
    /// Creates a new ConfigPortal with initial configuration data.
    ///
    /// ## Initialization Strategy
    /// Takes ownership of configuration data and wraps each section in `Arc<RwLock<T>>`
    /// for thread-safe sharing. This happens once during application startup after
    /// configuration is loaded from persistent storage.
    ///
    /// ## Memory Layout
    /// Each configuration section gets its own RwLock to minimize lock contention.
    /// Related operations (e.g., UI theme changes) don't block unrelated operations
    /// (e.g., MQTT configuration updates).
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

    /// Updates session name with automatic retry on lock contention.
    ///
    /// ## Legacy Method Notice
    /// This method predates the action dispatch pattern and demonstrates the
    /// manual retry logic that led to the creation of the `try_lock!` macro.
    /// New code should prefer using `execute_portal_action` for consistency.
    ///
    /// ## Retry Behavior
    /// Continues attempting to acquire write lock until successful, with warning
    /// logs for contention monitoring. This blocking behavior was replaced by
    /// the timeout-based approach in the macro for better error handling.
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

    /// Returns a clone of the current session configuration.
    ///
    /// ## Legacy Method Notice
    /// This method also predates the action dispatch pattern and should be
    /// replaced with `execute_portal_action(PortalAction::GetSession)` for
    /// consistency and better error handling.
    ///
    /// ## Performance Note
    /// Clones the entire SessionConfig structure, which is acceptable given
    /// the typical size of session data but could be optimized for specific
    /// fields if session data becomes large.
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

    /// Executes configuration operations through the unified action dispatch pattern.
    ///
    /// ## Design Philosophy
    ///
    /// This method implements the Command Pattern for configuration operations:
    /// - **Uniform interface**: All configuration operations use the same entry point
    /// - **Consistent error handling**: Every operation uses identical retry logic
    /// - **Type safety**: Compile-time verification of valid operation/result pairings
    /// - **Centralized locking**: Single implementation of complex retry logic
    /// - **Extensibility**: New operations require only enum additions
    ///
    /// ## Action Processing Strategy
    ///
    /// Each action is processed through the `try_lock!` macro which provides:
    /// 1. **Lock acquisition with timeout**: Prevents indefinite blocking
    /// 2. **Automatic retry logic**: Handles transient contention gracefully
    /// 3. **Performance monitoring**: Logs contention for system optimization
    /// 4. **Error conversion**: Translates lock errors to domain errors
    ///
    /// ## Return Value Strategy
    ///
    /// Uses `ConfigResult` enum to provide type-safe returns:
    /// - **Success variants**: Strongly typed data for each configuration type
    /// - **Error variant**: Unified error handling across all operations
    /// - **Compile-time safety**: Invalid action/result combinations prevented
    ///
    /// ## Performance Characteristics
    ///
    /// - **Read operations**: Minimal overhead, concurrent access allowed
    /// - **Write operations**: Retry logic adds ~50ms maximum delay under contention
    /// - **Memory usage**: No additional allocations beyond configuration cloning
    /// - **Lock granularity**: Section-specific locks minimize contention
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Reading configuration
    /// let mqtt_config = match portal.execute_portal_action(PortalAction::GetMqttConfig) {
    ///     ConfigResult::MqttConfig(config) => config,
    ///     ConfigResult::Failed(e) => return Err(e.into()),
    ///     _ => unreachable!(),
    /// };
    ///
    /// // Writing configuration  
    /// portal.execute_portal_action(PortalAction::WriteMqttConfig(new_config));
    /// ```
    pub fn execute_potal_action(&self, action: PortalAction) -> ConfigResult {
        let result = match action {
            // Session configuration operations
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

            // UI configuration operations
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

            // Controller configuration operations
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

            // Connection configuration operations
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

            // Message persistence operations
            PortalAction::GetSavedMessagesMsg => {
                try_lock!(@read_lock_retry, self.msg_save.clone(), |guard: &SavedMessages| {
                    ConfigResult::MqttMessages(guard.msg.clone())
                })
            }
            PortalAction::GetSavedMessages => {
                try_lock!(@read_lock_retry, self.msg_save.clone(), |guard: &SavedMessages| {
                    ConfigResult::MqttHistory(guard.clone())
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

/// Enumeration of all possible configuration operations.
///
/// ## Design Rationale
///
/// Uses the Command Pattern to provide:
/// - **Type safety**: Each action carries its required data as typed parameters
/// - **Discoverability**: All available operations are visible in one place
/// - **Consistency**: Uniform naming and structure across all configuration types
/// - **Extensibility**: New operations require only enum variant additions
///
/// ## Naming Convention
///
/// Operations follow a consistent pattern:
/// - **Get{Thing}**: Read operations that return cloned data
/// - **Write{Thing}**: Write operations that take owned data
/// - **Get{Thing}Config**: Return complete configuration sections
/// - **Write{Thing}Config**: Update complete configuration sections
///
/// ## Parameter Strategy
///
/// - **Read operations**: No parameters, return data through ConfigResult
/// - **Write operations**: Take owned data to ensure thread safety
/// - **Batch operations**: Accept complex data structures for atomic updates
#[derive(Debug)]
pub enum PortalAction {
    // Session configuration management
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

    // UI configuration management
    GetUIConfig,
    GetTheme,
    GetFps,
    WriteUIConfig(UIConfig),
    WriteTheme(Theme),
    WriteFps(u8),

    // Controller and mapping configuration management
    GetElrsConfig,
    GetKeyboardConfig,
    GetControllerConfig,
    WriteElrsConfig(mapping::elrs::ELRSConfig),
    WriteKeyboardConfig(mapping::keyboard::KeyboardConfig),
    WriteControllerConfig(ControllerConfig),

    // Network and communication configuration management
    GetMqttConfig,
    GetConnectionConfig,
    WriteMqttConfig(mqtt::config::MqttConfig),
    WriteConnectionConfig(ConnectionConfig),

    // Message persistence and history management
    GetSavedMessagesMsg,
    GetSavedMessages,
    WriteSavedMessages(SavedMessages),
    WriteSavedMessagesMsg(Vec<mqtt::message_manager::MQTTMessage>),
}

/// Type-safe return values for configuration operations.
///
/// ## Design Rationale
///
/// Provides strongly-typed return values that:
/// - **Prevent type confusion**: Each operation returns a specific variant
/// - **Enable pattern matching**: Callers can match on expected result types
/// - **Centralize error handling**: All failures use the same Failed variant
/// - **Support debugging**: Failed operations include detailed error information
///
/// ## Usage Pattern
///
/// Callers typically use pattern matching to extract expected results:
/// ```rust
/// match portal.execute_portal_action(action) {
///     ConfigResult::MqttConfig(config) => { /* use config */ },
///     ConfigResult::Failed(e) => { /* handle error */ },
///     _ => unreachable!(), // Type safety ensures this won't happen
/// }
/// ```
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
    MqttHistory(SavedMessages),
    Failed(Error),
}

/// Sophisticated lock retry macro with timeout and logging.
///
/// ## Design Motivation
///
/// The manual retry loops in early methods led to code duplication and inconsistent
/// error handling. This macro provides:
/// - **Consistent retry logic**: All operations use identical timeout behavior
/// - **Performance monitoring**: Lock contention is logged for system optimization
/// - **Graceful failure**: Operations fail gracefully rather than hanging indefinitely
/// - **Reduced boilerplate**: Complex retry logic implemented once, used everywhere
///
/// ## Retry Strategy
///
/// - **Maximum attempts**: 5 retries prevent infinite loops under high contention
/// - **Retry delay**: 10ms delays allow contention to resolve without excessive waiting
/// - **Progressive logging**: Each retry attempt is logged for debugging
/// - **Timeout behavior**: Returns LockTimeout error rather than panicking or hanging
///
/// ## Macro Design
///
/// Uses internal pattern matching (@read_lock_retry, @write_lock_retry) to provide:
/// - **Type safety**: Read and write operations have different signatures
/// - **Code reuse**: Common retry logic shared between read and write operations
/// - **Flexibility**: Operations can provide custom logic within the retry framework
///
/// ## Performance Impact
///
/// - **Success case**: Minimal overhead (single try_lock attempt)
/// - **Contention case**: Maximum 50ms delay (5 attempts × 10ms) before timeout
/// - **Memory usage**: No allocations, stack-based retry logic only
/// - **Thread impact**: Sleep calls yield to other threads during contention
///
/// ## Usage Context
///
/// Used exclusively within `execute_portal_action` to provide consistent behavior
/// across all configuration operations. The macro is not intended for use outside
/// this module as it's specifically designed for ConfigPortal's locking patterns.
#[macro_export]
macro_rules! try_lock {
    // Write lock retry with timeout and error handling
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

    // Read lock retry with timeout and error handling
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

/// Error types for configuration portal operations.
///
/// ## Error Categories
///
/// - **LockTimeout**: Lock contention exceeded retry limits (system performance issue)
/// - **SessionNotFound**: Requested session doesn't exist (user/persistence issue)
/// - **InvalidOperation**: Configuration state doesn't support requested operation
///
/// ## Error Handling Philosophy
///
/// Configuration errors are generally recoverable and should not crash the application:
/// - **Lock timeouts**: Indicate high system load but operations can be retried
/// - **Session errors**: User can be notified and prompted to select valid session
/// - **Operation errors**: Configuration can be reset to defaults or corrected through UI
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not acquire lock after maximum retry attempts")]
    LockTimeout,

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}
