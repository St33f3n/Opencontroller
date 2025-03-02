use std::default;

use crate::config;

use iced::advanced::graphics::core::font;
use iced::widget::{button, combo_box, text, Button, Column};
use iced::widget::{column, container, row, scrollable, vertical_space, Row};
use iced::{settings, window, Alignment, Element, Fill, Font, Length, Settings, Size, Theme};
use iced_aw::{drop_down, DropDown};
use statum::{machine, state};

fn update(machine: &mut UIMachine, message: MenuFunctions) {
    let current_situation = (&mut machine.menu, message);

    match current_situation {
        (MenuState::Main(data), MenuFunctions::SwitchToELRS) => {
            machine.menu = MenuState::Elrs(ELRSData::default());
            println!("Switched to ELRS");
        }
        (_, MenuFunctions::SwitchToMain) => {
            machine.menu = MenuState::Main(MainData::default());
            println!("Switched to Main");
        }
        (MenuState::Main(data), MenuFunctions::SwitchToMQTT) => {
            machine.menu = MenuState::Mqtt(MQTTData::default());
            println!("Switched to Mqtt");
        }
        (MenuState::Main(data), MenuFunctions::SwitchToSettings) => {
            machine.menu = MenuState::Settings(SettingData::default());
            println!("Switched to Settings");
        }
        (MenuState::Main(data), MenuFunctions::Close) => {
            panic!("TestClose");
        }
        (MenuState::Mqtt(data), MenuFunctions::Select(idx)) => {
            let mut new_data = data.clone();
            new_data.selected_server_idx = Some(idx);
            new_data.server_expanded = false;
            machine.menu = MenuState::Mqtt(new_data);
        }
        (MenuState::Mqtt(data), MenuFunctions::SetServerActiv) => {
            let mut new_data = data.clone();
            new_data.server_idx = new_data.selected_server_idx;
            machine.menu = MenuState::Mqtt(new_data);
        }
        (MenuState::Mqtt(data), MenuFunctions::SetSubActiv) => {
            let mut new_data = data.clone();
            new_data.server_idx = new_data.selected_server_idx;
            machine.menu = MenuState::Mqtt(new_data);
        }
        (MenuState::Mqtt(data), MenuFunctions::Dismiss) => {
            let mut new_data = data.clone();
            new_data.server_expanded = false;
            new_data.sub_expanded = false;
            machine.menu = MenuState::Mqtt(new_data);
        }
        (MenuState::Mqtt(data), MenuFunctions::ExpandServer) => {
            let mut new_data = data.clone();
            new_data.server_expanded = true;
            machine.menu = MenuState::Mqtt(new_data);
        }
        (MenuState::Mqtt(data), MenuFunctions::ExpandSub) => {
            let mut new_data = data.clone();
            new_data.server_expanded = true;
            machine.menu = MenuState::Mqtt(new_data);
        }
        _ => println!("Not Implemented"),
    }
}

fn view(menu: &UIMachine) -> Element<MenuFunctions> {
    match &menu.menu {
        MenuState::Elrs(data) => elrs_view(menu, data.to_owned()),
        MenuState::Main(data) => main_view(menu, data.to_owned()),
        MenuState::Settings(data) => settings_view(menu, data.to_owned()),
        MenuState::Mqtt(data) => mqtt_view(menu, data.to_owned()),
        _ => container(row!["False Input restart"]).into(),
    }
}

#[derive(Debug, Clone)]
enum MenuFunctions {
    SwitchToELRS,
    SwitchToMQTT,
    SwitchToSettings,
    SwitchToMain,
    ExpandServer,
    ExpandSub,
    Dismiss,
    SetServerActiv,
    SetSubActiv,
    Select(usize),
    Close,
    Save,
    ControllerInput(Controller),
}

#[derive(Debug, Clone)]
enum Controller {}

enum MenuState {
    Main(MainData),
    Settings(SettingData),
    Mqtt(MQTTData),
    Elrs(ELRSData),
}

impl Default for MenuState {
    fn default() -> Self {
        MenuState::Main(MainData::default())
    }
}

#[derive(Default, Debug, Clone)]
struct MainData {
    active_element: MainElement,
}
#[derive(Debug, Clone)]
struct MQTTData {
    active_element: MQTTElement,

    selected_server_idx: Option<usize>,
    server_idx: Option<usize>,
    available_servers: Vec<String>,
    server_expanded: bool,
    connection_status: MQTTStatus,

    selected_sub_idx: Option<usize>,
    subscription: Vec<String>,
    sub_idx: Option<usize>,
    sub_expanded: bool,
}

impl Default for MQTTData {
    fn default() -> Self {
        MQTTData {
            server_idx: Some(0),                               // Hier dein Default-Server
            subscription: vec![String::from("topic/default")], // Standard-Subscription
            sub_idx: Some(0),
            connection_status: MQTTStatus::default(), // Standard-Verbindungsstatus
            active_element: MQTTElement::ServerOption,
            available_servers: vec![
                "mqtt.organiccircuitlab.com".to_string(),
                "test.com".to_string(),
                "192.168.2.150:8123".to_string(),
            ],
            selected_server_idx: None,
            server_expanded: false,
            sub_expanded: false,
            selected_sub_idx: None,
        }
    }
}
#[derive(Default, Debug, Clone)]
struct SettingData {
    test: String,
}
#[derive(Default, Debug, Clone)]
struct ELRSData {
    test: String,
}

struct UIMachine {
    menu: MenuState,
    theme: Theme,
}

#[derive(Debug, Clone)]
enum MQTTElement {
    ServerOption,
    ConnectionTrigger,
    TopicSelector(Vec<String>),
    TopicEditor,
    MessageEditor,
    SavedMessages,
    SendMessage,
    SaveMessage,
}
#[derive(Default, Debug, Clone)]
enum MainElement {
    Close,
    #[default]
    MQTT,
    ELRS,
    Settings,
}

enum ELRSElement {
    ConnectionScan,
    ConnectionSelector(Vec<String>),
    ActiveConnection,
}

enum SettingElement {
    ControllerSettings(ControllerElements),
    NetworkSettings(NetworkElements),
}

enum ControllerElements {
    StartCalibration,
    StartMapping,
}

enum NetworkElements {
    ScanNetwork,
    SelectNetwork(Vec<String>),
}

impl Default for UIMachine {
    fn default() -> Self {
        UIMachine {
            menu: MenuState::Main(MainData::default()),
            theme: Theme::CatppuccinMocha,
        }
    }
}

pub fn run_ui() -> iced::Result {
    let mut font = Font::DEFAULT;
    font.family = font::Family::Name("MonaspiceKr Nerd Font Propo");

    let mut settings = window::Settings::default();
    settings.size = Size::new(800 as f32, 600 as f32);
    settings.min_size = Some(Size::new(740 as f32, 420 as f32));

    iced::application("Tests", update, view)
        .theme(theme_setting)
        .default_font(font)
        .window(settings)
        .run()
}

fn theme_setting(menu: &UIMachine) -> Theme {
    let mut custom_theme = menu.theme.clone();
    custom_theme
}

fn main_view(menu: &UIMachine, data: MainData) -> Element<MenuFunctions> {
    container(column![
        text("MainMenu").height(20).center(),
        row![
            column![
                button(
                    text("ELRS".to_string())
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center)
                )
                .width(Length::FillPortion(50))
                .height(Length::FillPortion(50))
                .padding(20)
                .on_press(MenuFunctions::SwitchToELRS),
                button(text("MQTT".to_string()).center())
                    .width(Length::FillPortion(50))
                    .height(Length::FillPortion(50))
                    .padding(20)
                    .on_press(MenuFunctions::SwitchToMQTT)
            ],
            column![
                button(text("Settings".to_string()).center())
                    .width(Length::FillPortion(50))
                    .height(Length::FillPortion(50))
                    .padding(20)
                    .on_press(MenuFunctions::SwitchToSettings),
                button(text("Close".to_string()).center())
                    .width(Length::FillPortion(50))
                    .height(Length::FillPortion(50))
                    .padding(20)
                    .on_press(MenuFunctions::Close)
            ]
        ]
    ])
    .into()
}

fn elrs_view(menu: &UIMachine, data: ELRSData) -> Element<MenuFunctions> {
    container(column![
        "ELRS",
        button(text("Back to Main".to_string())).on_press(MenuFunctions::SwitchToMain)
    ])
    .into()
}

fn settings_view(menu: &UIMachine, data: SettingData) -> Element<MenuFunctions> {
    container(column![
        "Settings",
        button(text("Back to Main".to_string())).on_press(MenuFunctions::SwitchToMain)
    ])
    .into()
}

fn mqtt_view(menu: &UIMachine, data: MQTTData) -> Element<MenuFunctions> {
    let subs = &data.subscription;
    let connection = &data.connection_status;
    let servers = data.available_servers;
    let activator_options = vec!["Connect 󱘖".to_string(), "Connect ".to_string()];
    let server = match &data.server_idx {
        Some(idx) => format!("Selected Server:\n{}", servers[*idx]),
        None => String::from("No Server selected"),
    };

    let (drop_down, set_active_button) = dropdown_wiget(
        &server,
        &servers,
        data.server_expanded,
        data.selected_server_idx,
        &activator_options,
    );

    container(column![
        row![
            button("󰌑")
                .on_press(MenuFunctions::SwitchToMain)
                .width(Length::FillPortion(5)),
            text("MQTT-Menu").width(Length::FillPortion(95))
        ],
        scrollable(row![
            container(drop_down)
                .padding(10)
                .width(Length::FillPortion(65))
                .align_x(Alignment::End),
            column![set_active_button, text(connection.to_string())]
                .align_x(Alignment::Start)
                .width(Length::FillPortion(35))
        ])
        .width(Length::Fill)
        .anchor_left()
    ])
    .width(Length::Fill)
    .into()
}

#[derive(Default, Debug, Clone)]
enum MQTTStatus {
    #[default]
    disconnected,
    connected,
    failure,
}

impl std::fmt::Display for MQTTStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::disconnected => "disconnected",
            Self::connected => "conncected",
            Self::failure => "failure",
        })
    }
}

fn dropdown_wiget<'ui>(
    selected: &String,
    available_options: &Vec<String>,
    expanded: bool,
    selected_idx: Option<usize>,
    activator_text: &Vec<String>,
) -> (DropDown<'ui, MenuFunctions>, Button<'ui, MenuFunctions>) {
    let underlay: Row<'_, MenuFunctions> = Row::new()
        .spacing(10)
        .push(text(selected.clone()))
        .push(button(text("󰁊")).on_press(MenuFunctions::ExpandServer));

    let mut overlay_children: Vec<Element<'_, MenuFunctions>> = Vec::new();
    for (idx, available_option) in available_options.iter().enumerate() {
        let row = Row::new()
            .spacing(10)
            .align_y(Alignment::Center)
            .push(container(text(available_option.clone())))
            .push(
                button(text("Select"))
                    .on_press(MenuFunctions::Select(idx))
                    .padding(10),
            );

        overlay_children.push(row.into());
    }

    let drop_down = DropDown::new(
        underlay,
        scrollable(
            Column::with_children(overlay_children)
                .max_width(600)
                .clip(false)
                .align_x(Alignment::End),
        )
        .spacing(5)
        .height(60)
        .anchor_right(),
        expanded,
    )
    .width(Length::Fill)
    .alignment(drop_down::Alignment::Bottom)
    .on_dismiss(MenuFunctions::Dismiss);

    let set_active_button: Button<'_, MenuFunctions> = if selected_idx.is_some() {
        Button::new(text(activator_text[0].clone())).on_press(MenuFunctions::SetServerActiv)
    } else {
        Button::new(text(activator_text[1].clone()))
    };

    (drop_down, set_active_button)
}
