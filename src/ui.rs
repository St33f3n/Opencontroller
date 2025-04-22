use chrono::NaiveDateTime;
use color_eyre::owo_colors::OwoColorize;
use eframe::egui::{
    self, containers, style, vec2, widgets, Area, Button, Color32, ComboBox, Context, Event, Frame,
    Id, Key, Label, Layout, ProgressBar, Rect, RichText, ScrollArea, Sense, Stroke, Style,
    TextBuffer, TextEdit, Ui, Vec2, Widget, Window,
};

use egui::Modal;
use tokio::sync::mpsc;

use std::cell::{Cell, RefCell};
use std::fmt::format;
use std::path::Path;
use std::rc::Rc;
use std::{default, f32, fmt};
use std::{str::FromStr, time::Duration};
use tokio::sync::watch::{self, Receiver};
use tracing::{debug, error, info};

use crate::controller;

enum MenuState {
    Main,
    MQTT,
    ELRS,
    Settings,
}

#[derive(Default)]
struct SessionData {
    last_session_path: String,
    mqtt_data: MQTTMenuData,
    elrs_data: ELRSMenuData,
    settings_data: SettingsMenuData,
}

#[derive(Default)]
struct ELRSConnection {}

#[derive(Default, Clone, PartialEq)]
struct MQTTServer {
    url: String,
    user: String,
    pw: String,
    connceted: bool,
}

impl fmt::Display for MQTTServer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}@{}", self.user, self.url)
    }
}

#[derive(Default, Clone, PartialEq, Eq)]
struct MQTTTopic {
    name: String,
}

impl MQTTTopic {
    fn from_string(name: String) -> Self {
        MQTTTopic { name }
    }
}

impl fmt::Display for MQTTTopic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Default, Clone, PartialEq, Eq)]
struct MQTTMessage {
    topic: MQTTTopic,
    content: String,
    timestamp: NaiveDateTime,
}

impl fmt::Display for MQTTMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut slice = self.content.clone();
        let preview = slice.split_off(10);
        write!(f, "{} - {}", self.timestamp, preview)
    }
}

impl MQTTMessage {
    fn from_topic(topic: MQTTTopic, content: String) -> Self {
        MQTTMessage {
            topic,
            content,
            timestamp: chrono::Local::now().naive_local(),
        }
    }

    fn render(&self) -> String {
        format!("{}: {}\n{}", self.timestamp, self.topic, self.content)
    }
}

#[derive(Default)]
struct MainMenuData {
    current_session_name: String,
    new_session_name: String,
    previous_sessions: Vec<SessionData>,
}

impl MainMenuData {
    fn mock_data() -> Self {
        let old_test_session = SessionData {
            last_session_path: "".to_string(),
            mqtt_data: MQTTMenuData::mock_data(),
            elrs_data: ELRSMenuData::mock_data(),
            settings_data: SettingsMenuData::mock_data(),
        };

        Self {
            current_session_name: "TestSession".to_string(),
            new_session_name: String::new(),
            previous_sessions: vec![old_test_session],
        }
    }
}

#[derive(Default)]
struct MQTTMenuData {
    active_server: MQTTServer,
    saved_servers: Vec<MQTTServer>,
    selected_topic: MQTTTopic,
    subscribed_topics: Vec<MQTTTopic>,
    available_topics: Vec<MQTTTopic>,
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
    fn mock_data() -> Self {
        let server = MQTTServer {
            url: "mqtt.testserver.com".to_string(),
            user: "test".to_string(),
            pw: "testpw".to_string(),
            ..Default::default()
        };
        let test_topic1 = MQTTTopic::from_string("test/topic1".to_string());
        let test_topic2 = MQTTTopic::from_string("test/topic2".to_string());
        let test_msg1 = "Testfiller".to_string();
        let test_msg2 = "Testfiller2".to_string();
        MQTTMenuData {
            active_server: server.clone(),
            saved_servers: vec![server],
            subscribed_topics: Vec::new(),
            available_topics: vec![test_topic1.clone(), test_topic2.clone()],
            message_history: Vec::new(),
            current_message: String::new(),
            received_messages: vec![
                MQTTMessage::from_topic(test_topic1, test_msg1),
                MQTTMessage::from_topic(test_topic2, test_msg2),
            ],
            adding_server: Cell::new(false),
            adding_topic: Cell::new(false),
            ..MQTTMenuData::default()
        }
    }
}

#[derive(Default)]
struct ELRSMenuData {
    transmitter_port: String,
    transmitter_connection: bool,
    connection: String,
    available_connections: Vec<String>,
    live_connect: bool,
}

impl ELRSMenuData {
    fn mock_data() -> Self {
        ELRSMenuData {
            transmitter_port: "Port Test 1".to_string(),
            transmitter_connection: true,
            connection: "TestCon".to_string(),
            available_connections: vec!["Test1".to_string(), "Test2".to_string()],
            live_connect: false,
        }
    }
}

#[derive(Default)]
struct SettingsMenuData {
    current_network: WiFiNetwork,
    selected_network: WiFiNetwork,
    available_networks: Vec<WiFiNetwork>,
    network_pw: String,
    connected: bool,
    display_brightness: f32,
    screensave: usize,
}

impl SettingsMenuData {
    fn mock_data() -> Self {
        let active_net = WiFiNetwork::new("NetActive".to_string(), "ddd".to_string());
        let networks = vec![
            WiFiNetwork::new("Test1".to_string(), "123".to_string()),
            WiFiNetwork::new("Test2".to_string(), "321".to_string()),
        ];
        Self {
            current_network: active_net.clone(),
            selected_network: active_net,
            available_networks: networks,
            ..Default::default()
        }
    }
}

#[derive(Default, Clone, PartialEq, Eq)]
struct WiFiNetwork {
    ssid: String,
    pw: String,
}

impl fmt::Display for WiFiNetwork {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.ssid)
    }
}

impl WiFiNetwork {
    pub fn new(ssid: String, pw: String) -> Self {
        Self { ssid, pw }
    }
}

pub struct OpencontrollerUI {
    menu_state: MenuState,
    event_receiver: mpsc::Receiver<Vec<egui::Event>>,
    main_menu_data: MainMenuData,
    elrs_menu_data: ELRSMenuData,
    mqtt_menu_data: MQTTMenuData,
    settings_menu_data: SettingsMenuData,
    bat_controller: usize,
    bat_pc: usize,
}

impl OpencontrollerUI {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        event_receiver: mpsc::Receiver<Vec<egui::Event>>,
    ) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        OpencontrollerUI {
            menu_state: MenuState::Main,
            event_receiver,
            main_menu_data: MainMenuData::mock_data(),
            elrs_menu_data: ELRSMenuData::mock_data(),
            mqtt_menu_data: MQTTMenuData::mock_data(),
            settings_menu_data: SettingsMenuData::mock_data(),
            bat_controller: 0,
            bat_pc: 0,
        }
    }
}

impl OpencontrollerUI {
    // In der OpencontrollerUI-Implementierung
    fn log_controller_state(&mut self) {
        // Aktuelle Controller-State auslesen
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

    fn main_menu(&mut self, ui: &mut Ui) {
        let available_size = ui.available_size();
        let border_color = Color32::from_rgb(60, 60, 60);

        ui.vertical(|ui| {
            // Obere Zeile mit Überschrift, Texteingabefeld und Save-Button
            ui.horizontal(|ui| {
                ui.heading("Mainmenu");
                ui.add(
                    TextEdit::singleline(&mut self.main_menu_data.new_session_name)
                        .hint_text(self.main_menu_data.current_session_name.as_str()),
                );
                if ui.button("Save").clicked() {
                    debug!("Saving Session");
                    // Platzhaltercode für die asynchrone Speicherung - wird später manuell ergänzt
                    let current_session =
                        String::from_str("Hier Sessioncreation einfügen").unwrap();
                    // TODO: hier noch zu session vektorhinzufügen und abspeichern
                }
            });

            // Session-Liste im Stil des message_log
            Frame::new()
                .fill(ui.visuals().extreme_bg_color)
                .stroke(Stroke::new(1.0, ui.visuals().widgets.active.bg_fill))
                .show(ui, |ui| {
                    let list_height = available_size.y - 40.0; // Höhe abzüglich des oberen Bereichs
                    ui.set_min_size(vec2(available_size.x, list_height));

                    ScrollArea::vertical().show(ui, |ui| {
                        ui.vertical(|ui| {
                            for (index, _session) in
                                self.main_menu_data.previous_sessions.iter().enumerate()
                            {
                                Frame::new()
                                    .stroke(Stroke::new(1.0, border_color))
                                    .inner_margin(2)
                                    .outer_margin(8)
                                    .fill(Color32::from_rgb(20, 20, 20))
                                    .show(ui, |ui| {
                                        if ui
                                            .add_sized(
                                                vec2(available_size.x - 20.0, list_height / 6.0),
                                                Label::new(format!("Predev Session {}", index + 1))
                                                    .selectable(true)
                                                    .sense(Sense::click()),
                                            )
                                            .clicked()
                                        {
                                            debug!("Loading Session")
                                            // Hier würde der Code zum Laden der Session kommen
                                            // (wird später manuell ergänzt)
                                        }
                                    });
                                ui.add_space(2.0);
                            }

                            // Falls keine Sessions vorhanden sind, einen Platzhalter anzeigen
                            if self.main_menu_data.previous_sessions.is_empty() {
                                ui.label("No saved sessions available");
                            }
                        });
                    });
                });
        });
    }

    fn mqtt_menu(&mut self, ui: &mut Ui) {
        // Obere Zeile mit MQTT-Überschrift, Server, Topic und Status-Indikator
        ui.horizontal(|ui| {
            ui.heading("MQTT");
            self.server_selection(ui);
            self.topic_selection(ui);

            let status_color = if self.mqtt_menu_data.active_server.connceted {
                Color32::from_rgb(50, 200, 20)
            } else {
                Color32::from_rgb(200, 50, 20)
            };
            ui.colored_label(status_color, "\u{2B24}");
        });

        let available_size = ui.available_size();

        // Helleres Grau für Hauptframe
        let main_bg = Color32::from_rgb(30, 30, 30);
        // Etwas dunkleres Grau für innere Frames
        let inner_bg = Color32::from_rgb(25, 25, 25);
        // Rahmenfarbe
        let border_color = Color32::from_rgb(60, 60, 60);

        Frame::new()
            .stroke(Stroke::new(1.0, border_color))
            .fill(main_bg)
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
                        .stroke(Stroke::new(1.0, border_color))
                        .fill(inner_bg)
                        .show(ui, |ui| {
                            ui.set_max_width(log_width);
                            ui.set_min_height(panel_height);

                            // Hier die angepasste message_log Funktion verwenden
                            self.message_log(ui, Vec2::new(log_width, panel_height), border_color);
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
                            .stroke(Stroke::new(1.0, border_color))
                            .fill(inner_bg)
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
                                            // Save-Logik
                                        }
                                        ui.add_space(2.0);
                                        if ui.button("Send").clicked() {
                                            // Send-Logik
                                        }
                                    },
                                );
                            });
                        });
                    });
                });
            });
    }

    fn topic_selection(&mut self, ui: &mut Ui) {
        let none_topic = MQTTTopic::default();
        let selected_topic = &mut self.mqtt_menu_data.selected_topic;

        let add_topic = &mut self.mqtt_menu_data.adding_topic;

        let available_topics = &mut self.mqtt_menu_data.available_topics;
        let subscribed_topics = &mut self.mqtt_menu_data.subscribed_topics;

        egui::ComboBox::from_id_salt("topic_selector")
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
            !selected_topic.name.is_empty(),
            subscribed_topics.iter().any(|sub| *sub == *selected_topic),
        );

        match validate {
            (false, _) => {}
            (true, true) => {
                let idx = subscribed_topics
                    .iter_mut()
                    .position(|sub| *sub == *selected_topic);
                if let Some(pos) = idx {
                    let _ = subscribed_topics.remove(pos);
                }
                *selected_topic = none_topic;
                println!("Deaktivate")
            }
            (true, false) => {
                subscribed_topics.push(selected_topic.clone());
                *selected_topic = none_topic;
                println!("Activate")
            }
        }

        if add_topic.get() {
            let modal = Modal::new(Id::new("Modal B"));

            modal.show(ui.ctx(), |ui| {
                let new_topic = &mut self.mqtt_menu_data.new_topic;

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
                            self.mqtt_menu_data.response_trigger = true;
                            validation = new_topic.is_empty();
                            if !validation {
                                available_topics.push(MQTTTopic::from_string(new_topic.clone()));
                            }
                        }

                        if self.mqtt_menu_data.response_trigger {
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
                                self.mqtt_menu_data.response_trigger = false;
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

    fn server_selection(&mut self, ui: &mut Ui) {
        egui::ComboBox::from_id_salt("mqtt_server")
            .selected_text(self.mqtt_menu_data.active_server.to_string())
            .show_ui(ui, |ui| {
                for serv in &mut self.mqtt_menu_data.saved_servers {
                    ui.selectable_value(
                        &mut self.mqtt_menu_data.active_server,
                        serv.to_owned(),
                        serv.to_string(),
                    );
                }
                ui.toggle_value(self.mqtt_menu_data.adding_server.get_mut(), "Add Server");
            });

        if self.mqtt_menu_data.adding_server.get() {
            let modal = Modal::new(Id::new("Modal A"));
            modal.show(ui.ctx(), |ui| {
                let new_server_url = &mut self.mqtt_menu_data.new_server_url;
                let new_user = &mut self.mqtt_menu_data.new_user;
                let new_pw = &mut self.mqtt_menu_data.new_pw;
                let servers = &mut self.mqtt_menu_data.saved_servers;
                let add_server = &self.mqtt_menu_data.adding_server;
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
                            self.mqtt_menu_data.response_trigger = true;
                            validation = (new_server_url.is_empty(), new_user.is_empty());
                        }

                        if self.mqtt_menu_data.response_trigger {
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
                                        connceted: false,
                                    };
                                    self.mqtt_menu_data.response_trigger = false;
                                    add_server.set(false);

                                    servers.push(new_server);
                                    err_msg.show(left.ctx(), |pop| {
                                        pop.label("Saved!");
                                    })
                                }
                            };
                            if err_response.should_close() {
                                self.mqtt_menu_data.response_trigger = false;
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

    fn message_history(&mut self, ui: &mut Ui) {
        egui::ComboBox::from_id_salt("message history")
            .selected_text("Message History")
            .show_ui(ui, |ui| {
                for message in &mut self.mqtt_menu_data.message_history {
                    ui.selectable_value(
                        &mut self.mqtt_menu_data.active_message,
                        message.clone(),
                        message.to_string(),
                    );
                }
            });
    }

    fn message_log(&mut self, ui: &mut Ui, size: Vec2, border_color: Color32) {
        Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(4)
            .stroke(Stroke::new(1.0, ui.visuals().widgets.active.bg_fill))
            .show(ui, |ui| {
                ui.set_min_size(size);

                ScrollArea::vertical().show(ui, |ui| {
                    ui.vertical(|ui| {
                        for msg in &self.mqtt_menu_data.received_messages {
                            Frame::new()
                                .stroke(Stroke::new(1.0, border_color))
                                .inner_margin(2)
                                .fill(Color32::from_rgb(20, 20, 20))
                                .show(ui, |ui| {
                                    if ui
                                        .add_sized(
                                            vec2(size.x, size.y / 6.0),
                                            Label::new(msg.render())
                                                .selectable(true)
                                                .sense(Sense::click()),
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

    fn msg_editor(&mut self, ui: &mut Ui, size: Vec2) {
        let textbuffer = &mut self.mqtt_menu_data.current_message;
        ScrollArea::vertical().id_salt("msg_editor").show(ui, |ui| {
            TextEdit::multiline(textbuffer)
                .min_size(size)
                .hint_text("Nachricht eingeben...")
                .code_editor()
                .show(ui);
        });
    }

    fn elrs_menu(&mut self, ui: &mut Ui) {
        // Header section
        ui.horizontal(|ui| {
            ui.heading("ELRS");
            if self.elrs_menu_data.transmitter_connection {
                ui.label("Transmitter Connected");
                ui.label("Port Test"); // TODO: Function to read port
            } else {
                ui.label("No Transmitter found");
            }
        });

        let available_size = ui.available_size();
        let border_color = Color32::from_rgb(60, 60, 60);
        let background_color = ui.visuals().extreme_bg_color;

        // Berechnung der Spaltenbreiten (ca. 70% links, 30% rechts)
        let left_width = available_size.x * 0.7;
        let right_width = available_size.x * 0.3 - 40.0;
        let panel_height = available_size.y - 30.0; // Höhe abzüglich Header

        ui.horizontal(|ui| {
            // Linke Spalte - Telemetrie
            ui.vertical(|ui| {
                ui.set_min_width(left_width);

                // Telemetrie-Box mit Überschrift und Inhalt
                Frame::new().inner_margin(4).show(ui, |ui| {
                    ui.set_min_width(left_width);
                    // Überschrift der Telemetrie-Box
                    ui.heading("Telemetrie");

                    // Inhalt der Telemetrie-Box
                    Frame::new()
                        .stroke(Stroke::new(1.0, border_color))
                        .fill(background_color)
                        .inner_margin(4.0)
                        .show(ui, |ui| {
                            ui.set_min_width(left_width);
                            ui.set_min_height(panel_height - 30.0); // Höhe abzüglich Überschrift
                            ui.label("Hier kommt Telemetrie");
                        });
                });
            });

            // Rechte Spalte - Steuerelemente
            ui.vertical(|ui| {
                ui.set_max_width(right_width);

                // Scan-Bereich mit Button und Dropdown
                Frame::new()
                    .stroke(Stroke::new(1.0, border_color))
                    .fill(Color32::from_rgb(25, 25, 25))
                    .corner_radius(2)
                    .inner_margin(6.0)
                    .outer_margin(0.0)
                    .show(ui, |ui| {
                        ui.set_min_width(right_width);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                if ui.button("Scan").clicked() {
                                    // Scan for connections
                                }

                                egui::ComboBox::from_id_salt("Connections")
                                    .selected_text(&self.elrs_menu_data.connection)
                                    .width(right_width - 70.0) // Breite anpassen
                                    .show_ui(ui, |ui| {
                                        for con in &mut self.elrs_menu_data.available_connections {
                                            ui.selectable_value(
                                                &mut self.elrs_menu_data.connection,
                                                con.to_string(),
                                                con.to_string(),
                                            );
                                        }
                                    });
                            });

                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                if ui.button("Live Connect").clicked() {
                                    // Live connect functionality
                                }

                                let status = if self.elrs_menu_data.live_connect {
                                    "Live On"
                                } else {
                                    "Live Off"
                                };
                                ui.add_space(4.0);
                                ui.label(status);
                            });
                        });
                    });
            });
        });
    }

    fn settings_menu(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.heading("Settings");

            // Farbdefinitionen für konsistentes Aussehen
            let main_bg = Color32::from_rgb(30, 30, 30); // Helleres Grau für Hauptframe
            let inner_bg = Color32::from_rgb(25, 25, 25); // Etwas dunkleres Grau für innere Frames
            let border_color = Color32::from_rgb(60, 60, 60); // Rahmenfarbe
            let section_spacing = 5.0; // Abstand zwischen Sektionen

            // WLAN-Sektion
            self.settings_wlan_section(ui, main_bg, inner_bg, border_color);

            ui.add_space(section_spacing);

            // Display-Sektion
            self.settings_display_section(ui, main_bg, inner_bg, border_color);
        });
    }

    fn settings_wlan_section(
        &mut self,
        ui: &mut Ui,
        main_bg: Color32,
        inner_bg: Color32,
        border_color: Color32,
    ) {
        Frame::new()
            .stroke(Stroke::new(1.0, border_color))
            .fill(main_bg)
            .inner_margin(8.0)
            .outer_margin(2.0)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.heading("WLAN");

                    // Aktuelles Netzwerk als Label
                    ui.horizontal(|ui| {
                        ui.label("Current Network:");
                        ui.label(self.settings_menu_data.current_network.to_string());
                    });

                    // Verfügbare Netzwerke, Passwort und Connect-Button in einer Zeile
                    ui.horizontal(|ui| {
                        let total_width = ui.available_width() - 30.0;
                        let network_width = total_width * 0.45;
                        let password_width = total_width * 0.45;
                        let connect_width = total_width * 0.1;

                        // Verfügbare Netzwerke - Dropdown-Menü
                        ui.vertical(|ui| {
                            let networkname = self.settings_menu_data.selected_network.to_string();
                            ui.set_min_width(network_width);
                            egui::ComboBox::from_id_salt("available_networks")
                                .selected_text(if networkname.is_empty() {
                                    "Available Networks"
                                } else {
                                    networkname.as_str()
                                })
                                .width(network_width - 10.0)
                                .show_ui(ui, |ui| {
                                    for network in &self.settings_menu_data.available_networks {
                                        ui.selectable_value(
                                            &mut self.settings_menu_data.selected_network,
                                            network.clone(),
                                            network.to_string(),
                                        );
                                    }
                                });
                        });

                        // Passwort-Feld
                        ui.vertical(|ui| {
                            ui.set_min_width(password_width);

                            ui.add(
                                TextEdit::singleline(&mut self.settings_menu_data.network_pw)
                                    .password(true)
                                    .hint_text("Enter Password"),
                            );
                        });

                        // Connect-Button
                        ui.vertical(|ui| {
                            ui.set_min_width(connect_width);

                            if ui.button("Connect").clicked() {
                                //TODO Send MSG to try and connect
                            }
                        });
                    });
                });
            });
    }

    fn settings_display_section(
        &mut self,
        ui: &mut Ui,
        main_bg: Color32,
        inner_bg: Color32,
        border_color: Color32,
    ) {
        Frame::new()
            .stroke(Stroke::new(1.0, border_color))
            .fill(main_bg)
            .inner_margin(8.0)
            .outer_margin(2.0)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    let total_width = ui.available_width() - 15.0;
                    ui.set_min_width(total_width);
                    ui.heading("Display");

                    // Helligkeitsregler
                    ui.horizontal(|ui| {
                        ui.label("Brightness:");
                        ui.add(egui::Slider::new(
                            &mut self.settings_menu_data.display_brightness,
                            0.0..=1.0,
                        ));
                    });

                    // Screensaver-Timeout
                    ui.horizontal(|ui| {
                        ui.label("Screensaver (seconds):");
                        ui.add(
                            egui::DragValue::new(&mut self.settings_menu_data.screensave)
                                .speed(1)
                                .range(0..=3600),
                        );
                    });
                });
            });
    }
}

impl eframe::App for OpencontrollerUI {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Ok(events) = self.event_receiver.try_recv() {
            for event in events {
                raw_input.events.push(event);
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        //self.log_controller_state();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.ctx().request_repaint_after(Duration::from_millis(33));
            let width = ui.available_width() - 60.0;

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

            egui::CentralPanel::default().show_inside(ui, |ui| match self.menu_state {
                MenuState::Main => self.main_menu(ui),
                MenuState::ELRS => {
                    self.elrs_menu(ui);
                }
                MenuState::MQTT => {
                    self.mqtt_menu(ui);
                }
                MenuState::Settings => {
                    self.settings_menu(ui);
                }
            });

            egui::TopBottomPanel::bottom("bottom_panel")
                .show_separator_line(false)
                .show_inside(ui, |ui| {
                    let connection_status = if self.settings_menu_data.connected {
                        String::from_str("🟢").unwrap()
                    } else {
                        String::from_str("🔴").unwrap()
                    };
                    ui.horizontal_centered(|ui| {
                        ui.label(format!(
                            "{} {}",
                            self.settings_menu_data.current_network, connection_status
                        ));
                        ui.label(format!("CBat: {}%", self.bat_controller));
                        ui.label(format!("PCBat: {}%", self.bat_pc));
                    });
                });
        });
    }
}
