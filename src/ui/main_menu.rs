use crate::config::{Config, ConfigAction, ConfigPortal};
use eframe::egui::{self, vec2, Color32, Frame, Label, ScrollArea, Stroke, TextEdit, Ui};
use std::sync::Arc;
use std::{ops::Deref, str::FromStr};
use tracing::{debug, error, info, warn};

use super::common::{SessionData, UiColors};

/// Datenstruktur für das Hauptmenü
pub struct MainMenuData {
    config_portal: Arc<ConfigPortal>,
    config_action_sender: tokio::sync::mpsc::Sender<ConfigAction>,
    current_session_name: String,
    new_session_name: String,
    previous_sessions: Vec<String>,
    loading_session: bool,
    session_load_error: Option<String>,
}

impl MainMenuData {
    /// Erstellt Mock-Daten für die Entwicklung
    pub fn mock_data(
        config_portal: Arc<ConfigPortal>,
        config_action_sender: tokio::sync::mpsc::Sender<ConfigAction>,
    ) -> Self {
        let previous_sessions = Vec::new();
        let mut data = Self {
            config_portal,
            config_action_sender,
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
        // Erstelle einen oneshot-Kanal für die Antwort
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Klone Werte für den Closure
        let session_name = self.new_session_name.clone();
        let sender = self.config_action_sender.clone();

        // Sende die Aktion an den Worker
        if session_name.is_empty() {
            self.session_load_error = Some("Session name cannot be empty".to_string());
        } else {
            self.loading_session = true;

            // Verwende tokio::spawn, um den Empfang asynchron zu verarbeiten
            tokio::spawn(async move {
                if let Err(e) = sender
                    .send(ConfigAction::CreateSession {
                        name: session_name,
                        response_tx: tx,
                    })
                    .await
                {
                    error!("Failed to send create session request: {}", e);
                }

                // Warte auf die Antwort
                match rx.await {
                    Ok(Ok(())) => {
                        info!("Session created successfully");
                        // Hier könnte ein Event ausgelöst werden, um die UI zu aktualisieren
                    }
                    Ok(Err(e)) => {
                        error!("Failed to create session: {}", e);
                        // Hier könnte ein Event ausgelöst werden, um die UI zu aktualisieren
                    }
                    Err(e) => {
                        error!("Failed to receive response: {}", e);
                        // Hier könnte ein Event ausgelöst werden, um die UI zu aktualisieren
                    }
                }
            });
        }
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
        let (tx, rx) = tokio::sync::oneshot::channel();

        let sender = self.config_action_sender.clone();

        // Verwende tokio::spawn, um den Empfang asynchron zu verarbeiten
        tokio::spawn(async move {
            if let Err(e) = sender
                .send(ConfigAction::LoadSession {
                    name,
                    response_tx: tx,
                })
                .await
            {
                error!("Failed to send create session request: {}", e);
            }

            // Warte auf die Antwort
            match rx.await {
                Ok(Ok(())) => {
                    info!("Session loaded successfully");
                    // Hier könnte ein Event ausgelöst werden, um die UI zu aktualisieren
                }
                Ok(Err(e)) => {
                    error!("Failed to load session: {}", e);
                    // Hier könnte ein Event ausgelöst werden, um die UI zu aktualisieren
                }
                Err(e) => {
                    error!("Failed to receive response: {}", e);
                    // Hier könnte ein Event ausgelöst werden, um die UI zu aktualisieren
                }
            }
        });
    }
}
