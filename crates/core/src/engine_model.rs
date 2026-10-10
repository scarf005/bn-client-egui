//! `GameModel` backed by a live engine connection.

use crate::clock::Clock;
use crate::colors::ColorMap;
use crate::keys::KeyMap;
use crate::model::{
    ActionEntry, Cell, CommandStatus, GameModel, Layer, Loading, LogLine, Menu, MenuChoice, Pos,
    Rgb,
};
use crate::protocol::{
    self, Cell as WorldCell, Expect, Incoming, Known, LoadingNotice, Look, Operation, Snapshot,
    Submit, WorldPos,
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
}

pub struct EngineModel {
    transport: Box<dyn Transport>,
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
    trace: Vec<(Instant, &'static str)>,
    received: u64,
    colors: ColorMap,
    keys: KeyMap,
    loading: Option<Loading>,
    /// Actions on show: between auto-move steps the engine offers none, which must not flicker the list.
    shown: Vec<ActionEntry>,
    status: Option<CommandStatus>,
    /// Where the avatar stood when a travel click was sent; a click that plans and moves nothing found no route.
    travel_from: Option<WorldPos>,
}

fn look_name(l: &Look) -> String {
    l.id.clone().unwrap_or_else(|| l.glyph.clone())
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
    pub fn new(transport: Box<dyn Transport>) -> Self {
        let mut m = Self {
            transport,
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
            trace: vec![],
            received: 0,
            colors: ColorMap::default(),
            keys: KeyMap::default(),
            loading: None,
            shown: vec![],
            status: None,
            travel_from: None,
        };
        m.call(
            "bn.hello",
            json!(
                protocol::HelloParams::new("bn-client").with_viewport(protocol::Viewport {
                    cols: VIEW.x as u32,
                    rows: VIEW.y as u32,
                })
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

    /// Uses the game's own palette for look colours.
    pub fn with_colors(mut self, colors: ColorMap) -> Self {
        self.colors = colors;
        self
    }

    /// Uses the game's key settings to label and trigger actions.
    pub fn with_keys(mut self, keys: KeyMap) -> Self {
        self.keys = keys;
        self
    }

    /// Timestamped protocol milestones (`submit`, `received`, stage names, `events`) since the last call.
    /// How many engine messages have arrived so far; live tests use it to tell progress from a stall.
    pub fn received(&self) -> u64 {
        self.received
    }

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
                        "completed" => {
                            let from = self.travel_from.take();
                            let stuck = from.is_some()
                                && from.as_ref() == self.avatar_pos()
                                && self.replica.route().is_empty();
                            if stuck {
                                "done: no route there"
                            } else {
                                "done"
                            }
                            .into()
                        }
                        _ if swap => "done".into(),
                        "rejected" | "interrupted" => match reason.as_deref() {
                            Some(why) => format!("{}: {}", n.stage, explain(why)),
                            None => n.stage.clone(),
                        },
                        other => other.into(),
                    };
                    if let Some(s) = &mut self.status {
                        s.stage = stage;
                        s.finished = n.is_terminal();
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
            Incoming::Unknown(_) => {}
            Incoming::Malformed(e) => self.messages.push(format!("Bad engine message: {e}")),
            Incoming::Closed => {
                self.connected = false;
                self.busy = false;
                self.messages.push("Engine connection closed.".into());
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
                s.stage = format!("rejected: {}", explain(&kind));
                s.finished = true;
            }
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
                self.mark("hello");
                self.subscribe();
            }
            Pending::Subscribe => match serde_json::from_value::<Snapshot>(value) {
                Ok(s) => self.replica.begin(s),
                Err(e) => self.messages.push(format!("Bad snapshot: {e}")),
            },
            Pending::Viewport => {}
            Pending::Submit => {
                self.mark("received");
                self.command_id = value["command_id"].as_str().map(String::from);
                if let Some(s) = self.status.as_mut().filter(|s| s.stage == "sent") {
                    s.stage = "accepted".into();
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
            _ => format!("Command {stage}: {}.", reason.unwrap_or(stage)),
        };
        if self.messages.last() != Some(&text) {
            self.messages.push(text);
        }
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
        self.travel_from = matches!(submit.operation, Operation::Travel { .. })
            .then(|| self.avatar_pos().cloned())
            .flatten();
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
                (found.map(|c| c.label.clone()), Some(choice_id.clone()))
            }
            Operation::Action { action_id } => {
                let found = boundary.and_then(|b| b.actions.iter().find(|a| &a.id == action_id));
                (found.map(|a| a.name.clone()), Some(action_id.clone()))
            }
            Operation::Travel { .. } => (Some("Travel".into()), None),
            Operation::Cancel => (Some("Cancel".into()), None),
            _ => (None, None),
        };
        CommandStatus {
            label: label.unwrap_or_else(|| "Command".into()),
            target,
            stage: "sent".into(),
            finished: false,
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

    fn refresh_actions(&mut self) {
        let offered = self.replica.boundary().map_or(&[][..], |b| &b.actions);
        if !offered.is_empty() || !self.busy {
            self.shown = offered
                .iter()
                .map(|a| ActionEntry {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    keys: self.keys.keys(&a.id).to_vec(),
                })
                .collect();
        }
    }

    fn refresh_items(&mut self) {
        let here = self
            .avatar_pos()
            .and_then(|p| self.replica.cell(&p.dim, p.x, p.y, p.z));
        self.ground = here.map_or(vec![], |c| c.items.iter().map(look_name).collect());
        self.inventory = self.replica.avatar().map_or(vec![], |a| {
            a.inventory
                .iter()
                .flatten()
                .map(|item| match item.count {
                    Some(count) if count > 1 => format!("{} ({count})", item.name),
                    _ => item.name.clone(),
                })
                .collect()
        });
    }
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
            glyph: glyph(look),
            color,
            bg,
        }
    }

    /// A character's overlays, which face as it does; an overlay without a sprite draws nothing.
    fn overlays(&self, body: &Look, overlays: &[Look], seed: u32) -> Vec<Layer> {
        overlays
            .iter()
            .map(|overlay| Layer {
                mirrored: body.facing.as_deref() == Some("left"),
                creature: true,
                glyph: '\0',
                ..self.layer(overlay, seed)
            })
            .collect()
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
            .map(|l| {
                let seeded = !matches!(l.kind.as_str(), "item" | "trap");
                self.layer(l, if seeded { seed } else { 0 })
            })
            .collect()
    }
}

impl GameModel for EngineModel {
    fn poll(&mut self) {
        while let Some(msg) = self.transport.recv() {
            self.received += 1;
            self.handle(msg);
        }
        self.messages.extend(self.replica.take_log());
        for line in self.replica.take_lines() {
            let line = LogLine {
                color: line.color.map(|name| self.colors.resolve(&name).0),
                id: line.id,
                text: line.text,
                count: line.count,
            };
            // A repeat replaces the line it counts; ids are never reused, so search the tail only.
            match self
                .native
                .iter_mut()
                .rev()
                .take(8)
                .find(|l| !line.id.is_empty() && l.id == line.id)
            {
                Some(old) => *old = line,
                None => self.native.push(line),
            }
        }
        self.send_viewport();
        self.refresh_view();
        self.refresh_items();
        self.refresh_actions();
    }

    fn set_waker(&mut self, waker: crate::transport::Waker) {
        self.transport.set_waker(waker);
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

    fn size(&self) -> Pos {
        if self.replica.ready() && self.origin.is_some() {
            self.shown()
        } else {
            Pos { x: 0, y: 0 }
        }
    }

    fn cell(&self, at: Pos) -> Cell {
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
        let mut layers = world.map_or(vec![], |c| self.layers(c));
        if self.avatar_pos() == Some(&p)
            && let Some(avatar) = self.replica.avatar()
            && let Some(look) = &avatar.look
        {
            let seed = u32::from(avatar.name.bytes().next().unwrap_or(0));
            layers.push(self.layer(look, seed));
            layers.extend(self.overlays(look, &avatar.overlays, seed));
        }
        let entity = self
            .replica
            .entities()
            .find(|e| e.at == p && e.look.is_some());
        if let Some(e) = entity {
            let look = e.look.as_ref().expect("filtered on look");
            let desc = e.name.clone().unwrap_or_else(|| look_name(look));
            layers.push(self.layer(look, id_seed(&e.id)));
            layers.extend(self.overlays(look, &e.overlays, id_seed(&e.id)));
            return Cell {
                layers,
                remembered: false,
                light: world
                    .and_then(|c| c.light.as_ref()?.as_u64())
                    .map(|l| l as u8),
                ..self.styled(look, desc)
            };
        }
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
            None => unknown,
        }
    }

    fn season(&self) -> Option<String> {
        self.replica.environment()?.season.clone()
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

    fn pick_up(&mut self, _index: usize) {
        if !self.busy {
            self.invoke(&["pickup".into(), "pick_up".into()]);
        }
    }

    fn drop_item(&mut self, _index: usize) {
        if !self.busy {
            self.invoke(&["drop".into()]);
        }
    }

    fn messages(&self) -> &[String] {
        &self.messages
    }

    fn native_log(&self) -> &[LogLine] {
        &self.native
    }

    fn say(&mut self, text: String) {
        self.messages.push(text);
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
        let (me, desc) = (self.player(), self.cell(at).desc);
        let step = (at.x - me.x, at.y - me.y);
        if desc.contains("door") && desc.ends_with("_c") && step.0.abs().max(step.1.abs()) == 1 {
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

    fn press(&mut self, key: &str) -> bool {
        let offered = self.replica.boundary().and_then(|b| {
            let bound = self.keys.actions_for(key);
            b.actions.iter().find(|a| bound.contains(&a.id.as_str()))
        });
        let id = offered.map(|a| a.id.clone());
        id.is_some_and(|id| self.invoke_action(&id))
    }

    fn menu(&self) -> Option<Menu> {
        let i = self.replica.boundary()?.interaction.as_ref()?;
        let title = [&i.title, &i.context, &i.kind]
            .into_iter()
            .find(|s| !s.is_empty());
        Some(Menu {
            title: title.cloned().unwrap_or_default(),
            message: i.message.clone(),
            choices: i
                .choices
                .iter()
                .map(|c| MenuChoice {
                    id: c.id.clone(),
                    label: c.label.clone(),
                    enabled: c.enabled && c.selectable,
                    denial: c.denial.clone(),
                    selected: c.selected,
                    group: c.pane_id.clone(),
                })
                .collect(),
            can_cancel: i.allow_cancel,
        })
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

    fn cancel_menu(&mut self) {
        if self.menu().is_some_and(|m| m.can_cancel) && !self.busy {
            self.submit(Operation::Cancel);
        }
    }
}
