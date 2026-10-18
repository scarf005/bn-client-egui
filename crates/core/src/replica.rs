//! Client-side reconstruction of the engine state from a snapshot plus contiguous events.

use crate::protocol::{
    At, Avatar, Boundary, Cell, Changes, Coverage, Entity, Environment, EventsParams, Resync,
    Screen, Snapshot, SnapshotPart, Text, WorldPos,
};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    /// Every event was already applied.
    Duplicate,
    /// A snapshot is still loading.
    Pending,
    /// State cannot be trusted; call `bn.subscribe` again.
    Resubscribe(&'static str),
}

/// A `sound.heard` event the message log has no line for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sound {
    pub text: String,
    /// The native name of where it comes from; absent at the avatar's square or from a source in view.
    pub direction: Option<String>,
    /// Squares between the avatar and the source.
    pub distance: Option<u32>,
}

/// A `message.logged` event: a new line, or a repeat that replaces the line with the same `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedLine {
    pub id: String,
    pub text: String,
    /// The game's colour name for the message type.
    pub color: Option<String>,
    pub count: u32,
    /// The game turn the line was logged on, or last repeated on.
    pub turn: Option<String>,
    /// The line as the native log prints it, with the repeat suffix.
    pub label: String,
}

/// A native animation fact (`projectile.moved`, `explosion.*`, `combat_text.shown`), kept as published.
#[derive(Clone, Debug, PartialEq)]
pub struct Fact {
    pub kind: String,
    pub data: serde_json::Value,
    /// `display.duration_ms`: how long the native animation holds it.
    pub duration_ms: u64,
}

#[derive(Default)]
pub struct Replica {
    at: Option<At>,
    coverage: Option<Coverage>,
    /// The squares a map click selects (the engine's terrain window).
    view: Option<Coverage>,
    boundary: Option<Boundary>,
    avatar: Option<Avatar>,
    environment: Option<Environment>,
    route: Vec<WorldPos>,
    route_text: Option<String>,
    entities: HashMap<String, Entity>,
    /// The entity drawn on each square: one with a look, the lowest id when several share the square.
    drawn: HashMap<WorldPos, String>,
    cells: HashMap<String, HashMap<(i32, i32, i32), Cell>>,
    /// `(parts expected, next part index)` while a snapshot is loading.
    loading: Option<(u32, u32)>,
    sounds: Vec<Sound>,
    /// What the engine's sound player was asked to play, in order.
    cues: Vec<crate::audio::Cue>,
    lines: Vec<LoggedLine>,
    facts: Vec<Fact>,
}

fn seq(s: &str) -> Option<u64> {
    s.parse().ok()
}

fn inside(c: &Coverage, p: &WorldPos) -> bool {
    p.dim == c.min.dim
        && (c.min.x..=c.max.x).contains(&p.x)
        && (c.min.y..=c.max.y).contains(&p.y)
        && (c.min.z..=c.max.z).contains(&p.z)
}

impl Replica {
    /// Starts a new snapshot, discarding everything known so far.
    pub fn begin(&mut self, s: Snapshot) {
        *self = Self {
            at: Some(s.at),
            coverage: s.coverage,
            view: s.view,
            boundary: Some(s.interaction),
            avatar: s.avatar,
            environment: s.environment,
            route: s.route,
            route_text: s.route_text,
            entities: s.entities.into_iter().map(|e| (e.id.clone(), e)).collect(),
            loading: (s.parts > 0).then_some((s.parts, 0)),
            // The log the engine already holds, as if each line had just been published.
            lines: s
                .messages
                .into_iter()
                .map(|m| LoggedLine {
                    id: m.id,
                    text: m.text.shown(),
                    color: Some(m.color),
                    count: m.count.max(1),
                    turn: m.turn,
                    label: m.label.shown(),
                })
                .collect(),
            ..Self::default()
        };
        self.index_entities();
    }

    pub fn part(&mut self, p: SnapshotPart) -> Outcome {
        let Some((parts, next)) = self.loading else {
            return Outcome::Resubscribe("snapshot part without a pending snapshot");
        };
        if Some(&p.at) != self.at.as_ref() || p.index != next || (p.last != (next + 1 == parts)) {
            return Outcome::Resubscribe("snapshot part out of order");
        }
        self.insert_cells(p.cells);
        if p.last {
            self.loading = None;
            Outcome::Applied
        } else {
            self.loading = Some((parts, next + 1));
            Outcome::Pending
        }
    }

    pub fn events(&mut self, batch: EventsParams) -> Outcome {
        let (Some(at), None) = (&self.at, self.loading) else {
            return Outcome::Resubscribe("events without a complete snapshot");
        };
        if batch.epoch != at.epoch {
            return Outcome::Resubscribe("epoch mismatch");
        }
        let Some(mut last) = seq(&at.sequence) else {
            return Outcome::Resubscribe("bad sequence");
        };
        let fresh = batch
            .events
            .into_iter()
            .filter(|e| seq(&e.sequence).is_none_or(|s| s > last))
            .collect::<Vec<_>>();
        if fresh.is_empty() {
            return Outcome::Duplicate;
        }
        for e in &fresh {
            if seq(&e.sequence) != Some(last + 1) {
                return Outcome::Resubscribe("sequence gap");
            }
            last += 1;
        }
        for e in fresh {
            if matches!(
                e.kind.as_str(),
                "projectile.moved"
                    | "explosion.started"
                    | "explosion.blast"
                    | "explosion.shrapnel"
                    | "explosion.ended"
                    | "combat_text.shown"
            ) && let Some(data) = &e.data
            {
                self.facts.push(Fact {
                    kind: e.kind.clone(),
                    data: data.clone(),
                    duration_ms: e
                        .display
                        .as_ref()
                        .map_or(0, |d| d["duration_ms"].as_u64().unwrap_or(0)),
                });
            }
            if let Some(cue) = e
                .data
                .as_ref()
                .and_then(|data| crate::audio::Cue::of_event(&e.kind, data))
            {
                self.cues.push(cue);
            }
            if let Some(data) = &e.data {
                let shown = |name: &str| {
                    serde_json::from_value::<Text>(data[name].clone())
                        .ok()
                        .map(|text| text.shown())
                };
                match e.kind.as_str() {
                    "message.logged" => {
                        if let Some(text) = shown("text") {
                            self.lines.push(LoggedLine {
                                id: data["id"].as_str().unwrap_or_default().to_string(),
                                label: shown("label").unwrap_or_else(|| text.clone()),
                                text,
                                color: data["color"].as_str().map(str::to_string),
                                count: data["count"]
                                    .as_u64()
                                    .map_or(1, |n| n.clamp(1, 1 << 31) as u32),
                                turn: data["turn"].as_str().map(str::to_string),
                            });
                        }
                    }
                    // A sound the engine also wrote to the message log is already shown there.
                    "sound.heard" if data["logged"] != true => {
                        if let Some(text) = shown("text") {
                            self.sounds.push(Sound {
                                text,
                                direction: data["direction"]["name"].as_str().map(str::to_string),
                                distance: data["distance"]
                                    .as_u64()
                                    .map(|n| n.min(u32::MAX as u64) as u32),
                            });
                        }
                    }
                    _ => {}
                }
            }
            self.apply(e.changes);
            if let Some(at) = &mut self.at {
                at.sequence = e.sequence;
                at.revision = e.revision;
            }
        }
        self.index_entities();
        Outcome::Applied
    }

    pub fn resync(&mut self, _r: &Resync) -> Outcome {
        Outcome::Resubscribe("engine requested resync")
    }

    fn index_entities(&mut self) {
        self.drawn.clear();
        for e in self.entities.values().filter(|e| e.look.is_some()) {
            self.drawn
                .entry(e.at.clone())
                .and_modify(|id| {
                    if e.id < *id {
                        id.clone_from(&e.id);
                    }
                })
                .or_insert_with(|| e.id.clone());
        }
    }

    fn insert_cells(&mut self, cells: Vec<Cell>) {
        for mut c in cells {
            c.complete();
            self.cells
                .entry(c.at.dim.clone())
                .or_default()
                .insert((c.at.x, c.at.y, c.at.z), c);
        }
    }

    /// Applies one event's changes in the order fixed by the design (§3.4).
    fn apply(&mut self, c: Changes) {
        if let Some(cov) = c.coverage {
            for (dim, cells) in &mut self.cells {
                cells.retain(|&(x, y, z), _| {
                    inside(
                        &cov,
                        &WorldPos {
                            dim: dim.clone(),
                            x,
                            y,
                            z,
                        },
                    )
                });
            }
            self.entities.retain(|_, e| inside(&cov, &e.at));
            self.coverage = Some(cov);
        }
        if let Some(view) = c.view {
            self.view = Some(view);
        }
        self.insert_cells(c.cells);
        for p in c.forgotten {
            if let Some(cells) = self.cells.get_mut(&p.dim) {
                cells.remove(&(p.x, p.y, p.z));
            }
        }
        for e in c.entities {
            self.entities.insert(e.id.clone(), e);
        }
        for g in c.gone {
            self.entities.remove(&g.id);
        }
        self.avatar = c
            .avatar
            .map(|change| change.apply(self.avatar.as_ref()))
            .or(self.avatar.take());
        self.environment = c.environment.or(self.environment.take());
        if let Some(route) = c.route {
            self.route = route;
            self.route_text = c.route_text;
        }
        if let Some(next) = c.interaction {
            let kept = self.boundary.take().map(|b| b.actions).unwrap_or_default();
            self.boundary = Some(Boundary {
                boundary_id: next.boundary_id,
                readiness: next.readiness,
                actions: next.actions.unwrap_or(kept),
                interaction: next.interaction,
                stack: next.stack,
            });
        }
    }

    pub fn ready(&self) -> bool {
        self.at.is_some() && self.loading.is_none()
    }

    pub fn at(&self) -> Option<&At> {
        self.at.as_ref()
    }

    pub fn coverage(&self) -> Option<&Coverage> {
        self.coverage.as_ref()
    }

    pub fn view(&self) -> Option<&Coverage> {
        self.view.as_ref()
    }

    pub fn boundary(&self) -> Option<&Boundary> {
        self.boundary.as_ref()
    }

    /// The open screens, outermost first; empty while the boundary has only the `interaction`.
    pub fn stack(&self) -> &[Screen] {
        self.boundary.as_ref().map_or(&[], |b| &b.stack)
    }

    pub fn avatar(&self) -> Option<&Avatar> {
        self.avatar.as_ref()
    }

    pub fn environment(&self) -> Option<&Environment> {
        self.environment.as_ref()
    }

    /// The engine's planned route, nearest square first; empty when none.
    pub fn route(&self) -> &[WorldPos] {
        &self.route
    }

    /// The engine's line for the planned route; none when nothing is planned.
    pub fn route_text(&self) -> Option<&str> {
        self.route_text.as_deref()
    }

    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    /// The entity drawn on `at`, when one with a look is there.
    pub fn entity_at(&self, at: &WorldPos) -> Option<&Entity> {
        self.entities.get(self.drawn.get(at)?)
    }

    pub fn cell(&self, dim: &str, x: i32, y: i32, z: i32) -> Option<&Cell> {
        self.cells.get(dim)?.get(&(x, y, z))
    }

    /// Animation facts since the last call, in the order the game produced them.
    pub fn take_facts(&mut self) -> Vec<Fact> {
        std::mem::take(&mut self.facts)
    }

    /// Message-log lines since the last call, in log order.
    pub fn take_lines(&mut self) -> Vec<LoggedLine> {
        std::mem::take(&mut self.lines)
    }

    /// Sound cues since the last call, in the order the engine published them.
    pub fn take_cues(&mut self) -> Vec<crate::audio::Cue> {
        std::mem::take(&mut self.cues)
    }

    /// Sounds the message log does not hold, since the last call.
    pub fn take_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.sounds)
    }
}
