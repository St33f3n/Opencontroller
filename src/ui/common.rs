//! # UI Common Components and Utilities
//!
//! This module provides shared UI components, state management, and styling utilities
//! for the OpenController application's eframe/egui-based user interface.
//!
//! ## Why This Module Exists
//!
//! The OpenController application uses a "diamond" architecture where the UI serves as the
//! central orchestration point. This common module exists to:
//! - Provide consistent styling and theming across all UI components
//! - Define shared data structures used throughout the UI layer
//! - Centralize UI state management (menu navigation, etc.)
//! - Offer reusable utility functions for frame creation and styling
//!
//! ## Key Abstractions
//!
//! ### Menu State Management
//! The [`MenuState`] enum implements a simple state machine for UI navigation,
//! allowing controlled transitions between different application screens
//! (Main, MQTT, ELRS, Settings).
//!
//! ### Configuration Data Structures
//! Common configuration structures ([`MQTTServer`], [`WiFiNetwork`]) are defined here
//! to ensure consistent data representation across different UI components and
//! backend integration points.
//!
//! ### Theme and Styling System
//! The [`UiColors`] struct provides a centralized dark theme color palette,
//! ensuring visual consistency and making theme changes manageable from a single location.
//!
//! ## Design Rationale
//!
//! This module follows the immediate-mode UI pattern of egui, where UI state is
//! reconstructed each frame. The shared utilities here support this pattern by:
//! - Providing stateless utility functions for consistent styling
//! - Defining lightweight data structures that can be easily cloned
//! - Using compile-time constants for colors to avoid runtime allocation
//!
//! ## Integration with ConfigPortal
//!
//! The data structures defined here are designed to integrate seamlessly with
//! the application's ConfigPortal persistence system, supporting serialization
//! and the session management architecture.

use eframe::egui::{self, vec2, Color32, Frame, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents the current active menu screen in the UI navigation system.
///
/// This enum implements the UI's navigation state machine, ensuring that only
/// valid menu transitions are possible and providing compile-time guarantees
/// about UI state consistency.
///
/// ## Design Rationale
/// Uses a simple enum rather than a complex state machine because UI navigation
/// in OpenController is straightforward - any menu can transition to any other menu
/// without restrictions. This keeps the navigation logic simple while maintaining
/// type safety.
///
/// ## Usage Context
/// Used by the main UI controller to determine which menu component to render
/// and by menu components to trigger navigation events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuState {
    /// Main menu showing session management and overview
    Main,
    /// MQTT debugging and message management interface  
    MQTT,
    /// ExpressLRS RC vehicle control interface
    ELRS,
    /// Application and system settings configuration
    Settings,
}

/// Configuration for MQTT server connections used across UI and backend modules.
///
/// This structure represents MQTT broker connection parameters and is shared
/// between UI components and the MQTT backend handler to ensure consistent
/// configuration management.
///
/// ## Design Rationale
/// Includes a `connected` field for UI state indication rather than deriving
/// connection status from the backend, allowing for immediate UI updates
/// without waiting for backend confirmation.
///
/// ## Serialization
/// Implements serde traits for persistence through the ConfigPortal system,
/// enabling session-based storage of MQTT configurations.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct MQTTServer {
    /// MQTT broker URL (e.g., "mqtt.example.com:1883")
    pub url: String,
    /// Username for MQTT authentication
    pub user: String,
    /// Password for MQTT authentication  
    pub pw: String,
    /// Current connection status for UI indication
    pub connected: bool,
}

impl fmt::Display for MQTTServer {
    /// Formats server for UI display as "user@url".
    ///
    /// Password is intentionally excluded for security when displaying
    /// server information in UI components.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}@{}", self.user, self.url)
    }
}

/// WiFi network configuration for device connectivity settings.
///
/// Used in the Settings menu to manage WiFi connections, particularly important
/// for Raspberry Pi deployments where network configuration affects remote access.
///
/// ## Security Note
/// Password is stored in plain text as this is intended for local device
/// configuration rather than secure credential storage.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct WiFiNetwork {
    /// Network SSID (Service Set Identifier)
    pub ssid: String,
    /// Network password/passphrase
    pub pw: String,
}

impl fmt::Display for WiFiNetwork {
    /// Formats network for UI display showing only SSID.
    ///
    /// Password is excluded from display for basic security.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.ssid)
    }
}

impl WiFiNetwork {
    /// Creates a new WiFi network configuration.
    pub fn new(ssid: String, pw: String) -> Self {
        Self { ssid, pw }
    }
}

/// Session data for maintaining UI state across application sessions.
///
/// Stores session-related information that needs to persist between
/// application runs, supporting the session management system.
#[derive(Default)]
pub struct SessionData {
    /// Path to the last used session configuration
    pub last_session_path: String,
}

/// Creates a styled frame with consistent visual parameters.
///
/// This utility function provides standardized frame creation for UI components,
/// ensuring visual consistency across different menu screens.
///
/// ## Design Rationale  
/// Centralizes frame styling to make theme changes easier and ensure consistent
/// margins, borders, and colors throughout the application.
///
/// # Parameters
/// - `ui`: The egui UI context for creating the frame
/// - `bg_color`: Background color for the frame interior
/// - `border_color`: Color for the frame border
///
/// # Returns
/// A configured [`Frame`] ready for use with egui's `.show()` method.
pub fn create_frame(ui: &mut egui::Ui, bg_color: Color32, border_color: Color32) -> Frame {
    Frame::new()
        .stroke(Stroke::new(1.0, border_color))
        .fill(bg_color)
        .inner_margin(4)
        .outer_margin(2)
}
/// Centralized color palette for the OpenController dark theme.
///
/// Provides compile-time color constants for consistent theming across
/// all UI components. The dark theme is optimized for low-light usage
/// scenarios common in workshop and control environments.
///
/// ## Design Rationale
/// Uses associated constants rather than a color struct to avoid runtime
/// allocation and enable compile-time optimizations. The color values are
/// carefully chosen to provide sufficient contrast while being comfortable
/// for extended use.
///
/// ## Future Migration Path
/// **Note**: This static color system will likely be replaced by the configurable
/// \[`Theme`\] system from the ConfigPortal in future versions. The existing
/// \[`Theme`\] struct in the persistence layer already defines a more flexible
/// color system with:
/// - Serializable color definitions (RGB tuples)
/// - Multiple background color levels
/// - Configurable highlight and frame colors
/// - Integration with the session management system
///
/// The migration would allow users to customize themes through the UI while
/// maintaining the current dark theme as the default configuration.
///
/// ## Color Hierarchy
/// Colors are organized from darkest to lightest background colors, with
/// semantic colors for status indication:
/// - **Background Colors**: EXTREME_BG → INNER_BG → MAIN_BG (darkest to lightest)
/// - **Status Colors**: ACTIVE (green) for connected/enabled states, INACTIVE (red) for disconnected/disabled states
/// - **Structural Colors**: BORDER for component separation
///
/// ## Known Limitations
/// Currently hardcoded and cannot be changed without recompilation. This
/// limitation will be addressed when migrating to the configurable Theme system.
pub struct UiColors;

impl UiColors {
    /// Primary background color for main content areas (RGB: 30, 30, 30)
    pub const MAIN_BG: Color32 = Color32::from_rgb(30, 30, 30);

    /// Secondary background color for nested components (RGB: 25, 25, 25)  
    pub const INNER_BG: Color32 = Color32::from_rgb(25, 25, 25);

    /// Deepest background color for emphasized content areas (RGB: 20, 20, 20)
    pub const EXTREME_BG: Color32 = Color32::from_rgb(20, 20, 20);

    /// Border color for component separation (RGB: 60, 60, 60)
    pub const BORDER: Color32 = Color32::from_rgb(60, 60, 60);

    /// Active/connected status indicator color (RGB: 50, 200, 20) - Green
    pub const ACTIVE: Color32 = Color32::from_rgb(50, 200, 20);

    /// Inactive/disconnected status indicator color (RGB: 200, 50, 20) - Red  
    pub const INACTIVE: Color32 = Color32::from_rgb(200, 50, 20);
}
