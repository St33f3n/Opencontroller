//! # Persistence Worker Module
//!
//! ## Why This Module Exists
//! The PersistenceManager provides a thread-safe, asynchronous interface for session management
//! operations. It implements a worker pattern that isolates all session I/O operations in a
//! dedicated task, preventing blocking of the main application threads and ensuring consistency
//! across concurrent session operations.
//!
//! ## Key Abstractions
//! - **Worker Pattern**: Session operations are processed sequentially in a dedicated task
//! - **Request-Response**: Uses oneshot channels for synchronous-style operations over async boundaries
//! - **Command Pattern**: All operations are modeled as SessionAction commands
//! - **Macro API**: Provides ergonomic macros for common session operations
//!
//! ## Error Handling Strategy
//! Uses `color_eyre` for rich error context throughout the session operation chain.
//! Errors from the worker are propagated back through oneshot channels, maintaining
//! error context across thread boundaries.
//!
//! ## Async Patterns Used
//! - **Actor Model**: Worker task processes commands sequentially from a message queue
//! - **Request-Response**: Oneshot channels provide synchronous semantics over async operations
//! - **Background Tasks**: Autosave runs independently without blocking other operations

use crate::mqtt::mqtt_handler::Configured;

use super::{
    config_portal::{ConfigPortal, ConfigResult, PortalAction},
    session_client::SessionClient,
};
use color_eyre::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Convenience macro for handling session action responses.
///
/// Reduces boilerplate in the worker loop by standardizing error handling
/// for failed response channel sends.
macro_rules! handle_action {
    ($action:expr, $response_tx:expr) => {
        if let Err(e) = $response_tx.send($action.await) {
            error!("Failed to send response: {:?}", e);
        }
    };
}

/// Manages session persistence operations through a dedicated worker task.
///
/// ## Design Rationale
/// The PersistenceManager implements a worker pattern to solve several problems:
/// - **Thread Safety**: All session operations are serialized through a single worker
/// - **Non-blocking**: The main application threads never block on disk I/O
/// - **Consistency**: Sequential processing prevents race conditions in session operations
/// - **Resource Management**: Centralizes file handles and cleanup operations
///
/// ## Usage Context
/// Created once at application startup and used throughout the application lifecycle.
/// Other modules obtain a sender to submit session operations and receive responses
/// through oneshot channels.
///
/// ## Architecture Notes
/// Uses the Actor pattern where the worker task is the actor processing SessionAction messages.
/// The autosave task runs independently to provide automatic backup functionality.
pub struct PersistenceManager {
    /// Channel sender for submitting session operations to the worker
    tx: Sender<SessionAction>,
    /// Handle to the main worker task for cleanup on shutdown
    worker_handle: tokio::task::JoinHandle<()>,
    /// Handle to the autosave task for independent cleanup
    autosave_handle: tokio::task::JoinHandle<()>,
    /// Shared access to the current session client for direct portal access
    session_client: Arc<Mutex<SessionClient>>,
}

impl PersistenceManager {
    /// Creates and initializes a new PersistenceManager with worker and autosave tasks.
    ///
    /// This is the primary initialization method that sets up the complete persistence
    /// subsystem including worker task spawning and autosave configuration.
    ///
    /// ## Design Rationale
    /// Spawns two independent tasks:
    /// - **Worker Task**: Processes session operations sequentially to prevent race conditions
    /// - **Autosave Task**: Provides automatic backup every 60 seconds for crash recovery
    ///
    /// The worker pattern ensures that all session operations are atomic and consistent,
    /// while the autosave provides a safety net against data loss.
    ///
    /// ## Error Handling
    /// Initialization is designed to always succeed - if the last session cannot be loaded,
    /// the system falls back to a default configuration. This ensures the application
    /// can always start even with corrupted session data.
    ///
    /// ## Async Behavior
    ///
    /// This function is async because it:
    /// - Loads the last used session from disk during initialization
    /// - Spawns background tasks that require async context
    ///
    /// **Cancellation**: Safe to cancel before completion, but may leave partial initialization
    /// **Concurrency**: Should only be called once during application startup
    ///
    /// ## Performance Notes
    /// The channel buffer size (32) is chosen to handle burst operations like rapid
    /// session switching without blocking the sender. The autosave interval (60s)
    /// balances crash recovery with disk I/O overhead.
    pub async fn new() -> Self {
        let session_client = Arc::new(Mutex::new(SessionClient::load_last_session().await));
        let session_cpy = session_client.clone();
        let (tx, mut rx) = channel::<SessionAction>(32);

        let handle = tokio::spawn(async move {
            while let Some(action) = rx.recv().await {
                match action {
                    SessionAction::CreateSession { name, response_tx } => {
                        handle_action!(session_client.lock().await.save_session(name), response_tx);
                    }
                    SessionAction::LoadSession { name, response_tx } => {
                        handle_action!(
                            session_client.lock().await.change_session(&name),
                            response_tx
                        );
                    }
                    SessionAction::SaveCurrentSession { response_tx } => {
                        handle_action!(
                            session_client.lock().await.save_current_session(),
                            response_tx
                        );
                    }
                    SessionAction::DeleteSession { name, response_tx } => {
                        handle_action!(
                            session_client.lock().await.delete_session(&name),
                            response_tx
                        );
                    }
                    SessionAction::ListSessions { response_tx } => {
                        handle_action!(SessionClient::scan_available_sessions(), response_tx);
                    }
                }
            }
        });

        let autosave = SessionClient::start_autosave_task(session_cpy.clone(), 60).await;

        Self {
            tx,
            autosave_handle: autosave,
            worker_handle: handle,
            session_client: session_cpy.clone(),
        }
    }

    /// Returns a sender channel for submitting session operations to the worker.
    ///
    /// Used by other modules to perform session operations asynchronously.
    /// Multiple senders can be created to allow concurrent submission from
    /// different parts of the application.
    pub fn get_sender(&self) -> Sender<SessionAction> {
        self.tx.clone()
    }

    /// Provides direct access to the configuration portal for runtime configuration access.
    ///
    /// Used when modules need to read or write configuration data without
    /// performing session persistence operations.
    ///
    /// # Errors
    ///
    /// This function uses internal locking and should not fail under normal conditions.
    /// Potential issues could arise from deadlocks if the session client is held
    /// for extended periods elsewhere.
    pub async fn get_cfg_portal(&self) -> Arc<ConfigPortal> {
        self.session_client.lock().await.get_portal_ref()
    }
}

/// Represents the various session operations that can be performed by the worker.
///
/// ## Design Rationale
/// Uses the Command pattern to encapsulate session operations as data.
/// Each variant includes a oneshot sender for returning results, enabling
/// synchronous-style programming over async boundaries.
///
/// ## Usage Context
/// These actions are created by client code (often through macros) and sent
/// to the persistence worker for processing.
#[derive(Debug)]
pub enum SessionAction {
    /// Creates a new session by saving current configuration with a new name
    CreateSession {
        name: String,
        response_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
    /// Switches to an existing session, loading its configuration
    LoadSession {
        name: String,
        response_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
    /// Saves the current session state to persistent storage
    SaveCurrentSession {
        response_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
    /// Removes a session from persistent storage
    DeleteSession {
        name: String,
        response_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
    /// Lists all available sessions for UI display
    ListSessions {
        response_tx: tokio::sync::oneshot::Sender<Result<HashMap<String, PathBuf>>>,
    },
}

/// Provides ergonomic macros for common session operations with built-in error handling.
///
/// ## Design Rationale
/// These macros solve the ergonomics problem of the request-response pattern over
/// async boundaries. Without macros, each operation would require:
/// 1. Creating a oneshot channel
/// 2. Constructing the appropriate SessionAction
/// 3. Sending the action
/// 4. Receiving and handling the response
///
/// The macros reduce this to a single call with automatic error propagation.
///
/// ## Error Handling Strategy
/// Uses a timeout-free approach with try_recv() and a small sleep. This prevents
/// indefinite blocking while still allowing the worker time to process operations.
/// In high-load scenarios, the caller may need to retry if the response isn't ready.
///
/// ## Performance Notes
/// The 10ms sleep is a compromise between responsiveness and CPU usage. Most session
/// operations complete within this timeframe, making the pattern feel synchronous
/// to the caller.
///
/// # Usage Examples
///
/// ```rust
/// // Create a new session
/// session_action!(@create, session_sender, "my_new_session")?;
///
/// // Load an existing session
/// session_action!(@load, session_sender, "production_config")?;
///
/// // Save current session
/// session_action!(@save, session_sender)?;
///
/// // Delete a session
/// session_action!(@delete, session_sender, "old_session")?;
///
/// // List all sessions
/// let sessions = session_action!(@list, session_sender)?;
/// ```
#[macro_export]
macro_rules! session_action {
    (@create, $session_sender:expr, $session_name:expr) => {{
        let (response_tx, mut response_rx) =
            tokio::sync::oneshot::channel::<color_eyre::Result<()>>();

        let action = $crate::persistence::persistence_worker::SessionAction::CreateSession {
            name: $session_name.to_string(),
            response_tx
        };

        session_action!(@send_and_receive, $session_sender, action, response_rx)
    }};

    (@load, $session_sender:expr, $session_name:expr) => {{
        let (response_tx, mut response_rx) =
            tokio::sync::oneshot::channel::<color_eyre::Result<()>>();

        let action = $crate::persistence::persistence_worker::SessionAction::LoadSession {
            name: $session_name.to_string(),
            response_tx
        };

        session_action!(@send_and_receive, $session_sender, action, response_rx)
    }};

    (@save, $session_sender:expr) => {{
        let (response_tx, mut response_rx) =
            tokio::sync::oneshot::channel::<color_eyre::Result<()>>();

        let action = $crate::persistence::persistence_worker::SessionAction::SaveCurrentSession { response_tx };

        session_action!(@send_and_receive, $session_sender, action, response_rx)
    }};

    (@delete, $session_sender:expr, $session_name:expr) => {{
        let (response_tx, mut response_rx) =
            tokio::sync::oneshot::channel::<color_eyre::Result<()>>();

        let action = $crate::persistence::persistence_worker::SessionAction::DeleteSession {
            name: $session_name.to_string(),
            response_tx
        };

        session_action!(@send_and_receive, $session_sender, action, response_rx)
    }};

    (@list, $session_sender:expr) => {{
        let (response_tx, mut response_rx) =
            tokio::sync::oneshot::channel::<color_eyre::Result<std::collections::HashMap<String, std::path::PathBuf>>>();

        let action = $crate::persistence::persistence_worker::SessionAction::ListSessions { response_tx };

        session_action!(@send_and_receive, $session_sender, action, response_rx)
    }};

    (@send_and_receive, $session_sender:expr, $action:expr, $response_rx:expr) => {{
        if let Err(e) = $session_sender.try_send($action) {
            Err(color_eyre::Report::msg(format!("Failed to send action: {}", e)))
        } else {
            // Brief wait to allow worker time to process the request
            std::thread::sleep(std::time::Duration::from_millis(10));
            match $response_rx.try_recv() {
                Ok(result) => result,
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    Err(color_eyre::Report::msg("Response not ready yet"))
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    Err(color_eyre::Report::msg("Response channel closed"))
                }
            }
        }
    }};
}
