//! A screen of the interaction stack and the affordances it offers.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenRole {
    /// The running game.
    Map,
    /// A place of its own, such as a menu or a manager.
    Screen,
    /// A question over another screen that the player answers and leaves.
    Dialog,
    /// A place that is the whole view, such as the main menu's editors: the running game under it is not shown.
    Workspace,
}

/// The side of the window a list window asked to sit at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenDock {
    Left,
    Right,
    Overlapping,
    Cursor,
}

/// One screen of the interaction stack.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Screen {
    /// One native screen instance: stable while it stays open, new when it is reopened.
    pub screen_id: String,
    /// What screen this is, the same at every opening and in every language; none where the engine does not
    /// name it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub role: ScreenRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<Text>,
    /// None: the screen is not described by components yet; the boundary's `interaction` is.
    pub root: Option<Component>,
    /// What the player can do here; only the top screen has any.
    pub affordances: Vec<Affordance>,
    /// Where the native cursor is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<ScreenFocus>,
    /// The engine redraws the screen on its own this often, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_ms: Option<u64>,
    /// Where the player asked a list window to sit; none when the screen has no such choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dock: Option<ScreenDock>,
    /// For diagnostics and tests; a client never branches on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// The shortcut bar of the map screen, in the order the data gives; empty on every other screen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shortcuts: Vec<Shortcut>,
}

impl Screen {
    /// The components of the screen, each before the ones inside it.
    pub fn components(&self) -> Vec<&Component> {
        self.root.iter().flat_map(Component::nested).collect()
    }
}

/// A button of the map's shortcut bar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    /// The registered action the button runs: the map's affordance of this id, while `enabled`.
    pub affordance_id: String,
    pub label: Text,
    pub keys: Vec<String>,
    /// An opaque sprite id the client may map to its own art.
    pub icon: String,
    /// What to draw when the client has no art for `icon`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenFocus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffordanceRole {
    Confirm,
    Cancel,
    /// Runs on a key no affordance binds.
    AnyKey,
    /// Moves the screen's cursor to the row it is given, which the client invokes when its focus moves.
    Select,
    /// Moves a cursor, page, tab or scroll position; arrows, paging and the wheel already do it.
    Navigate,
    /// Opens the native help or keybindings screen.
    Help,
    /// Types its key, a digit, into a counting list's count; typing the digit already does it.
    Digit,
    /// Only resizes or rearranges the native window.
    Layout,
    /// A key that edits the text of the screen's text field, which the client's own text box takes.
    Edit,
}

/// Which expectation an invoke of the affordance carries; the engine assigns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// The affordance takes no `row`, `rows`, `pos` or `text`; the invoke may queue in the engine.
    Screen,
    /// The engine checks the invoke's row ids against the full list.
    Boundary,
}

/// Something the player can do now, invoked by `id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Affordance {
    pub id: String,
    pub label: Text,
    /// One line on what it does, for a tooltip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Text>,
    /// The player's bindings, as keybinding-file key names.
    pub keys: Vec<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<AffordanceRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<ArgShape>,
    /// A step of the map the player can hold down: `bn.hold` on key down, `bn.release` on key up.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub holdable: bool,
    /// The menu of a workspace screen it belongs in, such as File.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<Text>,
    pub expect: Scope,
}

/// The arguments an invoke must carry: every member present, and no other.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgShape {
    /// One row id of that component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<ComponentRef>,
    /// Several rows of that list, each with a count where `counts`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<RowsShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<CountShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<PosShape>,
    /// A `panel` id among the children of that component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<ComponentRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentRef {
    pub component_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowsShape {
    pub component_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<bool>,
}

/// A number within `min` and `max` where given.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountShape {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<i64>,
}

/// A string for a form field; without `field_id` the field is the one `row` names.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextShape {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_id: Option<String>,
    /// The text the engine suggests, which the box opens with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosUnit {
    Square,
    Overmap,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosShape {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<PosUnit>,
    /// The `map` component whose cursor bounds and range the square must lie within.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
}
