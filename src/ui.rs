use chrono::NaiveDateTime;
use eframe::egui::{
    self, containers, style, Button, ComboBox, Context, Id, Label, Layout, ProgressBar, Rect,
    RichText, Ui, Vec2, Widget, Window,
};
use egui::Modal;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::{default, fmt};
use std::{str::FromStr, time::Duration};
use tokio::sync::watch::{self, Receiver};

enum MenuState {
    Main,
    MQTT,
    ELRS,
    Settings,
}

#[derive(Default)]
struct SessionData {}

#[derive(Default)]
pub struct ControllerState {}

#[derive(Default)]
struct ELRSConnection {}

#[derive(Default, Clone, PartialEq)]
struct MQTTServer {
    url: String,
    user: String,
    pw: String,
}

impl fmt::Display for MQTTServer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}@{}", self.user, self.url)
    }
}

#[derive(Default, Clone)]
struct MQTTTopic {
    name: String,
}

impl MQTTTopic {
    fn from_string(name: String) -> Self {
        MQTTTopic { name }
    }
}

#[derive(Default)]
struct MQTTMessage {
    topic: MQTTTopic,
    content: String,
    timestamp: NaiveDateTime,
}

impl MQTTMessage {
    fn from_topic(topic: MQTTTopic, content: String) -> Self {
        MQTTMessage {
            topic,
            content,
            timestamp: chrono::Local::now().naive_local(),
        }
    }
}

#[derive(Default)]
struct MainMenuData {
    current_session_name: String,
    previous_sessions: Vec<SessionData>,
}

#[derive(Default)]
struct MQTTMenuData {
    active_server: MQTTServer,
    saved_servers: Vec<MQTTServer>,
    subscribed_topics: Vec<MQTTTopic>,
    available_topics: Vec<MQTTTopic>,
    message_history: Vec<MQTTMessage>,
    current_message: String,
    received_messages: Vec<MQTTMessage>,
    adding_server: Cell<bool>,
    new_server_url: String,
    new_user: String,
    new_pw: String,
    adding_topic: bool,
    response_trigger: bool,
}

impl MQTTMenuData {
    fn mock_data() -> Self {
        let server = MQTTServer {
            url: "mqtt.testserver.com".to_string(),
            user: "test".to_string(),
            pw: "testpw".to_string(),
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
            adding_topic: false,
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
    current_network: String,
    available_networks: Vec<String>,
    network_pw: String,
    connected: bool,
    display_brightness: f32,
    screensave: usize,
}

pub struct OpencontrollerUI {
    menu_state: MenuState,
    controler_state: Receiver<ControllerState>,
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
        controler_state: Receiver<ControllerState>,
    ) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        OpencontrollerUI {
            menu_state: MenuState::Main,
            controler_state,
            main_menu_data: MainMenuData::default(),
            elrs_menu_data: ELRSMenuData::mock_data(),
            mqtt_menu_data: MQTTMenuData::mock_data(),
            settings_menu_data: SettingsMenuData::default(),
            bat_controller: 0,
            bat_pc: 0,
        }
    }
}

impl OpencontrollerUI {
    fn main_menu(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading("Mainmenu");
                ui.text_edit_singleline(&mut self.main_menu_data.current_session_name);
                if ui.button("Save").clicked() {
                    let current_session =
                        String::from_str("Hier Sessioncreation einfügen").unwrap();
                    //TODO hier noch zu session vektorhinzufügen und abspeichern
                }
            });
            egui::containers::ScrollArea::new([true, false]).show(ui, |ui| {
                for session in &mut self.main_menu_data.previous_sessions {
                    //TODO
                }
                ui.label("dummy");
            });
        });
    }

    fn mqtt_menu(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("MQTT");
            self.server_selection(ui);
        });
    }
    fn topic_selection(&mut self, ui: &mut Ui){
        
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

    fn elrs_menu(&mut self, ui: &mut Ui) {
        let size_erls_menu = ui.available_width();

        ui.horizontal(|ui| {
            ui.heading("ELRS");
            if self.elrs_menu_data.transmitter_connection {
                ui.label("Transmitter connected");
                ui.label("Port Test") //TODO hier funktion einfügen um port auszulesen.
            } else {
                ui.label("No Transmitter found")
            }
        });
        egui::containers::CentralPanel::default().show_inside(ui, |ui| {
            ui.columns(2, |column| {
                let heading = Label::new("Telemetrie").halign(egui::Align::Min);
                //TODO den scan button mit sides einfügen
                column[0].add(heading);

                egui::containers::CentralPanel::default().show_inside(&mut column[0], |col| {
                    col.label("Hier kommt Telemetrie") //TODO anzeige führ die empfangenen telemtrie daten
                });

                column[1].horizontal(|col| {
                    if col.button("Scan").clicked() {
                        //Spawn thread der im Hintergrund nach Verbindungspartnern sucht
                    };
                    egui::ComboBox::from_id_salt("Connections")
                        .selected_text(&self.elrs_menu_data.connection)
                        .show_ui(col, |col| {
                            for con in &mut self.elrs_menu_data.available_connections {
                                col.selectable_value(
                                    &mut self.elrs_menu_data.connection,
                                    con.to_string(),
                                    con.to_string(),
                                );
                            }
                        });
                });
                column[1].horizontal(|col| {
                    if col.button("Live Connect").clicked() {
                        //TODO führt funktion aus die Controllerbefehle in ELRS packets umwandelt und bis auf den off befehl keine events in die Ui injekted
                    }
                    if self.elrs_menu_data.live_connect {
                        col.label("Live On");
                    } else {
                        col.label("Live Off");
                    }
                });
            });
        });
    }
}

impl eframe::App for OpencontrollerUI {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.ctx().request_repaint_after(Duration::from_millis(30));
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
                    ui.label("test_Settings");
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
