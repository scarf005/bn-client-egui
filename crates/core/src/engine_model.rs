//! `GameModel` backed by a live engine connection.

use crate::clock::Clock;
use crate::colors::ColorMap;
use crate::keys;
use crate::markup::{self, Run};
use crate::model::{
    ActionEntry, Cell, CommandStatus, Effect, Failure, FailureKind, GameModel, Layer, Loading,
    LogLine, Menu, MenuChoice, MenuField, MenuPane, Mutation, Overmap, OvermapCell, OvermapTile,
    PaneTotals, Pos, Rgb, Shape,
};
use crate::protocol::{
    self, Cell as WorldCell, Entity, Expect, Incoming, Known, LoadingNotice, Look, ModTileset,
    Operation, Snapshot, Submit, Traits, WorldPos,
};
use crate::replica::{Outcome, Replica};
use crate::transport::{Transport, request};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The map window asked for until the UI reports its own size.
const VIEW: Pos = Pos { x: 40, y: 25 };
/// The window re-centres when the avatar gets this close to its edge (engines that report no view).
const MARGIN: i32 = 3;
/// A resized panel is sent to the engine once it has stayed this size for this long.
/// The engine takes up to this many cells on a side.
const MAX_VIEWPORT: i32 = 512;
const RESIZE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Clone, Copy)]
enum Pending {
    Hello,
    Subscribe,
    Submit,
    Viewport,
    Interrupt,
    /// A hello that only changes whether the engine sends tile ids.
    Retile,
    Describe,
    Choices,
}

/// The choices past the first page of the interaction with this boundary and schema.
#[derive(Default)]
struct Paged {
    key: (String, String),
    more: Vec<protocol::Choice>,
    /// A page was asked for and has not arrived.
    waiting: bool,
}

/// The engine sends at most this many choices per request.
const PAGE: u64 = 200;

/// Starts the engine again on the same profile.
pub type Respawn = Box<dyn FnMut() -> std::io::Result<Box<dyn Transport>>>;

/// Without a message for this long while a request is outstanding, the engine counts as stalled.
const STALL: Duration = Duration::from_secs(120);

pub struct EngineModel {
    /// Whether the client draws tiles now, and what the engine was last told.
    tiles: bool,
    tiles_told: bool,
    /// The ids of the tints the client's tileset has, and what the engine was last told.
    tint_ids: Vec<String>,
    tints_told: Vec<String>,
    /// The engine's `USE_TILES` and `TILES` options from its hello.
    display: Option<(bool, String)>,
    /// The earliest time to tell the engine again after it refused.
    tiles_retry: Duration,
    transport: Box<dyn Transport>,
    respawn: Option<Respawn>,
    waker: Option<crate::transport::Waker>,
    failure: Option<Failure>,
    /// When the engine last said anything, on the model's clock.
    heard: Duration,
    stall: Duration,
    replica: Replica,
    next_id: u64,
    pending: HashMap<u64, Pending>,
    /// A `bn.subscribe` is outstanding; events from the old stream are stale.
    subscribing: bool,
    connected: bool,
    /// A submitted command has not reached a terminal stage yet.
    busy: bool,
    command_id: Option<String>,
    rejected: bool,
    /// The command in flight was a walking step; only its failure stops the walk.
    walking: bool,
    origin: Option<(i32, i32)>,
    /// Map cells the UI can show; the engine is told, and the shown map never exceeds them.
    wanted: Pos,
    /// The size the engine was last told, and when the UI last asked for another one.
    declared: Pos,
    resized: Option<Duration>,
    clock: Clock,
    messages: Vec<String>,
    native: Vec<LogLine>,
    ground: Vec<String>,
    inventory: Vec<String>,
    /// The engine's own names for the same rows; the native menus list items by these.
    ground_names: Vec<String>,
    /// Pages of a long menu fetched after the first, for the interaction they belong to.
    paged: Paged,
    /// The tile each running explosion is drawn with, from its `explosion.started`.
    explosion_tiles: HashMap<String, String>,
    inventory_names: Vec<String>,
    trace: Vec<(Instant, &'static str)>,
    received: u64,
    colors: ColorMap,
    loading: Option<Loading>,
    /// Actions on show: between auto-move steps the engine offers none, which must not flicker the list.
    shown: Vec<ActionEntry>,
    status: Option<CommandStatus>,
    /// An item picked or dropped from the panels: the native menu opens, this finishes it.
    chore: Option<Chore>,
    /// The main menu was showing at the last poll; coming back to it ends the game whose lines are kept.
    at_main_menu: bool,
    /// The square whose description was asked for or received, the revision it is valid for and its lines.
    hovered: Option<Hover>,
}

struct Hover {
    pos: WorldPos,
    revision: String,
    /// None while the answer is outstanding.
    lines: Option<Vec<String>>,
}

/// One panel button press, carried through the native selection menu it opens.
struct Chore {
    /// The item as the panel shows it.
    name: String,
    /// The menu's choice was marked; the next step confirms.
    marked: bool,
}

fn look_name(l: &Look) -> String {
    (l.name.clone().or_else(|| l.id.clone())).unwrap_or_else(|| l.glyph.clone())
}

fn glyph(l: &Look) -> char {
    l.glyph.chars().next().unwrap_or('?')
}

/// The topmost thing drawn in a cell: item, field, furniture, terrain, then what memory holds
/// (its furniture or other overlay over its terrain).
fn top_look(c: &WorldCell) -> Option<&Look> {
    c.vehicle
        .as_ref()
        .or(c.items.last())
        .or(c.fields.last().map(|f| &f.look))
        .or(c.furniture.as_ref())
        .or(c.terrain.as_ref())
        .or(c
            .memory
            .as_ref()
            .map(|m| m.overlay.as_ref().unwrap_or(&m.terrain)))
}

/// One channel of a look's paint.
fn paint(look: &Look, pick: impl Fn(&protocol::Tint) -> Option<&str>) -> Option<Rgb> {
    let [r, g, b] = parse_hex(pick(look.tint.as_ref()?)?)?;
    Some(Rgb(r, g, b))
}

/// `#rrggbb` to its channels.
fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#').filter(|d| d.len() == 6)?;
    let channel = |i: usize| u8::from_str_radix(digits.get(i..i + 2)?, 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// How many levels down open air is looked through.
const MAX_DEPTH: u8 = 10;

/// The game's per-tile seed for weighted sprite variants: the square's coordinates mixed into an integer.
fn square_seed(p: &WorldPos) -> u32 {
    (p.x as u32).wrapping_add((p.y as u32).wrapping_mul(65536))
}

/// A stable seed for a creature; the game seeds monsters by address and characters by name or id.
fn id_seed(id: &str) -> u32 {
    id.bytes()
        .fold(2166136261, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619))
}

/// Native registered-action id for a one-square move.
fn direction(dx: i32, dy: i32) -> Option<&'static str> {
    Some(match (dx, dy) {
        (0, -1) => "UP",
        (1, -1) => "RIGHTUP",
        (1, 0) => "RIGHT",
        (1, 1) => "RIGHTDOWN",
        (0, 1) => "DOWN",
        (-1, 1) => "LEFTDOWN",
        (-1, 0) => "LEFT",
        (-1, -1) => "LEFTUP",
        _ => return None,
    })
}

impl EngineModel {
    /// The fetched pages of this interaction, when they belong to it.
    fn more_of(&self, i: &protocol::Interaction) -> &[protocol::Choice] {
        let b = self.replica.boundary();
        match b {
            Some(b) if self.paged.key == (b.boundary_id.clone(), i.schema_id.clone()) => {
                &self.paged.more
            }
            _ => &[],
        }
    }

    /// A published square in view coordinates; none on another level than the avatar's.
    fn view_of(&self, square: &Value) -> Option<Pos> {
        let (ox, oy) = self.origin?;
        let at = |k: &str| square[k].as_i64().map(|v| v as i32);
        (at("z")? == self.avatar_pos()?.z).then(|| Pos {
            x: at("x").unwrap_or_default() - ox,
            y: at("y").unwrap_or_default() - oy,
        })
    }

    fn effect_of(&self, fact: &crate::replica::Fact) -> Option<Effect> {
        let d = &fact.data;
        let squares = |key: &str| -> Vec<Pos> {
            d[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| self.view_of(s))
                .collect()
        };
        let color = |name: &Value| self.colors.resolve(name.as_str().unwrap_or("white")).0;
        let shape = match fact.kind.as_str() {
            "projectile.moved" => Shape::Projectile {
                path: squares("path"),
                glyph: d["look"]["glyph"].as_str()?.chars().next().unwrap_or('*'),
                sprite: d["look"]["id"].as_str().map(str::to_string),
                color: color(&d["look"]["color"]),
            },
            "explosion.started" => Shape::Burst {
                at: self.view_of(&d["at"])?,
                radius: d["radius"].as_u64().unwrap_or(0) as u32,
                color: color(&d["color"]),
                tile: d["tile"].as_str().unwrap_or("explosion").to_string(),
            },
            "explosion.blast" | "explosion.shrapnel" => Shape::Blast {
                cells: squares("cells"),
                shrapnel: fact.kind == "explosion.shrapnel",
                tile: self
                    .explosion_tiles
                    .get(d["explosion"].as_str().unwrap_or_default())
                    .cloned()
                    .unwrap_or_else(|| "explosion".into()),
            },
            "combat_text.shown" => Shape::Text {
                at: self.view_of(&d["at"])?,
                segments: d["segments"]
                    .as_array()?
                    .iter()
                    .map(|s| {
                        (
                            s["text"].as_str().unwrap_or_default().to_string(),
                            color(&s["color"]),
                        )
                    })
                    .collect(),
                scroll: Pos {
                    x: d["scroll"]["x"].as_i64().unwrap_or(0) as i32,
                    y: d["scroll"]["y"].as_i64().unwrap_or(-1) as i32,
                },
                hp: d["kind"] == "hp",
            },
            _ => return None,
        };
        Some(Effect {
            shape,
            duration_ms: fact.duration_ms,
        })
    }

    pub fn new(transport: Box<dyn Transport>) -> Self {
        Self::connect(transport, false)
    }

    /// Starts the session; `tiles` says the client draws tiles, so the engine sends tile ids too.
    pub fn connect(transport: Box<dyn Transport>, tiles: bool) -> Self {
        let mut m = Self {
            tiles,
            tiles_told: tiles,
            tint_ids: vec![],
            tints_told: vec![],
            display: None,
            tiles_retry: Duration::ZERO,
            transport,
            respawn: None,
            waker: None,
            failure: None,
            heard: Duration::ZERO,
            stall: STALL,
            replica: Replica::default(),
            next_id: 0,
            pending: HashMap::new(),
            subscribing: false,
            connected: true,
            busy: false,
            command_id: None,
            rejected: false,
            walking: false,
            origin: None,
            wanted: VIEW,
            declared: VIEW,
            resized: None,
            clock: Clock::default(),
            messages: vec![],
            native: vec![],
            ground: vec![],
            inventory: vec![],
            ground_names: vec![],
            paged: Paged::default(),
            explosion_tiles: HashMap::new(),
            inventory_names: vec![],
            trace: vec![],
            received: 0,
            colors: ColorMap::default(),
            loading: None,
            shown: vec![],
            status: None,
            chore: None,
            at_main_menu: false,
            hovered: None,
        };
        m.call(
            "bn.hello",
            json!(
                protocol::HelloParams::new("bn-client")
                    .with_viewport(protocol::Viewport {
                        cols: VIEW.x as u32,
                        rows: VIEW.y as u32,
                    })
                    .with_tiles(tiles)
            ),
            Pending::Hello,
        );
        m
    }

    /// Times the resize debounce with `clock`, so tests can advance it instead of sleeping.
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// How to start the engine again, for `restart`.
    pub fn with_respawn(mut self, respawn: Respawn) -> Self {
        self.respawn = Some(respawn);
        self
    }

    /// Silence that counts as a stall while a request is outstanding.
    pub fn with_stall(mut self, stall: Duration) -> Self {
        self.set_stall(stall);
        self
    }

    pub fn set_stall(&mut self, stall: Duration) {
        self.stall = stall;
    }

    /// Uses the game's own palette for look colours.
    pub fn with_colors(mut self, colors: ColorMap) -> Self {
        self.colors = colors;
        self
    }

    /// How many engine messages have arrived so far; live tests fail after 120 s without one, however
    /// busy the engine is: loading steps and activities send heartbeats.
    pub fn received(&self) -> u64 {
        self.received
    }

    /// Timestamped protocol milestones (`submit`, `received`, stage names, `events`) since the last call.
    pub fn take_trace(&mut self) -> Vec<(Instant, &'static str)> {
        std::mem::take(&mut self.trace)
    }

    fn mark(&mut self, what: &'static str) {
        if self.trace.len() < 4096 {
            self.trace.push((Instant::now(), what));
        }
    }

    fn call(&mut self, method: &str, params: Value, kind: Pending) {
        self.next_id += 1;
        self.pending.insert(self.next_id, kind);
        self.transport.send(request(self.next_id, method, params));
    }

    fn subscribe(&mut self) {
        if !self.subscribing {
            self.subscribing = true;
            self.call("bn.subscribe", json!({}), Pending::Subscribe);
        }
    }

    fn resubscribe_on(&mut self, outcome: Outcome) {
        if matches!(outcome, Outcome::Resubscribe(_)) {
            self.subscribe();
        }
    }

    fn handle(&mut self, msg: Incoming) {
        profiling::scope!("model.handle");
        match msg {
            Incoming::Response { id, result } => self.response(id, result),
            Incoming::SnapshotPart(p) => {
                let outcome = self.replica.part(p);
                self.resubscribe_on(outcome);
            }
            Incoming::Events(e) if !self.subscribing => {
                self.mark("events");
                let outcome = self.replica.events(e);
                self.resubscribe_on(outcome);
            }
            Incoming::Events(_) => {}
            Incoming::Command(n) => {
                self.mark(match n.stage.as_str() {
                    "validated" => "validated",
                    "executing" => "executing",
                    "completed" => "completed",
                    _ => "stage",
                });
                if self.command_id.as_deref() == Some(&n.command_id) {
                    let reason = n.error.as_ref().map(error_kind);
                    let swap = reason.as_deref() == Some("stale_epoch");
                    let stage = match n.stage.as_str() {
                        "validated" => "accepted".into(),
                        "completed" => "done".into(),
                        _ if swap => "done".into(),
                        "rejected" | "interrupted" => match reason.as_deref() {
                            Some(why) => format!("{}: {}", n.stage, explain(why)),
                            None => n.stage.clone(),
                        },
                        other => other.into(),
                    };
                    if let Some(s) = &mut self.status {
                        s.set_stage(stage, n.is_terminal());
                    }
                }
                if n.is_terminal() && self.command_id.as_deref() == Some(&n.command_id) {
                    self.busy = false;
                    self.command_id = None;
                    // The world swap after choosing the Tutorial ends its command with stale_epoch.
                    let swap = n.stage == "interrupted"
                        && n.error.as_ref().is_some_and(|e| e == "stale_epoch");
                    if n.stage != "completed" && !swap {
                        self.rejected = self.walking;
                        let reason = n.error.as_ref().map(error_kind);
                        self.report_failure(&n.stage, reason.as_deref());
                    }
                }
            }
            Incoming::Resync(r) => {
                let outcome = self.replica.resync(&r);
                self.resubscribe_on(outcome);
            }
            Incoming::Loading(n) => {
                self.loading = match n {
                    LoadingNotice::Progress(p) => Some(Loading {
                        title: p.title,
                        entries: p.entries,
                        index: p.index,
                        image: p.image.map(|i| i.path),
                    }),
                    LoadingNotice::Done(_) => None,
                };
            }
            Incoming::Progress | Incoming::Unknown(_) => {}
            Incoming::Malformed(e) => self.messages.push(format!("Bad engine message: {e}")),
            Incoming::Closed => {
                self.connected = false;
                self.busy = false;
                self.messages.push("Engine connection closed.".into());
                let message = match self.transport.exit_reason() {
                    Some(reason) => format!("The engine stopped ({reason})."),
                    None => "The engine stopped.".into(),
                };
                self.failure = Some(Failure {
                    kind: FailureKind::Exited,
                    message,
                    stderr: self.transport.stderr_tail(),
                });
            }
        }
    }

    fn response(&mut self, id: u64, result: Result<Value, protocol::RpcError>) {
        let Some(kind) = self.pending.remove(&id) else {
            return;
        };
        if matches!(kind, Pending::Subscribe) {
            self.mark("subscribed");
            self.subscribing = false;
        }
        if matches!(kind, Pending::Submit)
            && let Err(e) = &result
        {
            self.busy = false;
            let kind = e.data.as_ref().map_or(e.message.clone(), error_kind);
            if let Some(s) = &mut self.status {
                s.set_stage(format!("rejected: {}", explain(&kind)), true);
            }
        }
        if matches!(kind, Pending::Describe) {
            // A square outside the engine's window has no description; the cell's own name stands.
            let lines = result
                .ok()
                .and_then(|v| serde_json::from_value::<protocol::DescribeResult>(v).ok())
                .map(|d| d.lines)
                .unwrap_or_default();
            if let Some(h) = self.hovered.as_mut() {
                h.lines = Some(lines);
            }
            return;
        }
        if matches!(kind, Pending::Retile) {
            // The engine refused (a command was running): ask again a moment later.
            if result.is_err() {
                self.tiles_told = !self.tiles;
                self.tiles_retry = self.clock.now() + RESIZE_DEBOUNCE;
            }
            return;
        }
        if matches!(kind, Pending::Interrupt) {
            // The engine had no activity running; say so once, not per key press.
            if result.is_err() {
                let text = "There is nothing to interrupt.".to_string();
                if self.messages.last() != Some(&text) {
                    self.messages.push(text);
                }
            }
            return;
        }
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                let why = e.data.as_ref().map_or(e.message.clone(), error_kind);
                self.messages.push(format!("Engine error: {why}."));
                self.rejected = matches!(kind, Pending::Submit) && self.walking;
                return;
            }
        };
        match kind {
            Pending::Hello => {
                self.display = value["engine"]["display"].as_object().and_then(|d| {
                    Some((
                        d.get("tiles")?.as_bool()?,
                        d.get("tileset")?.as_str()?.to_string(),
                    ))
                });
                self.mark("hello");
                self.subscribe();
            }
            Pending::Subscribe => match serde_json::from_value::<Snapshot>(value) {
                Ok(s) => self.replica.begin(s),
                Err(e) => self.messages.push(format!("Bad snapshot: {e}")),
            },
            Pending::Viewport | Pending::Interrupt | Pending::Retile | Pending::Describe => {}
            Pending::Choices => {
                self.paged.waiting = false;
                if let Ok(page) = serde_json::from_value::<protocol::ChoicesResult>(value)
                    && self.paged.key.0 == page.boundary_id
                {
                    self.paged.more.extend(page.choices);
                }
            }
            Pending::Submit => {
                self.mark("received");
                self.command_id = value["command_id"].as_str().map(String::from);
                if let Some(s) = self.status.as_mut().filter(|s| s.stage == "sent") {
                    s.set_stage("accepted".into(), false);
                }
            }
        }
    }

    /// Logs a failed command; a repeat of the last line (a click held on an unreachable square) adds none.
    fn report_failure(&mut self, stage: &str, reason: Option<&str>) {
        let travel = self.status.as_ref().is_some_and(|s| s.label == "Travel");
        let text = match reason {
            Some("validation_failed") if travel => {
                "Travel rejected: that square is outside the map window the engine can resolve."
                    .to_string()
            }
            _ => format!("Command {stage}: {}.", reason.map_or(stage, explain)),
        };
        if self.messages.last() != Some(&text) {
            self.messages.push(text);
        }
    }

    /// The action the game key runs. The engine names the keys of its active context, the player's rebinds
    /// included; the client knows none. A digit with no binding may be a numpad key (terminals and windows do
    /// not tell them apart).
    fn action_for(&self, key: &str) -> Option<String> {
        let bound_to = |key: &str| {
            self.replica.boundary().and_then(|b| {
                b.actions.iter().find(|a| {
                    a.keys
                        .as_ref()
                        .is_some_and(|keys| keys.iter().any(|k| k == key))
                })
            })
        };
        let offered = bound_to(key).or_else(|| keys::numpad(key).and_then(|k| bound_to(&k)));
        offered.map(|a| a.id.clone())
    }

    fn submit(&mut self, operation: Operation) -> bool {
        let (Some(at), Some(b)) = (self.replica.at(), self.replica.boundary()) else {
            return false;
        };
        let submit = Submit {
            epoch: at.epoch.clone(),
            expect: Expect {
                revision: at.revision.clone(),
                boundary_id: b.boundary_id.clone(),
                schema_id: b.interaction.as_ref().map(|i| i.schema_id.clone()),
            },
            operation,
        };
        self.status = Some(self.describe(&submit.operation));
        self.busy = true;
        self.rejected = false;
        self.walking = false;
        let params = serde_json::to_value(submit).expect("submit serializes");
        self.mark("submit");
        self.call("bn.command.submit", params, Pending::Submit);
        true
    }

    /// What the user pressed, named from the boundary the command was sent against.
    fn describe(&self, operation: &Operation) -> CommandStatus {
        let boundary = self.replica.boundary();
        let (label, target) = match operation {
            Operation::Choose { choice_id } => {
                let choices = boundary
                    .and_then(|b| b.interaction.as_ref())
                    .map(|i| &i.choices);
                let found = choices.and_then(|c| c.iter().find(|c| &c.id == choice_id));
                (
                    found.map(|c| markup::plain(&c.label)),
                    Some(choice_id.clone()),
                )
            }
            Operation::Action { action_id } => {
                let found = boundary.and_then(|b| b.actions.iter().find(|a| &a.id == action_id));
                (
                    found.map(|a| markup::plain(&a.name)),
                    Some(action_id.clone()),
                )
            }
            Operation::Travel { .. } => (Some("Travel".into()), None),
            Operation::Context { .. } => (Some("Right click".into()), None),
            Operation::Cancel => (Some("Cancel".into()), None),
            _ => (None, None),
        };
        CommandStatus {
            label: label.unwrap_or_else(|| "Command".into()),
            target,
            stage: "sent".into(),
            finished: false,
            history: vec!["sent".into()],
        }
    }

    /// Submits a registered action whose id is one of `ids`; reports when the engine has none.
    fn invoke(&mut self, ids: &[String]) -> bool {
        let found = self.replica.boundary().and_then(|b| {
            b.actions
                .iter()
                .find(|a| ids.iter().any(|i| a.id.eq_ignore_ascii_case(i)))
        });
        let Some(action) = found.map(|a| a.id.clone()) else {
            self.messages
                .push(format!("The engine offers no {} action.", ids[0]));
            return false;
        };
        self.submit(Operation::Action { action_id: action })
    }

    fn avatar_pos(&self) -> Option<&WorldPos> {
        self.replica.avatar().map(|a| &a.at)
    }

    fn abs(&self, at: Pos) -> Option<WorldPos> {
        let (ox, oy) = self.origin?;
        let a = self.avatar_pos()?;
        Some(WorldPos {
            dim: a.dim.clone(),
            x: ox + at.x,
            y: oy + at.y,
            z: a.z,
        })
    }

    /// The part of the engine's clickable view that is shown: as large as the UI asked for, never larger
    /// than the engine's own window.
    fn shown(&self) -> Pos {
        self.replica.view().map_or(self.wanted, |v| Pos {
            x: self.wanted.x.min(v.max.x - v.min.x + 1),
            y: self.wanted.y.min(v.max.y - v.min.y + 1),
        })
    }

    fn refresh_view(&mut self) {
        let Some(a) = self.avatar_pos() else {
            self.origin = None;
            return;
        };
        let (ax, ay) = (a.x, a.y);
        let size = self.shown();
        if let Some(v) = self.replica.view() {
            // The engine's window is centred on the avatar; show the middle of it, so every shown
            // square is one a click can select.
            self.origin = Some((
                v.min.x + (v.max.x - v.min.x + 1 - size.x) / 2,
                v.min.y + (v.max.y - v.min.y + 1 - size.y) / 2,
            ));
            return;
        }
        let centred = (ax - size.x / 2, ay - size.y / 2);
        self.origin = match self.origin {
            Some((ox, oy))
                if (MARGIN..size.x - MARGIN).contains(&(ax - ox))
                    && (MARGIN..size.y - MARGIN).contains(&(ay - oy)) =>
            {
                Some((ox, oy))
            }
            _ => Some(centred),
        };
    }

    /// Tells the engine the new map size once the panel has stopped changing.
    fn send_viewport(&mut self) {
        let settled = self
            .resized
            .is_some_and(|at| self.clock.now().saturating_sub(at) >= RESIZE_DEBOUNCE);
        if settled && self.wanted != self.declared && self.replica.at().is_some() {
            self.declared = self.wanted;
            self.resized = None;
            let (cols, rows) = (self.wanted.x as u32, self.wanted.y as u32);
            self.call(
                "bn.viewport",
                json!({"cols": cols, "rows": rows}),
                Pending::Viewport,
            );
        }
    }

    /// Tells the engine whether tile ids are wanted once it is idle: a hello says so, and a viewport
    /// of the same size makes it publish the open screen again, now with or without them.
    fn send_tiles(&mut self) {
        if (self.tiles == self.tiles_told && self.tint_ids == self.tints_told)
            || self.busy
            || self.replica.at().is_none()
            || self.clock.now() < self.tiles_retry
        {
            return;
        }
        self.tiles_told = self.tiles;
        self.tints_told = self.tint_ids.clone();
        self.call(
            "bn.hello",
            json!(
                protocol::HelloParams::new("bn-client")
                    .with_tiles(self.tiles)
                    .with_tint_ids(self.tint_ids.clone())
            ),
            Pending::Retile,
        );
        let (cols, rows) = (self.declared.x as u32, self.declared.y as u32);
        self.call(
            "bn.viewport",
            json!({"cols": cols, "rows": rows}),
            Pending::Viewport,
        );
    }

    fn refresh_actions(&mut self) {
        let offered = self.replica.boundary().map_or(&[][..], |b| &b.actions);
        if !offered.is_empty() || !self.busy {
            // An action the engine lists with no keys cannot be run, so there is nothing to offer.
            self.shown = offered
                .iter()
                .filter(|a| a.keys.as_ref().is_none_or(|keys| !keys.is_empty()))
                .map(|a| ActionEntry {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    keys: a.keys.clone().unwrap_or_default(),
                })
                .collect();
        }
    }

    fn refresh_items(&mut self) {
        let avatar = self.replica.avatar();
        let ground = match avatar.and_then(|a| a.ground.as_ref()) {
            Some(stacks) => stacks
                .iter()
                .map(|i| (item_line(i), i.name.clone()))
                .collect(),
            None => self
                .avatar_pos()
                .and_then(|p| self.replica.cell(&p.dim, p.x, p.y, p.z))
                .map_or(vec![], |c| {
                    c.items
                        .iter()
                        .map(|l| (look_name(l), look_name(l)))
                        .collect()
                }),
        };
        let carried = avatar.map_or(vec![], |a| {
            a.inventory
                .iter()
                .flatten()
                .map(|i| (item_line(i), i.name.clone()))
                .collect()
        });
        (self.ground, self.ground_names) = ground.into_iter().unzip();
        (self.inventory, self.inventory_names) = carried.into_iter().unzip();
    }

    /// Marks the chore's item in the native menu it opened, then confirms; ends it when the menu is gone.
    fn advance_chore(&mut self) {
        if self.busy {
            return;
        }
        let Some(Chore { name, marked }) = self.chore.take() else {
            return;
        };
        let target = self.menu().and_then(|m| {
            m.choices
                .into_iter()
                .find(|c| c.enabled && c.label.contains(&name))
        });
        match target {
            Some(c) if !marked && !c.selected => {
                self.chore = Some(Chore { name, marked: true });
                self.choose(&c.id);
            }
            Some(_) => {
                self.invoke_action("CONFIRM");
            }
            None => {}
        }
    }
}

/// An item the way the native lists name it, with how many and where it is kept.
fn item_line(item: &protocol::InventoryItem) -> String {
    let count = item
        .count
        .filter(|n| *n > 1)
        .map_or(String::new(), |n| format!(" x{n}"));
    let slot = item
        .slot
        .as_deref()
        .filter(|s| *s != "carried")
        .map_or(String::new(), |s| format!(" ({s})"));
    format!("{}{count}{slot}", item.name)
}

/// Engine command errors in words a player can act on.
fn explain(kind: &str) -> &str {
    match kind {
        "stale_epoch" => "the world changed",
        "stale_revision" | "stale_boundary" | "stale_interaction_schema" => {
            "the screen changed, try again"
        }
        "validation_failed" => "not possible here",
        "not_ready" => "the game is not ready",
        other => other,
    }
}

fn error_kind(e: &Value) -> String {
    e.as_str()
        .or_else(|| e["kind"].as_str())
        .map_or_else(|| e.to_string(), String::from)
}

impl EngineModel {
    fn styled(&self, look: &Look, desc: String) -> Cell {
        let (color, bg) = self.colors.resolve(&look.color);
        Cell {
            glyph: glyph(look),
            color,
            bg,
            desc,
            id: look.id.clone().unwrap_or_default(),
            ..Cell::default()
        }
    }

    /// The sprite request for one look; `seed` is the game's choice among weighted variants.
    fn layer(&self, look: &Look, seed: u32) -> Layer {
        let (color, bg) = self.colors.resolve(&look.color);
        Layer {
            id: look
                .tile
                .clone()
                .or_else(|| look.id.clone())
                .unwrap_or_default(),
            fallbacks: look.looks_like.clone(),
            subtile: look.subtile.clone(),
            rotation: look.rotation.unwrap_or(0),
            mirrored: look.facing.as_deref() == Some("left"),
            creature: matches!(look.kind.as_str(), "monster" | "npc" | "avatar"),
            seed,
            tint_bg: paint(look, |t| t.bg.as_deref()),
            tint_fg: paint(look, |t| t.fg.as_deref()),
            mutation: None,
            tint_keys: look.tint_keys.clone(),
            tint_vars: look.tint_vars.clone(),
            glyph: glyph(look),
            color,
            bg,
        }
    }

    /// A character's overlays, which face as it does; an overlay without a sprite draws nothing.
    fn overlays(&self, body: &Look, overlays: &[Look], traits: &Traits, seed: u32) -> Vec<Layer> {
        let traits = std::sync::Arc::new(traits.clone());
        overlays
            .iter()
            .map(|overlay| Layer {
                mirrored: body.facing.as_deref() == Some("left"),
                creature: true,
                glyph: '\0',
                mutation: overlay.mutation.as_ref().map(|m| Mutation {
                    id: m.id.clone(),
                    types: m.types.clone(),
                    flags: m.flags.clone(),
                    traits: traits.clone(),
                }),
                ..self.layer(overlay, seed)
            })
            .collect()
    }

    /// The cell without its square seed.
    fn cell_at(&self, at: Pos) -> Cell {
        let unknown = Cell {
            glyph: ' ',
            color: Rgb(128, 128, 128),
            desc: "unknown".into(),
            ..Cell::default()
        };
        let Some(p) = self.abs(at) else {
            return unknown;
        };
        let world = self.replica.cell(&p.dim, p.x, p.y, p.z);
        // Nothing published here but a square in sight below: open air, through which the game draws it.
        let lower = (world.is_none()).then(|| self.below(&p)).flatten();
        let mut layers = world
            .or(lower.map(|(c, _)| c))
            .map_or(vec![], |c| self.layers(c));
        if self.avatar_pos() == Some(&p)
            && let Some(avatar) = self.replica.avatar()
            && let Some(look) = &avatar.look
        {
            let seed = u32::from(avatar.name.bytes().next().unwrap_or(0));
            layers.push(self.layer(look, seed));
            layers.extend(self.overlays(look, &avatar.overlays, &avatar.traits, seed));
        }
        let entity = self
            .replica
            .entities()
            .find(|e| e.at == p && e.look.is_some());
        if let Some(e) = entity {
            let look = e.look.as_ref().expect("filtered on look");
            let desc = e.name.clone().unwrap_or_else(|| look_name(look));
            layers.push(self.layer(look, id_seed(&e.id)));
            layers.extend(self.overlays(look, &e.overlays, &e.traits, id_seed(&e.id)));
            layers.extend(self.attitude(e));
            layers.extend(self.revival(world));
            return Cell {
                layers,
                remembered: false,
                light: world
                    .and_then(|c| c.light.as_ref()?.as_u64())
                    .map(|l| l as u8),
                ..self.styled(look, desc)
            };
        }
        layers.extend(self.revival(world));
        match world {
            Some(c) => match top_look(c) {
                Some(l) => Cell {
                    layers,
                    remembered: c.known == Known::Remembered,
                    light: c.light.as_ref().and_then(Value::as_u64).map(|l| l as u8),
                    ..self.styled(l, look_name(l))
                },
                None => Cell {
                    glyph: '?',
                    color: Rgb(128, 128, 128),
                    desc: "sensed".into(),
                    ..Cell::default()
                },
            },
            None => Cell {
                layers,
                depth: lower.map_or(0, |(_, depth)| depth),
                light: lower.and_then(|(c, _)| c.light.as_ref()?.as_u64().map(|l| l as u8)),
                ..unknown
            },
        }
    }

    /// What a cell draws, bottom first, in the order of the native view: terrain, furniture, trap,
    /// field, item, vehicle part; a cell only remembered draws its memorized terrain and overlay.
    fn layers(&self, c: &WorldCell) -> Vec<Layer> {
        let seed = square_seed(&c.at);
        let live = [
            c.terrain.as_ref(),
            c.furniture.as_ref(),
            c.traps.last(),
            c.fields.last().map(|f| &f.look),
            c.items.last(),
            c.vehicle.as_ref(),
        ];
        let looks = match live.iter().any(Option::is_some) {
            true => live.into_iter().flatten().collect::<Vec<_>>(),
            false => c
                .memory
                .iter()
                .flat_map(|m| std::iter::once(&m.terrain).chain(m.overlay.as_ref()))
                .collect(),
        };
        looks
            .into_iter()
            .flat_map(|l| {
                let seeded = !matches!(l.kind.as_str(), "item" | "trap");
                // A loaded cargo part is highlighted over the part, a pile over its displayed item.
                let loaded = c.cargo && l.kind == "vehicle_part";
                let stacked = loaded || l.stack.is_some_and(|n| n > 1);
                let mark = stacked.then(|| self.marker("highlight_item"));
                [Some(self.layer(l, if seeded { seed } else { 0 })), mark]
            })
            .flatten()
            .collect()
    }

    /// A sprite the tileset may have and that stands for nothing else: it draws nothing without one.
    fn marker(&self, id: &str) -> Layer {
        Layer {
            id: id.into(),
            ..Layer::default()
        }
    }

    /// The nearest square in sight under `p`, with how far down it is.
    fn below(&self, p: &WorldPos) -> Option<(&WorldCell, u8)> {
        (1..=MAX_DEPTH).find_map(|depth| {
            let cell = self
                .replica
                .cell(&p.dim, p.x, p.y, p.z - i32::from(depth))?;
            // A roof seen from above a vehicle has no terrain published, only the part.
            (cell.known == Known::Visible && (cell.terrain.is_some() || cell.vehicle.is_some()))
                .then_some((cell, depth))
        })
    }

    /// The mark over a square holding a corpse that can rise: the last thing the native view draws.
    fn revival(&self, world: Option<&WorldCell>) -> Option<Layer> {
        world
            .is_some_and(|c| c.reviving)
            .then(|| self.marker("zombie_revival_indicator"))
    }

    /// The overlay the native view puts over a monster or NPC for how it regards the avatar.
    fn attitude(&self, e: &Entity) -> Option<Layer> {
        let attitude = e.attitude.as_ref()?;
        let aware = if e.aware { "_sees_player" } else { "" };
        Some(self.marker(&format!("overlay_{attitude}{aware}")))
    }
}

impl GameModel for EngineModel {
    fn poll(&mut self) {
        profiling::scope!("model.poll");
        let now = self.clock.now();
        while let Some(msg) = self.transport.recv() {
            self.received += 1;
            self.heard = now;
            // Anything the engine says shows it is alive again.
            if self
                .failure
                .as_ref()
                .is_some_and(|f| f.kind == FailureKind::Stalled)
            {
                self.failure = None;
            }
            self.handle(msg);
        }
        let waiting = self.busy || !self.pending.is_empty();
        if self.failure.is_none()
            && self.connected
            && self.replica.at().is_some()
            && waiting
            && now.saturating_sub(self.heard) >= self.stall
        {
            self.failure = Some(Failure {
                kind: FailureKind::Stalled,
                message: format!(
                    "The engine has not answered for {} seconds.",
                    self.stall.as_secs()
                ),
                stderr: self.transport.stderr_tail(),
            });
        }
        self.messages.extend(self.replica.take_log());
        for line in self.replica.take_lines() {
            let line = LogLine {
                color: line.color.map(|name| self.colors.resolve(&name).0),
                id: line.id,
                text: line.text,
                count: line.count,
            };
            // Ids only grow and are never reused. A line past the newest one is new; any other is a
            // repeat, or a line a fresh snapshot carries again, and replaces its own line.
            let id = line.id.parse::<u64>().ok();
            let newest = self.native.last().and_then(|l| l.id.parse::<u64>().ok());
            if id.is_some() && id > newest {
                self.native.push(line);
            } else if let Some(old) = self
                .native
                .iter_mut()
                .rev()
                .find(|l| !line.id.is_empty() && l.id == line.id)
            {
                *old = line;
            } else if id.is_none() || newest.is_none() {
                self.native.push(line);
            }
        }
        let at_main_menu = self.menu().is_some_and(|m| m.context == "MAIN_MENU");
        if at_main_menu && !std::mem::replace(&mut self.at_main_menu, true) {
            self.messages.clear();
            self.native.clear();
        }
        self.at_main_menu = at_main_menu;
        self.send_viewport();
        self.send_tiles();
        self.refresh_view();
        self.refresh_items();
        self.refresh_actions();
        self.advance_chore();
    }

    fn set_waker(&mut self, waker: crate::transport::Waker) {
        self.transport.set_waker(waker.clone());
        self.waker = Some(waker);
    }

    fn display_default(&self) -> Option<(bool, String)> {
        self.display.clone()
    }

    fn set_tiles(&mut self, tiles: bool) {
        self.tiles = tiles;
    }

    fn set_tint_ids(&mut self, ids: &[String]) {
        if self.tint_ids != ids {
            self.tint_ids = ids.to_vec();
        }
    }

    fn failure(&self) -> Option<Failure> {
        self.failure.clone()
    }

    fn restart(&mut self) -> bool {
        let Some(mut respawn) = self.respawn.take() else {
            return false;
        };
        match respawn() {
            Ok(transport) => {
                let mut fresh = EngineModel::connect(transport, self.tiles)
                    .with_clock(self.clock.clone())
                    .with_colors(self.colors.clone())
                    .with_stall(self.stall);
                fresh.heard = self.clock.now();
                fresh.respawn = Some(respawn);
                if let Some(waker) = self.waker.take() {
                    fresh.set_waker(waker);
                }
                *self = fresh;
                true
            }
            Err(e) => {
                self.respawn = Some(respawn);
                if let Some(f) = &mut self.failure {
                    f.message = format!("The engine could not be restarted: {e}");
                }
                false
            }
        }
    }

    fn origin(&self) -> Pos {
        let (x, y) = self.origin.unwrap_or_default();
        Pos { x, y }
    }

    fn live(&self) -> bool {
        self.connected
    }

    fn busy(&self) -> bool {
        self.busy
    }

    fn interrupt(&mut self) -> bool {
        if self.busy {
            self.call("bn.interrupt", json!({}), Pending::Interrupt);
        }
        self.busy
    }

    fn size(&self) -> Pos {
        if self.replica.ready() && self.origin.is_some() {
            self.shown()
        } else {
            Pos { x: 0, y: 0 }
        }
    }

    fn cell(&self, at: Pos) -> Cell {
        profiling::scope!("model.cell");
        let square = self.abs(at).map_or(0, |p| square_seed(&p));
        Cell {
            square,
            ..self.cell_at(at)
        }
    }

    fn season(&self) -> Option<String> {
        self.replica.environment()?.season.clone()
    }

    fn mod_tilesets(&self) -> &[ModTileset] {
        self.replica
            .environment()
            .map_or(&[], |e| e.mod_tilesets.as_slice())
    }

    fn player(&self) -> Pos {
        let (Some((ox, oy)), Some(a)) = (self.origin, self.avatar_pos()) else {
            return Pos { x: 0, y: 0 };
        };
        Pos {
            x: a.x - ox,
            y: a.y - oy,
        }
    }

    fn step_toward(&mut self, target: Pos) -> bool {
        if self.rejected {
            self.rejected = false;
            return false;
        }
        let me = self.player();
        let (dx, dy) = ((target.x - me.x).signum(), (target.y - me.y).signum());
        if self.busy || (dx, dy) == (0, 0) {
            return true;
        }
        let Some(id) = direction(dx, dy) else {
            return false;
        };
        let sent = self.invoke(&[id.into()]);
        self.walking = sent;
        sent
    }

    fn ground(&self) -> &[String] {
        &self.ground
    }

    fn inventory(&self) -> &[String] {
        &self.inventory
    }

    fn pick_up(&mut self, index: usize) {
        let name = self.ground_names.get(index).cloned();
        if let (false, Some(name)) = (self.busy, name) {
            self.chore = self
                .invoke(&["pickup_all".into(), "pickup".into()])
                .then_some(Chore {
                    name,
                    marked: false,
                });
        }
    }

    fn drop_item(&mut self, index: usize) {
        let name = self.inventory_names.get(index).cloned();
        if let (false, Some(name)) = (self.busy, name) {
            self.chore = self.invoke(&["drop".into()]).then_some(Chore {
                name,
                marked: false,
            });
        }
    }

    fn messages(&self) -> &[String] {
        &self.messages
    }

    fn sidebar(&self) -> Vec<crate::sidebar::Line> {
        self.replica
            .avatar()
            .map_or(vec![], |a| crate::sidebar::lines(a, &self.colors))
    }

    fn native_log(&self) -> &[LogLine] {
        &self.native
    }

    fn say(&mut self, text: String) {
        if self.messages.last() != Some(&text) {
            self.messages.push(text);
        }
    }

    fn take_effects(&mut self) -> Vec<Effect> {
        let mut effects = vec![];
        for fact in self.replica.take_facts() {
            let id = fact.data["explosion"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            match fact.kind.as_str() {
                "explosion.started" => {
                    let tile = fact.data["tile"].as_str().unwrap_or("explosion");
                    self.explosion_tiles.insert(id, tile.to_string());
                }
                "explosion.ended" => {
                    self.explosion_tiles.remove(&id);
                }
                _ => {}
            }
            effects.extend(self.effect_of(&fact));
        }
        effects
    }

    fn loading(&self) -> Option<Loading> {
        self.loading.clone()
    }

    fn actions(&self) -> Vec<ActionEntry> {
        self.shown.clone()
    }

    fn command(&self) -> Option<CommandStatus> {
        self.status.clone()
    }

    fn set_viewport(&mut self, cells: Pos) {
        let cells = Pos {
            x: cells.x.clamp(1, MAX_VIEWPORT),
            y: cells.y.clamp(1, MAX_VIEWPORT),
        };
        if cells != self.wanted {
            self.wanted = cells;
            self.resized = Some(self.clock.now());
        }
    }

    fn travel(&mut self, at: Pos) -> bool {
        // Like Tiles, walking into a closed door next to you opens it; the engine plans no route to one.
        let (me, id) = (self.player(), self.cell(at).id);
        let step = (at.x - me.x, at.y - me.y);
        if id.contains("door") && id.ends_with("_c") && step.0.abs().max(step.1.abs()) == 1 {
            if let (false, Some(id)) = (self.busy, direction(step.0, step.1)) {
                self.invoke(&[id.into()]);
            }
            return true;
        }
        // A square outside the engine's window cannot be selected by a click; ignore it quietly.
        let reachable = |p: &WorldPos| {
            self.replica.view().is_none_or(|v| {
                (v.min.x..=v.max.x).contains(&p.x) && (v.min.y..=v.max.y).contains(&p.y)
            })
        };
        let free = self
            .replica
            .boundary()
            .is_some_and(|b| b.interaction.is_none());
        if let (false, true, Some(pos)) = (self.busy, free, self.abs(at).filter(reachable)) {
            self.submit(Operation::Travel { pos });
        }
        true
    }

    fn route(&self) -> Vec<Pos> {
        let Some((ox, oy)) = self.origin else {
            return vec![];
        };
        self.replica
            .route()
            .iter()
            .map(|p| Pos {
                x: p.x - ox,
                y: p.y - oy,
            })
            .collect()
    }

    fn invoke_action(&mut self, id: &str) -> bool {
        !self.busy && self.invoke(&[id.into()])
    }

    fn key_bound(&self, key: &str) -> bool {
        self.action_for(key).is_some()
    }

    fn press(&mut self, key: &str) -> bool {
        if let Some(id) = self.action_for(key) {
            return self.invoke_action(&id);
        }
        // A "press any key" popup has no key to bind: whatever is pressed acknowledges it.
        let any_key = self
            .replica
            .boundary()
            .and_then(|b| b.interaction.as_ref())
            .is_some_and(|i| {
                i.choices
                    .iter()
                    .any(|c| c.id == "acknowledge" && c.description == "ANY_INPUT")
            });
        any_key
            && !self.busy
            && self.submit(Operation::Choose {
                choice_id: "acknowledge".into(),
            })
    }

    fn menu(&self) -> Option<Menu> {
        profiling::scope!("model.menu");
        let b = self.replica.boundary()?;
        let i = b.interaction.as_ref()?;
        let text = |k: &str| {
            let v = i.field.as_ref()?.get(k)?.as_str();
            v.map(String::from)
        };
        let field = text("id").map(|id| MenuField {
            id,
            label: text("label").unwrap_or_default(),
            value: text("value").unwrap_or_default(),
            kind: text("type").unwrap_or_default(),
        });
        // A cursor the engine moves (look mode) is drawn on the map, not listed.
        if i.kind == "target" {
            return None;
        }
        let pane_labels = i
            .compat
            .as_ref()
            .and_then(|c| c["panes"].as_array())
            .map_or(&[][..], Vec::as_slice);
        let pane_of = |label: &str| {
            pane_labels
                .iter()
                .find(|p| p["label"] == label)
                .and_then(|p| p["id"].as_str())
        };
        // Without the engine's pane list, the rows that belong to no pane are the tabs of a paned menu.
        let more = self.more_of(i);
        let all = || i.choices.iter().chain(more);
        let paned = all().any(|c| c.pane_id.is_some());
        let tab = |c: &protocol::Choice| {
            c.pane_id.is_none()
                && (pane_of(&c.label).is_some() || (pane_labels.is_empty() && paned))
        };
        // The cursor travels as `compat.focus`; a popup row's description is its action id, for tools.
        let focus = i
            .compat
            .as_ref()
            .and_then(|c| c["focus"]["choice_id"].as_str());
        let actions = self.replica.boundary().map_or(&[][..], |b| &b.actions);
        let described = |text: &str| {
            let machine = actions.iter().any(|a| a.id == text);
            if machine {
                String::new()
            } else {
                text.to_string()
            }
        };
        Some(Menu {
            context: i.context.clone(),
            field,
            can_set_count: i.allow_set_count,
            title: i.title.clone(),
            message: i.message.clone(),
            message_tone: i.message_tone.clone().unwrap_or_default(),
            category_mode: i.category_mode,
            banner: i.banner.clone().unwrap_or_default(),
            total: i.choice_total as usize,
            choices: all()
                .map(|c| MenuChoice {
                    id: c.id.clone(),
                    label: c.label.clone(),
                    enabled: c.enabled && c.selectable,
                    denial: c.denial.clone(),
                    hotkey: c.hotkey.clone().unwrap_or_default(),
                    selected: c.selected,
                    group: c.pane_id.clone(),
                    tab: tab(c),
                    highlighted: Some(c.id.as_str()) == focus,
                    selected_count: c.selected_count,
                    available_count: c.available_count,
                    description: described(&c.description),
                    columns: c
                        .columns
                        .iter()
                        .map(|col| (col.label.clone(), col.value.clone()))
                        .collect(),
                    editor: c.editor.clone(),
                })
                .collect(),
            panes: pane_labels
                .iter()
                .filter(|p| p["area_label"].as_str().is_some_and(|a| !a.is_empty()))
                .map(|p| {
                    let text = |k: &str| p[k].as_str().unwrap_or_default().to_string();
                    MenuPane {
                        id: text("id"),
                        label: text("label"),
                        role: text("role"),
                        area: text("area_label"),
                        storage: text("storage_kind"),
                        description: text("area_description"),
                        filter: text("filter"),
                        sort: text("sort"),
                        totals: p["totals"].is_object().then(|| {
                            let t =
                                |k: &str| p["totals"][k].as_str().unwrap_or_default().to_string();
                            let b = |k: &str| p["totals"][k].as_bool().unwrap_or_default();
                            PaneTotals {
                                weight: t("weight"),
                                weight_max: t("weight_max"),
                                weight_unit: t("weight_unit"),
                                volume: t("volume"),
                                volume_max: t("volume_max"),
                                volume_unit: t("volume_unit"),
                                over_weight: b("over_weight"),
                                over_volume: b("over_volume"),
                            }
                        }),
                        page: p["page"]["number"]
                            .as_u64()
                            .zip(p["page"]["count"].as_u64()),
                        blocked: p["blocked_areas"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|a| a.as_str().map(String::from))
                            .collect(),
                    }
                })
                .collect(),
            can_cancel: i.allow_cancel,
            pane: all()
                .find(|c| tab(c) && c.selected)
                .and_then(|c| pane_of(&c.label).map(String::from)),
            screen: i.kind == "custom",
            focus: i
                .compat
                .as_ref()
                .and_then(|c| c["focus"]["choice_id"].as_str())
                .map(String::from),
        })
    }

    fn overmap(&self) -> Option<Overmap> {
        let i = self.replica.boundary()?.interaction.as_ref()?;
        let wire = i.overmap.as_ref()?;
        let at = |p: &WorldPos| (p.x, p.y, p.z);
        let cursor =
            serde_json::from_value::<WorldPos>(i.target.as_ref()?["current"].clone()).ok()?;
        let black = Rgb(0, 0, 0);
        let cells = wire
            .cells
            .iter()
            .flatten()
            .map(|(glyph, fg, bg)| {
                let bg = self.colors.index(*bg);
                OvermapCell {
                    glyph: glyph.clone(),
                    color: self.colors.index(*fg),
                    bg: (bg != black).then_some(bg),
                }
            })
            .collect();
        Some(Overmap {
            cols: wire.cols as i32,
            rows: wire.rows as i32,
            cells,
            origin: at(&wire.origin),
            cursor: at(&cursor),
            player: at(&wire.player),
            legend: wire.legend.clone(),
            notes: wire
                .notes
                .iter()
                .map(|n| (at(&n.pos), n.text.clone()))
                .collect(),
            path: wire.path.iter().map(at).collect(),
            tiles: wire
                .tiles
                .iter()
                .flat_map(|t| {
                    t.cells
                        .iter()
                        .flatten()
                        .map(|(id, rotation, subtile)| OvermapTile {
                            id: id.clone(),
                            rotation: *rotation,
                            subtile: subtile.clone(),
                            fallbacks: t.looks_like.get(id).cloned().unwrap_or_default(),
                        })
                })
                .collect(),
        })
    }

    fn overmap_cursor_to(&mut self, at: (i32, i32, i32)) {
        if self.overmap().is_some() && !self.busy {
            self.submit(Operation::SetTarget {
                candidate_id: None,
                pos: Some(WorldPos {
                    dim: String::new(),
                    x: at.0,
                    y: at.1,
                    z: at.2,
                }),
            });
        }
    }

    fn close_overmap(&mut self) {
        if self.overmap().is_some() && !self.busy {
            self.submit(Operation::Cancel);
        }
    }

    fn more_choices(&mut self) {
        let Some(b) = self.replica.boundary() else {
            return;
        };
        let Some(i) = b.interaction.as_ref() else {
            return;
        };
        let key = (b.boundary_id.clone(), i.schema_id.clone());
        if self.paged.key != key {
            self.paged = Paged {
                key,
                ..Paged::default()
            };
        }
        let loaded = i.choices.len() + self.paged.more.len();
        if self.paged.waiting || loaded as u64 >= i.choice_total {
            return;
        }
        let (epoch, boundary_id) = (
            self.replica.at().map(|a| a.epoch.clone()),
            b.boundary_id.clone(),
        );
        let Some(epoch) = epoch else {
            return;
        };
        self.paged.waiting = true;
        let params = protocol::ChoicesParams {
            epoch,
            boundary_id,
            offset: loaded as u64,
            limit: PAGE,
        };
        self.call(
            "bn.interaction.choices",
            serde_json::to_value(params).expect("choices params serialize"),
            Pending::Choices,
        );
    }

    fn focus(&self) -> Option<Pos> {
        let i = self.replica.boundary()?.interaction.as_ref()?;
        if i.overmap.is_some() {
            // The overmap's cursor is an overmap square, not a map square.
            return None;
        }
        let at = serde_json::from_value::<WorldPos>(i.target.as_ref()?["current"].clone()).ok()?;
        let (ox, oy) = self.origin?;
        Some(Pos {
            x: at.x - ox,
            y: at.y - oy,
        })
    }

    fn aiming(&self) -> Option<String> {
        let i = self.replica.boundary()?.interaction.as_ref()?;
        let t = i.target.as_ref().filter(|_| i.context == "TARGET")?;
        let at = |k: &str| serde_json::from_value::<WorldPos>(t[k].clone()).ok();
        let (from, to, range) = (at("source")?, at("current")?, t["range"].as_i64()?);
        let (dx, dy) = (f64::from(to.x - from.x), f64::from(to.y - from.y));
        let distance = if t["distance_metric"] == "trig" {
            dx.hypot(dy).round()
        } else {
            dx.abs().max(dy.abs())
        };
        let status = match t["status"].as_str()? {
            "good" => "ready to fire",
            "out_of_range" => "out of range",
            "out_of_ammo" => "out of ammo",
            _ => "no valid target",
        };
        let title = markup::plain(&i.title);
        Some(format!("{title}: range {distance}/{range}, {status}"))
    }

    fn styled(&self, text: &str) -> Vec<Run> {
        markup::runs(text, &self.colors)
    }

    fn choose(&mut self, choice_id: &str) {
        let enabled = self
            .menu()
            .is_some_and(|m| m.choices.iter().any(|c| c.id == choice_id && c.enabled));
        if enabled && !self.busy {
            self.submit(Operation::Choose {
                choice_id: choice_id.into(),
            });
        }
    }

    fn hover(&mut self, at: Pos) {
        let (Some(pos), Some(boundary), Some(now)) =
            (self.abs(at), self.replica.boundary(), self.replica.at())
        else {
            return;
        };
        let asked = self
            .hovered
            .as_ref()
            .is_some_and(|h| h.pos == pos && h.revision == now.revision);
        if asked || boundary.interaction.is_some() {
            return;
        }
        let params = serde_json::to_value(protocol::DescribeParams {
            epoch: now.epoch.clone(),
            boundary_id: boundary.boundary_id.clone(),
            pos: pos.clone(),
        })
        .expect("describe params serialize");
        self.hovered = Some(Hover {
            pos,
            revision: now.revision.clone(),
            lines: None,
        });
        self.call("bn.world.describe", params, Pending::Describe);
    }

    fn hover_text(&self, at: Pos) -> Option<Vec<String>> {
        let (pos, now) = (self.abs(at)?, self.replica.at()?);
        let h = self.hovered.as_ref()?;
        (h.pos == pos && h.revision == now.revision)
            .then(|| h.lines.clone())
            .flatten()
            .filter(|l| !l.is_empty())
    }

    fn native_context(&self) -> bool {
        true
    }

    fn context_click(&mut self, at: Pos) -> bool {
        let free = self
            .replica
            .boundary()
            .is_some_and(|b| b.interaction.is_none());
        if let (false, true, Some(pos)) = (self.busy, free, self.abs(at)) {
            self.submit(Operation::Context { pos });
        }
        true
    }

    fn aim(&mut self, at: Pos) -> bool {
        let Some(here) = self.focus() else {
            return false;
        };
        if self.busy {
            return true;
        }
        match (here == at, self.abs(at)) {
            // The cursor is already there: the second click confirms it, as Enter does.
            (true, _) => {
                self.invoke(&["CONFIRM".into()]);
            }
            (false, Some(pos)) => {
                self.submit(Operation::SetTarget {
                    candidate_id: None,
                    pos: Some(pos),
                });
            }
            _ => {}
        }
        true
    }

    fn can_scroll_menu(&self) -> bool {
        self.replica.boundary().is_some_and(|b| {
            b.interaction.is_some() && b.actions.iter().any(|a| a.id == "SCROLL_UP")
        })
    }

    fn scroll_menu(&mut self, up: bool) -> bool {
        let id = if up { "SCROLL_UP" } else { "SCROLL_DOWN" };
        self.can_scroll_menu() && !self.busy && self.invoke(&[id.into()])
    }

    fn cancel_menu(&mut self) {
        if self.menu().is_some_and(|m| m.can_cancel) && !self.busy {
            self.submit(Operation::Cancel);
        }
    }

    fn fill(&mut self, field_id: &str, value: &str, submit: bool) {
        // An editable row is filled by its own id, like an option of the options menu.
        let has_field = self.menu().is_some_and(|m| {
            m.field.is_some_and(|f| f.id == field_id)
                || m.choices
                    .iter()
                    .any(|c| c.id == field_id && c.enabled && c.editor.is_some())
        });
        if has_field && !self.busy {
            self.submit(Operation::Fill {
                field_id: field_id.into(),
                value: value.into(),
                submit,
            });
        }
    }

    fn set_count(&mut self, choice_id: &str, count: u64) {
        let allowed = self.menu().is_some_and(|m| {
            m.can_set_count && m.choices.iter().any(|c| c.id == choice_id && c.enabled)
        });
        if allowed && !self.busy {
            self.submit(Operation::SetCount {
                choice_id: choice_id.into(),
                count,
            });
        }
    }
}
