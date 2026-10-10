//! Client-side reconstruction of the engine state from a snapshot plus contiguous events.

use crate::protocol::{
    At, Avatar, Boundary, Cell, Changes, Coverage, Entity, Environment, EventsParams, Resync,
    Snapshot, SnapshotPart, WorldPos,
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

/// A `message.logged` event: a new line, or a repeat that replaces the line with the same `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedLine {
    pub id: String,
    pub text: String,
    /// The game's colour name for the message type.
    pub color: Option<String>,
    pub count: u32,
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
    entities: HashMap<String, Entity>,
    cells: HashMap<String, HashMap<(i32, i32, i32), Cell>>,
    /// `(parts expected, next part index)` while a snapshot is loading.
    loading: Option<(u32, u32)>,
    log: Vec<String>,
    lines: Vec<LoggedLine>,
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
            entities: s.entities.into_iter().map(|e| (e.id.clone(), e)).collect(),
            loading: (s.parts > 0).then_some((s.parts, 0)),
            ..Self::default()
        };
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
            if let Some(data) = &e.data
                && let Some(text) = data["text"].as_str()
            {
                match e.kind.as_str() {
                    "message.logged" => self.lines.push(LoggedLine {
                        id: data["id"].as_str().unwrap_or_default().to_string(),
                        text: text.to_string(),
                        color: data["color"].as_str().map(str::to_string),
                        count: data["count"]
                            .as_u64()
                            .map_or(1, |n| n.clamp(1, 1 << 31) as u32),
                    }),
                    "sound.heard" => self.log.push(text.to_string()),
                    _ => {}
                }
            }
            self.apply(e.changes);
            if let Some(at) = &mut self.at {
                at.sequence = e.sequence;
                at.revision = e.revision;
            }
        }
        Outcome::Applied
    }

    pub fn resync(&mut self, _r: &Resync) -> Outcome {
        Outcome::Resubscribe("engine requested resync")
    }

    fn insert_cells(&mut self, cells: Vec<Cell>) {
        for c in cells {
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
        self.avatar = c.avatar.or(self.avatar.take());
        self.environment = c.environment.or(self.environment.take());
        if let Some(route) = c.route {
            self.route = route;
        }
        self.boundary = c.interaction.or(self.boundary.take());
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

    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    pub fn cell(&self, dim: &str, x: i32, y: i32, z: i32) -> Option<&Cell> {
        self.cells.get(dim)?.get(&(x, y, z))
    }

    /// Message-log lines since the last call, in log order.
    pub fn take_lines(&mut self) -> Vec<LoggedLine> {
        std::mem::take(&mut self.lines)
    }

    /// Sounds since the last call.
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }
}
