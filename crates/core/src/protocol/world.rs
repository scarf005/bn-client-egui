//! The streamed world: squares and their looks, creatures, the avatar and its surroundings.

use super::*;
use serde_json::Value;

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
    /// What terrain, furniture or a field stands as, from its game data.
    #[serde(default, skip_serializing_if = "is_default")]
    pub form: LookForm,
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

/// What terrain, furniture or a field stands as, which a 3D view builds it in; `None` for anything else, also for
/// a form a newer engine names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookForm {
    Door,
    Fence,
    Plant,
    Seat,
    Table,
    Bed,
    Fire,
    Gas,
    #[default]
    #[serde(other)]
    None,
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
    /// The square is not outside: a roof or a floor above covers it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub roofed: bool,
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
    /// The settings `SetMode` sets, each with the value it holds now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modes: Vec<Mode>,
    /// The creatures in sight by direction, what the native compass panel lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compass: Option<Vec<CompassDirection>>,
    /// The stacks on the avatar's own square; absent from engines that predate it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<Vec<InventoryItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<Activity>,
}

/// A setting that native keys toggle or cycle, with the value it holds and the values it takes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mode {
    pub id: String,
    pub label: String,
    /// The `id` of the value it holds now.
    pub value: String,
    pub values: Vec<ModeValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeValue {
    pub id: String,
    pub label: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modes: Vec<Mode>,
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
            modes: if kept("modes") {
                before.map(|a| a.modes.clone()).unwrap_or_default()
            } else {
                self.modes.clone()
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
    /// The sky over the avatar; absent from older engines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sky: Option<Sky>,
}

/// The sky over the avatar: how much daylight there is, where the sun or the moon is and what the weather lets fall.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sky {
    /// The daylight now, in percent of full day.
    pub daylight: u8,
    pub precipitation: Precipitation,
    /// What falls is rain, not snow.
    #[serde(default)]
    pub rain: bool,
    /// How much ambient light the weather takes away, in the game's light points.
    #[serde(default)]
    pub dimming: u32,
    /// How far the sun has crossed the sky from sunrise in the east to sunset in the west, in percent; none at night.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sun: Option<u8>,
    /// How far the moon has crossed the night sky from sunset to sunrise, in percent; none by day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moon: Option<u8>,
    #[serde(default)]
    pub moon_phase: MoonPhase,
}

/// The moon's phase, in the order it goes through them after `New`; `New` also for a phase a newer engine names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoonPhase {
    WaxingCrescent,
    HalfMoonWaxing,
    WaxingGibbous,
    Full,
    WaningGibbous,
    HalfMoonWaning,
    WaningCrescent,
    #[default]
    #[serde(other)]
    New,
}

/// How much falls from the sky; `None` also for an amount a newer engine names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Precipitation {
    VeryLight,
    Light,
    Medium,
    Heavy,
    #[default]
    #[serde(other)]
    None,
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
