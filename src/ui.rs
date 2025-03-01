use std::default;

use crate::config;
use iced::overlay::menu::Menu;
use iced::widget::{button, text};
use iced::widget::{column, container, row};
use iced::{Element, Fill};
use statum::{machine, state};

fn update(machine: &mut UIMachine, message: MenuFunctions) {
    match message {
        MenuFunctions::SwitchToELRS => {
            machine.menu = MenuState::ELRS;
            println!("Switched to ELRS");
        }
        MenuFunctions::SwitchToMain => {
            machine.menu = MenuState::Main;
            println!("Switched to Main");
        }
        _ => println!("Not Implemented"),
    }
}

fn main_view(menu: &UIMachine) -> Element<MenuFunctions> {
    
    match menu.menu {
        MenuState::ELRS => container(column!["SubMenu", button(text("Back to Main".to_string())).on_press(MenuFunctions::SwitchToMain)]).into(),
        MenuState::Main => container(column!["MainMenu", button(text("To ELRS".to_string())).on_press(MenuFunctions::SwitchToELRS)]).into(),
        _ => container(row!["False Input restart"]).into(),
    }
    
}

#[derive(Debug, Clone)]
enum MenuFunctions {
    SwitchToELRS,
    SwitchToMQTT,
    SwitchToSettings,
    SwitchToMain
}

pub enum MenuState {
    Main,
    Settings,
    MQTT,
    ELRS,
}

impl Default for MenuState {
    fn default() -> Self {
        MenuState::Main
    }
}

#[derive(Default)]
struct UIMachine {
    pub menu: MenuState,
}

pub fn run_ui() -> iced::Result {
    let view = main_view;

    iced::run("Tests", update, view)
}
