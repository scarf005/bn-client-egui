//! Serde types for the unified engine protocol 0.2 (newline-delimited JSON-RPC 2.0).
//! Shapes follow protocol-unification.md rev 2 (methods §1, clock §2, values §3).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: &str = "0.2";
/// The raw-byte SHA-256 prefix of the vendored `schema/0.2.schema.json`, computed by `build.rs`: what a 0.2 hello
/// tells the engine this client was built against.
pub const SCHEMA_HASH: &str = env!("BN_SCHEMA_HASH");

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
    /// Terrain that stands as a wall: a wall, or a closed door or window in a wall's line.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wall: bool,
    /// Terrain that is a window, in a wall's line or not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub window: bool,
    /// How much of a person standing behind it the terrain or furniture covers, in percent; 0 when none.
    #[serde(default, skip_serializing_if = "is_default")]
    pub coverage: u8,
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

/// A hurt creature's health in the fifths and native colour name of the game's look-around bar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub fifths: u8,
    pub color: String,
}

/// The coloured light on a square in sight: its `#rrggbb` and its strength, 1 to 255.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Glow {
    pub color: String,
    pub strength: u8,
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
    pub glow: Option<Glow>,
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
    /// None at full health.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<Health>,
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
    /// The abbreviation the native sidebar prints before the value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_label: Option<String>,
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
    /// The creatures in sight by direction, what the native compass panel lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compass: Option<Vec<CompassDirection>>,
    /// The stacks on the avatar's own square; absent from engines that predate it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<Vec<InventoryItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<Activity>,
}

/// What the avatar is busy with over turns: the engine's id and words for it and, for one of a set length, the
/// thousandths of it done.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<u16>,
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
    pub compass: Option<Vec<CompassDirection>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<Vec<InventoryItem>>,
    /// Never kept: absent means the avatar is not busy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<Activity>,
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
            compass: if kept("compass") {
                before.and_then(|a| a.compass.clone())
            } else {
                self.compass.clone()
            },
            ground: if kept("ground") {
                before.and_then(|a| a.ground.clone())
            } else {
                self.ground.clone()
            },
            activity: self.activity,
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
    /// How much the row holds in native units (charges of a charges item, else items): the most a
    /// `Pickup` or `Drop` of the row takes.
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

/// Overmap squares as the player knows them: `[glyph, foreground, background]` per cell, row-major,
/// with curses color indexes, and what the screen says of the cursor square.
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
    /// The heading of `notes`, localized by the engine.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes_title: String,
    /// The names the native window prints over squares.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<OvermapLabel>,
    /// The active mission's target while the overlays are shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission_target: Option<WorldPos>,
    #[serde(default)]
    pub path: Vec<WorldPos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiles: Option<OvermapTiles>,
}

/// The tile drawn on one overmap square.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OvermapTile<'a> {
    pub id: &'a str,
    pub rotation: u8,
    /// The multitile key, empty for a plain tile.
    pub subtile: &'a str,
    /// Tile ids to try when the tileset has no sprite for `id`, nearest first.
    pub fallbacks: &'a [String],
}

/// The cells of an overmap a view shows: `cols` by `rows` from cell (`left`, `top`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OvermapWindow {
    pub left: usize,
    pub top: usize,
    pub cols: usize,
    pub rows: usize,
}

/// The mission target marked at the edge of a window it lies beyond: the edge cell and the arrow toward it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetMark {
    pub col: usize,
    pub row: usize,
    pub arrow: &'static str,
}

impl OvermapWire {
    /// The cells that fit in `room` (columns, rows), kept around `cursor` and within the overmap; none when no
    /// cell fits.
    pub fn window(&self, cursor: &WorldPos, room: (usize, usize)) -> Option<OvermapWindow> {
        let (cols, rows) = (self.cols.min(room.0), self.rows.min(room.1));
        if cols == 0 || rows == 0 {
            return None;
        }
        let (col, row) = self
            .cell_of(cursor)
            .unwrap_or((self.cols / 2, self.rows / 2));
        let first =
            |at: usize, shown: usize, all: usize| at.saturating_sub(shown / 2).min(all - shown);
        Some(OvermapWindow {
            left: first(col, cols, self.cols),
            top: first(row, rows, self.rows),
            cols,
            rows,
        })
    }

    /// Where the mission target is marked when it lies beyond `window`, as the native overmap marks it; none
    /// without a target or with the target inside.
    pub fn target_mark(&self, window: &OvermapWindow, cursor: &WorldPos) -> Option<TargetMark> {
        let target = self.mission_target.as_ref()?;
        let (dx, dy) = (target.x - self.origin.x, target.y - self.origin.y);
        let (left, top) = (window.left as i32, window.top as i32);
        let (right, bottom) = (left + window.cols as i32 - 1, top + window.rows as i32 - 1);
        if (left..=right).contains(&dx) && (top..=bottom).contains(&dy) {
            return None;
        }
        let arrow = match (
            (target.x - cursor.x).signum(),
            (target.y - cursor.y).signum(),
        ) {
            (0, -1) => "^",
            (1, -1) | (-1, 1) => "/",
            (1, 0) => ">",
            (1, 1) | (-1, -1) => "\\",
            (0, 1) => "v",
            (-1, 0) => "<",
            _ => " ",
        };
        Some(TargetMark {
            col: dx.clamp(left, right) as usize,
            row: dy.clamp(top, bottom) as usize,
            arrow,
        })
    }

    /// The absolute overmap square drawn in cell `(col, row)`.
    pub fn square(&self, col: usize, row: usize) -> WorldPos {
        WorldPos {
            dim: self.origin.dim.clone(),
            x: self.origin.x + col as i32,
            y: self.origin.y + row as i32,
            z: self.origin.z,
        }
    }

    /// The cell `(col, row)` for an absolute square; none outside the cells.
    pub fn cell_of(&self, at: &WorldPos) -> Option<(usize, usize)> {
        let col = usize::try_from(at.x - self.origin.x).ok()?;
        let row = usize::try_from(at.y - self.origin.y).ok()?;
        (at.z == self.origin.z && col < self.cols && row < self.rows).then_some((col, row))
    }

    /// The tile of cell `(col, row)`, when the engine sent tile ids.
    pub fn tile(&self, col: usize, row: usize) -> Option<OvermapTile<'_>> {
        let tiles = self.tiles.as_ref()?;
        let (id, rotation, subtile) = tiles.cells.get(row)?.get(col)?;
        Some(OvermapTile {
            id,
            rotation: *rotation,
            subtile,
            fallbacks: tiles.looks_like.get(id).map_or(&[], Vec::as_slice),
        })
    }

    /// The words of the note on this square, if any, without the colour tags the native list draws.
    pub fn note(&self, at: &WorldPos) -> Option<String> {
        self.notes
            .iter()
            .find(|n| n.pos == *at)
            .map(|n| crate::markup::plain(&n.text))
    }

    /// The status lines the engine says of the cursor square, in the order it says them, without colour tags.
    pub fn status(&self) -> Vec<String> {
        [
            self.cursor_here.as_deref(),
            Some(self.cursor_description.as_str()),
            self.cursor_note.as_deref(),
            self.route_text.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|t| !t.is_empty())
        .map(crate::markup::plain)
        .collect()
    }

    /// Every note in view with its square, for a reader that cannot see the markers.
    pub fn note_lines(&self) -> Vec<String> {
        self.notes
            .iter()
            .map(|n| {
                let words = crate::markup::plain(&n.text);
                format!("({}, {}, {}) {words}", n.pos.x, n.pos.y, n.pos.z)
            })
            .collect()
    }
}

/// Tile ids of the overmap squares, for a client that said it draws tiles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OvermapTiles {
    /// `[tile, rotation, subtile]` per square, row-major like the glyph cells.
    pub cells: Vec<Vec<(String, u8, String)>>,
    pub looks_like: std::collections::BTreeMap<String, Vec<String>>,
}

/// A name printed over an overmap square, in curses color indexes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OvermapLabel {
    pub pos: WorldPos,
    pub text: String,
    pub foreground: u8,
    pub background: u8,
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
    /// The open screens, outermost first; the last takes the input. Empty while the boundary has only the `interaction`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<Screen>,
}

/// A boundary in an event: `actions` is absent when it keeps the previous boundary's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoundaryChange {
    pub boundary_id: String,
    pub readiness: Readiness,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<Action>>,
    pub interaction: Option<Interaction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<Screen>,
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
    /// The engine's localized line for a non-empty `route`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_text: Option<String>,
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
    pub text: Text,
    pub kind: String,
    pub color: String,
    pub count: u32,
    /// The game turn the line was logged on, or last repeated on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<String>,
    /// The line as the native log prints it, with the repeat suffix.
    pub label: Text,
    /// The BN language id of a line that is not in the connection locale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
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
    /// The engine's localized line for `route`; omitted with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_text: Option<String>,
    pub entities: Vec<Entity>,
    /// The newest message log lines, oldest first; omitted when the log is empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<LoggedMessage>,
    /// The catalogue files the connection's LocalisedStrings are worded from, in lookup order; omitted for a
    /// language with none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catalogues: Vec<std::path::PathBuf>,
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
    /// The engine's localized words for `stage`.
    #[serde(default)]
    pub stage_text: String,
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
    /// The schema hash the engine was built with; a 0.2 engine answers with the one the hello named.
    #[serde(
        default,
        deserialize_with = "schema_hash",
        skip_serializing_if = "Option::is_none"
    )]
    pub schema: Option<String>,
    /// The locale this connection receives, and the engine's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

impl Hello {
    /// How many screen-scoped commands the engine holds behind the running one; 1 when it does not say.
    pub fn screen_queue(&self) -> usize {
        self.limits["screen_queue"]
            .as_u64()
            .map_or(1, |n| n.max(1) as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Controller,
    Spectator,
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

/// What a command is conditional on. The form follows the operation: an `invoke` carries the expectation its
/// affordance's scope names, every other operation the 0.1 form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Expect {
    /// Holds while that screen is the top of the stack (`expect: screen`).
    Screen(ScreenExpect),
    /// Holds while the boundary and revision stand (`expect: boundary`).
    Boundary(BoundaryExpect),
    Interaction(InteractionExpect),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenExpect {
    pub screen_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryExpect {
    pub boundary_id: String,
    pub revision: String,
}

/// The 0.1 expectation of the operations that are not an `invoke`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionExpect {
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
    /// Picks up `count` of the `avatar.ground` row `item_id`, in the row's native units.
    Pickup {
        item_id: String,
        count: u64,
    },
    /// Drops `count` of the `avatar.inventory` row `item_id`, in the row's native units.
    Drop {
        item_id: String,
        count: u64,
    },
    /// Takes one of the actions a described map square lists, as its native action does.
    TileAction {
        #[serde(deserialize_with = "opaque_id")]
        action_id: String,
        pos: WorldPos,
    },
    /// Runs an affordance of the top screen, by id.
    Invoke {
        #[serde(deserialize_with = "opaque_id")]
        affordance_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        args: Option<InvokeArgs>,
    },
}

/// The members an affordance's `args` shape names, and no others.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvokeArgs {
    #[serde(
        default,
        deserialize_with = "optional_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub row: Option<String>,
    #[serde(
        default,
        deserialize_with = "some_rows",
        skip_serializing_if = "Option::is_none"
    )]
    pub rows: Option<Vec<RowPick>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<WorldPos>,
    #[serde(
        default,
        deserialize_with = "optional_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub pane: Option<String>,
}

/// One row of a `rows` argument, with how much of it where the shape counts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowPick {
    #[serde(deserialize_with = "opaque_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

/// An id the engine mints: 1 to 256 characters, opaque to the client.
fn opaque_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let id = String::deserialize(d)?;
    match (1..=256).contains(&id.chars().count()) {
        true => Ok(id),
        false => Err(serde::de::Error::custom(
            "expected an id of 1 to 256 characters",
        )),
    }
}

fn optional_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    opaque_id(d).map(Some)
}

fn some_rows<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<RowPick>>, D::Error> {
    match Vec::<RowPick>::deserialize(d)? {
        rows if rows.is_empty() => Err(serde::de::Error::custom("expected at least one row")),
        rows => Ok(Some(rows)),
    }
}

/// `sha256:` and 16 hex digits.
fn schema_hash<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let hash = String::deserialize(d)?;
    let digits = hash.strip_prefix("sha256:").unwrap_or_default();
    let hex = digits
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    match digits.len() == 16 && hex {
        true => Ok(Some(hash)),
        false => Err(serde::de::Error::custom(
            "expected sha256: and 16 hex digits",
        )),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SubmitWire")]
pub struct Submit {
    pub epoch: String,
    pub expect: Expect,
    pub operation: Operation,
}

#[derive(Deserialize)]
struct SubmitWire {
    epoch: String,
    expect: Expect,
    operation: Operation,
}

/// An `invoke` carries a 0.2 expectation and every other operation the 0.1 one.
impl TryFrom<SubmitWire> for Submit {
    type Error = String;

    fn try_from(wire: SubmitWire) -> Result<Self, String> {
        let invoke = matches!(wire.operation, Operation::Invoke { .. });
        let legacy = matches!(wire.expect, Expect::Interaction(_));
        match invoke != legacy {
            true => Ok(Self {
                epoch: wire.epoch,
                expect: wire.expect,
                operation: wire.operation,
            }),
            false => Err("the operation does not take this form of expectation".into()),
        }
    }
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
    /// 0.2: the hash of the schema the client was built with.
    #[serde(
        default,
        deserialize_with = "schema_hash",
        skip_serializing_if = "Option::is_none"
    )]
    pub schema: Option<String>,
    /// 0.2: the BN language id the client wants its text in; the engine's own when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// 0.2: the members above that say what the client can do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Capabilities>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiles: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
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
            schema: Some(SCHEMA_HASH.into()),
            locale: None,
            capabilities: None,
        }
    }

    pub fn with_tint_ids(self, tint_ids: Vec<String>) -> Self {
        Self { tint_ids, ..self }
    }

    pub fn with_tiles(self, tiles: bool) -> Self {
        Self { tiles, ..self }
    }

    /// Asks for the engine's text in a BN language id (`lang/languages.json`).
    pub fn with_locale(self, locale: Option<String>) -> Self {
        Self { locale, ..self }
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
    /// 0.2: the list whose rows are asked for; absent asks for the 0.1 interaction's choices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    pub offset: u64,
    #[serde(deserialize_with = "page_limit")]
    pub limit: u64,
}

/// A page holds 1 to 200 rows.
fn page_limit<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    match u64::deserialize(d)? {
        n if (1..=PAGE_ROWS).contains(&n) => Ok(n),
        _ => Err(serde::de::Error::custom("expected 1 to 200 rows")),
    }
}

/// The most rows the engine sends for a page, and a page of a list holds.
pub const PAGE_ROWS: u64 = 200;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChoicesResult {
    pub boundary_id: String,
    pub total: u64,
    pub choices: Vec<Choice>,
}

/// The rows of a list past its first page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowsResult {
    pub boundary_id: String,
    pub component_id: String,
    pub total: u64,
    pub rows: Vec<Row>,
}

/// The answer of `bn.interaction.choices`: rows of a named list in 0.2, the interaction's choices in 0.1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChoicesReply {
    Rows(RowsResult),
    Choices(ChoicesResult),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldCellsParams {
    pub epoch: String,
    pub min: WorldPos,
    pub max: WorldPos,
}

/// A native screen the client shows in a window of its own while the game goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    Character,
    Inventory,
}

/// `bn.panel.view`: the screen of `panel` with its cursor on `row`, else where the native screen opens it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelParams {
    pub epoch: String,
    pub panel: PanelKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<String>,
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

/// One fact the engine's look view prints of a square.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TilePart {
    pub kind: String,
    /// Markup.
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// What the fact is about, such as a terrain id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Numbers the fact reports, by name.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub values: std::collections::BTreeMap<String, i64>,
}

/// Something the avatar can do with a described square now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileAction {
    /// The native action's id, or `travel`, `attack` or `talk`; opaque to the client.
    pub id: String,
    pub label: Text,
    /// The player's bindings of the native action.
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DescribeResult {
    pub parts: Vec<TilePart>,
    pub actions: Vec<TileAction>,
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
    /// On a hello refused for its `schema`: the hash each side has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaMismatch>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaMismatch {
    pub engine: String,
    pub client: String,
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
        // An error in another shape still ends its request, with the engine's words as the message.
        Some(e) => Err(
            serde_json::from_value(e.clone()).unwrap_or_else(|_| RpcError {
                code: 0,
                message: e.as_str().map_or_else(|| e.to_string(), String::from),
                data: None,
            }),
        ),
        None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
    };
    Ok(Incoming::Response { id, result })
}

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

/// Player-facing text: a string already localized for the connection, or a message key with its arguments
/// for the client to render from the engine's catalogues.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Text {
    Plain(String),
    Localised(LocalisedString),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalisedString {
    /// The msgid. The empty key joins `args`.
    pub key: String,
    /// The gettext context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctx: Option<String>,
    /// The count that selects the plural form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural: Option<serde_json::Number>,
    /// The msgid of the plural form, shown for a count other than one when no catalogue translates the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural_key: Option<String>,
    /// The values of the placeholders, in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<TextArg>>,
    /// What a `<npcname>` in the worded line stands for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npcname: Option<Box<Text>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TextArg {
    Number(serde_json::Number),
    Text(Text),
}

impl Text {
    /// What is shown: a string as it is, a message rendered from the engine's catalogue.
    pub fn shown(&self) -> String {
        match self {
            Text::Plain(text) => text.clone(),
            Text::Localised(message) => crate::localise::render(message),
        }
    }
}

/// The members every component has.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Head {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Text>,
    /// The ids of the screen's affordances that act on this component.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affordances: Vec<String>,
}

/// A generic component, by `kind`: a client draws each without knowing the screen.
// A screen holds a few components, read once per screen: the size of the largest kind does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Component {
    List(List),
    Tabs(Tabs),
    Split(Split),
    Panel(Panel),
    Form(Form),
    Text(TextBlock),
    Grid(Grid),
    Map(MapComponent),
    Canvas(CanvasComponent),
}

impl Component {
    pub fn head(&self) -> &Head {
        match self {
            Component::List(c) => &c.head,
            Component::Tabs(c) => &c.head,
            Component::Split(c) => &c.head,
            Component::Panel(c) => &c.head,
            Component::Form(c) => &c.head,
            Component::Text(c) => &c.head,
            Component::Grid(c) => &c.head,
            Component::Map(c) => &c.head,
            Component::Canvas(c) => &c.head,
        }
    }

    pub fn id(&self) -> &str {
        &self.head().id
    }

    /// This component and every one inside it, each before the ones inside it.
    pub fn nested(&self) -> Vec<&Component> {
        std::iter::once(self)
            .chain(self.children().into_iter().flat_map(Component::nested))
            .collect()
    }

    /// The components directly inside this one.
    pub fn children(&self) -> Vec<&Component> {
        match self {
            Component::Tabs(c) => c.child.iter().map(|child| &**child).collect(),
            Component::Split(c) => c.children.iter().collect(),
            Component::Panel(c) => c.child.iter().map(|child| &**child).collect(),
            _ => vec![],
        }
    }

    /// The streamed world's map in this component or inside it, when the screen puts a cursor on it.
    pub fn cursor_map(&self) -> Option<&MapComponent> {
        match self {
            Component::Map(m) if m.mode == MapMode::World && m.target.is_some() => Some(m),
            _ => self.children().into_iter().find_map(Component::cursor_map),
        }
    }

    /// This component or the one inside it with `id`.
    pub fn find(&self, id: &str) -> Option<&Component> {
        match self.id() == id {
            true => Some(self),
            false => self.children().into_iter().find_map(|c| c.find(id)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    None,
    Single,
    Multi,
    Count,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct List {
    #[serde(flatten)]
    pub head: Head,
    pub columns: Vec<ListColumn>,
    /// The first page, at most 200 rows from `offset`; `bn.interaction.choices` pages the rest.
    pub rows: Vec<Row>,
    pub total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    pub selection: Selection,
    /// The affordance that runs on a row's activation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activate: Option<String>,
    /// The list takes a filter text: its filter affordance is among `affordances`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filterable: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListColumn {
    pub key: String,
    pub label: Text,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: String,
    pub label: Text,
    pub description: Text,
    pub denial: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    pub enabled: bool,
    pub selectable: bool,
    pub selected: bool,
    pub columns: Vec<RowColumn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markup: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nest_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<Mark>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_count: Option<u64>,
    /// How much of the row there is, in native units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u64>,
    /// A heading over the rows that follow: it is not a thing to choose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<bool>,
    /// The row switches something on or off, and this is which it is now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggled: Option<bool>,
    /// The affordances this row allows; absent: all of the list's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affordances: Option<Vec<RowAffordance>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Look>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowColumn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub label: Text,
    pub value: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowAffordance {
    pub id: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tabs {
    #[serde(flatten)]
    pub head: Head,
    pub tabs: Vec<Tab>,
    /// The affordance that selects a tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<Component>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub label: Text,
    pub selected: bool,
    pub enabled: bool,
    /// Why the tab is off, in the engine's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial: Option<Text>,
    /// Something in the tab is new to the player.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unread: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Peers,
    Detail,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Split {
    #[serde(flatten)]
    pub head: Head,
    pub children: Vec<Component>,
    pub relation: Relation,
    /// The child that has the focus, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Good,
    Bad,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    #[serde(flatten)]
    pub head: Head,
    pub title: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<Vec<PanelStat>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<Component>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelStat {
    pub key: String,
    pub label: Text,
    pub value: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Form {
    #[serde(flatten)]
    pub head: Head,
    pub fields: Vec<FormField>,
    /// The affordance that confirms the form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Integer,
    Float,
    Key,
    Bool,
    Select,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormField {
    pub id: String,
    pub label: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Text>,
    /// The current value, in the form `fill` takes.
    pub value: String,
    #[serde(rename = "type")]
    pub kind: FieldKind,
    /// -1: unbounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printable: Option<bool>,
    /// The text may run over several lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiline: Option<bool>,
    /// Where the cursor stands in the text, in characters from its start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<FieldOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Number>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldOption {
    pub id: String,
    pub label: Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextRole {
    TerminalOutput,
    Banner,
}

/// Text with the native colour tags in `markup`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextBlock {
    #[serde(flatten)]
    pub head: Head,
    pub markup: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<Tone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<TextRole>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    #[serde(flatten)]
    pub head: Head,
    pub cols: u32,
    pub rows: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<GridCursor>,
    pub cells: Vec<GridCell>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridCursor {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridCell {
    pub x: u32,
    pub y: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The affordance that takes this cell as its `row`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affordance: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapMode {
    World,
    Overmap,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapComponent {
    #[serde(flatten)]
    pub head: Head,
    pub mode: MapMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overmap: Option<OvermapWire>,
    /// The avatar's item lists the world map shows beside it, in order, also while the map is covered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_lists: Vec<ItemList>,
}

/// Squares a screen highlights on the map besides its cursor: every square, or the rectangle with both corners.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenMark {
    pub role: MarkRole,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub squares: Vec<WorldPos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<Coverage>,
}

/// What a screen's marked squares are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkRole {
    /// The squares the map editor's edits apply to.
    Selection,
    /// The edge of the overmap terrain square mapgen targets.
    OmtBorder,
    /// The rectangle a mapgen preview is generated into, marked at its corners.
    Stamp,
}

/// A list of the avatar's items beside the map, under the engine's heading.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemList {
    pub id: ItemListId,
    pub label: Text,
    /// What moving a row out of the list does.
    pub verb: Text,
}

/// The avatar member that holds an item list's rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemListId {
    #[serde(rename = "avatar.ground")]
    Ground,
    #[serde(rename = "avatar.inventory")]
    Inventory,
}

impl MapComponent {
    /// The square the screen's cursor is on.
    pub fn cursor(&self) -> Option<WorldPos> {
        serde_json::from_value(self.target.as_ref()?["current"].clone()).ok()
    }

    /// The squares the screen highlights besides its cursor; a mark of a role this client does not know is left
    /// out.
    pub fn marks(&self) -> Vec<ScreenMark> {
        let marks = self.target.as_ref().map(|t| &t["marks"]);
        let marks = marks.and_then(Value::as_array).into_iter().flatten();
        marks
            .filter_map(|m| serde_json::from_value(m.clone()).ok())
            .collect()
    }

    /// What the engine says of the cursor's square: the screen's status in words, then each fact it prints of
    /// the square with what more it says of it, as markup lines.
    pub fn cursor_lines(&self) -> Vec<String> {
        let Some(target) = &self.target else {
            return vec![];
        };
        let status = target["status_text"].as_str().map(String::from);
        let parts = target["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| serde_json::from_value::<TilePart>(p.clone()).ok())
            .flat_map(|p| std::iter::once(p.text).chain(p.detail));
        status.into_iter().chain(parts).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasComponent {
    #[serde(flatten)]
    pub head: Head,
    pub cols: usize,
    pub rows: Vec<Vec<(String, u8, u8)>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenRole {
    /// The running game.
    Map,
    /// A place of its own, such as a menu or a manager.
    Screen,
    /// A question over another screen that the player answers and leaves.
    Dialog,
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
