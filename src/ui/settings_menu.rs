use eframe::egui::{self, Color32, DragValue, Frame, Slider, Stroke, TextEdit, Ui};

use super::common::{UiColors, WiFiNetwork};

/// Datenstruktur für das Einstellungsmenü
#[derive(Default)]
pub struct SettingsMenuData {
    current_network: WiFiNetwork,
    selected_network: WiFiNetwork,
    available_networks: Vec<WiFiNetwork>,
    network_pw: String,
    connected: bool,
    display_brightness: f32,
    screensave: usize,
}

impl SettingsMenuData {
    /// Erstellt Mock-Daten für die Entwicklung
    pub fn mock_data() -> Self {
        let active_net = WiFiNetwork::new("NetActive".to_string(), "ddd".to_string());
        let networks = vec![
            WiFiNetwork::new("Test1".to_string(), "123".to_string()),
            WiFiNetwork::new("Test2".to_string(), "321".to_string()),
        ];
        Self {
            current_network: active_net.clone(),
            selected_network: active_net,
            available_networks: networks,
            network_pw: String::new(),
            connected: false,
            display_brightness: 0.7,
            screensave: 300,
        }
    }

    /// Getter für den aktuellen Verbindungsstatus
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Getter für den Namen des aktuellen Netzwerks
    pub fn get_network_name(&self) -> String {
        self.current_network.ssid.clone()
    }

    /// Rendert das Einstellungsmenü
    pub fn render(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.heading("Settings");

            // Abstand zwischen Sektionen
            let section_spacing = 5.0;

            // WLAN-Sektion
            self.render_wlan_section(ui);

            ui.add_space(section_spacing);

            // Display-Sektion
            self.render_display_section(ui);
        });
    }

    /// Rendert den WLAN-Bereich der Einstellungen
    fn render_wlan_section(&mut self, ui: &mut Ui) {
        Frame::new()
            .stroke(Stroke::new(1.0, UiColors::BORDER))
            .fill(UiColors::MAIN_BG)
            .inner_margin(8.0)
            .outer_margin(2.0)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.heading("WLAN");

                    // Aktuelles Netzwerk als Label
                    ui.horizontal(|ui| {
                        ui.label("Current Network:");
                        ui.label(self.current_network.to_string());
                    });

                    // Verfügbare Netzwerke, Passwort und Connect-Button in einer Zeile
                    ui.horizontal(|ui| {
                        let total_width = ui.available_width() - 30.0;
                        let network_width = total_width * 0.45;
                        let password_width = total_width * 0.45;
                        let connect_width = total_width * 0.1;

                        // Verfügbare Netzwerke - Dropdown-Menü
                        ui.vertical(|ui| {
                            let networkname = self.selected_network.to_string();
                            ui.set_min_width(network_width);
                            egui::ComboBox::from_id_salt("available_networks")
                                .selected_text(if networkname.is_empty() {
                                    "Available Networks"
                                } else {
                                    networkname.as_str()
                                })
                                .width(network_width - 10.0)
                                .show_ui(ui, |ui| {
                                    for network in &self.available_networks {
                                        ui.selectable_value(
                                            &mut self.selected_network,
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
                                TextEdit::singleline(&mut self.network_pw)
                                    .password(true)
                                    .hint_text("Enter Password"),
                            );
                        });

                        // Connect-Button
                        ui.vertical(|ui| {
                            ui.set_min_width(connect_width);

                            if ui.button("Connect").clicked() {
                                //TODO Send MSG to try and connect
                                // Das simulieren wir hier für die Ansicht
                                self.connected = true;
                                self.current_network = self.selected_network.clone();
                            }
                        });
                    });
                });
            });
    }

    /// Rendert den Display-Bereich der Einstellungen
    fn render_display_section(&mut self, ui: &mut Ui) {
        Frame::new()
            .stroke(Stroke::new(1.0, UiColors::BORDER))
            .fill(UiColors::MAIN_BG)
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
                        ui.add(Slider::new(&mut self.display_brightness, 0.0..=1.0));
                    });

                    // Screensaver-Timeout
                    ui.horizontal(|ui| {
                        ui.label("Screensaver (seconds):");
                        ui.add(
                            DragValue::new(&mut self.screensave)
                                .speed(1)
                                .range(0..=3600),
                        );
                    });
                });
            });
    }
}
