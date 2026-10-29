//! The 0.1 interaction: the menu a boundary holds while its screen is not described by components.

use super::*;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub description: String,
    pub denial: String,
    /// The key that chooses the row natively; absent when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub enabled: bool,
    pub selectable: bool,
    pub selected: bool,
    pub columns: Vec<Column>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<Editor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_count: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    /// Game color names of the row's text and of its hotkey.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_color: Option<String>,
    /// The row's name with the native color tags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markup: Option<String>,
    /// The registered action the row runs, such as `open_options`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    /// The color of the nesting slash drawn before a nested row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nest_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<Mark>,
}

/// The selection mark the native list prints before a row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    /// `+` all of the row, `#` some of it, `-` none.
    pub glyph: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A value the row can be set to directly with a `fill` on the row's id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Editor {
    /// `bool`, `select`, `integer`, `float` or `text`.
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub values: Vec<EditorValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<serde_json::Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<serde_json::Number>,
    #[serde(default = "no_limit")]
    pub max_length: i64,
    /// A text that may run over several lines.
    #[serde(default)]
    pub multiline: bool,
}

fn no_limit() -> i64 {
    -1
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorValue {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    /// The stable, untranslated name clients look the column up by.
    #[serde(default)]
    pub key: String,
    pub label: String,
    pub value: String,
    /// The game color name of the value, when the native screen tints it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interaction {
    pub schema_id: String,
    pub kind: String,
    pub context: String,
    pub title: String,
    pub message: String,
    /// `good` or `bad` where the native screen colors the message line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_tone: Option<String>,
    /// Whether the native category-selection mode is on, where the screen has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_mode: Option<bool>,
    /// The choice any key press takes, on a press-any-key screen that binds no key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub any_key_choice_id: Option<String>,
    pub allow_cancel: bool,
    pub allow_set_count: bool,
    pub choices: Vec<Choice>,
    pub choice_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compat: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Value>,
    /// Where the engine redraws the screen on its own, the interval in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_ms: Option<u64>,
}
