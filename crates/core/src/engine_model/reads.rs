//! What the client asks the engine to read besides the stream: a square's description, the squares of the map
//! window and the screens of the panel windows.

use super::*;

impl EngineModel {
    /// Asks the engine what is on `pos`; the answer holds for the revision it was asked at.
    pub(super) fn ask_describe(&mut self, pos: WorldPos) {
        let (Some(boundary), Some(now)) = (self.replica.boundary(), self.replica.at()) else {
            return;
        };
        // A running command changes the boundary at every prompt, so the engine would refuse the question.
        if self.busy
            || self
                .describing
                .as_ref()
                .is_some_and(|(id, _)| self.pending.contains_key(id))
        {
            self.describe_wanted = Some(pos);
            return;
        }
        self.describe_wanted = None;
        let asked = (boundary.boundary_id.clone(), now.revision.clone());
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
        self.describing = Some((
            self.call("bn.world.describe", params, Pending::Describe),
            asked,
        ));
    }

    /// Reads the squares the map window shows once the world or the window changed. The player's square is the
    /// one the engine's last read gave, so a first read, or one after the player left the squares, is followed
    /// by another around where the player is.
    pub(super) fn read_overmap(&mut self) {
        let (Some(watch), false) = (self.overmap.watch, self.overmap.waiting) else {
            return;
        };
        let (Some(now), Some(avatar)) = (self.replica.at(), self.avatar_pos()) else {
            return;
        };
        // Overmap squares name no dimension: they are the ones of the avatar's.
        let z = (avatar.z + watch.level).clamp(-OVERMAP_LEVELS, OVERMAP_LEVELS);
        let player = self.overmap.view.as_ref().map_or_else(
            || WorldPos {
                dim: String::new(),
                x: 0,
                y: 0,
                z,
            },
            |v| WorldPos {
                z,
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

    /// Reads the screen of each panel window once the world or the window's focus changed.
    pub(super) fn read_panels(&mut self) {
        let Some(now) = self.replica.at() else {
            return;
        };
        let due = self
            .panels
            .iter()
            .filter_map(|(panel, p)| {
                let asked = (p.watch.clone(), now.epoch.clone(), now.revision.clone());
                (p.waiting.is_none() && p.asked.as_ref() != Some(&asked)).then_some((*panel, asked))
            })
            .collect::<Vec<_>>();
        for (panel, asked) in due {
            let params = serde_json::to_value(protocol::PanelParams {
                epoch: asked.1.clone(),
                panel,
                row: asked.0.row.clone(),
            })
            .expect("panel params serialize");
            let id = self.call("bn.panel.view", params, Pending::Panel(panel));
            let p = self.panels.get_mut(&panel).expect("an open panel");
            p.asked = Some(asked);
            p.waiting = Some(id);
        }
    }

    /// Asks again for the hovered square once its answer is older than the world, such as after the
    /// engine's window changed size, so a pointer that stays still keeps its description.
    pub(super) fn refresh_hover(&mut self) {
        let free = self
            .replica
            .boundary()
            .is_some_and(|b| b.interaction.is_none());
        let settled = self
            .replica
            .boundary()
            .zip(self.replica.at())
            .is_some_and(|(b, now)| {
                self.describe_refused.as_ref()
                    != Some(&(b.boundary_id.clone(), now.revision.clone()))
            });
        if let (Some(pos), true) = (self.describe_wanted.clone(), settled && free) {
            self.ask_describe(pos);
        }
        let stale = self.hovered.as_ref().filter(|h| {
            h.parts.is_some()
                && self
                    .replica
                    .at()
                    .is_some_and(|now| now.revision != h.revision)
        });
        if let (Some(pos), true) = (stale.map(|h| h.pos.clone()), free) {
            self.ask_describe(pos);
        }
    }
}
