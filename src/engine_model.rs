//! `GameModel` backed by a live engine connection.

use crate::model::{Cell, GameModel, Menu, MenuChoice, Pos};
use crate::protocol::{
    self, Cell as WorldCell, Expect, Incoming, Look, Operation, Snapshot, Submit, WorldPos,
};
use crate::replica::{Outcome, Replica};
use crate::transport::{Transport, request};
use eframe::egui::Color32;
use serde_json::{Value, json};
use std::collections::HashMap;

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
    origin: Option<(i32, i32)>,
    messages: Vec<String>,
    ground: Vec<String>,
    inventory: Vec<String>,
}

fn look_name(l: &Look) -> String {
    l.id.clone().unwrap_or_else(|| l.glyph.clone())
}

fn color(name: &str) -> Color32 {
    let (r, g, b) = match name {
        "black" => (60, 60, 60),
        "red" => (170, 40, 40),
        "green" => (40, 160, 60),
        "brown" => (150, 100, 50),
        "blue" => (60, 90, 200),
        "magenta" => (170, 60, 170),
        "cyan" => (40, 160, 170),
        "dark_gray" => (110, 110, 110),
        "light_red" => (240, 90, 90),
        "light_green" => (110, 240, 110),
        "yellow" => (240, 220, 80),
        "light_blue" => (110, 150, 255),
        "light_magenta" => (240, 110, 240),
        "light_cyan" => (110, 240, 240),
        "white" => (250, 250, 250),
        _ => (180, 180, 180),
    };
    Color32::from_rgb(r, g, b)
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

fn direction(dx: i32, dy: i32) -> Option<[&'static str; 2]> {
    Some(match (dx, dy) {
        (0, -1) => ["north", "n"],
        (1, -1) => ["northeast", "ne"],
        (1, 0) => ["east", "e"],
        (1, 1) => ["southeast", "se"],
        (0, 1) => ["south", "s"],
        (-1, 1) => ["southwest", "sw"],
        (-1, 0) => ["west", "w"],
        (-1, -1) => ["northwest", "nw"],
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
            origin: None,
            messages: vec![],
            ground: vec![],
            inventory: vec![],
        };
        m.call(
            "bn.hello",
            protocol::hello_params("bn-client-egui"),
            Pending::Hello,
        );
        m
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
                let outcome = self.replica.events(e);
                self.resubscribe_on(outcome);
            }
            Incoming::Events(_) => {}
            Incoming::Command(n) => {
                if n.is_terminal() && self.command_id.as_deref() == Some(&n.command_id) {
                    self.busy = false;
                    self.command_id = None;
                    if n.stage != "completed" {
                        self.rejected = true;
                        let why = n.error.as_ref().map_or(n.stage.clone(), error_kind);
                        self.messages.push(format!("Command {}: {why}.", n.stage));
                    }
                }
            }
            Incoming::Resync(r) => {
                let outcome = self.replica.resync(&r);
                self.resubscribe_on(outcome);
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
            self.subscribing = false;
        }
        if matches!(kind, Pending::Submit) && result.is_err() {
            self.busy = false;
        }
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                let why = e.data.as_ref().map_or(e.message.clone(), error_kind);
                self.messages.push(format!("Engine error: {why}."));
                self.rejected = matches!(kind, Pending::Submit);
                return;
            }
        };
        match kind {
            Pending::Hello => self.subscribe(),
            Pending::Subscribe => match serde_json::from_value::<Snapshot>(value) {
                Ok(s) => self.replica.begin(s),
                Err(e) => self.messages.push(format!("Bad snapshot: {e}")),
            },
            Pending::Submit => {
                self.command_id = value["command_id"].as_str().map(String::from);
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
        self.busy = true;
        self.rejected = false;
        let params = serde_json::to_value(submit).expect("submit serializes");
        self.call("bn.command.submit", params, Pending::Submit);
        true
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

    fn refresh_items(&mut self) {
        let here = self
            .avatar_pos()
            .and_then(|p| self.replica.cell(&p.dim, p.x, p.y, p.z));
        self.ground = here.map_or(vec![], |c| c.items.iter().map(look_name).collect());
        self.inventory = self
            .replica
            .avatar()
            .map_or(vec![], |a| a.inventory.iter().map(look_name).collect());
    }
}

fn error_kind(e: &Value) -> String {
    e["kind"]
        .as_str()
        .map_or_else(|| e.to_string(), String::from)
}

impl GameModel for EngineModel {
    fn poll(&mut self) {
        while let Some(msg) = self.transport.recv() {
            self.handle(msg);
        }
        self.messages.extend(self.replica.take_log());
        self.refresh_view();
        self.refresh_items();
    }

    fn live(&self) -> bool {
        self.connected
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
            color: Color32::GRAY,
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
            return Cell {
                glyph: glyph(look),
                color: color(&look.color),
                desc,
            };
        }
        match self.replica.cell(&p.dim, p.x, p.y, p.z) {
            Some(c) => match top_look(c) {
                Some(l) => Cell {
                    glyph: glyph(l),
                    color: color(&l.color),
                    desc: look_name(l),
                },
                None => Cell {
                    glyph: '?',
                    color: Color32::GRAY,
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
        let Some(names) = direction(dx, dy) else {
            return false;
        };
        let ids = names.map(|n| format!("move_{n}"));
        self.invoke(&ids)
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
                    enabled: c.enabled && c.selectable.unwrap_or(true),
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
