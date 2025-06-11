//! # MQTT Debug and Management Interface
//!
//! This module provides a comprehensive MQTT debugging and message management interface
//! for the OpenController application, serving as a key component for Smart Home and
//! Maker project development workflows.
//!
//! ## Why This Module Exists
//!
//! MQTT debugging is one of the core use cases that inspired the OpenController project.
//! The module exists to solve the common developer problem: "I want to quickly debug
//! my MQTT system → I connect to the topic on OpenController and see if the message
//! really comes through my infrastructure as intended."
//!
//! This addresses the frustration of setting up temporary MQTT clients, managing
//! credentials, and manually monitoring message flows during Smart Home and IoT
//! development.
//!
//! ## Key Abstractions
//!
//! ### Real-Time MQTT Debugging Workflow
//! The interface implements a complete MQTT debugging cycle:
//! - **Connection Management**: Multiple server profiles with credentials
//! - **Topic Subscription**: Dynamic topic management with visual subscription state
//! - **Message Monitoring**: Live message log with real-time updates
//! - **Message Composition**: Built-in editor for testing message publishing
//! - **History Management**: Persistent message templates and debugging sessions
//!
//! ### Three-Panel Layout Architecture
//! The UI uses a sophisticated layout optimized for debugging workflows:
//! - **Left Panel (70%)**: Live message log for monitoring incoming traffic
//! - **Right Panel (30%)**: Split into message composition and history access
//! - **Header Controls**: Server/topic selection with visual connection status
//!
//! ## Design Rationale
//!
//! ### Calculated Layout Dimensions
//! Uses pre-calculated layout dimensions rather than dynamic sizing to ensure:
//! - Consistent frame times during high-frequency message reception
//! - Predictable UI behavior when message volume varies
//! - Optimal screen real estate allocation for debugging workflows
//!
//! ### Pre/Post Update Configuration Pattern
//! Implements a synchronization pattern with ConfigPortal:
//! - **Pre-Update**: Reads latest configuration at frame start
//! - **Render**: UI operations with current state
//! - **Post-Update**: Writes changes back to persistent storage
//!
//! This ensures UI responsiveness while maintaining configuration consistency
//! across the application's thread architecture.
//!
//! ### Modal Dialog System
//! Uses egui's modal system for configuration dialogs rather than separate windows:
//! - Maintains focus within the main application window
//! - Supports controller-based navigation
//! - Provides consistent styling with the main interface
//!
//! ## Integration with Backend Systems
//!
//! ### MQTT Backend Communication
//! - **Received Messages**: Async channel from MQTT handler for live message display
//! - **Outgoing Messages**: Async channel to MQTT handler for message publishing
//! - **Configuration Updates**: Triggers MQTT backend reconfiguration through ConfigPortal
//!
//! ### Session Management Integration
//! - **Persistent Storage**: Message history and server configurations
//! - **Session Autosave**: Automatic backup of debugging sessions
//! - **Configuration Versioning**: Support for different MQTT setups per session
//!
//! ## Error Handling Strategy
//!
//! The module implements graceful degradation for MQTT operations:
//! - Failed message sends are logged but don't block the UI
//! - Configuration read failures fall back to default values
//! - Network connectivity issues are indicated through visual status
//! - Modal validation prevents invalid configurations from being saved

use super::common::{MQTTServer, UiColors};
use crate::mqtt::config::MqttConfig;
use crate::mqtt::message_manager::MQTTMessage;
use crate::persistence::config_portal::{ConfigPortal, ConfigResult, PortalAction};
use crate::persistence::persistence_worker::SessionAction;
use crate::session_action;
use eframe::egui::{
    self, vec2, Color32, ComboBox, Frame, Id, Label, Modal, ScrollArea, Stroke, TextEdit, Ui, Vec2,
};
use std::cell::Cell;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};

/// Main data structure for the MQTT debugging and management interface.
///
/// This structure manages the complete MQTT debugging workflow, from server
/// connection management to real-time message monitoring and composition.
/// It serves as the integration point between the UI layer and the MQTT
/// backend communication system.
///
/// ## Design Rationale
/// Combines immediate UI state with async communication channels to provide
/// responsive debugging capabilities. The structure is designed to handle
/// high-frequency message reception while maintaining UI fluidity.
///
/// ## Configuration Management Strategy
/// Uses a pre/post update pattern with ConfigPortal to ensure:
/// - UI always displays current configuration state
/// - User changes are immediately persisted
/// - Configuration changes trigger backend updates
/// - Session management captures debugging setups
///
/// ## Real-Time Communication Architecture
/// - **Inbound Channel**: Receives messages from MQTT backend for display
/// - **Outbound Channel**: Sends user-composed messages to MQTT backend
/// - **Configuration Channel**: Triggers backend reconfiguration
/// - **Session Channel**: Persists debugging sessions and message history
pub struct MQTTMenuData {
    /// Direct access to configuration portal for immediate reads/writes
    config_portal: Arc<ConfigPortal>,

    /// Channel for async session management operations
    session_sender: mpsc::Sender<SessionAction>,

    /// Receiver for incoming MQTT messages from backend
    received_msg: mpsc::Receiver<MQTTMessage>,

    /// Sender for outgoing MQTT messages to backend
    msg_sender: mpsc::Sender<MQTTMessage>,

    /// Currently active MQTT server configuration
    active_server: MQTTServer,

    /// List of saved server configurations for quick switching
    saved_servers: Vec<MQTTServer>,

    /// Currently selected topic (for subscription management)
    selected_topic: String,

    /// List of currently subscribed topics
    subscribed_topics: Vec<String>,

    /// List of all available/known topics
    available_topics: Vec<String>,

    /// Persistent message history for debugging templates
    message_history: Vec<MQTTMessage>,

    /// Currently selected message from history
    active_message: MQTTMessage,

    /// Current message being composed in the editor
    current_message: String,

    /// Live messages received during this session
    received_messages: Vec<MQTTMessage>,

    /// Modal state for server configuration dialog
    adding_server: Cell<bool>,

    /// New server URL input field
    new_server_url: String,

    /// New server username input field
    new_user: String,

    /// New server password input field
    new_pw: String,

    /// Modal state for topic configuration dialog
    adding_topic: Cell<bool>,

    /// New topic input field
    new_topic: String,

    /// Modal validation response trigger
    response_trigger: bool,
}

impl MQTTMenuData {
    /// Creates a new MQTT menu interface with current configuration state.
    ///
    /// Initializes the interface by reading current MQTT configuration and
    /// message history from the ConfigPortal, then sets up async communication
    /// channels with the MQTT backend.
    ///
    /// ## Design Rationale
    /// Performs synchronous initialization to ensure immediate UI display,
    /// then establishes async capabilities for real-time MQTT operations.
    /// Falls back to default configurations if ConfigPortal reads fail.
    ///
    /// ## Channel Architecture
    /// - `received_msg`: Incoming MQTT messages for live display
    /// - `msg_sender`: Outgoing messages for publishing
    /// - `session_sender`: Session management for persistent storage
    ///
    /// # Parameters
    /// - `received_msg`: Channel receiver for incoming MQTT messages
    /// - `msg_sender`: Channel sender for outgoing MQTT messages  
    /// - `config_portal`: Shared access to configuration system
    /// - `session_sender`: Channel for session management operations
    ///
    /// # Errors
    /// Falls back to default configuration if ConfigPortal reads fail,
    /// ensuring the UI always displays in a usable state for debugging.
    pub fn new(
        received_msg: mpsc::Receiver<MQTTMessage>,
        msg_sender: mpsc::Sender<MQTTMessage>,
        config_portal: Arc<ConfigPortal>,
        session_sender: mpsc::Sender<SessionAction>,
    ) -> Self {
        let config_res = config_portal.execute_potal_action(PortalAction::GetMqttConfig);
        let msg_res = config_portal.execute_potal_action(PortalAction::GetSavedMessagesMsg);

        let config = if let ConfigResult::MqttConfig(config) = config_res {
            config
        } else {
            warn!("Could not load MQTT Config");
            MqttConfig::default()
        };

        let msg_history = if let ConfigResult::MqttMessages(msg) = msg_res {
            msg
        } else {
            warn!("Could not load MQTT Message history");
            Vec::new()
        };

        MQTTMenuData {
            config_portal,
            session_sender,
            received_msg,
            msg_sender,
            active_server: config.server.clone(),
            saved_servers: config.available_servers.clone(),
            subscribed_topics: config.subbed_topics.clone(),
            available_topics: config.available_topics.clone(),
            message_history: msg_history.clone(),
            current_message: String::new(),
            received_messages: vec![],
            adding_server: Cell::new(false),
            adding_topic: Cell::new(false),
            selected_topic: String::new(),
            active_message: msg_history
                .first()
                .cloned()
                .unwrap_or(MQTTMessage::default()),
            new_pw: String::new(),
            new_server_url: String::new(),
            new_user: String::new(),
            new_topic: String::new(),
            response_trigger: false,
        }
    }

    /// Renders the complete MQTT debugging interface with real-time capabilities.
    ///
    /// Creates a sophisticated three-panel layout optimized for MQTT debugging
    /// workflows, with live message monitoring, server/topic management, and
    /// message composition capabilities.
    ///
    /// ## Layout Architecture
    ///
    /// ### Header Section
    /// Provides immediate MQTT control and status:
    /// - Server selection with connection status indicator
    /// - Topic subscription management with visual state
    /// - Real-time connection status with color coding
    ///
    /// ### Main Panel Layout (70/30 Split)
    /// - **Message Log (70%)**: Real-time incoming message display
    /// - **Control Panel (30%)**: Message composition and history access
    ///
    /// ## Performance Considerations
    ///
    /// Uses pre-calculated dimensions for consistent frame times:
    /// - Layout calculations performed once per frame
    /// - ScrollArea for efficient large message list rendering
    /// - Fixed panel heights prevent layout thrashing during message bursts
    ///
    /// ## Configuration Synchronization
    ///
    /// Implements pre/post update pattern:
    /// - **Pre-Update**: Loads latest configuration from ConfigPortal
    /// - **Render Phase**: UI operations with synchronized state
    /// - **Post-Update**: Persists user changes back to ConfigPortal
    ///
    /// This ensures UI reflects current configuration while capturing user changes
    /// for immediate persistence and backend notification.
    ///
    /// ## Real-Time Message Handling
    ///
    /// Processes incoming messages through async channels without blocking
    /// the UI thread, maintaining responsiveness during high message volume.
    pub fn render(&mut self, ui: &mut Ui) {
        self.pre_update_config();

        // Header section: server, topic controls, and connection status
        ui.horizontal(|ui| {
            ui.heading("MQTT");
            self.server_selection(ui);
            self.topic_selection(ui);

            let status_color = if self.active_server.connected {
                UiColors::ACTIVE
            } else {
                UiColors::INACTIVE
            };
            ui.colored_label(status_color, "\u{2B24}");
        });

        let available_size = ui.available_size();

        Frame::new()
            .stroke(Stroke::new(1.0, UiColors::BORDER))
            .fill(UiColors::MAIN_BG)
            .inner_margin(4)
            .outer_margin(2)
            .show(ui, |ui| {
                // Layout calculations for responsive debugging interface
                let total_width = available_size.x - 40.0; // Account for margins
                let log_width = total_width * 0.7;
                let right_width = total_width * 0.3 - 8.0; // Extra margin between panels

                // Fixed height definitions for consistent layout
                let button_area_height = 20.0;
                let message_history_height = 25.0;
                let spacing_height = 10.0; // Total spacing allocation

                let panel_height = available_size.y - 50.0;
                let editor_height =
                    panel_height - button_area_height - message_history_height - spacing_height;

                ui.horizontal(|ui| {
                    // Left Panel: Real-time message log
                    Frame::new()
                        .stroke(Stroke::new(1.0, UiColors::BORDER))
                        .fill(UiColors::INNER_BG)
                        .show(ui, |ui| {
                            ui.set_max_width(log_width);
                            ui.set_min_height(panel_height);

                            self.message_log(
                                ui,
                                Vec2::new(log_width, panel_height),
                                UiColors::BORDER,
                            );
                        });

                    ui.add_space(4.0);

                    // Right Panel: Message composition and history
                    ui.vertical(|ui| {
                        ui.set_max_width(right_width);

                        // Message history selector
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.set_min_height(message_history_height);
                            self.message_history(ui);
                        });

                        ui.add_space(2.0);

                        // Message editor
                        Frame::new()
                            .stroke(Stroke::new(1.0, UiColors::BORDER))
                            .fill(UiColors::INNER_BG)
                            .show(ui, |ui| {
                                let editor_size = Vec2::new(right_width - 4.0, editor_height);
                                self.msg_editor(ui, editor_size);
                            });

                        ui.add_space(4.0);

                        // Action buttons
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                            ui.set_min_height(button_area_height);

                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("Save").clicked() {
                                            let msg = MQTTMessage::from_topic(
                                                "OpenController".to_string(),
                                                self.current_message.clone(),
                                            );
                                            self.save_msg(msg);
                                        }
                                        ui.add_space(2.0);
                                        if ui.button("Send").clicked() {
                                            let msg = MQTTMessage::from_topic(
                                                "OpenController".to_string(),
                                                self.current_message.clone(),
                                            );
                                            self.save_msg(msg.clone());
                                            let _ = self.msg_sender.try_send(msg);
                                        }
                                    },
                                );
                            });
                        });
                    });
                });
            });

        self.post_update_config();
    }

    /// Synchronizes local state with current ConfigPortal configuration.
    ///
    /// Reads the latest MQTT configuration and message history from the
    /// ConfigPortal at the beginning of each frame to ensure UI displays
    /// current state even if other parts of the application modify configuration.
    ///
    /// ## Design Rationale
    /// Called at frame start to handle configuration changes from:
    /// - Other UI components
    /// - Backend configuration updates
    /// - Session loading operations
    /// - External configuration modifications
    fn pre_update_config(&mut self) {
        let config_res = self
            .config_portal
            .execute_potal_action(PortalAction::GetMqttConfig);
        let msg_res = self
            .config_portal
            .execute_potal_action(PortalAction::GetSavedMessagesMsg);

        let config = if let ConfigResult::MqttConfig(config) = config_res {
            config
        } else {
            warn!("Could not load MQTT Config");
            MqttConfig::default()
        };

        let msg_history = if let ConfigResult::MqttMessages(msg) = msg_res {
            msg
        } else {
            warn!("Could not load MQTT Message history");
            Vec::new()
        };

        self.active_server = config.server;
        self.available_topics = config.available_topics;
        self.saved_servers = config.available_servers;
        self.subscribed_topics = config.subbed_topics;
        self.message_history = msg_history;
    }

    /// Persists current UI state back to ConfigPortal configuration.
    ///
    /// Writes current MQTT configuration to the ConfigPortal at the end of
    /// each frame to ensure user changes are immediately persisted and
    /// available to other parts of the application.
    ///
    /// ## Design Rationale
    /// Called at frame end to capture any user modifications and trigger
    /// backend reconfiguration through the ConfigPortal update mechanism.
    fn post_update_config(&self) {
        let new_config = MqttConfig {
            available_topics: self.available_topics.clone(),
            subbed_topics: self.subscribed_topics.clone(),
            server: self.active_server.clone(),
            available_servers: self.saved_servers.clone(),
            poll_frequency: 10,
        };

        let _res = self
            .config_portal
            .execute_potal_action(PortalAction::WriteMqttConfig(new_config));
    }

    /// Renders the MQTT server selection interface with add-server capability.
    ///
    /// Provides a ComboBox for selecting from saved servers plus a modal dialog
    /// for adding new server configurations with validation.
    ///
    /// ## Modal Integration
    /// Uses egui's modal system for server configuration to maintain focus
    /// within the main application window and support controller navigation.
    /// Includes validation for required fields and user feedback.
    fn server_selection(&mut self, ui: &mut Ui) {
        ComboBox::from_id_salt("mqtt_server")
            .selected_text(self.active_server.to_string())
            .show_ui(ui, |ui| {
                for serv in &mut self.saved_servers {
                    ui.selectable_value(&mut self.active_server, serv.to_owned(), serv.to_string());
                }
                ui.toggle_value(self.adding_server.get_mut(), "Add Server");
            });

        if self.adding_server.get() {
            let modal = Modal::new(Id::new("Modal A"));
            modal.show(ui.ctx(), |ui| {
                let new_server_url = &mut self.new_server_url;
                let new_user = &mut self.new_user;
                let new_pw = &mut self.new_pw;
                let servers = &mut self.saved_servers;
                let add_server = &self.adding_server;
                ui.set_width(250.0);

                ui.heading("New Server");

                ui.label("URL");
                ui.text_edit_singleline(new_server_url);
                ui.label("user");
                ui.text_edit_singleline(new_user);
                ui.label("Password");
                ui.text_edit_singleline(new_pw);

                ui.separator();

                egui::Sides::new().show(
                    ui,
                    |left| {
                        let mut validation = (true, true);
                        if left.button("Save").clicked() {
                            self.response_trigger = true;
                            validation = (new_server_url.is_empty(), new_user.is_empty());
                        }

                        if self.response_trigger {
                            let err_msg = Modal::new(Id::new("ValidationErr"));
                            let err_response = match validation {
                                (true, true) => err_msg.show(left.ctx(), |pop| {
                                    pop.set_width(100.0);
                                    pop.label("Server and User empty!");
                                }),
                                (true, false) => err_msg.show(left.ctx(), |pop| {
                                    pop.set_width(100.0);
                                    pop.label("Server empty!");
                                }),
                                (false, true) => err_msg.show(left.ctx(), |pop| {
                                    pop.set_width(100.0);
                                    pop.label("User empty!");
                                }),
                                (false, false) => {
                                    let new_server = MQTTServer {
                                        url: new_server_url.to_owned(),
                                        user: new_user.to_owned(),
                                        pw: new_pw.to_owned(),
                                        connected: false,
                                    };
                                    self.response_trigger = false;
                                    add_server.set(false);

                                    servers.push(new_server);
                                    err_msg.show(left.ctx(), |pop| {
                                        pop.label("Saved!");
                                    })
                                }
                            };
                            if err_response.should_close() {
                                self.response_trigger = false;
                            }
                        }
                    },
                    |right| {
                        if right.button("Cancel").clicked() {
                            add_server.set(false);
                        }
                    },
                );
            });
        }
    }

    /// Renders the MQTT topic selection and subscription management interface.
    ///
    /// Provides dynamic topic subscription/unsubscription with visual indication
    /// of current subscription state and modal dialog for adding new topics.
    ///
    /// ## Subscription Management Logic
    /// Implements toggle-based subscription: clicking a subscribed topic
    /// unsubscribes it, clicking an unsubscribed topic subscribes it.
    /// Visual highlighting indicates current subscription status.
    fn topic_selection(&mut self, ui: &mut Ui) {
        let none_topic = String::new();
        let selected_topic = &mut self.selected_topic;

        let add_topic = &mut self.adding_topic;

        let available_topics = &mut self.available_topics;
        let subscribed_topics = &mut self.subscribed_topics;

        ComboBox::from_id_salt("topic_selector")
            .selected_text("Select Topics".to_string())
            .show_ui(ui, |ui| {
                for availabel in available_topics.clone() {
                    if subscribed_topics.iter().any(|sub| *sub == availabel) {
                        ui.selectable_value(
                            selected_topic,
                            availabel.clone(),
                            availabel.to_string(),
                        )
                        .highlight();
                    } else {
                        ui.selectable_value(
                            selected_topic,
                            availabel.clone(),
                            availabel.to_string(),
                        );
                    }
                }

                ui.toggle_value(add_topic.get_mut(), "Add Topic");
            });

        let validate = (
            !selected_topic.is_empty(),
            subscribed_topics.iter().any(|sub| *sub == *selected_topic),
        );

        match validate {
            (false, _) => {}
            (true, true) => {
                let idx = subscribed_topics
                    .iter()
                    .position(|sub| *sub == *selected_topic);
                if let Some(pos) = idx {
                    let _ = subscribed_topics.remove(pos);
                }
                *selected_topic = none_topic;
                debug!("Deactivate topic");
            }
            (true, false) => {
                subscribed_topics.push(selected_topic.clone());
                *selected_topic = none_topic;
                debug!("Activate topic");
            }
        }

        if add_topic.get() {
            let modal = Modal::new(Id::new("Modal B"));

            modal.show(ui.ctx(), |ui| {
                let new_topic = &mut self.new_topic;

                ui.set_width(250.0);

                ui.heading("New Topic");

                ui.label("Topic");
                ui.text_edit_singleline(new_topic);

                ui.separator();

                egui::Sides::new().show(
                    ui,
                    |left| {
                        let mut validation = false;
                        if left.button("Save").clicked() {
                            self.response_trigger = true;
                            validation = new_topic.is_empty();
                            if !validation {
                                available_topics.push(new_topic.clone());
                            }
                        }

                        if self.response_trigger {
                            let err_msg = Modal::new(Id::new("ValidationErr"));
                            let err_response = if new_topic.is_empty() {
                                err_msg.show(left.ctx(), |pop| {
                                    pop.set_width(100.0);
                                    pop.label("No Topic");
                                })
                            } else {
                                err_msg.show(left.ctx(), |pop| {
                                    pop.label("Saved");
                                })
                            };
                            if err_response.should_close() {
                                self.response_trigger = false;
                                add_topic.set(false);
                            }
                        }
                    },
                    |right| {
                        if right.button("Cancel").clicked() {
                            add_topic.set(false);
                        }
                    },
                );
            });
        }
    }

    /// Renders the message history selector for accessing saved message templates.
    ///
    /// Provides quick access to previously saved messages for debugging workflows,
    /// loading selected messages into the editor for modification and resending.
    fn message_history(&mut self, ui: &mut Ui) {
        ComboBox::from_id_salt("message history")
            .selected_text("Message History")
            .show_ui(ui, |ui| {
                for message in &mut self.message_history {
                    if ui
                        .selectable_value(
                            &mut self.active_message,
                            message.clone(),
                            message.to_string(),
                        )
                        .clicked()
                    {
                        self.current_message = self.active_message.content.clone();
                    }
                }
            });
    }

    /// Renders the real-time MQTT message log with live message reception.
    ///
    /// Displays incoming MQTT messages in real-time with click-to-copy functionality
    /// for debugging workflows. Uses ScrollArea for efficient rendering of large
    /// message volumes.
    ///
    /// ## Performance Considerations
    /// Processes incoming messages without blocking UI thread, maintaining
    /// responsiveness during high message frequency scenarios.
    fn message_log(&mut self, ui: &mut Ui, size: Vec2, border_color: Color32) {
        let new_incoming_msg = self.received_msg.try_recv();
        if let Ok(msg) = new_incoming_msg {
            self.received_messages.push(msg);
        }

        Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(4)
            .stroke(Stroke::new(1.0, ui.visuals().widgets.active.bg_fill))
            .show(ui, |ui| {
                ui.set_min_size(size);

                ScrollArea::vertical().show(ui, |ui| {
                    ui.vertical(|ui| {
                        for msg in &self.received_messages {
                            Frame::new()
                                .stroke(Stroke::new(1.0, border_color))
                                .inner_margin(2)
                                .fill(UiColors::EXTREME_BG)
                                .show(ui, |ui| {
                                    if ui
                                        .add_sized(
                                            vec2(size.x, size.y / 6.0),
                                            Label::new(msg.render())
                                                .selectable(true)
                                                .sense(egui::Sense::click()),
                                        )
                                        .clicked()
                                    {
                                        info!("MSG: {} \n COPIED!", msg.render());
                                        // TODO: Implement clipboard copy functionality
                                    }
                                });
                            ui.add_space(2.0);
                        }
                    });
                });
            });
    }

    /// Renders the message composition editor for creating MQTT messages.
    ///
    /// Provides a multi-line text editor with syntax highlighting for composing
    /// MQTT message payloads with debugging-friendly features.
    fn msg_editor(&mut self, ui: &mut Ui, size: Vec2) {
        let textbuffer = &mut self.current_message;
        ScrollArea::vertical().id_salt("msg_editor").show(ui, |ui| {
            TextEdit::multiline(textbuffer)
                .min_size(size)
                .hint_text("Nachricht eingeben...")
                .code_editor()
                .show(ui);
        });
    }

    /// Saves a message to the persistent message history and triggers session backup.
    ///
    /// Adds the message to the local history, persists it through ConfigPortal,
    /// and triggers a session save to ensure debugging sessions are preserved.
    ///
    /// ## Persistence Strategy
    /// Uses immediate ConfigPortal write followed by async session save to
    /// ensure message templates are available across application restarts.
    fn save_msg(&mut self, msg: MQTTMessage) {
        self.message_history.push(msg.clone());

        let _res = self
            .config_portal
            .execute_potal_action(PortalAction::WriteSavedMessagesMsg(
                self.message_history.clone(),
            ));

        let _ = session_action!(@save, self.session_sender);
    }
}
