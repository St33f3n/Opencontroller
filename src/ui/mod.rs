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

/// Zentrale UI-Komponente, die alle Menü-Daten und den Zustand verwaltet
pub struct OpencontrollerUI {
    menu_state: MenuState,
    event_receiver: mpsc::Receiver<Vec<egui::Event>>,
    main_menu_data: MainMenuData,
    elrs_menu_data: ELRSMenuData,
    mqtt_menu_data: MQTTMenuData,
    settings_menu_data: SettingsMenuData,
    bat_controller: usize,
    bat_pc: usize,
    config_portal: Arc<ConfigPortal>,
    session_sender: mpsc::Sender<SessionAction>,
}

impl OpencontrollerUI {
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

    // Hilfsfunktion um Controller-Events zu loggen
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

            // Top-Panel mit Menü-Buttons
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

            // Zentraler Bereich mit dem aktuellen Menü
            egui::CentralPanel::default().show_inside(ui, |ui| match self.menu_state {
                MenuState::Main => self.main_menu_data.render(ui),
                MenuState::MQTT => self.mqtt_menu_data.render(ui),
                MenuState::ELRS => self.elrs_menu_data.render(ui),
                MenuState::Settings => self.settings_menu_data.render(ui),
            });

            // Bottom-Panel mit Status-Informationen
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
