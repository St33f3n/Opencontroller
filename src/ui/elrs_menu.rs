use eframe::egui::{self, Color32, ComboBox, Frame, Layout, Stroke, Ui, Vec2};

use super::common::UiColors;

/// Datenstruktur für das ELRS-Menü
#[derive(Default)]
pub struct ELRSMenuData {
    transmitter_port: String,
    transmitter_connection: bool,
    connection: String,
    available_connections: Vec<String>,
    live_connect: bool,
}

impl ELRSMenuData {
    /// Erstellt Mock-Daten für die Entwicklung
    pub fn mock_data() -> Self {
        ELRSMenuData {
            transmitter_port: "Port Test 1".to_string(),
            transmitter_connection: true,
            connection: "TestCon".to_string(),
            available_connections: vec!["Test1".to_string(), "Test2".to_string()],
            live_connect: false,
        }
    }

    /// Rendert das ELRS-Menü
    pub fn render(&mut self, ui: &mut Ui) {
        // Header section
        ui.horizontal(|ui| {
            ui.heading("ELRS");
            if self.transmitter_connection {
                ui.label("Transmitter Connected");
                ui.label("Port Test"); // TODO: Function to read port
            } else {
                ui.label("No Transmitter found");
            }
        });

        let available_size = ui.available_size();
        let border_color = UiColors::BORDER;
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
                    .fill(UiColors::INNER_BG)
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

                                ComboBox::from_id_salt("Connections")
                                    .selected_text(&self.connection)
                                    .width(right_width - 70.0) // Breite anpassen
                                    .show_ui(ui, |ui| {
                                        for con in &mut self.available_connections {
                                            ui.selectable_value(
                                                &mut self.connection,
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
                                    self.live_connect = !self.live_connect;
                                }

                                let status = if self.live_connect {
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
}
