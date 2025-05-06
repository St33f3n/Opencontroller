use eframe::egui::{self, vec2, Color32, Frame, Label, ScrollArea, Stroke, TextEdit, Ui};
use std::str::FromStr;
use tracing::debug;

use super::common::{SessionData, UiColors};

/// Datenstruktur für das Hauptmenü
#[derive(Default)]
pub struct MainMenuData {
    current_session_name: String,
    new_session_name: String,
    previous_sessions: Vec<SessionData>,
}

impl MainMenuData {
    /// Erstellt Mock-Daten für die Entwicklung
    pub fn mock_data() -> Self {
        let old_test_session = SessionData {
            last_session_path: "".to_string(),
        };

        Self {
            current_session_name: "TestSession".to_string(),
            new_session_name: String::new(),
            previous_sessions: vec![old_test_session],
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
                            for (index, _session) in self.previous_sessions.iter().enumerate() {
                                Frame::new()
                                    .stroke(Stroke::new(1.0, border_color))
                                    .inner_margin(2)
                                    .outer_margin(8)
                                    .fill(UiColors::EXTREME_BG)
                                    .show(ui, |ui| {
                                        if ui
                                            .add_sized(
                                                vec2(available_size.x - 20.0, list_height / 6.0),
                                                Label::new(format!("Predev Session {}", index + 1))
                                                    .selectable(true)
                                                    .sense(egui::Sense::click()),
                                            )
                                            .clicked()
                                        {
                                            debug!("Loading Session");
                                            // Hier würde der Code zum Laden der Session kommen
                                            // (wird später manuell ergänzt)
                                        }
                                    });
                                ui.add_space(2.0);
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
}
