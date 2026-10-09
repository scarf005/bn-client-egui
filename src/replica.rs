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

#[derive(Default)]
pub struct Replica {
    at: Option<At>,
    coverage: Option<Coverage>,
    boundary: Option<Boundary>,
    avatar: Option<Avatar>,
    environment: Option<Environment>,
    entities: HashMap<String, Entity>,
    cells: HashMap<String, HashMap<(i32, i32, i32), Cell>>,
    /// `(parts expected, next part index)` while a snapshot is loading.
    loading: Option<(u32, u32)>,
    log: Vec<String>,
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
            coverage: Some(s.coverage),
            boundary: Some(s.interaction),
            avatar: Some(s.avatar),
            environment: Some(s.environment),
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
            if matches!(e.kind.as_str(), "message.logged" | "sound.heard")
                && let Some(text) = e.data.as_ref().and_then(|d| d["text"].as_str())
            {
                self.log.push(text.to_string());
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

    pub fn boundary(&self) -> Option<&Boundary> {
        self.boundary.as_ref()
    }

    pub fn avatar(&self) -> Option<&Avatar> {
        self.avatar.as_ref()
    }

    pub fn environment(&self) -> Option<&Environment> {
        self.environment.as_ref()
    }

    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    pub fn cell(&self, dim: &str, x: i32, y: i32, z: i32) -> Option<&Cell> {
        self.cells.get(dim)?.get(&(x, y, z))
    }

    /// Messages and sounds since the last call.
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }
}
