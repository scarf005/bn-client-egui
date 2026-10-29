//! The part of the world the map shows: where it lies, how large the engine is told it is, and what a target
//! screen says of it.

use super::*;

impl EngineModel {
    /// The cursor map of the screen on top, when it puts a cursor on the streamed world.
    pub(super) fn cursor_map(&self) -> Option<&protocol::MapComponent> {
        self.replica.stack().last()?.root.as_ref()?.cursor_map()
    }

    /// The affordance of the screen on top that puts the cursor on a square.
    pub(super) fn cursor_move(&self) -> Option<String> {
        let top = self.replica.stack().last()?;
        top.root.as_ref()?.cursor_map()?;
        top.affordances
            .iter()
            .find(|a| a.args.as_ref().is_some_and(|args| args.pos.is_some()))
            .map(|a| a.id.clone())
    }

    pub(super) fn avatar_pos(&self) -> Option<&WorldPos> {
        self.replica.avatar().map(|a| &a.at)
    }

    /// The level the map shows: the engine's view's, which a look cursor moves up and down, else the avatar's.
    pub(super) fn shown_z(&self) -> Option<i32> {
        let a = self.avatar_pos()?;
        Some(self.replica.view().map_or(a.z, |v| v.min.z))
    }

    pub(super) fn abs(&self, at: Pos) -> Option<WorldPos> {
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
    pub(super) fn shown(&self) -> Pos {
        self.replica.view().map_or(self.wanted, |v| Pos {
            x: self.wanted.x.min(v.max.x - v.min.x + 1),
            y: self.wanted.y.min(v.max.y - v.min.y + 1),
        })
    }

    pub(super) fn refresh_view(&mut self) {
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
    pub(super) fn send_viewport(&mut self) {
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
    pub(super) fn send_tiles(&mut self) {
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
}

/// What a target screen other than a shot says: the step of a zone's area (`message`), for the map
/// editor the rows of the square under its cursor and the keys of its edit actions.
pub(super) fn target_note(i: &protocol::Interaction) -> Option<String> {
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
