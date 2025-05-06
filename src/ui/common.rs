use eframe::egui::{self, vec2, Color32, Frame, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Enum für die verschiedenen Menü-Zustände
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuState {
    Main,
    MQTT,
    ELRS,
    Settings,
}

/// MQTT Server Konfiguration, wiederverwendet in verschiedenen Modulen
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct MQTTServer {
    pub url: String,
    pub user: String,
    pub pw: String,
    pub connceted: bool,
}

impl fmt::Display for MQTTServer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}@{}", self.user, self.url)
    }
}

/// WiFi Netzwerk Konfiguration
#[derive(Default, Clone, PartialEq, Eq)]
pub struct WiFiNetwork {
    pub ssid: String,
    pub pw: String,
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

/// Session Daten, die übergreifend gespeichert werden
#[derive(Default)]
pub struct SessionData {
    pub last_session_path: String,
}

/// Gemeinsame UI-Hilfsfunktionen

/// Erstellt einen Rahmen mit konsistenten Stilparametern
pub fn create_frame(ui: &mut egui::Ui, bg_color: Color32, border_color: Color32) -> Frame {
    Frame::new()
        .stroke(Stroke::new(1.0, border_color))
        .fill(bg_color)
        .inner_margin(4)
        .outer_margin(2)
}

/// Standardfarben für UI-Elemente
pub struct UiColors;

impl UiColors {
    pub const MAIN_BG: Color32 = Color32::from_rgb(30, 30, 30);
    pub const INNER_BG: Color32 = Color32::from_rgb(25, 25, 25);
    pub const EXTREME_BG: Color32 = Color32::from_rgb(20, 20, 20);
    pub const BORDER: Color32 = Color32::from_rgb(60, 60, 60);
    pub const ACTIVE: Color32 = Color32::from_rgb(50, 200, 20);
    pub const INACTIVE: Color32 = Color32::from_rgb(200, 50, 20);
}
