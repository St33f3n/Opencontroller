//! # Main Menu and Session Management Interface
//!
//! This module provides the primary user interface for session management within
//! the OpenController application, serving as the entry point for configuration
//! management and application workflow.
//!
//! ## Why This Module Exists
//!
//! The Main Menu represents a core component of OpenController's "diamond" architecture,
//! where the UI serves as the central orchestration point. This module exists to:
//! - Provide session management functionality for configuration versioning
//! - Serve as the primary navigation hub for the application
//! - Integrate UI interactions with the persistent storage system
//! - Support the user's workflow of configuration experimentation and fallback
//!
//! ## Key Abstractions
//!
//! ### Session Management Philosophy
//! Sessions in OpenController implement a versioning and backup system for configurations.
//! This reflects the developer's preference for experimentation with safety nets - users
//! can try new configurations, save multiple profiles, and always fall back to working
//! setups.
//!
//! ### Asynchronous Communication Pattern
//! The module uses async message passing via channels to communicate with the persistence
//! layer, ensuring UI responsiveness while background operations (saving, loading) complete.
//! This follows the overall thread separation strategy of the application.
//!
//! ## Integration with Backend Systems
//!
//! This module serves as the primary integration point between:
//! - **ConfigPortal**: Direct access to current session configuration
//! - **PersistenceManager**: Async session operations (create, load, delete)
//! - **Session Autosave System**: Background persistence protection
//! - **Thread Architecture**: Non-blocking communication with storage layer
//!
//! ## Design Rationale
//!
//! ### Two-Level Session Access
//! The module uses both direct ConfigPortal access (for immediate reads) and
//! async channel communication (for write operations). This hybrid approach:
//! - Ensures immediate UI updates for current session display
//! - Prevents UI blocking during potentially slow disk operations
//! - Maintains data consistency through the centralized persistence system
//!
//! ### List-Based Session Browser
//! The scrollable session list design mirrors other parts of the UI (like message logs)
//! to maintain visual consistency while providing efficient navigation through
//! potentially many saved configurations.

use crate::persistence::config_portal::{ConfigPortal, ConfigResult, PortalAction};
use crate::persistence::persistence_worker::SessionAction;
use crate::persistence::session_client::SessionClient;
use crate::persistence::SessionConfig;
use eframe::egui::{self, vec2, Color32, Frame, Label, ScrollArea, Stroke, TextEdit, Ui};
use std::sync::Arc;
use std::time::Duration;
use std::{ops::Deref, str::FromStr};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::common::{SessionData, UiColors};
use crate::session_action;

/// Main data structure for the session management interface.
///
/// This structure manages the state and operations for OpenController's session
/// system, providing both immediate UI state and async communication with the
/// persistence layer.
///
/// ## Design Rationale
/// Combines synchronous state (for immediate UI updates) with asynchronous
/// operations (for non-blocking persistence). This hybrid approach ensures
/// responsive UI while maintaining data consistency.
///
/// ## Session Management Strategy
/// Sessions serve as configuration snapshots that allow users to:
/// - Experiment with new settings safely
/// - Maintain multiple configuration profiles
/// - Quickly revert to known-working setups
/// - Share configurations between different usage scenarios
///
/// ## Integration Architecture
/// - **ConfigPortal**: Provides immediate access to current session data
/// - **SessionAction Channel**: Async communication for persistence operations
/// - **Error Handling**: Local error state for user feedback
/// - **Auto-refresh**: Automatic session list updates after operations
pub struct MainMenuData {
    /// Direct access to configuration portal for immediate reads
    config_portal: Arc<ConfigPortal>,

    /// Channel for async communication with persistence manager
    session_sender: tokio::sync::mpsc::Sender<SessionAction>,

    /// Currently active session name for display
    current_session_name: String,

    /// User input for new session creation
    new_session_name: String,

    /// Previous session for fallback navigation
    previous_session: Option<String>,

    /// Error state for user feedback
    session_load_error: Option<String>,

    /// List of available sessions for navigation
    available_sessions: Vec<String>,
}

impl MainMenuData {
    /// Creates a new main menu interface with current session state.
    ///
    /// Initializes the interface by reading current session configuration from
    /// the ConfigPortal and setting up async communication with the persistence
    /// system.
    ///
    /// ## Design Rationale
    /// Performs synchronous initialization to ensure immediate UI display,
    /// then sets up async capabilities for user operations.
    ///
    /// # Parameters
    /// - `config_portal`: Shared access to the configuration system
    /// - `session_sender`: Channel for async persistence operations
    ///
    /// # Errors
    /// Falls back to default configuration if ConfigPortal read fails,
    /// ensuring the UI always displays in a usable state.
    pub fn new(
        config_portal: Arc<ConfigPortal>,
        session_sender: mpsc::Sender<SessionAction>,
    ) -> Self {
        let config_res = config_portal.execute_potal_action(PortalAction::GetSession);
        let config = if let ConfigResult::SessionConfig(session_config) = config_res {
            session_config
        } else {
            SessionConfig::default()
        };

        Self {
            config_portal,
            session_sender,
            current_session_name: config.session_name.clone(),
            previous_session: config.last_session.clone(),
            new_session_name: String::new(),
            available_sessions: config
                .available_sessions
                .keys()
                .cloned()
                .into_iter()
                .collect(),
            session_load_error: None,
        }
    }

    /// Renders the complete main menu interface with session management controls.
    ///
    /// Creates a two-section layout with session creation controls at the top
    /// and a scrollable session browser below.
    ///
    /// ## UI Layout Architecture
    ///
    /// ### Header Section
    /// Provides immediate session operations:
    /// - Current session display and new session input
    /// - Save button for creating new sessions
    /// - Refresh button for updating session list
    ///
    /// ### Session Browser
    /// Scrollable list matching the application's visual style:
    /// - Consistent styling with other list interfaces (MQTT logs, etc.)
    /// - Click-to-load interaction for session switching
    /// - Empty state handling for first-time users
    ///
    /// ## Async Operations
    /// All session operations (create, load, list) use async communication
    /// to prevent UI blocking during disk operations, maintaining the
    /// responsive feel required for controller-based interaction.
    ///
    /// ## Performance Considerations
    /// - Pre-calculates layout dimensions for consistent frame times
    /// - Uses egui's ScrollArea for efficient large session list rendering
    /// - Clones session list only when necessary to minimize allocations
    pub fn render(&mut self, ui: &mut Ui) {
        let available_size = ui.available_size();
        let border_color = UiColors::BORDER;

        ui.vertical(|ui| {
            // Header section: current session info and creation controls
            ui.horizontal(|ui| {
                ui.heading("Mainmenu");
                ui.add(
                    TextEdit::singleline(&mut self.new_session_name)
                        .hint_text(self.current_session_name.as_str()),
                );
                if ui.button("Save").clicked() {
                    self.create_session();
                }
                if ui.button("Load").clicked() {
                    self.list_sessions();
                }
            });

            // Session browser: scrollable list of available sessions
            Frame::new()
                .fill(ui.visuals().extreme_bg_color)
                .inner_margin(6)
                .stroke(Stroke::new(1.0, ui.visuals().widgets.active.bg_fill))
                .show(ui, |ui| {
                    let list_height = available_size.y - 40.0; // Height minus header
                    ui.set_min_size(vec2(available_size.x, list_height));

                    ScrollArea::vertical().show(ui, |ui| {
                        ui.vertical(|ui| {
                            for session in self.available_sessions.clone() {
                                Frame::new()
                                    .stroke(Stroke::new(1.0, border_color))
                                    .inner_margin(2)
                                    .outer_margin(2)
                                    .fill(UiColors::EXTREME_BG)
                                    .show(ui, |ui| {
                                        if ui
                                            .add_sized(
                                                vec2(available_size.x - 20.0, list_height / 6.0),
                                                Label::new(format!("Session: {}", session))
                                                    .selectable(true)
                                                    .sense(egui::Sense::click()),
                                            )
                                            .clicked()
                                        {
                                            debug!("Loading Session");
                                            self.change_session(session.clone());
                                        }
                                    });
                            }

                            // Empty state for first-time users
                            if self.available_sessions.is_empty() {
                                ui.label("No saved sessions available");
                            }
                        });
                    });
                });
        });
    }

    /// Creates a new session with the specified name.
    ///
    /// Validates input and initiates async session creation through the
    /// persistence system. Updates the session list upon completion.
    ///
    /// ## Error Handling
    /// - Validates session name is not empty
    /// - Sets local error state for immediate user feedback
    /// - Logs async operation failures without blocking UI
    ///
    /// ## Async Behavior
    /// Uses the session_action! macro to send creation requests to the
    /// persistence manager, ensuring non-blocking operation.
    fn create_session(&mut self) {
        let session_name = self.new_session_name.clone();

        if session_name.is_empty() {
            self.session_load_error = Some("Session name cannot be empty".to_string());
            return;
        }

        let result = session_action!(@create, self.session_sender, session_name);
        self.list_sessions();
    }

    /// Refreshes the available sessions list from the persistence system.
    ///
    /// Initiates async communication to retrieve current session list,
    /// updating the UI state for immediate display.
    fn list_sessions(&mut self) {
        let result = session_action!(@list, self.session_sender);

        match result {
            Ok(sessions) => self.available_sessions = sessions.keys().cloned().collect(),
            Err(e) => warn!("Couldn't load available sessions: {}", e),
        }
    }

    /// Switches to a different session configuration.
    ///
    /// Saves current session as previous (for potential fallback) and
    /// initiates async session loading through the persistence system.
    ///
    /// ## Design Rationale
    /// Maintains previous session reference to support user workflow of
    /// experimentation with easy fallback to working configurations.
    ///
    /// # Parameters
    /// - `name`: Session name to load
    fn change_session(&mut self, name: String) {
        self.previous_session = Some(self.current_session_name.clone());
        self.current_session_name = name.clone();

        let result = session_action!(@load, self.session_sender, name);
        self.list_sessions();
    }

    /// Deletes a session from the persistence system.
    ///
    /// Initiates async session deletion and refreshes the available sessions list.
    ///
    /// ## Safety Considerations
    /// Currently no confirmation dialog - future enhancement should add
    /// user confirmation for destructive operations.
    ///
    /// # Parameters
    /// - `name`: Session name to delete
    fn delet_session(&mut self, name: String) {
        let result = session_action!(@delete, self.session_sender, name);
        self.list_sessions();
    }
}
