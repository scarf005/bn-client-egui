//! `GameModel` backed by a live engine connection.

use crate::clock::Clock;
use crate::colors::ColorMap;
use crate::interaction::{self, InvokeError, Pages};
use crate::keys;
use crate::markup::{self, Run};
use crate::model::{
    ActionEntry, Canvas, CanvasRun, Cell, CommandStatus, Effect, Failure, FailureKind, GameModel,
    Heard, Layer, Loading, LogLine, Menu, MenuChoice, MenuField, MenuPane, Mutation, Overmap,
    OvermapCell, OvermapTile, PaneTotals, Pos, Rgb, Shape,
};
use crate::protocol::{
    self, CanvasWire, Cell as WorldCell, Entity, Incoming, InvokeArgs, Known, LoadingNotice, Look,
    ModTileset, Operation, Row, Screen, Snapshot, Traits, WorldPos,
};
use crate::replica::{Outcome, Replica};
use crate::transport::{Transport, request};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// The map window asked for until the UI reports its own size.
const VIEW: Pos = Pos { x: 40, y: 25 };
/// A resized panel is sent to the engine once it has stayed this size for this long.
/// The engine takes up to this many cells on a side.
const MAX_VIEWPORT: i32 = 512;
/// Heard sounds kept; older ones have long faded.
const MAX_SOUNDS: usize = 50;
const RESIZE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

/// How a session starts.
#[derive(Clone, Copy, Debug)]
pub struct ConnectOptions {
    /// The client draws tiles, so the engine sends tile ids too.
    pub tiles: bool,
    /// The most frames per second the client can show while an activity runs.
    pub max_hz: u16,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            tiles: false,
            max_hz: DEFAULT_MAX_HZ,
        }
    }
}

#[derive(Clone, Copy)]
enum Pending {
    Hello,
    Subscribe,
    Submit,
    Viewport,
    Interrupt,
    /// The outcome of the command in flight, asked for when the engine says it has nothing to interrupt.
    Outcome,
    /// A hello that only changes whether the engine sends tile ids.
    Retile,
    Describe,
    Choices,
    /// The rows of a list of the top screen, past its first page; the list is in `row_requests`.
    Rows,
}

/// The choices past the first page of the interaction with this boundary and schema.
#[derive(Default)]
struct Paged {
    key: (String, String),
    more: Vec<protocol::Choice>,
    /// A page was asked for and has not arrived.
    waiting: bool,
}

/// Starts the engine again on the same profile.
pub type Respawn = Box<dyn FnMut() -> std::io::Result<Box<dyn Transport>>>;

/// Without a message for this long while a request is outstanding, the engine counts as stalled.
/// The frame rate a client asks the engine to publish at while an activity runs, unless it knows better.
pub const DEFAULT_MAX_HZ: u16 = 60;
/// JSON-RPC code of a request whose params the engine does not accept.
const INVALID_PARAMS: i64 = -32602;
/// JSON-RPC code of a method the engine does not know.
const METHOD_NOT_FOUND: i64 = -32601;
const STALL: Duration = Duration::from_secs(120);

pub struct EngineModel {
    /// Whether the client draws tiles now, and what the engine was last told.
    tiles: bool,
    tiles_told: bool,
    /// The most frames per second the client shows, asked of the engine in every hello.
    max_hz: u16,
    /// The ids of the tints the client's tileset has, and what the engine was last told.
    tint_ids: Vec<String>,
    tints_told: Vec<String>,
    /// The engine's `USE_TILES` and `TILES` options from its hello.
    display: Option<(bool, String)>,
    /// The earliest time to tell the engine again after it refused.
    tiles_retry: Duration,
    transport: Box<dyn Transport>,
    respawn: Option<Respawn>,
    /// The engine binary the client started, shown when the engine turns out to be incompatible.
    engine: Option<std::path::PathBuf>,
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
    /// A submitted command has not reached a terminal stage yet: `unanswered` or `running` is not empty.
    busy: bool,
    /// How many screen-scoped commands the engine holds behind the running one; its hello says.
    screen_queue: usize,
    /// Submits whose answer has not arrived; the engine queues screen-scoped ones behind a running command.
    unanswered: u32,
    /// The commands the engine accepted that have not reached a terminal stage.
    running: HashSet<String>,
    command_id: Option<String>,
    /// The command in flight and the boundary it was sent against; a resend is not recorded.
    sent: Option<(Operation, String)>,
    /// A command a timed screen's redraw made stale: it goes once to the next boundary.
    resend: Option<(Operation, String)>,
    origin: Option<(i32, i32)>,
    /// Map cells the UI can show; the engine is told, and the shown map never exceeds them.
    wanted: Pos,
    /// The size the engine was last told, and when the UI last asked for another one.
    declared: Pos,
    resized: Option<Duration>,
    clock: Clock,
    messages: Vec<String>,
    sounds: Vec<Heard>,
    native: Vec<LogLine>,
    ground: Vec<String>,
    inventory: Vec<String>,
    ground_cells: Vec<Cell>,
    inventory_cells: Vec<Cell>,
    /// The engine's row id and item count of each `ground` row, for `Pickup`; empty id when it names none.
    ground_items: Vec<ItemRow>,
    /// Pages of a long menu fetched after the first, for the interaction they belong to.
    paged: Paged,
    /// The same for the lists of the top screen, by component id, and which list each request asked for.
    pages: Pages,
    row_requests: HashMap<u64, String>,
    /// The tile each running explosion is drawn with, from its `explosion.started`.
    explosion_tiles: HashMap<String, String>,
    /// The same for the rows of `inventory`, for `Drop`.
    inventory_items: Vec<ItemRow>,
    trace: Vec<(Instant, &'static str)>,
    received: u64,
    colors: ColorMap,
    loading: Option<Loading>,
    /// Actions on show: between auto-move steps the engine offers none, which must not flicker the list.
    shown: Vec<ActionEntry>,
    status: Option<CommandStatus>,
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

/// The engine's id of an item row and how many items it stands for.
type ItemRow = (String, u64);

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

/// The topmost thing on a level the game draws without its ground: vehicle part, item, field.
fn on_top(c: &WorldCell) -> Option<&Look> {
    c.vehicle
        .as_ref()
        .or(c.items.last())
        .or(c.fields.last().map(|f| &f.look))
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
impl EngineModel {
    /// The engine's window with its curses colors resolved.
    fn canvas(&self, wire: &CanvasWire) -> Canvas {
        Canvas {
            cols: wire.cols,
            rows: wire
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|(text, fg, bg)| CanvasRun {
                            text: text.clone(),
                            color: self.colors.index(*fg),
                            bg: (*bg != 0).then(|| self.colors.index(*bg)),
                        })
                        .collect()
                })
                .collect(),
        }
    }

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
        Self::connect_with(transport, tiles, DEFAULT_MAX_HZ)
    }

    /// Like `connect`, with the most frames per second the client can show while an activity runs.
    pub fn connect_with(transport: Box<dyn Transport>, tiles: bool, max_hz: u16) -> Self {
        Self::open(transport, ConnectOptions { tiles, max_hz })
    }

    /// Starts the session as `options` say.
    pub fn open(transport: Box<dyn Transport>, options: ConnectOptions) -> Self {
        let ConnectOptions { tiles, max_hz } = options;
        let mut m = Self {
            max_hz,
            tiles,
            tiles_told: tiles,
            tint_ids: vec![],
            tints_told: vec![],
            display: None,
            tiles_retry: Duration::ZERO,
            transport,
            respawn: None,
            engine: None,
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
            screen_queue: 1,
            unanswered: 0,
            running: HashSet::new(),
            command_id: None,
            sent: None,
            resend: None,
            origin: None,
            wanted: VIEW,
            declared: VIEW,
            resized: None,
            clock: Clock::default(),
            messages: vec![],
            sounds: vec![],
            native: vec![],
            ground: vec![],
            inventory: vec![],
            ground_cells: vec![],
            inventory_cells: vec![],
            ground_items: vec![],
            paged: Paged::default(),
            pages: Pages::default(),
            row_requests: HashMap::new(),
            explosion_tiles: HashMap::new(),
            inventory_items: vec![],
            trace: vec![],
            received: 0,
            colors: ColorMap::default(),
            loading: None,
            shown: vec![],
            status: None,
            at_main_menu: false,
            hovered: None,
        };
        m.hello();
        m
    }

    /// Opens the session with the delivery asked for now.
    fn hello(&mut self) {
        let params = protocol::HelloParams::new("bn-client")
            .with_viewport(protocol::Viewport {
                cols: VIEW.x as u32,
                rows: VIEW.y as u32,
            })
            .with_tiles(self.tiles);
        self.call("bn.hello", json!(self.with_session(params)), Pending::Hello);
    }

    /// A hello with the delivery this client needs.
    fn with_session(&self, params: protocol::HelloParams) -> protocol::HelloParams {
        params.with_delivery(crate::transport::CREDIT_WINDOW, Some(self.max_hz))
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

    /// The engine binary the client started, for a failure that is about which engine it is.
    pub fn with_engine(mut self, engine: std::path::PathBuf) -> Self {
        self.engine = Some(engine);
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

    /// Sends a request and returns its id.
    fn call(&mut self, method: &str, params: Value, kind: Pending) -> u64 {
        self.next_id += 1;
        self.pending.insert(self.next_id, kind);
        self.transport.send(request(self.next_id, method, params));
        self.next_id
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

    /// Tells the transport what a hello granted; a new hello replaces the old grant, so an answer without
    /// one ends credits.
    fn set_credits(&mut self, hello: &Value) {
        let window = serde_json::from_value::<protocol::Hello>(hello.clone())
            .ok()
            .and_then(|h| h.delivery)
            .map(|d| d.credits);
        self.transport.set_credits(window);
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
            Incoming::Command(n) => self.command_notice(n),
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
                self.unanswered = 0;
                self.running.clear();
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
                    engine: None,
                });
            }
        }
    }

    /// A command's stage, from a notification or from `bn.command.result`.
    fn command_notice(&mut self, n: protocol::CommandNotice) {
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
                "rejected" | "interrupted" => match &n.error_text {
                    Some(why) => format!("{}: {why}", n.stage),
                    None => n.stage.clone(),
                },
                other => other.into(),
            };
            if let Some(s) = &mut self.status {
                s.set_stage(stage, n.is_terminal());
            }
        }
        let latest = self.command_id.as_deref() == Some(&n.command_id);
        let outstanding = n.is_terminal() && self.running.remove(&n.command_id);
        if outstanding {
            self.busy = self.unanswered > 0 || !self.running.is_empty();
        }
        if n.is_terminal() && (latest || outstanding) {
            if latest {
                self.command_id = None;
            }
            // The world swap after choosing the Tutorial ends its command with stale_epoch.
            let swap =
                n.stage == "interrupted" && n.error.as_ref().is_some_and(|e| e == "stale_epoch");
            if n.stage != "completed" && !swap {
                let reason = n.error.as_ref().map(error_kind);
                // A kept resend is the latest command; an earlier queued one is only reported.
                if !(latest && self.stale_resend(reason.as_deref())) {
                    self.report_failure(n.error_text.as_deref().unwrap_or(&n.stage));
                }
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
            self.unanswered = self.unanswered.saturating_sub(1);
            self.busy = self.unanswered > 0 || !self.running.is_empty();
            let kind = e.data.as_ref().map_or(e.message.clone(), error_kind);
            self.stale_resend(Some(&kind));
            if let Some(s) = &mut self.status {
                s.set_stage(format!("rejected: {}", reason_of(&e)), true);
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
        if matches!(kind, Pending::Rows) {
            let asked = self.row_requests.remove(&id);
            let reply = result
                .ok()
                .and_then(|v| serde_json::from_value::<protocol::ChoicesReply>(v).ok());
            match (reply, asked) {
                (Some(protocol::ChoicesReply::Rows(page)), _) => self.pages.accept(page),
                (_, Some(component_id)) => self.pages.failed(&component_id),
                _ => {}
            }
            return;
        }
        if matches!(kind, Pending::Retile) {
            if let Ok(v) = &result {
                self.set_credits(v);
            }
            // The engine refused (a command was running): ask again a moment later.
            if result.is_err() {
                self.tiles_told = !self.tiles;
                self.tiles_retry = self.clock.now() + RESIZE_DEBOUNCE;
            }
            return;
        }
        if matches!(kind, Pending::Interrupt) {
            // The engine had no activity running; say so once, not per key press.
            if let Err(e) = &result {
                self.report_failure(&reason_of(e));
                // An engine with nothing to interrupt may have finished the command without our hearing of it.
                if let (Some(at), Some(command_id)) = (self.replica.at(), self.command_id.clone()) {
                    let params = protocol::ResultParams {
                        epoch: at.epoch.clone(),
                        command_id,
                    };
                    self.call("bn.command.result", json!(params), Pending::Outcome);
                }
            }
            return;
        }
        if matches!(kind, Pending::Outcome) {
            if let Some(n) = result.ok().and_then(|v| serde_json::from_value(v).ok()) {
                self.command_notice(n);
            }
            return;
        }
        if matches!(kind, Pending::Hello)
            && let Err(e) = &result
            && incompatible(e)
        {
            self.failure = Some(Failure {
                kind: FailureKind::Incompatible,
                message: reason_of(e),
                stderr: self.transport.stderr_tail(),
                engine: self.engine.clone(),
            });
            return;
        }
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                self.report_failure(&reason_of(&e));
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
                self.screen_queue = serde_json::from_value::<protocol::Hello>(value.clone())
                    .map_or(1, |hello| hello.screen_queue());
                self.set_credits(&value);
                self.subscribe();
            }
            Pending::Subscribe => match serde_json::from_value::<Snapshot>(value) {
                Ok(s) => self.replica.begin(s),
                Err(e) => self.messages.push(format!("Bad snapshot: {e}")),
            },
            Pending::Viewport
            | Pending::Interrupt
            | Pending::Outcome
            | Pending::Retile
            | Pending::Rows
            | Pending::Describe => {}
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
                self.unanswered = self.unanswered.saturating_sub(1);
                self.command_id = value["command_id"].as_str().map(String::from);
                self.running.extend(self.command_id.clone());
                self.busy = self.unanswered > 0 || !self.running.is_empty();
                if let Some(s) = self.status.as_mut().filter(|s| s.stage == "sent") {
                    s.set_stage("accepted".into(), false);
                }
            }
        }
    }

    /// Keeps a command rejected as `stale_boundary` for the next boundary: a timed screen redraws on its
    /// own, so a key sent as a tick lands is rejected though the player did nothing wrong.
    fn stale_resend(&mut self, reason: Option<&str>) -> bool {
        let timed = self
            .replica
            .boundary()
            .and_then(|b| b.interaction.as_ref())
            .is_some_and(|i| i.refresh_ms.is_some());
        let sent = self.sent.take();
        let keep = reason == Some("stale_boundary") && timed && sent.is_some();
        if keep {
            self.resend = sent;
        }
        keep
    }

    /// Sends the kept command once the engine has published a newer boundary of a timed screen; a screen
    /// that no longer redraws on its own drops it.
    fn send_resend(&mut self) {
        let Some((_, stale)) = &self.resend else {
            return;
        };
        let Some(b) = self.replica.boundary().filter(|b| &b.boundary_id != stale) else {
            return;
        };
        let timed = b
            .interaction
            .as_ref()
            .is_some_and(|i| i.refresh_ms.is_some());
        if let Some((operation, _)) = self.resend.take().filter(|_| timed) {
            self.submit(operation);
            self.sent = None;
        }
    }

    /// Logs a failed command's reason; a repeat of the last line (a click held on an unreachable square) adds none.
    fn report_failure(&mut self, text: &str) {
        if self.messages.last().map(String::as_str) != Some(text) {
            self.messages.push(text.to_string());
        }
    }

    /// The action the game key runs. The engine names the keys of its active context, the player's rebinds
    /// included; the client knows none. A digit with no binding may be a numpad key (terminals and windows do
    /// not tell them apart).
    fn action_for(&self, key: &str) -> Option<String> {
        self.bound_action(key).or_else(|| {
            self.row_for(key)
                .is_none()
                .then(|| self.numpad_action(key))
                .flatten()
        })
    }

    fn bound_action(&self, key: &str) -> Option<String> {
        let b = self.replica.boundary()?;
        let bound = b.actions.iter().find(|a| {
            a.keys
                .as_ref()
                .is_some_and(|keys| keys.iter().any(|k| k == key))
        });
        bound.map(|a| a.id.clone())
    }

    /// The action of the numpad key a typed digit may have come from; a menu row showing the digit as its
    /// hotkey wins, as in a native list.
    fn numpad_action(&self, key: &str) -> Option<String> {
        keys::numpad(key).and_then(|k| self.bound_action(&k))
    }

    /// The enabled row of a menu without a text field that shows `key` as its hotkey.
    fn row_for(&self, key: &str) -> Option<MenuChoice> {
        self.menu()
            .filter(|m| m.field.is_none())
            .and_then(|m| m.choices.into_iter().find(|c| c.enabled && c.hotkey == key))
    }

    fn submit(&mut self, operation: Operation) -> bool {
        self.send(operation).is_ok()
    }

    /// The one way a command leaves the client: `interaction::submit` builds it with the expectation its
    /// operation takes, or says why there is none.
    fn send(&mut self, operation: Operation) -> Result<(), InvokeError> {
        let (Some(at), Some(b)) = (self.replica.at(), self.replica.boundary()) else {
            return Err(InvokeError::NoScreen);
        };
        let basis = interaction::Basis {
            at,
            boundary: b,
            outstanding: self.unanswered as usize + self.running.len(),
            queue: self.screen_queue,
        };
        let submit = interaction::submit(&basis, operation)?;
        let boundary_id = b.boundary_id.clone();
        self.status = Some(self.describe(&submit.operation));
        self.sent = Some((submit.operation.clone(), boundary_id));
        self.unanswered += 1;
        self.busy = true;
        let params = serde_json::to_value(submit).expect("submit serializes");
        self.mark("submit");
        self.call("bn.command.submit", params, Pending::Submit);
        Ok(())
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
            Operation::Invoke { affordance_id, .. } => {
                let found = boundary
                    .and_then(|b| b.stack.last())
                    .and_then(|s| s.affordances.iter().find(|a| &a.id == affordance_id));
                (
                    found.map(|a| markup::plain(&a.label.shown())),
                    Some(affordance_id.clone()),
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

    /// Submits a registered action whose id is one of `ids`; false when the engine offers none.
    fn invoke(&mut self, ids: &[String]) -> bool {
        let found = self.replica.boundary().and_then(|b| {
            b.actions
                .iter()
                .find(|a| ids.iter().any(|i| a.id.eq_ignore_ascii_case(i)))
        });
        let Some(action_id) = found.map(|a| a.id.clone()) else {
            return false;
        };
        self.submit(Operation::Action { action_id })
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
        // Like Tiles, the centre square is `size / 2`: the engine's window is centred on the view point
        // (the avatar, or the look cursor that pans it), so every shown square is one a click can select.
        let (cx, cy) = self.replica.view().map_or((ax, ay), |v| {
            (
                v.min.x + (v.max.x - v.min.x + 1) / 2,
                v.min.y + (v.max.y - v.min.y + 1) / 2,
            )
        });
        self.origin = Some((cx - size.x / 2, cy - size.y / 2));
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
    /// of the same size makes it publish the open screen again, now with or without them. A repeated
    /// hello ends the stream, so a new subscription follows to receive that screen.
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
        let params = protocol::HelloParams::new("bn-client")
            .with_tiles(self.tiles)
            .with_tint_ids(self.tint_ids.clone());
        self.call(
            "bn.hello",
            json!(self.with_session(params)),
            Pending::Retile,
        );
        let (cols, rows) = (self.declared.x as u32, self.declared.y as u32);
        self.call(
            "bn.viewport",
            json!({"cols": cols, "rows": rows}),
            Pending::Viewport,
        );
        self.subscribe();
    }

    /// The enabled affordance of the map that is the registered action `id`.
    fn map_affordance(&self, id: &str) -> Option<&protocol::Affordance> {
        let map = self
            .replica
            .stack()
            .last()
            .filter(|s| s.role == protocol::ScreenRole::Map)?;
        map.affordances
            .iter()
            .find(|a| a.enabled && a.id.eq_ignore_ascii_case(id))
    }

    fn refresh_actions(&mut self) {
        let offered = self.replica.boundary().map_or(&[][..], |b| &b.actions);
        if !offered.is_empty() || !self.busy {
            // An action with no keys is offered only where the map's affordances run it.
            self.shown = offered
                .iter()
                .filter(|a| {
                    a.keys.as_ref().is_none_or(|keys| !keys.is_empty())
                        || self.map_affordance(&a.id).is_some()
                })
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
        let row = |look: &Look, line: String, item: ItemRow| (line, item, self.item_cell(look));
        let ground = match avatar.and_then(|a| a.ground.as_ref()) {
            Some(stacks) => stacks
                .iter()
                .map(|i| {
                    row(
                        &i.look,
                        i.label.clone(),
                        (i.id.clone(), i.count.unwrap_or(1)),
                    )
                })
                .collect(),
            None => self
                .avatar_pos()
                .and_then(|p| self.replica.cell(&p.dim, p.x, p.y, p.z))
                .map_or(vec![], |c| {
                    c.items
                        .iter()
                        .map(|l| row(l, look_name(l), (String::new(), 1)))
                        .collect()
                }),
        };
        let carried = avatar.map_or(vec![], |a| {
            a.inventory
                .iter()
                .flatten()
                .map(|i| {
                    row(
                        &i.look,
                        i.label.clone(),
                        (i.id.clone(), i.count.unwrap_or(1)),
                    )
                })
                .collect()
        });
        (self.ground, self.ground_items, self.ground_cells) = unzip3(ground);
        (self.inventory, self.inventory_items, self.inventory_cells) = unzip3(carried);
    }

    /// How a list row draws its item: the same layers a map square gets for it.
    fn item_cell(&self, look: &Look) -> Cell {
        Cell {
            layers: vec![self.layer(look, 0)],
            ..self.styled(look, look_name(look))
        }
    }
}

fn unzip3<A, B, C>(rows: Vec<(A, B, C)>) -> (Vec<A>, Vec<B>, Vec<C>) {
    let (mut a, mut b, mut c) = (vec![], vec![], vec![]);
    for (x, y, z) in rows {
        a.push(x);
        b.push(y);
        c.push(z);
    }
    (a, b, c)
}

/// The engine's localized reason for a failed call: its `message` in the error data, else the RPC message.
fn reason_of(e: &protocol::RpcError) -> String {
    e.data
        .as_ref()
        .and_then(|d| d["message"].as_str())
        .map_or_else(|| e.message.clone(), String::from)
}

/// A hello the engine cannot accept from this client at all: another protocol version, or none.
fn incompatible(e: &protocol::RpcError) -> bool {
    e.code == METHOD_NOT_FOUND
        || e.code == INVALID_PARAMS
        || e.data.as_ref().map(error_kind).as_deref() == Some("negotiation_failed")
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
        // A square without a floor, through which the game draws the square the engine names below.
        let below = world.and_then(|c| c.below);
        let level = |z: i32| self.replica.cell(&p.dim, p.x, p.y, z);
        let lower = below.and_then(|d| level(p.z - i32::from(d)));
        // The levels from above the drawn square up to this one draw only what is on them, no ground.
        let over = below.map_or(vec![], |d| {
            (p.z - i32::from(d) + 1..=p.z).filter_map(level).collect()
        });
        let mut layers = match below {
            Some(_) => lower.map_or(vec![], |c| self.layers(c, true)),
            None => world.map_or(vec![], |c| self.layers(c, true)),
        };
        layers.extend(over.iter().flat_map(|c| self.layers(c, false)));
        // The square drawn there: the level below through open air.
        let shown = lower.or(world);
        if self.avatar_pos() == Some(&p)
            && let Some(avatar) = self.replica.avatar()
            && let Some(look) = &avatar.look
        {
            let seed = u32::from(avatar.name.bytes().next().unwrap_or(0));
            layers.push(self.layer(look, seed));
            layers.extend(self.overlays(look, &avatar.overlays, &avatar.traits, seed));
        }
        // A creature here, else the nearest one on the levels seen through.
        let on = |at: &WorldPos| {
            self.replica
                .entities()
                .find(|e| &e.at == at && e.look.is_some())
        };
        let entity = on(&p).or_else(|| {
            over.iter()
                .rev()
                .copied()
                .chain(lower)
                .find_map(|c| on(&c.at))
        });
        if let Some(e) = entity {
            let look = e.look.as_ref().expect("filtered on look");
            let desc = e.name.clone().unwrap_or_else(|| look_name(look));
            layers.push(self.layer(look, id_seed(&e.id)));
            layers.extend(self.overlays(look, &e.overlays, &e.traits, id_seed(&e.id)));
            layers.extend(self.attitude(e));
            layers.extend(
                lower
                    .iter()
                    .chain(&over)
                    .filter_map(|c| self.revival(Some(*c))),
            );
            layers.extend(self.revival(world.filter(|_| below.is_none())));
            return Cell {
                layers,
                remembered: false,
                light: shown
                    .and_then(|c| c.light.as_ref()?.as_u64())
                    .map(|l| l as u8),
                depth: below.unwrap_or(0),
                ..self.styled(look, desc)
            };
        }
        layers.extend(
            lower
                .iter()
                .chain(&over)
                .filter_map(|c| self.revival(Some(*c))),
        );
        layers.extend(self.revival(world.filter(|_| below.is_none())));
        match world.filter(|_| below.is_none()) {
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
            None => {
                // What is on the levels seen through, else the square drawn below, describes it.
                let look = over
                    .iter()
                    .rev()
                    .find_map(|c| on_top(c))
                    .or_else(|| lower.and_then(top_look));
                Cell {
                    layers,
                    remembered: lower.is_some_and(|c| c.known == Known::Remembered),
                    depth: below.unwrap_or(0),
                    light: lower.and_then(|c| c.light.as_ref()?.as_u64().map(|l| l as u8)),
                    ..look.map_or(unknown, |l| self.styled(l, look_name(l)))
                }
            }
        }
    }

    /// What a cell draws, bottom first, in the order of the native view: terrain, furniture, trap,
    /// field, item, vehicle part; a cell only remembered draws its memorized terrain and overlay.
    fn layers(&self, c: &WorldCell, ground: bool) -> Vec<Layer> {
        let seed = square_seed(&c.at);
        // The displayed field may hide the items under it, and so their highlight.
        let hidden = c.fields.last().is_some_and(|f| f.hides_items);
        let live = [
            c.terrain.as_ref().filter(|_| ground),
            c.furniture.as_ref().filter(|_| ground),
            c.traps.last().filter(|_| ground),
            c.fields.last().map(|f| &f.look),
            c.items.last().filter(|_| !hidden),
            c.vehicle.as_ref(),
        ];
        // A level drawn without its ground draws no memory either.
        let looks = match live.iter().any(Option::is_some) || !ground {
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

impl EngineModel {
    /// The generic menu an interaction shows.
    fn menu_of(&self, i: &protocol::Interaction) -> Menu {
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
        Menu {
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
                    columns: c.columns.clone(),
                    favorite: c.favorite,
                    color: c.color.clone().unwrap_or_default(),
                    key_color: c.key_color.clone().unwrap_or_default(),
                    markup: c.markup.clone().unwrap_or_default(),
                    action: c.action_id.clone().unwrap_or_default(),
                    nest_color: c.nest_color.clone().unwrap_or_default(),
                    mark: c.mark.clone(),
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
                        placement: if text("role") == "pickup" {
                            text("area_id")
                        } else {
                            String::new()
                        },
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
                    }
                })
                .collect(),
            can_cancel: i.allow_cancel,
            pane_tabs: pane_labels
                .iter()
                .filter(|p| matches!(p["role"].as_str(), Some("current" | "inactive")))
                .map(|p| {
                    let text = |k: &str| p[k].as_str().unwrap_or_default().to_string();
                    (text("id"), text("label"), p["role"] == "current")
                })
                .collect(),
            pane: all()
                .find(|c| tab(c) && c.selected)
                .and_then(|c| pane_of(&c.label).map(String::from)),
            screen: i.kind == "custom",
            vehicle: i.vehicle.clone(),
            canvas: i.canvas.as_ref().map(|wire| self.canvas(wire)),
            focus: i
                .compat
                .as_ref()
                .and_then(|c| c["focus"]["choice_id"].as_str())
                .map(String::from),
            sections: pane_labels
                .iter()
                .filter_map(|p| Some((p["id"].as_str()?.into(), p["label"].as_str()?.into())))
                .collect(),
        }
    }
}

/// What a target screen other than a shot says: the step of a zone's area (`message`), for the map
/// editor the rows of the square under its cursor and the keys of its edit actions, and what the
/// native look window prints of the square under the cursor.
fn target_note(i: &protocol::Interaction) -> Option<String> {
    i.target.as_ref().filter(|_| i.overmap.is_none())?;
    let rows = |pane: &str| {
        i.choices
            .iter()
            .filter(|c| c.pane_id.as_deref() == Some(pane))
            .collect::<Vec<_>>()
    };
    let tile = rows("tile")
        .iter()
        .map(|c| format!("{} {}", c.label, markup::plain(&c.description)))
        .collect::<Vec<_>>()
        .join(", ");
    let keys = rows("actions")
        .iter()
        .filter(|c| !c.hotkey.as_deref().unwrap_or_default().is_empty())
        .map(|c| format!("{} {}", c.hotkey.as_deref().unwrap_or_default(), c.label))
        .collect::<Vec<_>>()
        .join(", ");
    let described = i
        .target
        .as_ref()
        .and_then(|t| t["description"].as_array())
        .map_or_else(String::new, |lines| {
            let lines = lines.iter().filter_map(Value::as_str).map(markup::plain);
            lines.collect::<Vec<_>>().join(" / ")
        });
    let note = [markup::plain(&i.message), tile, keys, described]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(". ");
    (!note.is_empty()).then(|| format!("{}: {note}", markup::plain(&i.title)))
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
                engine: None,
            });
        }
        let now = self.clock.now();
        self.sounds
            .extend(self.replica.take_sounds().into_iter().map(|s| Heard {
                text: s.text,
                direction: s.direction,
                distance: s.distance,
                at: now,
            }));
        self.sounds
            .drain(..self.sounds.len().saturating_sub(MAX_SOUNDS));
        for line in self.replica.take_lines() {
            let line = LogLine {
                color: line.color.map(|name| self.colors.resolve(&name).0),
                id: line.id,
                text: line.text,
                count: line.count,
                label: line.label,
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
            self.sounds.clear();
            self.native.clear();
        }
        self.at_main_menu = at_main_menu;
        self.send_resend();
        self.send_viewport();
        self.send_tiles();
        self.refresh_view();
        self.refresh_items();
        self.refresh_actions();
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
                let options = ConnectOptions {
                    tiles: self.tiles,
                    max_hz: self.max_hz,
                };
                let mut fresh = EngineModel::open(transport, options)
                    .with_clock(self.clock.clone())
                    .with_colors(self.colors.clone())
                    .with_stall(self.stall);
                fresh.engine = self.engine.clone();
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

    fn ground(&self) -> &[String] {
        &self.ground
    }

    fn inventory(&self) -> &[String] {
        &self.inventory
    }

    fn ground_cells(&self) -> &[Cell] {
        &self.ground_cells
    }

    fn inventory_cells(&self) -> &[Cell] {
        &self.inventory_cells
    }

    fn pick_up(&mut self, index: usize) {
        if let (false, Some((item_id, count))) = (self.busy, self.ground_items.get(index).cloned())
            && !item_id.is_empty()
        {
            self.submit(Operation::Pickup { item_id, count });
        }
    }

    fn drop_item(&mut self, index: usize) {
        if let (false, Some((item_id, count))) =
            (self.busy, self.inventory_items.get(index).cloned())
            && !item_id.is_empty()
        {
            self.submit(Operation::Drop { item_id, count });
        }
    }

    fn messages(&self) -> &[String] {
        &self.messages
    }

    fn sounds(&self) -> Vec<crate::sounds::SoundLine> {
        crate::sounds::grouped(&self.sounds, self.clock.now())
    }

    fn sidebar(&self) -> Vec<crate::sidebar::Line> {
        self.replica
            .avatar()
            .map_or(vec![], |a| crate::sidebar::lines(a, &self.colors))
    }

    fn sidebar_layout(&self) -> Option<crate::sidebar::Layout> {
        let avatar = self.replica.avatar()?;
        crate::sidebar::layout(avatar, &self.colors, |look| self.item_cell(look))
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

    fn travel(&mut self, at: Pos) {
        let free = self
            .replica
            .boundary()
            .is_some_and(|b| b.interaction.is_none());
        if let (false, true, Some(pos)) = (self.busy, free, self.abs(at)) {
            self.submit(Operation::Travel { pos });
        }
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

    /// A registered action of the map is its screen-scoped affordance: the engine queues it behind the
    /// command in flight, and says `stale_screen` for one that outlived the map. Any other action waits
    /// for the engine to be free.
    fn invoke_action(&mut self, id: &str) -> bool {
        let offered = self.map_affordance(id).map(|a| a.id.clone());
        match offered {
            Some(affordance_id) => self
                .send(Operation::Invoke {
                    affordance_id,
                    args: None,
                })
                .is_ok(),
            None => !self.busy && self.invoke(&[id.into()]),
        }
    }

    fn key_bound(&self, key: &str) -> bool {
        self.action_for(key).is_some()
    }

    fn press(&mut self, key: &str) -> bool {
        if let Some(id) = self.action_for(key) {
            return self.invoke_action(&id);
        }
        // As in a native list, a key the menu binds to no action selects the enabled row showing it.
        if let Some(row) = self.row_for(key) {
            return !self.busy && self.submit(Operation::Choose { choice_id: row.id });
        }
        // A "press any key" screen has no key to bind: whatever is pressed takes the choice the engine names.
        let any_key = self
            .replica
            .boundary()
            .and_then(|b| b.interaction.as_ref())
            .and_then(|i| i.any_key_choice_id.clone());
        any_key.is_some_and(|choice_id| !self.busy && self.submit(Operation::Choose { choice_id }))
    }

    fn menu(&self) -> Option<Menu> {
        profiling::scope!("model.menu");
        let i = self.replica.boundary()?.interaction.as_ref()?;
        // A cursor the engine moves (look mode) is drawn on the map, not listed.
        (i.kind != "target").then(|| self.menu_of(i))
    }

    fn target_panel(&self) -> Option<Menu> {
        let i = self.replica.boundary()?.interaction.as_ref()?;
        (i.kind == "target" && i.overmap.is_none() && !i.choices.is_empty())
            .then(|| self.menu_of(i))
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
            title: markup::plain(&i.title),
            cols: wire.cols as i32,
            rows: wire.rows as i32,
            cells,
            origin: at(&wire.origin),
            cursor: at(&cursor),
            player: at(&wire.player),
            legend: wire.legend.clone(),
            cursor_description: wire.cursor_description.clone(),
            cursor_here: wire.cursor_here.clone(),
            cursor_note: wire.cursor_note.clone(),
            route_text: wire.route_text.clone(),
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

    fn colors(&self) -> &ColorMap {
        &self.colors
    }

    fn screens(&self) -> &[Screen] {
        self.replica.stack()
    }

    fn invoke_affordance(
        &mut self,
        affordance_id: &str,
        args: InvokeArgs,
    ) -> Result<(), InvokeError> {
        self.send(Operation::Invoke {
            affordance_id: affordance_id.into(),
            args: Some(args),
        })
    }

    fn more_rows(&mut self, component_id: &str) {
        let (Some(b), Some(at)) = (self.replica.boundary(), self.replica.at()) else {
            return;
        };
        let list = match b
            .stack
            .last()
            .and_then(|s| s.root.as_ref()?.find(component_id))
        {
            Some(protocol::Component::List(list)) => list,
            _ => return,
        };
        let (epoch, boundary_id) = (at.epoch.clone(), b.boundary_id.clone());
        let Some(page) = self.pages.next(&boundary_id, list) else {
            return;
        };
        let params = protocol::ChoicesParams {
            epoch,
            boundary_id,
            component_id: Some(page.component_id.clone()),
            offset: page.offset,
            limit: page.limit,
        };
        let id = self.call(
            "bn.interaction.choices",
            serde_json::to_value(params).expect("choices params serialize"),
            Pending::Rows,
        );
        self.row_requests.insert(id, page.component_id);
    }

    fn rows<'a>(&'a self, list: &'a protocol::List) -> Vec<&'a Row> {
        match self.replica.boundary() {
            Some(b) => self.pages.rows(&b.boundary_id, list),
            None => list.rows.iter().collect(),
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
            component_id: None,
            offset: loaded as u64,
            limit: protocol::PAGE_ROWS,
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
        if i.context != "TARGET" {
            return target_note(i);
        }
        let status = i.target.as_ref()?["status_text"].as_str()?;
        Some(format!("{}: {status}", markup::plain(&i.title)))
    }

    fn styled(&self, text: &str) -> Vec<Run> {
        markup::runs(text, &self.colors)
    }

    fn choose(&mut self, choice_id: &str) {
        let enabled = self
            .menu()
            .or_else(|| self.target_panel())
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

    fn aim(&mut self, at: Pos, click: bool) -> bool {
        let Some(here) = self.focus() else {
            return false;
        };
        if self.busy {
            return true;
        }
        match (here == at, self.abs(at)) {
            // The cursor is already there: a click confirms it, as Enter does; a hover leaves it.
            (true, _) => {
                if click {
                    self.invoke(&["CONFIRM".into()]);
                }
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
