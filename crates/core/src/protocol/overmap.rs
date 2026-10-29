//! The overmap squares a map screen shows, and the windows a view cuts from them.

use super::*;

/// Overmap squares as the player knows them: `[glyph, foreground, background]` per cell, row-major,
/// with curses color indexes, and what the screen says of the cursor square.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OvermapWire {
    pub origin: WorldPos,
    pub player: WorldPos,
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Vec<(String, u8, u8)>>,
    pub legend: Vec<String>,
    /// What the sidebar says of the cursor square, without the window frame or its glyph.
    #[serde(default)]
    pub cursor_description: String,
    /// `You are here.`, localized, while the cursor is on the player's square.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_here: Option<String>,
    /// The note the native window prints for the cursor square, as one localized line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_note: Option<String>,
    /// The length of `path` as one localized line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_text: Option<String>,
    pub notes: Vec<OvermapNote>,
    /// The heading of `notes`, localized by the engine.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes_title: String,
    /// The names the native window prints over squares.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<OvermapLabel>,
    /// The active mission's target while the overlays are shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission_target: Option<WorldPos>,
    #[serde(default)]
    pub path: Vec<WorldPos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiles: Option<OvermapTiles>,
}

/// The tile drawn on one overmap square.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OvermapTile<'a> {
    pub id: &'a str,
    pub rotation: u8,
    /// The multitile key, empty for a plain tile.
    pub subtile: &'a str,
    /// Tile ids to try when the tileset has no sprite for `id`, nearest first.
    pub fallbacks: &'a [String],
}

/// The cells of an overmap a view shows: `cols` by `rows` from cell (`left`, `top`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OvermapWindow {
    pub left: usize,
    pub top: usize,
    pub cols: usize,
    pub rows: usize,
}

/// The mission target marked at the edge of a window it lies beyond: the edge cell and the arrow toward it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetMark {
    pub col: usize,
    pub row: usize,
    pub arrow: &'static str,
}

impl OvermapWire {
    /// The cells that fit in `room` (columns, rows), kept around `cursor` and within the overmap; none when no
    /// cell fits.
    pub fn window(&self, cursor: &WorldPos, room: (usize, usize)) -> Option<OvermapWindow> {
        let (cols, rows) = (self.cols.min(room.0), self.rows.min(room.1));
        if cols == 0 || rows == 0 {
            return None;
        }
        let (col, row) = self
            .cell_of(cursor)
            .unwrap_or((self.cols / 2, self.rows / 2));
        let first =
            |at: usize, shown: usize, all: usize| at.saturating_sub(shown / 2).min(all - shown);
        Some(OvermapWindow {
            left: first(col, cols, self.cols),
            top: first(row, rows, self.rows),
            cols,
            rows,
        })
    }

    /// Where the mission target is marked when it lies beyond `window`, as the native overmap marks it; none
    /// without a target or with the target inside.
    pub fn target_mark(&self, window: &OvermapWindow, cursor: &WorldPos) -> Option<TargetMark> {
        let target = self.mission_target.as_ref()?;
        let (dx, dy) = (target.x - self.origin.x, target.y - self.origin.y);
        let (left, top) = (window.left as i32, window.top as i32);
        let (right, bottom) = (left + window.cols as i32 - 1, top + window.rows as i32 - 1);
        if (left..=right).contains(&dx) && (top..=bottom).contains(&dy) {
            return None;
        }
        let arrow = match (
            (target.x - cursor.x).signum(),
            (target.y - cursor.y).signum(),
        ) {
            (0, -1) => "^",
            (1, -1) | (-1, 1) => "/",
            (1, 0) => ">",
            (1, 1) | (-1, -1) => "\\",
            (0, 1) => "v",
            (-1, 0) => "<",
            _ => " ",
        };
        Some(TargetMark {
            col: dx.clamp(left, right) as usize,
            row: dy.clamp(top, bottom) as usize,
            arrow,
        })
    }

    /// The absolute overmap square drawn in cell `(col, row)`.
    pub fn square(&self, col: usize, row: usize) -> WorldPos {
        WorldPos {
            dim: self.origin.dim.clone(),
            x: self.origin.x + col as i32,
            y: self.origin.y + row as i32,
            z: self.origin.z,
        }
    }

    /// The cell `(col, row)` for an absolute square; none outside the cells.
    pub fn cell_of(&self, at: &WorldPos) -> Option<(usize, usize)> {
        let col = usize::try_from(at.x - self.origin.x).ok()?;
        let row = usize::try_from(at.y - self.origin.y).ok()?;
        (at.z == self.origin.z && col < self.cols && row < self.rows).then_some((col, row))
    }

    /// The tile of cell `(col, row)`, when the engine sent tile ids.
    pub fn tile(&self, col: usize, row: usize) -> Option<OvermapTile<'_>> {
        let tiles = self.tiles.as_ref()?;
        let (id, rotation, subtile) = tiles.cells.get(row)?.get(col)?;
        Some(OvermapTile {
            id,
            rotation: *rotation,
            subtile,
            fallbacks: tiles.looks_like.get(id).map_or(&[], Vec::as_slice),
        })
    }

    /// The words of the note on this square, if any, without the colour tags the native list draws.
    pub fn note(&self, at: &WorldPos) -> Option<String> {
        self.notes
            .iter()
            .find(|n| n.pos == *at)
            .map(|n| crate::markup::plain(&n.text))
    }

    /// The status lines the engine says of the cursor square, in the order it says them, without colour tags.
    pub fn status(&self) -> Vec<String> {
        [
            self.cursor_here.as_deref(),
            Some(self.cursor_description.as_str()),
            self.cursor_note.as_deref(),
            self.route_text.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|t| !t.is_empty())
        .map(crate::markup::plain)
        .collect()
    }

    /// Every note in view with its square, for a reader that cannot see the markers.
    pub fn note_lines(&self) -> Vec<String> {
        self.notes
            .iter()
            .map(|n| {
                let words = crate::markup::plain(&n.text);
                format!("({}, {}, {}) {words}", n.pos.x, n.pos.y, n.pos.z)
            })
            .collect()
    }
}

/// Tile ids of the overmap squares, for a client that said it draws tiles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OvermapTiles {
    /// `[tile, rotation, subtile]` per square, row-major like the glyph cells.
    pub cells: Vec<Vec<(String, u8, String)>>,
    pub looks_like: std::collections::BTreeMap<String, Vec<String>>,
}

/// A name printed over an overmap square, in curses color indexes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OvermapLabel {
    pub pos: WorldPos,
    pub text: String,
    pub foreground: u8,
    pub background: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OvermapNote {
    pub pos: WorldPos,
    pub text: String,
}
