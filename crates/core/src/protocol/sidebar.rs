//! The avatar's native sidebar, as values for the client to lay out.

use super::*;

/// A value the engine's sidebar prints, after the panel label it prints before it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarText {
    /// The translated panel label; absent where the panel prints none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub text: String,
    /// Native colour name where the colour carries meaning; empty otherwise.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarLimb {
    pub id: String,
    pub label: SidebarText,
    pub hp: i64,
    pub hp_max: i64,
    pub color: String,
    pub broken: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Focus {
    pub label: String,
    pub value: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Morale {
    pub level: i64,
    pub face: String,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamina {
    pub label: String,
    pub current: i64,
    pub max: i64,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speed {
    pub label: String,
    pub value: i64,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Movement {
    pub counter: i64,
    /// `walk`, `run`, `crouch` or `prone`.
    pub mode: String,
    /// The translated one-letter name of the mode.
    pub mode_text: String,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeMode {
    pub label: String,
    pub enabled: bool,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayTime {
    pub season: String,
    pub day_label: String,
    pub day: i64,
    /// The watch time, else the approximate time of day, else the engine's unknown-time text.
    pub clock: String,
    pub has_watch: bool,
}

/// A direction as the native game names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heading {
    /// The horizontal part (`north`, `northeast`, ..., `center`), then `_above` or `_below`.
    pub id: String,
    /// The native name, in the connection's language.
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SidebarSide {
    Left,
    Right,
}

/// A panel of the native sidebar options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutPanel {
    /// The panel manager's name for it, untranslated.
    pub id: String,
    /// The name the native panel options print, translated.
    pub label: String,
    /// The player has not switched the panel off.
    pub enabled: bool,
    /// Rows; absent for the panel that takes the rest of the column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// The native sidebar options: where it sits, how wide, and its panels top to bottom.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarLayout {
    pub id: String,
    pub side: SidebarSide,
    /// Columns the sidebar takes.
    pub width: u32,
    pub panels: Vec<LayoutPanel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attitude {
    Hostile,
    Friendly,
    Neutral,
    Any,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompassCreature {
    pub id: String,
    pub look: Look,
    pub name: String,
    pub attitude: Attitude,
    /// Squares from the avatar.
    pub distance: u32,
}

/// The creatures the native compass panel lists toward one direction, nearest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompassDirection {
    pub toward: Heading,
    /// The heading the native panel prints for it (`North:`); absent for `center`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub creatures: Vec<CompassCreature>,
}

/// The native classic sidebar of the avatar: values and the labels its panels print. The client lays it out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidebar {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<SidebarLayout>,
    pub limbs: Vec<SidebarLimb>,
    pub pain: SidebarText,
    pub hunger: SidebarText,
    pub thirst: SidebarText,
    pub fatigue: SidebarText,
    pub focus: Focus,
    pub morale: Morale,
    pub stamina: Stamina,
    pub speed: Speed,
    pub movement: Movement,
    pub temperature: SidebarText,
    pub power: SidebarText,
    pub safe_mode: SafeMode,
    pub location: SidebarText,
    pub weather: SidebarText,
    pub time: DayTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ambient_temperature: Option<SidebarText>,
    pub weapon: SidebarText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<SidebarText>,
}
