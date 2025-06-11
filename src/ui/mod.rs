//! # OpenController User Interface Module
//!
//! This module implements the complete user interface system for OpenController,
//! serving as the central orchestration point in the application's "diamond" architecture.
//! It provides a unified interface for Smart Home debugging, RC vehicle control, and
//! system configuration through gamepad-based navigation.
//!
//! ## Why This Module Exists
//!
//! The UI module represents the core of OpenController's vision: providing an intuitive,
//! gamepad-controlled interface for maker and Smart Home applications. This module exists to:
//!
//! - **Unify Control Paradigms**: Enable gamepad control for traditionally keyboard/mouse-based tools
//! - **Centralize System Access**: Provide single-point access to MQTT debugging, RC control, and system settings
//! - **Enable Workshop Workflows**: Support rapid switching between different debugging and control tasks
//! - **Simplify Embedded Deployment**: Offer full functionality without requiring external input devices
//!
//! ## Architectural Role in OpenController
//!
//! ### Diamond Architecture Implementation
//! This module implements the "diamond" pattern central to OpenController's design:
//! `UI(Central Hub) → Backend Modules → Persistence/Configuration`
//!
//! The UI serves as both the user interaction layer and the coordination point for:
//! - Controller input processing and event distribution
//! - Backend system configuration and status monitoring
//! - Session management and persistent state coordination
//! - Real-time data flow between different subsystems
//!
//! ### Integration Strategy
//! Rather than directly implementing business logic, this module orchestrates
//! communication between specialized backend systems through well-defined channels
//! and configuration interfaces. This maintains clear separation of concerns while
//! providing seamless user experience.
//!
//! ## Key Design Decisions
//!
//! ### Immediate Mode UI Choice (egui)
//! Selected egui after comprehensive evaluation because:
//! - **Rust Native**: Avoids language mixing and integrates seamlessly with tokio
//! - **Raspberry Pi Compatible**: Renders correctly on embedded hardware
//! - **Controller Friendly**: Immediate mode simplifies event integration
//! - **Minimal Dependencies**: Reduces complexity for embedded deployments
//!
//! The immediate mode pattern aligns perfectly with the controller-based input model,
//! where the entire UI state is reconstructed each frame based on current backend state.
//!
//! ### Three-Panel Layout Architecture
//! The interface uses a consistent three-panel layout across all screens:
//! - **Top Panel**: Navigation buttons for primary application areas
//! - **Central Panel**: Context-specific content based on current menu
//! - **Bottom Panel**: System status information (network, battery, etc.)
//!
//! This layout provides:
//! - Consistent navigation patterns for gamepad users
//! - Always-visible status information for system monitoring
//! - Optimal screen real estate usage across different display sizes
//!
//! ### Controller Event Integration Strategy
//! Uses `raw_input_hook` to inject controller events directly into egui's event stream,
//! enabling seamless gamepad interaction with standard UI controls. This approach:
//! - Maintains compatibility with existing egui widgets
//! - Provides consistent interaction patterns across different input types
//! - Enables future extension to additional input methods
//! - Supports both navigation and text input through controllers
//!
//! ## Backend Communication Architecture
//!
//! ### Channel-Based Integration
//! The module integrates with backend systems through carefully designed channel patterns:
//! - **MQTT Communication**: Bidirectional message flow for debugging workflows
//! - **Session Management**: Async session operations without UI blocking
//! - **Controller Events**: Real-time input processing and command generation
//! - **Configuration Updates**: Immediate persistence of user changes
//!
//! ### Configuration Synchronization
//! Uses a hybrid approach for configuration management:
//! - **Immediate Reads**: Direct ConfigPortal access for current state display
//! - **Async Writes**: Channel-based persistence for non-blocking operations
//! - **Status Integration**: Real-time backend status reflected in UI state
//!
//! ## Performance Considerations
//!
//! ### Frame Rate Management
//! Requests 30fps refresh rate (`Duration::from_millis(33)`) to balance:
//! - Responsive controller input handling
//! - Smooth visual feedback for user interactions
//! - Reasonable resource usage on embedded hardware
//! - Compatibility with typical display refresh rates
//!
//! ### Memory Efficiency
//! Uses pre-calculated layout dimensions and efficient widget patterns to:
//! - Minimize per-frame allocations
//! - Leverage egui's built-in caching systems
//! - Maintain consistent performance during high-activity periods
//! - Support extended operation on resource-constrained hardware

pub mod common;
pub mod elrs_menu;
pub mod main_menu;
pub mod mqtt_menu;
pub mod settings_menu;

use eframe::egui::{self, Button, Color32, Context, Event, Layout, Vec2};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::mqtt::config::MqttConfig;
use crate::mqtt::message_manager::MQTTMessage;
use crate::persistence::config_portal::{ConfigPortal, ConfigResult};
use crate::persistence::persistence_worker::SessionAction;
use crate::persistence::session_client::SessionClient;

use self::common::MenuState;
use self::elrs_menu::ELRSMenuData;
use self::main_menu::MainMenuData;
use self::mqtt_menu::MQTTMenuData;
use self::settings_menu::SettingsMenuData;

pub use common::MQTTServer;

/// Central UI orchestration component implementing OpenController's "diamond" architecture.
///
/// This structure serves as the primary coordination point between user interaction,
/// backend systems, and persistent configuration. It manages the complete user interface
/// lifecycle from initialization through ongoing operation.
///
/// ## Architectural Responsibilities
///
/// ### User Interface Coordination
/// - **Menu Navigation**: State machine for primary application screens
/// - **Layout Management**: Consistent three-panel layout across all contexts
/// - **Event Distribution**: Controller input routing to appropriate handlers
/// - **Status Integration**: Real-time backend status display
///
/// ### Backend System Integration
/// - **Configuration Management**: Coordination with ConfigPortal for settings persistence
/// - **Session Coordination**: Integration with session management for state preservation
/// - **Real-time Communication**: Channel-based integration with MQTT, controller, and other subsystems
/// - **Status Monitoring**: Battery, network, and system status aggregation
///
/// ## Design Rationale
///
/// ### Component Organization
/// Each menu area (Main, MQTT, ELRS, Settings) is managed by a dedicated data structure,
/// providing clear separation of concerns while maintaining centralized coordination.
/// This enables independent development and testing of different UI areas.
///
/// ### Channel Integration Strategy
/// Uses distinct channels for different types of backend communication:
/// - High-frequency data (controller events, MQTT messages)
/// - Configuration operations (session management, settings)
/// - Status updates (connection state, system monitoring)
///
/// This separation ensures appropriate handling for different data types and frequencies.
///
/// ### State Management Philosophy
/// Maintains minimal UI-specific state while relying on backend systems for authoritative
/// data. This ensures UI consistency and enables easy recovery from temporary issues.
pub struct OpencontrollerUI {
    /// Current active menu screen for navigation state machine
    menu_state: MenuState,

    /// Receiver for processed controller events from mapping system
    event_receiver: mpsc::Receiver<Vec<egui::Event>>,

    /// Session management and configuration interface
    main_menu_data: MainMenuData,

    /// ELRS RC vehicle control interface (currently mock implementation)
    elrs_menu_data: ELRSMenuData,

    /// MQTT debugging and message management interface
    mqtt_menu_data: MQTTMenuData,

    /// System settings and configuration interface
    settings_menu_data: SettingsMenuData,

    /// Controller battery level for status display
    bat_controller: usize,

    /// PC/System battery level for status display
    bat_pc: usize,

    /// Direct access to configuration portal for immediate reads
    config_portal: Arc<ConfigPortal>,

    /// Channel for session management operations
    session_sender: mpsc::Sender<SessionAction>,
}

impl OpencontrollerUI {
    /// Creates a new OpenController UI with backend system integration.
    ///
    /// Initializes the complete UI system by setting up menu components, establishing
    /// backend communication channels, and configuring the egui rendering context
    /// for optimal gamepad interaction.
    ///
    /// ## Initialization Strategy
    ///
    /// ### Theme Configuration
    /// Sets dark theme immediately to provide optimal visibility in workshop
    /// environments and reduce eye strain during extended debugging sessions.
    ///
    /// ### Component Architecture
    /// Each menu component is initialized with appropriate backend integration:
    /// - **Main Menu**: Direct ConfigPortal and session management integration
    /// - **MQTT Menu**: Full bidirectional MQTT communication setup
    /// - **ELRS Menu**: Mock data for development (pending backend implementation)
    /// - **Settings Menu**: Mock data with planned system integration
    ///
    /// ### Channel Distribution
    /// Distributes communication channels to appropriate components while maintaining
    /// centralized coordination through the main UI structure.
    ///
    /// # Parameters
    /// - `cc`: eframe creation context for egui initialization
    /// - `event_receiver`: Channel for receiving processed controller events
    /// - `received_msg`: Channel for incoming MQTT messages
    /// - `msg_sender`: Channel for outgoing MQTT messages  
    /// - `config_portal`: Shared access to configuration system
    /// - `session_sender`: Channel for session management operations
    ///
    /// # Design Rationale
    /// Takes all necessary communication channels at initialization to ensure
    /// proper backend integration from startup, avoiding complex runtime
    /// channel management or connection establishment.
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        event_receiver: mpsc::Receiver<Vec<egui::Event>>,
        received_msg: mpsc::Receiver<MQTTMessage>,
        msg_sender: mpsc::Sender<MQTTMessage>,
        config_portal: Arc<ConfigPortal>,
        session_sender: mpsc::Sender<SessionAction>,
    ) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        OpencontrollerUI {
            menu_state: MenuState::Main,
            event_receiver,
            main_menu_data: MainMenuData::new(config_portal.clone(), session_sender.clone()),
            elrs_menu_data: ELRSMenuData::mock_data(),
            mqtt_menu_data: MQTTMenuData::new(
                received_msg,
                msg_sender,
                config_portal.clone(),
                session_sender.clone(),
            ),
            config_portal: config_portal.clone(),
            session_sender: session_sender.clone(),
            settings_menu_data: SettingsMenuData::mock_data(),
            bat_controller: 0,
            bat_pc: 0,
        }
    }

    /// Logs controller events for debugging and development purposes.
    ///
    /// Provides detailed logging of controller event processing to support
    /// debugging of input mapping and event flow through the system.
    ///
    /// ## Usage Context
    /// Currently disabled in normal operation but available for debugging
    /// controller integration issues or developing new input mappings.
    fn log_controller_state(&mut self) {
        let controller_events = self.event_receiver.try_recv();

        if let Ok(events) = controller_events {
            for element in events {
                info!(
                    "This event got succesfully transfered into UI:\n{:?}",
                    element
                );
            }
        }
    }
}

impl eframe::App for OpencontrollerUI {
    /// Integrates controller events into egui's input processing pipeline.
    ///
    /// This hook runs before egui processes each frame, allowing injection of
    /// controller-generated events into the standard egui event stream. This
    /// approach enables seamless gamepad interaction with all standard egui widgets.
    ///
    /// ## Integration Strategy
    ///
    /// Controller events are processed by the mapping system and converted to
    /// standard egui events (keyboard, mouse, text input) before injection.
    /// This maintains compatibility with existing UI code while enabling
    /// gamepad-based interaction.
    ///
    /// ## Performance Considerations
    /// Uses non-blocking channel operations to avoid frame delays if no
    /// controller events are available. Event processing occurs within
    /// the existing egui frame processing, maintaining consistent timing.
    ///
    /// # Parameters
    /// - `_ctx`: egui context (unused in current implementation)
    /// - `raw_input`: Mutable reference to egui's input state for event injection
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Ok(events) = self.event_receiver.try_recv() {
            for event in events {
                raw_input.events.push(event);
            }
        }
    }

    /// Main UI update loop implementing the three-panel layout and menu coordination.
    ///
    /// Executes every frame to update the UI state, process user interactions,
    /// and coordinate with backend systems. Implements the core layout strategy
    /// and navigation logic for the OpenController interface.
    ///
    /// ## Frame Processing Strategy
    ///
    /// ### Refresh Rate Management
    /// Requests 30fps refresh (`33ms`) to balance responsiveness with resource usage.
    /// This rate provides smooth interaction feedback while supporting extended
    /// operation on embedded hardware.
    ///
    /// ### Layout Architecture
    /// Implements consistent three-panel layout:
    /// - **Top Panel**: Navigation buttons with calculated sizing for gamepad use
    /// - **Central Panel**: Dynamic content based on current menu state
    /// - **Bottom Panel**: System status with real-time backend information
    ///
    /// ### Navigation State Machine
    /// Uses simple direct state transitions between menu screens, as OpenController's
    /// navigation model allows unrestricted movement between any application areas.
    ///
    /// ## Performance Optimizations
    ///
    /// ### Button Sizing Strategy
    /// Pre-calculates button dimensions based on available width to:
    /// - Ensure consistent appearance across different display sizes
    /// - Provide touch-friendly interaction targets
    /// - Optimize for gamepad navigation patterns
    /// - Minimize per-frame layout calculations
    ///
    /// ### Status Integration
    /// Directly accesses component state for status display rather than
    /// using additional channels, reducing complexity while maintaining
    /// real-time status accuracy.
    ///
    /// # Parameters
    /// - `ctx`: egui context for UI rendering and event processing
    /// - `frame`: eframe application frame (unused in current implementation)
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Uncomment for controller event debugging
        // self.log_controller_state();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.ctx().request_repaint_after(Duration::from_millis(33));
            let width = ui.available_width() - 60.0;

            // Top navigation panel with application area buttons
            egui::TopBottomPanel::top("top_panel")
                .show_separator_line(false)
                .show_inside(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        let main_button = Button::new("MainMenu").min_size(Vec2 {
                            x: width / 4.0,
                            y: 20.0,
                        });
                        let mqtt_button = Button::new("MQTT").min_size(Vec2 {
                            x: width / 4.0,
                            y: 20.0,
                        });
                        let elrs_button = Button::new("ELRS").min_size(Vec2 {
                            x: width / 4.0,
                            y: 20.0,
                        });
                        let settings_button = Button::new("Settings").min_size(Vec2 {
                            x: width / 4.0,
                            y: 20.0,
                        });

                        if ui.add(main_button).clicked() {
                            self.menu_state = MenuState::Main;
                        };
                        if ui.add(mqtt_button).clicked() {
                            self.menu_state = MenuState::MQTT;
                        };
                        if ui.add(elrs_button).clicked() {
                            self.menu_state = MenuState::ELRS;
                        };
                        if ui.add(settings_button).clicked() {
                            self.menu_state = MenuState::Settings;
                        };
                    });
                });

            // Central content panel with menu-specific content
            egui::CentralPanel::default().show_inside(ui, |ui| match self.menu_state {
                MenuState::Main => self.main_menu_data.render(ui),
                MenuState::MQTT => self.mqtt_menu_data.render(ui),
                MenuState::ELRS => self.elrs_menu_data.render(ui),
                MenuState::Settings => self.settings_menu_data.render(ui),
            });

            // Bottom status panel with system information
            egui::TopBottomPanel::bottom("bottom_panel")
                .show_separator_line(false)
                .show_inside(ui, |ui| {
                    let connection_status = if self.settings_menu_data.is_connected() {
                        "🟢"
                    } else {
                        "🔴"
                    };
                    ui.horizontal_centered(|ui| {
                        ui.label(format!(
                            "{} {}",
                            self.settings_menu_data.get_network_name(),
                            connection_status
                        ));
                        ui.label(format!("CBat: {}%", self.bat_controller));
                        ui.label(format!("PCBat: {}%", self.bat_pc));
                    });
                });
        });
    }
}
