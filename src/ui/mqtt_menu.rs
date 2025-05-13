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

/// Datenstruktur für das MQTT-Menü
pub struct MQTTMenuData {
    config_portal: Arc<ConfigPortal>,
    session_sender: mpsc::Sender<SessionAction>,
    received_msg: mpsc::Receiver<MQTTMessage>,
    msg_sender: mpsc::Sender<MQTTMessage>,
    active_server: MQTTServer,
    saved_servers: Vec<MQTTServer>,
    selected_topic: String,
    subscribed_topics: Vec<String>,
    available_topics: Vec<String>,
    message_history: Vec<MQTTMessage>,
    active_message: MQTTMessage,
    current_message: String,
    received_messages: Vec<MQTTMessage>,
    adding_server: Cell<bool>,
    new_server_url: String,
    new_user: String,
    new_pw: String,
    adding_topic: Cell<bool>,
    new_topic: String,
    response_trigger: bool,
}

impl MQTTMenuData {
    /// Erstellt Mock-Daten für die Entwicklung
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

    /// Rendert das MQTT-Menü
    pub fn render(&mut self, ui: &mut Ui) {
        self.pre_update_config();
        // Obere Zeile mit MQTT-Überschrift, Server, Topic und Status-Indikator
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
                // Layout mit einer festen Aufteilung
                let total_width = available_size.x - 40.0; // Etwas Platz für Margins
                let log_width = total_width * 0.7;
                let right_width = total_width * 0.3 - 8.0; // Extra Margin zwischen den Bereichen

                // WICHTIG: Feste Höhen definieren vor der Panelhöhe
                let button_area_height = 20.0;
                let message_history_height = 25.0;
                let spacing_height = 10.0; // Gesamter Raum für Abstände

                // Verfügbare Höhe für das Panel berechnen
                let panel_height = available_size.y - 50.0;

                // Editor-Höhe als Rest berechnen (verfügbare Höhe minus Buttons und History)
                let editor_height =
                    panel_height - button_area_height - message_history_height - spacing_height;

                ui.horizontal(|ui| {
                    // Message Log - Links
                    Frame::new()
                        .stroke(Stroke::new(1.0, UiColors::BORDER))
                        .fill(UiColors::INNER_BG)
                        .show(ui, |ui| {
                            ui.set_max_width(log_width);
                            ui.set_min_height(panel_height);

                            // Hier die angepasste message_log Funktion verwenden
                            self.message_log(
                                ui,
                                Vec2::new(log_width, panel_height),
                                UiColors::BORDER,
                            );
                        });

                    ui.add_space(4.0);

                    // Rechter Bereich mit fester Breite
                    ui.vertical(|ui| {
                        ui.set_max_width(right_width);

                        // WICHTIG: Feste Höhe für Message History mit Größe reservieren
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.set_min_height(message_history_height);
                            self.message_history(ui);
                        });

                        ui.add_space(2.0);

                        // Editor in einem separaten Frame mit fester Höhe
                        Frame::new()
                            .stroke(Stroke::new(1.0, UiColors::BORDER))
                            .fill(UiColors::INNER_BG)
                            .show(ui, |ui| {
                                // WICHTIG: Editor-Höhe als berechneten Wert übergeben
                                let editor_size = Vec2::new(right_width - 4.0, editor_height);
                                self.msg_editor(ui, editor_size);
                            });

                        // WICHTIG: Explizit verbleibenden Platz reservieren für Buttons
                        ui.add_space(4.0);

                        // Feste Höhe für die Buttons-Zeile
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                            ui.set_min_height(button_area_height);

                            ui.horizontal(|ui| {
                                // Buttons rechts ausrichten
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

    /// Rendert die Server-Auswahl
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

    /// Rendert die Topic-Auswahl
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

    /// Rendert die Nachrichtenhistorie
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

    /// Rendert das Message-Log
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
                                        // Hier Kopieren-Logik
                                    }
                                });
                            ui.add_space(2.0);
                        }
                    });
                });
            });
    }

    /// Rendert den Nachrichten-Editor
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
