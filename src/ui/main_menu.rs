use crate::config::{Config, ConfigAction, ConfigClient, ConfigPortal};
use eframe::egui::{self, vec2, Color32, Frame, Label, ScrollArea, Stroke, TextEdit, Ui};
use std::sync::Arc;
use std::time::Duration;
use std::{ops::Deref, str::FromStr};
use tokio::time;
use tracing::{debug, error, info, warn};

use super::common::{SessionData, UiColors};

/// Datenstruktur für das Hauptmenü
pub struct MainMenuData {
    config_portal: Arc<ConfigPortal>,
    config_client: ConfigClient,
    current_session_name: String,
    new_session_name: String,
    previous_sessions: Vec<String>,
    loading_session: bool,
    session_load_error: Option<String>,
}

impl MainMenuData {
    /// Erstellt Mock-Daten für die Entwicklung
    pub fn mock_data(config_portal: Arc<ConfigPortal>, config_client: ConfigClient) -> Self {
        let previous_sessions = Vec::new();
        let mut data = Self {
            config_portal,
            config_client,
            current_session_name: "TestSession".to_string(),
            new_session_name: String::new(),
            previous_sessions,
            loading_session: false,
            session_load_error: None,
        };
        data.loading_sessions();
        data
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
                    self.saving_session();
                    std::thread::sleep(Duration::from_millis(200));
                    self.loading_sessions();
                }
                if ui.button("Load").clicked() {
                    self.loading_sessions();
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
                            for session in self.previous_sessions.clone() {
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
                                            self.load_session(session.clone());
                                        }
                                    });
                            }

                            // Falls keine Sessions vorhanden sind, einen Platzhalter anzeigen
                            if self.previous_sessions.is_empty() {
                                ui.label("No saved sessions available");
                            }
                        });
                    });
                });
        });
    }

    fn saving_session(&mut self) {
        let session_name = self.new_session_name.clone();

        if session_name.is_empty() {
            self.session_load_error = Some("Session name cannot be empty".to_string());
            return;
        }

        let client = self.config_client.clone();

        // Asynchrone Ausführung mit vereinfachtem Zugriff
        self.config_client.execute_async(move |client| {
            Box::pin(async move {
                match client.create_session(session_name).await {
                    Ok(()) => {
                        info!("Session created successfully");
                        // Hier könnte ein Event ausgelöst werden
                    }
                    Err(e) => {
                        error!("Failed to create session: {}", e);
                        // Hier könnte ein Event ausgelöst werden
                    }
                }
            })
        });
    }

    fn loading_sessions(&mut self) {
        let sessions = self.config_portal.session.try_read();
        match sessions {
            Ok(session_guard) => {
                let session = session_guard.deref();
                self.previous_sessions = session.available_sessions.keys().cloned().collect();
                self.current_session_name = session.session_name.clone();
            }
            Err(e) => {
                warn!("Fehler beim Lesen der ConfigPortal: {}", e);
            }
        }
    }

    fn load_session(&mut self, name: String) {
        let client = self.config_client.clone();

        self.config_client.execute_async(move |client| {
            Box::pin(async move {
                match client.load_session(name).await {
                    Ok(()) => {
                        info!("Session loaded successfully");
                        // Hier könnte ein Event ausgelöst werden
                    }
                    Err(e) => {
                        error!("Failed to load session: {}", e);
                        // Hier könnte ein Event ausgelöst werden
                    }
                }
            })
        });
    }
}
