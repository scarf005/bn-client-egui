//! Serde types for the unified engine protocol 0.1 (newline-delimited JSON-RPC 2.0).
//! Shapes follow protocol-unification.md rev 2 (methods §1, clock §2, values §3).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: &str = "0.1";

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Absolute map square; `dim` is `""` for the primary dimension.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldPos {
    pub dim: String,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Stream position: includes every event up to `sequence`. Counters are decimal strings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct At {
    pub epoch: String,
    pub sequence: String,
    pub revision: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Look {
    /// Absent in a cell layer, whose name already says it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    pub id: Option<String>,
    /// The player-facing name, when the engine sends one; otherwise only `id` identifies it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub glyph: String,
    pub color: String,
    /// Sprite id when it differs from `id` (corpses, characters, vehicle parts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile: Option<String>,
    /// The data's fallback sprite ids, nearest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub looks_like: Vec<String>,
    /// Multitile key the engine's tile selection chose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtile: Option<String>,
    /// Quarter turns of the engine's tile selection: 0 north, 1 west, 2 south, 3 east.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<u8>,
    /// `left` or `right`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<String>,
    /// Items on the square when there are several, on the displayed item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<u32>,
    /// Paint of a vehicle part, as `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<Tint>,
    /// Set on the overlay of a mutation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation: Option<LookMutation>,
    /// Set on the overlay of an item, a bionic or an effect: the ids a tileset's tints are keyed by.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tint_keys: Vec<String>,
    /// The tint an item's own variables ask for, over what the tileset's tints give it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint_vars: Option<TintVars>,
}

/// The paint an item's variables ask for in the background and the foreground.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TintVars {
    #[serde(default, skip_serializing_if = "TintVarsLayer::is_empty")]
    pub bg: TintVarsLayer,
    #[serde(default, skip_serializing_if = "TintVarsLayer::is_empty")]
    pub fg: TintVarsLayer,
}

/// One layer of an item's tint variables: a field it does not set keeps the tileset's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TintVarsLayer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contrast: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saturation: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f32>,
}

impl TintVarsLayer {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// What a tileset's `tint_pairs` are keyed by to tint the sprite of a mutation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookMutation {
    pub id: String,
    pub types: Vec<String>,
    pub flags: Vec<String>,
}

/// The ids of a character's mutations by mutation type.
pub type Traits = std::collections::BTreeMap<String, Vec<String>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldLook {
    pub look: Look,
    pub intensity: u32,
    /// Its type hides the items under it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hides_items: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Known {
    Remembered,
    Visible,
    Sensed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub terrain: Look,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<Look>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub at: WorldPos,
    pub known: Known,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terrain: Option<Look>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub furniture: Option<Look>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldLook>,
    /// A corpse here can rise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reviving: bool,
    /// A cargo part of a vehicle here holds items.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cargo: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traps: Vec<Look>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Look>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicle: Option<Look>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<Memory>,
    /// Open air in sight on the avatar's level: how many levels down the square is that the native
    /// tiles view draws here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub below: Option<u8>,
}

impl Cell {
    /// Puts back what the engine leaves out of a layer: the kind its name says and the default
    /// `center` subtile of a shaped layer.
    pub fn complete(&mut self) {
        let shape = |look: &mut Look, kind: &str| {
            if look.kind.is_empty() {
                look.kind = kind.into();
            }
            look.subtile.get_or_insert_with(|| "center".into());
        };
        let layers = [
            (self.terrain.as_mut(), "terrain"),
            (self.furniture.as_mut(), "furniture"),
            (self.vehicle.as_mut(), "vehicle_part"),
        ];
        layers.into_iter().for_each(|(look, kind)| {
            look.into_iter().for_each(|look| shape(look, kind));
        });
        self.traps.iter_mut().for_each(|look| shape(look, "trap"));
        self.fields
            .iter_mut()
            .for_each(|f| shape(&mut f.look, "field"));
        for look in &mut self.items {
            if look.kind.is_empty() {
                look.kind = "item".into();
            }
        }
        if let Some(memory) = &mut self.memory {
            shape(&mut memory.terrain, "terrain");
            if let Some(overlay) = &mut memory.overlay {
                overlay.subtile.get_or_insert_with(|| "center".into());
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub at: WorldPos,
    pub known: Known,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Look>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Sprites drawn over a character, bottom first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlays: Vec<Look>,
    /// The ids of its mutations by mutation type, which tint the mutation overlays.
    #[serde(default, skip_serializing_if = "Traits::is_empty")]
    pub traits: Traits,
    /// `hostile`, `friendly`, `neutral` or `any`: how the creature regards the avatar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attitude: Option<String>,
    /// The creature is aware of the avatar.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub aware: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sense: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stat {
    pub id: String,
    pub label: String,
    pub value: Value,
    /// Native colour name, when the sidebar colours the stat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Avatar {
    pub id: String,
    pub at: WorldPos,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Look>,
    /// Sprites drawn over the avatar, bottom first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlays: Vec<Look>,
    /// The ids of its mutations by mutation type, which tint the mutation overlays.
    #[serde(default, skip_serializing_if = "Traits::is_empty")]
    pub traits: Traits,
    pub stats: Vec<Stat>,
    /// Carried items; optional on the wire (absent and empty both mean nothing carried).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<Vec<InventoryItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<Sidebar>,
    /// The stacks on the avatar's own square; absent from engines that predate it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<Vec<InventoryItem>>,
}

/// An avatar in an event. The members named in `keep` are left out and carry over from the previous
/// avatar; any other absent member is empty.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvatarChange {
    pub id: String,
    pub at: WorldPos,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Look>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlays: Vec<Look>,
    #[serde(default, skip_serializing_if = "Traits::is_empty")]
    pub traits: Traits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<Vec<Stat>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<Vec<InventoryItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<Sidebar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<Vec<InventoryItem>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keep: Vec<String>,
}

impl AvatarChange {
    /// The whole avatar after the event, taking the kept members from `previous`.
    pub fn apply(self, previous: Option<&Avatar>) -> Avatar {
        let kept = |member: &str| self.keep.iter().any(|k| k == member);
        let before = previous.filter(|_| !self.keep.is_empty());
        Avatar {
            look: if kept("look") {
                before.and_then(|a| a.look.clone())
            } else {
                self.look.clone()
            },
            overlays: if kept("overlays") {
                before.map(|a| a.overlays.clone()).unwrap_or_default()
            } else {
                self.overlays.clone()
            },
            traits: if kept("traits") {
                before.map(|a| a.traits.clone()).unwrap_or_default()
            } else {
                self.traits.clone()
            },
            stats: if kept("stats") {
                before.map(|a| a.stats.clone()).unwrap_or_default()
            } else {
                self.stats.clone().unwrap_or_default()
            },
            inventory: if kept("inventory") {
                before.and_then(|a| a.inventory.clone())
            } else {
                self.inventory.clone()
            },
            sidebar: if kept("sidebar") {
                before.and_then(|a| a.sidebar.clone())
            } else {
                self.sidebar.clone()
            },
            ground: if kept("ground") {
                before.and_then(|a| a.ground.clone())
            } else {
                self.ground.clone()
            },
            id: self.id,
            at: self.at,
            name: self.name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InventoryItem {
    /// Names the row for `Pickup` and `Drop`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub look: Look,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    /// `wielded`, `worn` or `carried`; absent for items on the ground.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// The condition marker `name` starts with and the colour the native lists draw it in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durability: Option<Durability>,
    /// The row as the native lists word it, with its count, condition marker and slot.
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Durability {
    pub symbol: String,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub turn: String,
    pub time: String,
    pub weather: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
    /// The files of game data that add sprites to tilesets, in the engine's load order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mod_tilesets: Vec<ModTileset>,
}

/// A file of game data that adds sprites to the tilesets its `compatibility` names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModTileset {
    pub path: String,
    /// The folder the sheet files of the file are named from.
    pub base: String,
    /// Which `mod_tileset` of the file this is, counting from 1.
    pub index: u32,
    pub compatibility: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub min: WorldPos,
    pub max: WorldPos,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Readiness {
    pub phase: String,
    pub game_ready: bool,
    pub accepts_interaction_commands: bool,
    pub accepts_registered_actions: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub name: String,
    /// Keyboard keys the active input context binds to the action; absent from older engines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<String>>,
}

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
    /// Fixed-width lines drawn above the menu: the main menu's title art and version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<Vec<String>>,
    pub allow_cancel: bool,
    pub allow_set_count: bool,
    pub choices: Vec<Choice>,
    /// The advanced inventory's area picker, laid out and keyed as the native header draws it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<Area>,
    pub choice_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compat: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overmap: Option<OvermapWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicle: Option<Vehicle>,
    /// A window the engine draws itself, such as a software mini-game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<CanvasWire>,
    /// Where the engine redraws the screen on its own, the interval in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_ms: Option<u64>,
}

/// One square of the advanced inventory's area picker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Area {
    /// The registered action that points the active pane at the area.
    pub action_id: String,
    pub label: String,
    /// The key the player bound to the action; empty when none.
    pub key: String,
    /// Character cells from the picker's top-left corner.
    pub x: u16,
    pub y: u16,
    /// The screen offers the action.
    pub available: bool,
}

/// A native window as drawn: per row, `[text, foreground, background]` runs of cells that share
/// their colors, with curses color indexes. Blank cells that end a row are left out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasWire {
    pub cols: usize,
    pub rows: Vec<Vec<(String, u8, u8)>>,
}

/// The native vehicle interaction window: the grid around the cursor, the parts on the cursor
/// square, the stat lines, and in a task list the preview of the highlighted part.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vehicle {
    pub name: String,
    pub owner: String,
    pub owned: bool,
    /// `main`, `install`, `repair`, `remove`, `mend`, `refill` or `select`.
    pub mode: String,
    /// What the task does to the highlighted part, such as `Install`; empty on the main screen.
    #[serde(default)]
    pub verb: String,
    pub cursor: VehicleCursor,
    /// The cursor square cannot take a part.
    pub blocked: bool,
    pub cells: Vec<VehicleCell>,
    pub parts: Vec<VehiclePart>,
    /// The native stat lines in reading order; an empty line is a blank slot.
    pub stats: Vec<String>,
    #[serde(default)]
    pub tabs: Option<VehicleTabs>,
    #[serde(default)]
    pub detail: Option<VehicleDetail>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleCursor {
    pub x: i32,
    pub y: i32,
}

/// A square of the grid, relative to the cursor in screen cells (x right, y down).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleCell {
    pub dx: i32,
    pub dy: i32,
    pub glyph: String,
    /// A game color name such as `light_gray`.
    pub color: String,
}

/// One part on the cursor square as the native part list prints it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehiclePart {
    pub name: String,
    pub open: String,
    pub close: String,
    pub health: u8,
    pub broken: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleTabs {
    pub labels: Vec<String>,
    pub selected: usize,
    pub filter: String,
}

/// What the native detail pane shows for the highlighted part.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleDetail {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub time_heading: Option<String>,
    /// The requirement headings in the order the native pane prints them.
    #[serde(default)]
    pub groups: Vec<RequirementGroup>,
    pub stats: Vec<VehicleStat>,
    pub requirements: Vec<VehicleRequirement>,
    /// Every requirement is met, so choosing the part starts the task.
    pub can_do: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleStat {
    pub label: String,
    pub value: String,
}

/// A heading of the requirement list; `kind` is `skill`, `component`, `tool` or `extra`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequirementGroup {
    pub kind: String,
    pub heading: String,
    /// Printed under the heading when no requirement has this kind; empty to leave the group out then.
    pub empty_text: String,
}

/// `kind` is `skill`, `component`, `tool` or `extra`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleRequirement {
    pub kind: String,
    pub text: String,
    pub met: bool,
}

/// The native overmap window as drawn: `[glyph, foreground, background]` per cell, row-major,
/// with curses color indexes, and the sidebar text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OvermapWire {
    pub origin: WorldPos,
    pub player: WorldPos,
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Vec<(String, u8, u8)>>,
    pub legend: Vec<String>,
    /// What the sidebar says of the cursor square, without the window frame or its glyph.
    #[serde(default)]
    pub cursor_description: String,
    /// `You are here.`, localized, while the cursor is on the player's square.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_here: Option<String>,
    /// The note the native window prints for the cursor square, as one localized line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_note: Option<String>,
    /// The length of `path` as one localized line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_text: Option<String>,
    pub notes: Vec<OvermapNote>,
    #[serde(default)]
    pub path: Vec<WorldPos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiles: Option<OvermapTiles>,
}

/// Tile ids of the overmap squares, for a client that said it draws tiles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OvermapTiles {
    /// `[tile, rotation, subtile]` per square, row-major like the glyph cells.
    pub cells: Vec<Vec<(String, u8, String)>>,
    pub looks_like: std::collections::BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OvermapNote {
    pub pos: WorldPos,
    pub text: String,
}

/// Interaction boundary: what the engine is waiting for right now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Boundary {
    pub boundary_id: String,
    pub readiness: Readiness,
    pub actions: Vec<Action>,
    pub interaction: Option<Interaction>,
}

/// A boundary in an event: `actions` is absent when it keeps the previous boundary's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoundaryChange {
    pub boundary_id: String,
    pub readiness: Readiness,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<Action>>,
    pub interaction: Option<Interaction>,
}

/// The only state-bearing part of an event, applied in field order (§3.4).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Changes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
    /// Replaces the clickable view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<Coverage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<Cell>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forgotten: Vec<WorldPos>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<Entity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gone: Vec<Gone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<AvatarChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Environment>,
    /// Replaces the planned route; `[]` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<Vec<WorldPos>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<BoundaryChange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gone {
    pub id: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub sequence: String,
    pub revision: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub changes: Changes,
}

/// One line of the native message log, as `message.logged` events and snapshots carry it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoggedMessage {
    pub id: String,
    pub text: String,
    pub kind: String,
    pub color: String,
    pub count: u32,
    /// The line as the native log prints it, with the repeat suffix.
    pub label: String,
}

/// Result of `bn.subscribe`; `parts` snapshot-part notifications follow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub at: At,
    /// Absent, with `avatar` and `environment`, while no world is loaded (menus).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
    /// The squares of the avatar's level a map click selects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<Coverage>,
    pub interaction: Boundary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Avatar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Environment>,
    /// Planned travel route, nearest square first; omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route: Vec<WorldPos>,
    pub entities: Vec<Entity>,
    /// The newest message log lines, oldest first; omitted when the log is empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<LoggedMessage>,
    pub parts: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotPart {
    pub epoch: String,
    pub at: At,
    pub index: u32,
    pub last: bool,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventsParams {
    pub epoch: String,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandNotice {
    pub epoch: String,
    pub command_id: String,
    pub stage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    /// The engine's localized reason for `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<At>,
}

impl CommandNotice {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.stage.as_str(),
            "completed" | "rejected" | "interrupted"
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resync {
    pub epoch: String,
    pub reason: String,
    pub lost_after: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub version: String,
    pub epoch: String,
    pub engine: Value,
    pub limits: Value,
    /// Present when the client opted in to credit delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryGrant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryGrant {
    pub credits: u16,
    pub held_frames: u64,
    pub held_bytes: u64,
    /// The frame rate cap the hello asked for, echoed; an engine without the cap leaves it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_hz: Option<u16>,
}

/// A count of pushed frames: a window of 1 to 256.
fn frame_window<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    let n = u16::deserialize(d)?;
    match (1..=256).contains(&n) {
        true => Ok(n),
        false => Err(serde::de::Error::custom("expected 1 to 256 frames")),
    }
}

/// A frames-per-second cap: 1 to 240.
fn frame_rate<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u16>, D::Error> {
    match Option::<u16>::deserialize(d)? {
        Some(n) if !(1..=240).contains(&n) => Err(serde::de::Error::custom("expected 1 to 240 Hz")),
        hz => Ok(hz),
    }
}

/// Opts in to credit delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryRequest {
    #[serde(deserialize_with = "frame_window")]
    pub credits: u16,
    /// The most frames per second the engine publishes while an activity runs.
    #[serde(
        default,
        deserialize_with = "frame_rate",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_hz: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditParams {
    #[serde(deserialize_with = "frame_window")]
    pub frames: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditResult {
    pub credits: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expect {
    pub revision: String,
    pub boundary_id: String,
    pub schema_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    Choose {
        choice_id: String,
    },
    Fill {
        field_id: String,
        value: String,
        submit: bool,
    },
    SetCount {
        choice_id: String,
        count: u64,
    },
    SetTarget {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        candidate_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pos: Option<WorldPos>,
    },
    Cancel,
    Action {
        action_id: String,
    },
    /// The right click (SEC_SELECT) on a map square: examine, close a door, pick up or fire.
    Context {
        pos: WorldPos,
    },
    /// The left click of Tiles and curses on a map square: plans a route, a repeat walks it.
    Travel {
        pos: WorldPos,
    },
    /// Picks up `count` items of the `avatar.ground` row `item_id`.
    Pickup {
        item_id: String,
        count: u64,
    },
    /// Drops `count` items of the `avatar.inventory` row `item_id`.
    Drop {
        item_id: String,
        count: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submit {
    pub epoch: String,
    pub expect: Expect,
    pub operation: Operation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloParams {
    pub versions: Vec<String>,
    pub client: ClientInfo,
    /// Map cells the client can show; the engine sizes its terrain window to them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
    /// The client draws tiles, so the engine adds tile ids where it has them (the overmap).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tiles: bool,
    /// The ids of the tints the client's tileset has; the engine publishes `tint_keys` among them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tint_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewport {
    pub cols: u32,
    pub rows: u32,
}

impl HelloParams {
    pub fn new(client: &str) -> Self {
        Self {
            versions: vec![VERSION.into()],
            client: ClientInfo {
                name: client.into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            viewport: None,
            tiles: false,
            tint_ids: vec![],
            delivery: None,
        }
    }

    pub fn with_tint_ids(self, tint_ids: Vec<String>) -> Self {
        Self { tint_ids, ..self }
    }

    pub fn with_tiles(self, tiles: bool) -> Self {
        Self { tiles, ..self }
    }

    /// Asks for credits and a frame rate cap, kept within the 1 to 240 the engine accepts.
    pub fn with_delivery(self, credits: u16, max_hz: Option<u16>) -> Self {
        Self {
            delivery: Some(DeliveryRequest {
                credits,
                max_hz: max_hz.map(|hz| hz.clamp(1, 240)),
            }),
            ..self
        }
    }

    pub fn with_viewport(self, viewport: Viewport) -> Self {
        Self {
            viewport: Some(viewport),
            ..self
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoicesParams {
    pub epoch: String,
    pub boundary_id: String,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChoicesResult {
    pub boundary_id: String,
    pub total: u64,
    pub choices: Vec<Choice>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldCellsParams {
    pub epoch: String,
    pub min: WorldPos,
    pub max: WorldPos,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldCellsResult {
    pub at: At,
    pub cells: Vec<Cell>,
    pub forgotten: Vec<WorldPos>,
}

/// Params of `bn.world.describe`: what the native mouse view prints for a hovered square.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescribeParams {
    pub epoch: String,
    pub boundary_id: String,
    pub pos: WorldPos,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescribeResult {
    pub lines: Vec<String>,
}

/// Params of `bn.command.result`; its result is a `CommandNotice`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultParams {
    pub epoch: String,
    pub command_id: String,
}

/// `error.data` of an application error (code 1000).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationError {
    pub kind: String,
    /// The engine's localized reason for `kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<At>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadingImage {
    /// Relative to the engine's base path (the game root).
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// `bn.loading`: progress of the engine's loading screen, or its end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoadingNotice {
    Progress(LoadingProgress),
    Done(LoadingDone),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadingProgress {
    pub epoch: String,
    pub title: String,
    pub entries: Vec<String>,
    pub index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<LoadingImage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadingDone {
    pub epoch: String,
    pub done: bool,
}

/// One parsed line from the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    Response {
        id: u64,
        result: Result<Value, RpcError>,
    },
    SnapshotPart(SnapshotPart),
    Events(EventsParams),
    Command(CommandNotice),
    Resync(Resync),
    Loading(LoadingNotice),
    /// The engine's heartbeat while an activity runs; it only shows the engine is alive.
    Progress,
    /// Notification this client does not know; ignored.
    Unknown(String),
    /// A line that is not valid protocol JSON.
    Malformed(String),
    /// The engine closed the stream.
    Closed,
}

fn params<T: for<'de> Deserialize<'de>>(msg: &Value) -> Result<T, serde_json::Error> {
    T::deserialize(msg.get("params").cloned().unwrap_or(Value::Null))
}

pub fn parse_line(line: &str) -> Incoming {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return Incoming::Malformed(e.to_string()),
    };
    parse_message(&msg).unwrap_or_else(|e| Incoming::Malformed(e.to_string()))
}

pub fn parse_message(msg: &Value) -> Result<Incoming, serde_json::Error> {
    if let Some(method) = msg.get("method").and_then(Value::as_str) {
        return Ok(match method {
            "bn.snapshot.part" => Incoming::SnapshotPart(params(msg)?),
            "bn.events" => Incoming::Events(params(msg)?),
            "bn.command" => Incoming::Command(params(msg)?),
            "bn.resync" => Incoming::Resync(params(msg)?),
            "bn.loading" => Incoming::Loading(params(msg)?),
            "bn.progress" => Incoming::Progress,
            other => Incoming::Unknown(other.to_string()),
        });
    }
    let id = msg.get("id").and_then(Value::as_u64);
    let Some(id) = id else {
        return Ok(Incoming::Malformed("response without numeric id".into()));
    };
    let result = match msg.get("error") {
        Some(e) => Err(serde_json::from_value(e.clone())?),
        None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
    };
    Ok(Incoming::Response { id, result })
}

/// Text the engine's sidebar prints, in a native colour name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarText {
    pub text: String,
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
pub struct Morale {
    pub level: i64,
    pub face: String,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamina {
    pub current: i64,
    pub max: i64,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speed {
    pub value: i64,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Movement {
    pub counter: i64,
    /// `walk`, `run`, `crouch` or `prone`.
    pub mode: String,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeMode {
    pub enabled: bool,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayTime {
    pub season: String,
    pub day: i64,
    /// The watch time, else the approximate time of day; empty underground.
    pub clock: String,
    pub has_watch: bool,
}

/// The native classic sidebar of the avatar, as the engine reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidebar {
    pub limbs: Vec<SidebarLimb>,
    pub pain: SidebarText,
    pub hunger: SidebarText,
    pub thirst: SidebarText,
    pub fatigue: SidebarText,
    pub focus: i64,
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
    pub ambient_temperature: Option<String>,
    pub weapon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<SidebarText>,
    /// The classic panel as the native sidebar words it, row by row.
    pub rows: Vec<Vec<SidebarText>>,
}
