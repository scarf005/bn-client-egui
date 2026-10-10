//! `GameModel` backed by a live engine connection.

use crate::colors::ColorMap;
use crate::keys::KeyMap;
use crate::model::{
    ActionEntry, Cell, CommandStatus, GameModel, Loading, Menu, MenuChoice, Pos, Rgb,
};
use crate::protocol::{
    self, Cell as WorldCell, Expect, Incoming, LoadingNotice, Look, Operation, Snapshot, Submit,
    WorldPos,
};
use crate::replica::{Outcome, Replica};
use crate::transport::{Transport, request};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Instant;

/// The visible window onto the engine's much larger coverage.
const VIEW: Pos = Pos { x: 40, y: 25 };
/// The window re-centres when the avatar gets this close to its edge.
const MARGIN: i32 = 3;

#[derive(Clone, Copy)]
enum Pending {
    Hello,
    Subscribe,
    Submit,
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
    messages: Vec<String>,
    ground: Vec<String>,
    inventory: Vec<String>,
    trace: Vec<(Instant, &'static str)>,
    colors: ColorMap,
    keys: KeyMap,
    loading: Option<Loading>,
    /// Actions on show: between auto-move steps the engine offers none, which must not flicker the list.
    shown: Vec<ActionEntry>,
    status: Option<CommandStatus>,
}

fn look_name(l: &Look) -> String {
    l.id.clone().unwrap_or_else(|| l.glyph.clone())
}

fn glyph(l: &Look) -> char {
    l.glyph.chars().next().unwrap_or('?')
}

/// The topmost thing drawn in a cell: item, field, furniture, terrain, then remembered terrain.
fn top_look(c: &WorldCell) -> Option<&Look> {
    c.vehicle
        .as_ref()
        .or(c.items.last())
        .or(c.fields.last().map(|f| &f.look))
        .or(c.furniture.as_ref())
        .or(c.terrain.as_ref())
        .or(c.memory.as_ref().map(|m| &m.terrain))
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
            messages: vec![],
            ground: vec![],
            inventory: vec![],
            trace: vec![],
            colors: ColorMap::default(),
            keys: KeyMap::default(),
            loading: None,
            shown: vec![],
            status: None,
        };
        m.call(
            "bn.hello",
            json!(protocol::HelloParams::new("bn-client")),
            Pending::Hello,
        );
        m
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
                        "completed" => "done".into(),
                        _ if swap => "done".into(),
                        "rejected" | "interrupted" => {
                            let why = reason.as_deref().map_or("no reason given", explain);
                            format!("{}: {why}", n.stage)
                        }
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
                        let why = n.error.as_ref().map_or(n.stage.clone(), error_kind);
                        self.messages.push(format!("Command {}: {why}.", n.stage));
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
            Pending::Submit => {
                self.mark("received");
                self.command_id = value["command_id"].as_str().map(String::from);
                if let Some(s) = self.status.as_mut().filter(|s| s.stage == "sent") {
                    s.stage = "accepted".into();
                }
            }
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

    fn refresh_view(&mut self) {
        let Some(a) = self.avatar_pos() else {
            self.origin = None;
            return;
        };
        let (ax, ay) = (a.x, a.y);
        let centred = (ax - VIEW.x / 2, ay - VIEW.y / 2);
        self.origin = match self.origin {
            Some((ox, oy))
                if (MARGIN..VIEW.x - MARGIN).contains(&(ax - ox))
                    && (MARGIN..VIEW.y - MARGIN).contains(&(ay - oy)) =>
            {
                Some((ox, oy))
            }
            _ => Some(centred),
        };
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
        }
    }
}

impl GameModel for EngineModel {
    fn poll(&mut self) {
        while let Some(msg) = self.transport.recv() {
            self.handle(msg);
        }
        self.messages.extend(self.replica.take_log());
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
            VIEW
        } else {
            Pos { x: 0, y: 0 }
        }
    }

    fn cell(&self, at: Pos) -> Cell {
        let unknown = Cell {
            glyph: ' ',
            color: Rgb(128, 128, 128),
            bg: None,
            desc: "unknown".into(),
        };
        let Some(p) = self.abs(at) else {
            return unknown;
        };
        let entity = self
            .replica
            .entities()
            .find(|e| e.at == p && e.look.is_some());
        if let Some(e) = entity {
            let look = e.look.as_ref().expect("filtered on look");
            let desc = e.name.clone().unwrap_or_else(|| look_name(look));
            return self.styled(look, desc);
        }
        match self.replica.cell(&p.dim, p.x, p.y, p.z) {
            Some(c) => match top_look(c) {
                Some(l) => self.styled(l, look_name(l)),
                None => Cell {
                    glyph: '?',
                    color: Rgb(128, 128, 128),
                    bg: None,
                    desc: "sensed".into(),
                },
            },
            None => unknown,
        }
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

    fn travel(&mut self, at: Pos) -> bool {
        let free = self
            .replica
            .boundary()
            .is_some_and(|b| b.interaction.is_none());
        if let (false, true, Some(pos)) = (self.busy, free, self.abs(at)) {
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
