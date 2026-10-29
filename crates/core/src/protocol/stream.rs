//! What the engine sends: the boundary, snapshots, events, notices and responses, and how a line is parsed.

use super::*;
use serde_json::Value;

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
