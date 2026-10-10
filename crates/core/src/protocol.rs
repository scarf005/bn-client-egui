//! Serde types for the unified engine protocol 1.0 (newline-delimited JSON-RPC 2.0).
//! Shapes follow protocol-unification.md rev 2 (methods §1, clock §2, values §3).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: &str = "1.0";

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Look {
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldLook {
    pub look: Look,
    pub intensity: u32,
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
    pub stats: Vec<Stat>,
    /// Carried items; optional on the wire (absent and empty both mean nothing carried).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<Vec<InventoryItem>>,
    /// The stacks on the avatar's own square; absent from engines that predate it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<Vec<InventoryItem>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryItem {
    pub look: Look,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    /// `wielded`, `worn` or `carried`; absent for items on the ground.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub turn: String,
    pub time: String,
    pub weather: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub enabled: bool,
    pub selectable: bool,
    pub selected: bool,
    pub columns: Vec<Column>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<Editor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_count: Option<u64>,
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
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interaction {
    pub schema_id: String,
    pub kind: String,
    pub context: String,
    pub title: String,
    pub message: String,
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
}

/// Interaction boundary: what the engine is waiting for right now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Boundary {
    pub boundary_id: String,
    pub readiness: Readiness,
    pub actions: Vec<Action>,
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
    pub avatar: Option<Avatar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Environment>,
    /// Replaces the planned route; `[]` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<Vec<WorldPos>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<Boundary>,
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
    /// The left click of Tiles and curses on a map square: plans a route, a repeat walks it.
    Travel {
        pos: WorldPos,
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
