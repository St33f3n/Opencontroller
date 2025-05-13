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

pub struct MainMenuData {
    config_portal: Arc<ConfigPortal>,
    session_sender: tokio::sync::mpsc::Sender<SessionAction>,
    current_session_name: String,
    new_session_name: String,
    previous_session: Option<String>,
    session_load_error: Option<String>,
    available_sessions: Vec<String>,
}

impl MainMenuData {
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

    /// Rendert das Hauptmenü
    pub fn render(&mut self, ui: &mut Ui) {
        let available_size = ui.available_size();
        let border_color = UiColors::BORDER;

        ui.vertical(|ui| {
            // Obere Zeile mit Überschrift, Texteingabefeld und Save-Button
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

            // Session-Liste im Stil des message_log
            Frame::new()
                .fill(ui.visuals().extreme_bg_color)
                .inner_margin(6)
                .stroke(Stroke::new(1.0, ui.visuals().widgets.active.bg_fill))
                .show(ui, |ui| {
                    let list_height = available_size.y - 40.0; // Höhe abzüglich des oberen Bereichs
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

                            // Falls keine Sessions vorhanden sind, einen Platzhalter anzeigen
                            if self.available_sessions.is_empty() {
                                ui.label("No saved sessions available");
                            }
                        });
                    });
                });
        });
    }

    fn create_session(&mut self) {
        let session_name = self.new_session_name.clone();

        if session_name.is_empty() {
            self.session_load_error = Some("Session name cannot be empty".to_string());
            return;
        }

        let result = session_action!(@create, self.session_sender, session_name);
        self.list_sessions();
    }

    fn list_sessions(&mut self) {
        let result = session_action!(@list, self.session_sender);

        match result {
            Ok(sessions) => self.available_sessions = sessions.keys().cloned().collect(),
            Err(e) => warn!("Couldn't load available sessions: {}", e),
        }
    }

    fn change_session(&mut self, name: String) {
        self.previous_session = Some(self.current_session_name.clone());
        self.current_session_name = name.clone();

        let result = session_action!(@load, self.session_sender, name);
        self.list_sessions();
    }

    fn delet_session(&mut self, name: String) {
        let result = session_action!(@delete, self.session_sender, name);
        self.list_sessions();
    }
}
