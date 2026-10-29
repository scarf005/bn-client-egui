//! `GameModel` backed by a live engine connection.

mod commands;
mod effects;
mod items;
mod log;
mod map;
mod menu;
mod reads;
mod responses;
mod session;
mod view;

pub use session::ConnectOptions;

use crate::clock::Clock;
use crate::colors::ColorMap;
use crate::interaction::{self, InvokeError, Pages};
use crate::localise::{self, Catalogue, Value as Arg, tr_format, tr_plural};
use crate::markup::{self, Run};
use crate::model::{
    ActionEntry, Cell, CommandStatus, Effect, Failure, FailureKind, Follow, GameModel, Glow,
    Health, Heard, Layer, Loading, LogLine, MapMark, Mutation, OVERMAP_LEVELS, OvermapWatch,
    PanelWatch, Pellet, Pos, Rgb, Shape,
};
use crate::protocol::{
    self, Cell as WorldCell, Entity, InvokeArgs, Known, Look, ModTileset, Operation, PanelKind,
    Row, Screen, Traits, WorldPos,
};
use crate::replica::Replica;
use crate::transport::Transport;
use effects::Volley;
use items::{ItemRow, ids, row_of};
use map::{look_name, square_seed};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use view::target_note;

/// The map window asked for until the UI reports its own size.
const VIEW: Pos = Pos { x: 40, y: 25 };
/// The engine takes up to this many cells on a side.
const MAX_VIEWPORT: i32 = 512;
/// A resized panel is sent to the engine once it has stayed this size for this long.
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
    /// The screen of a panel window.
    Panel(PanelKind),
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
    /// The multishot whose pellets are being told.
    volley: Option<Volley>,
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
    /// The request for `hovered` whose answer is awaited; at most one is in flight.
    describing: Option<(u64, (String, String))>,
    /// The latest square to describe once nothing is in flight and no command runs.
    describe_wanted: Option<WorldPos>,
    /// The boundary and revision of the question the engine refused as stale; not asked again until they change.
    describe_refused: Option<(String, String)>,
    overmap: Watched,
    /// The panel windows open.
    panels: HashMap<PanelKind, WatchedPanel>,
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

/// A panel window: the row its focus is on, the read last asked for it and the screen read.
#[derive(Default)]
struct WatchedPanel {
    watch: PanelWatch,
    /// The watch, the epoch and the revision of the last read, which is not asked for again.
    asked: Option<(PanelWatch, String, String)>,
    /// The request whose answer is awaited; any other answer is stale.
    waiting: Option<u64>,
    view: Option<Screen>,
}

struct Hover {
    pos: WorldPos,
    revision: String,
    /// None while the answer is outstanding.
    parts: Option<Vec<protocol::TilePart>>,
    /// What the avatar can do with the square; empty until the first answer for it.
    actions: Vec<protocol::TileAction>,
}

impl GameModel for EngineModel {
    fn poll(&mut self) {
        profiling::scope!("model.poll");
        localise::install(self.catalogue.clone());
        localise::install_client(self.chrome.clone());
        let now = self.clock.now();
        self.expire_messages(now);
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
        self.take_log();
        self.send_resend();
        self.read_overmap();
        self.read_panels();
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

    fn sky(&self) -> Option<protocol::Sky> {
        self.replica.environment()?.sky.clone()
    }

    fn mod_tilesets(&self) -> &[ModTileset] {
        self.replica
            .environment()
            .map_or(&[], |e| e.mod_tilesets.as_slice())
    }

    fn player(&self) -> Pos {
        let at = self.avatar_pos().and_then(|a| self.local(a));
        at.unwrap_or(Pos { x: 0, y: 0 })
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

    fn wielded_cell(&self) -> Option<Cell> {
        let carried = self.replica.avatar()?.inventory.as_ref()?;
        let held = carried
            .iter()
            .find(|i| i.slot.as_deref() == Some("wielded"))?;
        Some(self.item_cell(&held.look))
    }

    fn ground_ids(&self) -> Vec<String> {
        ids(&self.ground_items)
    }

    fn inventory_ids(&self) -> Vec<String> {
        ids(&self.inventory_items)
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

    fn modes(&self) -> Vec<crate::protocol::Mode> {
        self.replica
            .avatar()
            .map_or(vec![], |avatar| avatar.modes.clone())
    }

    fn set_mode(&mut self, mode: &str, value: &str) -> bool {
        !self.busy
            && self.free()
            && self.submit(Operation::SetMode {
                mode: mode.into(),
                value: value.into(),
            })
    }

    fn sidebar(&self) -> Vec<crate::sidebar::Line> {
        self.replica
            .avatar()
            .map_or(vec![], |a| crate::sidebar::lines(a, &self.colors))
    }

    fn look_cell(&self, look: &Look) -> Option<Cell> {
        Some(self.item_cell(look))
    }

    fn looks_cell(&self, looks: &[Look], seed: u32) -> Option<Cell> {
        let top = looks.last()?;
        Some(Cell {
            layers: looks.iter().map(|l| self.layer(l, seed)).collect(),
            ..self.styled(top, look_name(top))
        })
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
        self.told_effects()
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
        if let Some(pos) = self.open_square(at) {
            self.submit(Operation::Travel { pos });
        }
    }

    fn route(&self) -> Vec<Pos> {
        let route = self.replica.route().iter();
        route.filter_map(|p| self.local(p)).collect()
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

    fn watch_panel(&mut self, panel: PanelKind, watch: Option<PanelWatch>) {
        match watch {
            Some(watch) => self.panels.entry(panel).or_default().watch = watch,
            None => {
                self.panels.remove(&panel);
            }
        }
    }

    fn watched_panel(&self, panel: PanelKind) -> Option<&Screen> {
        self.panels.get(&panel)?.view.as_ref()
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
        self.local(&at)
    }

    fn marks(&self) -> Vec<MapMark> {
        let Some(z) = self.shown_z() else {
            return vec![];
        };
        // A list over the map editor (the mapgen previews) leaves the editor's marks on the map.
        let stack = self.replica.stack().iter().rev();
        let Some(map) = stack.filter_map(|s| s.root.as_ref()?.cursor_map()).next() else {
            return vec![];
        };
        let mark = |role, min, max| {
            Some(MapMark {
                role,
                min: self.local(min)?,
                max: self.local(max)?,
            })
        };
        map.marks()
            .iter()
            .flat_map(|m| {
                let squares = m.squares.iter().filter(|p| p.z == z);
                let squares = squares.map(|p| mark(m.role, p, p));
                let rect = m.rect.iter().filter(|r| (r.min.z..=r.max.z).contains(&z));
                squares
                    .chain(rect.map(|r| mark(m.role, &r.min, &r.max)))
                    .collect::<Vec<_>>()
            })
            .flatten()
            .collect()
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
        if let Some(pos) = self.open_square(at) {
            self.submit(Operation::Context { pos });
        }
        true
    }

    fn tile_action(&mut self, at: Pos, action_id: &str) -> bool {
        self.open_square(at).is_some_and(|pos| {
            self.submit(Operation::TileAction {
                action_id: action_id.into(),
                pos,
            })
        })
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
