//! `GameModel` backed by a live engine connection.

mod commands;
mod map;
mod menu;
mod responses;
mod session;

pub use session::ConnectOptions;

use crate::clock::Clock;
use crate::colors::ColorMap;
use crate::interaction::{self, InvokeError, Pages};
use crate::localise::{self, Catalogue, Value as Arg, tr_format, tr_plural};
use crate::markup::{self, Run};
use crate::model::{
    ActionEntry, Cell, CommandStatus, Effect, Failure, FailureKind, GameModel, Glow, Health, Heard,
    Layer, Loading, LogLine, Mutation, OvermapWatch, Pos, Rgb, Shape,
};
use crate::protocol::{
    self, Cell as WorldCell, Entity, InvokeArgs, Known, Look, ModTileset, Operation, Row, Screen,
    Traits, WorldPos,
};
use crate::replica::Replica;
use crate::transport::Transport;
use map::{look_name, square_seed};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The map window asked for until the UI reports its own size.
const VIEW: Pos = Pos { x: 40, y: 25 };
/// A resized panel is sent to the engine once it has stayed this size for this long.
/// The engine takes up to this many cells on a side.
const MAX_VIEWPORT: i32 = 512;
/// Heard sounds kept; older ones have long faded.
const MAX_SOUNDS: usize = 50;
/// The log lines kept: the engine contract keeps the newest 100 (`messages` in the snapshot).
const MAX_LOG_LINES: usize = 100;
const RESIZE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Clone, Copy)]
enum Pending {
    Hello,
    Subscribe,
    Submit,
    Viewport,
    /// `bn.hold` or `bn.release`; a refusal only ends the hold.
    Hold,
    Interrupt,
    /// The outcome of the command in flight, asked for when the engine says it has nothing to interrupt.
    Outcome,
    /// A hello that only changes whether the engine sends tile ids.
    Retile,
    Describe,
    Choices,
    /// The rows of a list of the top screen, past its first page; the list is in `row_requests`.
    Rows,
    /// The squares of the client's map window.
    Overmap,
}

/// The choices past the first page of the interaction with this boundary and schema.
#[derive(Default)]
struct Paged {
    key: (String, String),
    more: Vec<protocol::Choice>,
    /// A page was asked for and has not arrived.
    waiting: bool,
    /// The engine refused a page; no more are asked for in this boundary.
    closed: bool,
}

/// Starts the engine again on the same profile.
pub type Respawn = Box<dyn FnMut() -> std::io::Result<Box<dyn Transport>>>;

/// The frame rate a client asks the engine to publish at while an activity runs, unless it knows better.
pub const DEFAULT_MAX_HZ: u16 = 60;
/// Without a message for this long while a request is outstanding, the engine counts as stalled.
const STALL: Duration = Duration::from_secs(120);

pub struct EngineModel {
    /// Whether the client draws tiles now, and what the engine was last told.
    tiles: bool,
    tiles_told: bool,
    /// The most frames per second the client shows, asked of the engine in every hello.
    max_hz: u16,
    /// The language the client asked the engine to word its text in, asked in every hello.
    locale: Option<String>,
    /// The game root whose catalogues word the engine's LocalisedStrings, and the catalogue of the locale the
    /// last hello granted.
    data_root: Option<std::path::PathBuf>,
    catalogue: Option<Arc<Catalogue>>,
    /// The client's own catalogue for the locale asked for, then for the one the last hello granted.
    chrome: Option<Arc<Catalogue>>,
    /// The ids of the tints the client's tileset has, and what the engine was last told.
    tint_ids: Vec<String>,
    tints_told: Vec<String>,
    /// The engine's `USE_TILES` and `TILES` options from its hello.
    display: Option<(bool, String)>,
    /// The sound pack folder from the hello.
    soundpack: Option<std::path::PathBuf>,
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
    /// The affordance the engine was told to hold, until the key comes up.
    holding: Option<String>,
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
    /// The client's own lines (command results, errors), newest last, and when each was told.
    messages: Vec<String>,
    told: Vec<Duration>,
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
    overmap: Watched,
}

/// The client's map window: what it asks to see, what was last asked for it and the squares read.
#[derive(Default)]
struct Watched {
    watch: Option<OvermapWatch>,
    /// The watch, the revision and the centre square of the last read, which is not asked for again.
    asked: Option<(OvermapWatch, String, WorldPos)>,
    waiting: bool,
    view: Option<protocol::OvermapWire>,
}

struct Hover {
    pos: WorldPos,
    revision: String,
    /// None while the answer is outstanding.
    parts: Option<Vec<protocol::TilePart>>,
    /// What the avatar can do with the square; empty until the first answer for it.
    actions: Vec<protocol::TileAction>,
}

/// The engine's id of an item row and how many items it stands for.
type ItemRow = (String, u64);

/// How long a client line stays in the log.
const MESSAGE_LIFE: Duration = Duration::from_secs(20);

/// The row the engine names `item_id`; none for an empty id or a row that is gone.
fn row_of(rows: &[ItemRow], item_id: &str) -> Option<ItemRow> {
    rows.iter()
        .find(|(id, _)| !id.is_empty() && id == item_id)
        .cloned()
}

/// Native registered-action id for a one-square move.
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

    /// The world square of a fact on the shown level; the view places it when it draws it.
    fn level_square(&self, square: &Value) -> Option<Pos> {
        let at = |k: &str| square[k].as_i64().map(|v| v as i32);
        (at("z")? == self.shown_z()?).then(|| Pos {
            x: at("x").unwrap_or_default(),
            y: at("y").unwrap_or_default(),
        })
    }

    fn effect_of(&self, fact: &crate::replica::Fact) -> Option<Effect> {
        let d = &fact.data;
        let squares = |key: &str| -> Vec<Pos> {
            d[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| self.level_square(s))
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
                at: self.level_square(&d["at"])?,
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
                at: self.level_square(&d["at"])?,
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

    /// The cursor map of the screen on top, when it puts a cursor on the streamed world.
    fn cursor_map(&self) -> Option<&protocol::MapComponent> {
        self.replica.stack().last()?.root.as_ref()?.cursor_map()
    }

    /// The affordance of the screen on top that puts the cursor on a square.
    fn cursor_move(&self) -> Option<String> {
        let top = self.replica.stack().last()?;
        top.root.as_ref()?.cursor_map()?;
        top.affordances
            .iter()
            .find(|a| a.args.as_ref().is_some_and(|args| args.pos.is_some()))
            .map(|a| a.id.clone())
    }

    fn avatar_pos(&self) -> Option<&WorldPos> {
        self.replica.avatar().map(|a| &a.at)
    }

    /// The level the map shows: the engine's view's, which a look cursor moves up and down, else the avatar's.
    fn shown_z(&self) -> Option<i32> {
        let a = self.avatar_pos()?;
        Some(self.replica.view().map_or(a.z, |v| v.min.z))
    }

    fn abs(&self, at: Pos) -> Option<WorldPos> {
        let (ox, oy) = self.origin?;
        let a = self.avatar_pos()?;
        Some(WorldPos {
            dim: a.dim.clone(),
            x: ox + at.x,
            y: oy + at.y,
            z: self.shown_z()?,
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

    /// Asks the engine what is on `pos`; the answer holds for the revision it was asked at.
    fn ask_describe(&mut self, pos: WorldPos) {
        let (Some(boundary), Some(now)) = (self.replica.boundary(), self.replica.at()) else {
            return;
        };
        let params = serde_json::to_value(protocol::DescribeParams {
            epoch: now.epoch.clone(),
            boundary_id: boundary.boundary_id.clone(),
            pos: pos.clone(),
        })
        .expect("describe params serialize");
        // The square's earlier answer stands until the new one comes, so its text does not flip to the bare
        // cell and back, nor its open menu lose its actions, with every revision.
        let (parts, actions) = self
            .hovered
            .take()
            .filter(|h| h.pos == pos)
            .map_or((None, Vec::new()), |h| (h.parts, h.actions));
        self.hovered = Some(Hover {
            pos,
            revision: now.revision.clone(),
            parts,
            actions,
        });
        self.call("bn.world.describe", params, Pending::Describe);
    }

    /// Reads the squares the map window shows once the world or the window changed. The player's square is the
    /// one the engine's last read gave, so a first read, or one after the player left the squares, is followed
    /// by another around where the player is.
    fn read_overmap(&mut self) {
        let (Some(watch), false) = (self.overmap.watch, self.overmap.waiting) else {
            return;
        };
        let (Some(now), Some(avatar)) = (self.replica.at(), self.avatar_pos()) else {
            return;
        };
        // Overmap squares name no dimension: they are the ones of the avatar's.
        let player = self.overmap.view.as_ref().map_or_else(
            || WorldPos {
                dim: String::new(),
                x: 0,
                y: 0,
                z: avatar.z,
            },
            |v| WorldPos {
                z: avatar.z,
                ..v.player.clone()
            },
        );
        let centre = WorldPos {
            x: player.x + watch.pan.x,
            y: player.y + watch.pan.y,
            ..player
        };
        let asked = (watch, now.revision.clone(), centre.clone());
        if self.overmap.asked.as_ref() == Some(&asked) {
            return;
        }
        let min = WorldPos {
            x: centre.x - watch.cols / 2,
            y: centre.y - watch.rows / 2,
            ..centre.clone()
        };
        let max = WorldPos {
            x: min.x + watch.cols - 1,
            y: min.y + watch.rows - 1,
            ..centre
        };
        let params = serde_json::to_value(protocol::WorldCellsParams {
            epoch: now.epoch.clone(),
            min,
            max,
        })
        .expect("overmap params serialize");
        self.overmap.asked = Some(asked);
        self.overmap.waiting = true;
        self.call("bn.overmap.cells", params, Pending::Overmap);
    }

    /// Asks again for the hovered square once its answer is older than the world, such as after the
    /// engine's window changed size, so a pointer that stays still keeps its description.
    fn refresh_hover(&mut self) {
        let stale = self.hovered.as_ref().filter(|h| {
            h.parts.is_some()
                && self
                    .replica
                    .at()
                    .is_some_and(|now| now.revision != h.revision)
        });
        let free = self
            .replica
            .boundary()
            .is_some_and(|b| b.interaction.is_none());
        if let (Some(pos), true) = (stale.map(|h| h.pos.clone()), free) {
            self.ask_describe(pos);
        }
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

/// What a target screen other than a shot says: the step of a zone's area (`message`), for the map
/// editor the rows of the square under its cursor and the keys of its edit actions.
fn target_note(i: &protocol::Interaction) -> Option<String> {
    i.target.as_ref()?;
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
    let note = [markup::plain(&i.message), tile, keys]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(". ");
    (!note.is_empty()).then(|| format!("{}: {note}", markup::plain(&i.title)))
}

impl GameModel for EngineModel {
    fn poll(&mut self) {
        profiling::scope!("model.poll");
        localise::install(self.catalogue.clone());
        localise::install_client(self.chrome.clone());
        let now = self.clock.now();
        // The client's lines stand beside the game's log for a while; they are not part of its history.
        let expired = self
            .told
            .iter()
            .take_while(|at| now.saturating_sub(**at) >= MESSAGE_LIFE)
            .count();
        self.messages.drain(..expired);
        self.told.drain(..expired);
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
                message: tr_plural(
                    "The engine has not answered for %d second.",
                    "The engine has not answered for %d seconds.",
                    self.stall.as_secs(),
                    &[Arg::Int(self.stall.as_secs() as i64)],
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
                turn: line.turn,
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
        self.native
            .drain(..self.native.len().saturating_sub(MAX_LOG_LINES));
        let at_main_menu = interaction::titled(self.replica.stack());
        if at_main_menu && !std::mem::replace(&mut self.at_main_menu, true) {
            self.messages.clear();
            self.told.clear();
            self.sounds.clear();
            self.native.clear();
        }
        self.at_main_menu = at_main_menu;
        self.send_resend();
        self.read_overmap();
        self.send_viewport();
        self.send_tiles();
        self.refresh_view();
        self.refresh_items();
        self.refresh_actions();
        self.refresh_hover();
    }

    fn set_waker(&mut self, waker: crate::transport::Waker) {
        self.transport.set_waker(waker.clone());
        self.waker = Some(waker);
    }

    fn display_default(&self) -> Option<(bool, String)> {
        self.display.clone()
    }

    fn soundpack(&self) -> Option<std::path::PathBuf> {
        self.soundpack.clone()
    }

    fn take_cues(&mut self) -> Vec<crate::audio::Cue> {
        self.replica.take_cues()
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
                    locale: self.locale.clone(),
                };
                let mut fresh = EngineModel::open(transport, options)
                    .with_clock(self.clock.clone())
                    .with_colors(self.colors.clone())
                    .with_data_root(self.data_root.clone())
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
                    f.message = tr_format(
                        "The engine could not be restarted: %s",
                        &[Arg::Text(e.to_string())],
                    );
                }
                false
            }
        }
    }

    fn origin(&self) -> Pos {
        let (x, y) = self.origin.unwrap_or_default();
        Pos { x, y }
    }

    fn map_version(&self) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let at = self.replica.at()?;
        let mut hash = std::hash::DefaultHasher::new();
        (
            &at.epoch,
            &at.sequence,
            self.origin,
            self.shown().x,
            self.shown().y,
        )
            .hash(&mut hash);
        Some(hash.finish())
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

    fn ground_ids(&self) -> Vec<String> {
        self.ground_items.iter().map(|(id, _)| id.clone()).collect()
    }

    fn inventory_ids(&self) -> Vec<String> {
        self.inventory_items
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn pick_up(&mut self, item_id: &str) {
        if let (false, Some((item_id, count))) = (self.busy, row_of(&self.ground_items, item_id)) {
            self.submit(Operation::Pickup { item_id, count });
        }
    }

    fn drop_item(&mut self, item_id: &str) {
        if let (false, Some((item_id, count))) = (self.busy, row_of(&self.inventory_items, item_id))
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

    fn activity(&self) -> Option<crate::protocol::Activity> {
        self.replica.avatar()?.activity.clone()
    }

    fn sidebar(&self) -> Vec<crate::sidebar::Line> {
        self.replica
            .avatar()
            .map_or(vec![], |a| crate::sidebar::lines(a, &self.colors))
    }

    fn look_cell(&self, look: &Look) -> Option<Cell> {
        Some(self.item_cell(look))
    }

    fn sidebar_layout(&self) -> Option<crate::sidebar::Layout> {
        let avatar = self.replica.avatar()?;
        crate::sidebar::layout(avatar, &self.colors, |look| self.item_cell(look))
    }

    fn native_log(&self) -> &[LogLine] {
        &self.native
    }

    fn say(&mut self, text: String) {
        self.tell(text);
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

    fn hold(&mut self, key: &str) -> bool {
        let Some(step) = self
            .action_for(key)
            .and_then(|id| self.map_affordance(&id))
            .filter(|a| a.holdable)
            .map(|a| a.id.clone())
        else {
            return false;
        };
        // The OS repeats a held key; the engine already walks.
        if self.holding.as_deref() == Some(step.as_str()) {
            return true;
        }
        let (Some(at), Some(screen), false) = (
            self.replica.at(),
            self.replica.stack().last(),
            self.subscribing,
        ) else {
            return false;
        };
        let params =
            json!({"epoch": at.epoch, "screen_id": screen.screen_id, "affordance_id": step});
        self.holding = Some(step);
        self.mark("hold");
        self.call("bn.hold", params, Pending::Hold);
        true
    }

    fn release_hold(&mut self) {
        if self.holding.take().is_some() {
            self.call("bn.release", json!({}), Pending::Hold);
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

    fn route_text(&self) -> Option<String> {
        self.replica.route_text().map(str::to_owned)
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

    fn colors(&self) -> &ColorMap {
        &self.colors
    }

    fn watch_overmap(&mut self, watch: Option<OvermapWatch>) {
        if watch.is_none() {
            self.overmap = Watched::default();
        }
        self.overmap.watch = watch;
    }

    fn watched_overmap(&self) -> Option<&protocol::OvermapWire> {
        self.overmap.view.as_ref()
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

    fn focus(&self) -> Option<Pos> {
        let target = match self.cursor_map() {
            Some(map) => map.target.clone()?,
            None => {
                let i = self.replica.boundary()?.interaction.as_ref()?;
                i.target.clone()?
            }
        };
        let at = serde_json::from_value::<WorldPos>(target["current"].clone()).ok()?;
        let (ox, oy) = self.origin?;
        Some(Pos {
            x: at.x - ox,
            y: at.y - oy,
        })
    }

    fn aiming(&self) -> Option<String> {
        let Some(map) = self.cursor_map() else {
            return target_note(self.replica.boundary()?.interaction.as_ref()?);
        };
        let lines = map
            .cursor_lines()
            .iter()
            .map(|l| markup::plain(l))
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>();
        let title = self.replica.stack().last()?.title.as_ref()?.shown();
        (!lines.is_empty()).then(|| format!("{}: {}", markup::plain(&title), lines.join(" / ")))
    }

    fn styled(&self, text: &str) -> Vec<Run> {
        markup::runs(text, &self.colors)
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
        self.ask_describe(pos);
    }

    fn hover_parts(&self, at: Pos) -> Option<Vec<protocol::TilePart>> {
        let pos = self.abs(at)?;
        let h = self.hovered.as_ref()?;
        (h.pos == pos).then(|| h.parts.clone()).flatten()
    }

    fn tile_actions(&mut self, at: Pos) -> Option<Vec<protocol::TileAction>> {
        self.hover(at);
        // Outside the map, without a game, or under a screen the map takes no action, so there is none to ask for.
        let (Some(pos), Some(now), Some(boundary)) =
            (self.abs(at), self.replica.at(), self.replica.boundary())
        else {
            return Some(Vec::new());
        };
        if boundary.interaction.is_some() {
            return Some(Vec::new());
        }
        let h = self.hovered.as_ref()?;
        (h.pos == pos && h.revision == now.revision && h.parts.is_some()).then(|| h.actions.clone())
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

    fn tile_action(&mut self, at: Pos, action_id: &str) -> bool {
        let free = self
            .replica
            .boundary()
            .is_some_and(|b| b.interaction.is_none());
        match (self.busy, free, self.abs(at)) {
            (false, true, Some(pos)) => self.submit(Operation::TileAction {
                action_id: action_id.into(),
                pos,
            }),
            _ => false,
        }
    }

    fn aim(&mut self, at: Pos, click: bool) -> bool {
        let Some(here) = self.focus() else {
            return false;
        };
        if self.busy {
            return true;
        }
        match (here == at, self.abs(at)) {
            // The cursor is already there: a click is the screen's Enter (throwing throws, looking confirms);
            // a hover leaves it.
            (true, _) => {
                if click {
                    self.press("RETURN");
                }
            }
            (false, Some(pos)) => match self.cursor_move() {
                Some(id) => {
                    let args = InvokeArgs {
                        pos: Some(pos),
                        ..InvokeArgs::default()
                    };
                    let _ = self.invoke_affordance(&id, args);
                }
                None => {
                    self.submit(Operation::SetTarget {
                        candidate_id: None,
                        pos: Some(pos),
                    });
                }
            },
            _ => {}
        }
        true
    }
}
