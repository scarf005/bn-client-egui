//! The grid component: sent as runs of styles, read as whole cells.

use super::*;

/// A grid, its squares each a whole cell however the engine sent them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "GridWire", into = "GridWire")]
pub struct Grid {
    pub head: Head,
    pub cols: u32,
    pub rows: u32,
    pub cursor: Option<GridCursor>,
    pub cells: Vec<GridCell>,
    /// The looks the cells name, each once.
    pub looks: Vec<Look>,
    pub role: Option<GridRole>,
    /// How a pointer drag over the cells runs their affordance; none: only a click does.
    pub drag: Option<GridDrag>,
    /// The affordance run when a drag begins, before the first square's; none: no such step.
    pub drag_begin: Option<String>,
    /// The affordance run when a drag ends, after the last square's, so the engine can make the drag one step.
    pub drag_end: Option<String>,
    /// The size in cells of the blocks a client outlines, counted from the top left; none: no outline.
    pub block: Option<GridBlock>,
    /// Rectangles of squares a client outlines over the cells; none: no outline.
    pub regions: Vec<GridRegion>,
    /// The affordance a drag of a region runs, on the region's `id` with how far its corners moved; none: regions
    /// are not dragged.
    pub region_drag: Option<String>,
    /// The affordance a secondary click of a cell runs, on the cell's `id`; none: a secondary click does nothing.
    pub secondary: Option<String>,
    /// How a secondary drag, such as a right drag, runs `secondary`; none: a secondary click only.
    pub secondary_drag: Option<GridDrag>,
    /// Why the tool in hand cannot act on a cell, each reason once; a cell's `refused` is an index into it.
    pub refusals: Vec<String>,
    /// What every square the engine did not list shows, when it sent the squares as runs of styles; such a grid
    /// names each square `square:<x>,<y>`.
    pub blank: Option<GridCell>,
    /// Where the squares the tool in hand puts down land, from the square under the pointer.
    pub stamp: Vec<GridSquare>,
}

/// A grid as it travels: with `blank`, its looks once in `styles` and its other squares as runs east.
#[derive(Serialize, Deserialize)]
struct GridWire {
    #[serde(flatten)]
    head: Head,
    cols: u32,
    rows: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cursor: Option<GridCursor>,
    cells: Vec<WireCell>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    blank: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    styles: Vec<GridStyle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    refusals: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    looks: Vec<Look>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    role: Option<GridRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    drag: Option<GridDrag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    drag_begin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    drag_end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    block: Option<GridBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    regions: Vec<GridRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    region_drag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secondary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secondary_drag: Option<GridDrag>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    stamp: Vec<GridSquare>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum WireCell {
    Run(GridRun),
    Whole(Box<GridCell>),
}

/// `n` squares east from `x`, `y` that show `styles[style]`.
#[derive(Serialize, Deserialize)]
struct GridRun {
    x: u32,
    y: u32,
    style: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    n: Option<u32>,
}

/// A cell but its place and id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct GridStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    glyph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    affordance: Option<String>,
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    denial: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    looks: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    faded: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refused: Option<usize>,
}

impl GridStyle {
    fn of(cell: GridCell) -> Self {
        Self {
            glyph: cell.glyph,
            label: cell.label,
            color: cell.color,
            affordance: cell.affordance,
            enabled: cell.enabled,
            denial: cell.denial,
            selected: cell.selected,
            looks: cell.looks,
            faded: cell.faded,
            refused: cell.refused,
        }
    }

    fn at(self, x: u32, y: u32, id: Option<String>) -> GridCell {
        GridCell {
            x,
            y,
            id,
            glyph: self.glyph,
            label: self.label,
            color: self.color,
            affordance: self.affordance,
            enabled: self.enabled,
            denial: self.denial,
            selected: self.selected,
            looks: self.looks,
            faded: self.faded,
            refused: self.refused,
        }
    }
}

/// The row a grid sent as runs names its square by.
pub fn square_id(x: u32, y: u32) -> String {
    format!("square:{x},{y}")
}

/// Runs become one whole cell per square, each named by its place.
impl From<GridWire> for Grid {
    fn from(wire: GridWire) -> Self {
        let blank = wire.blank.and_then(|i| wire.styles.get(i).cloned());
        let cells = match &blank {
            None => wire
                .cells
                .into_iter()
                .filter_map(|cell| match cell {
                    WireCell::Whole(cell) => Some(*cell),
                    WireCell::Run(_) => None,
                })
                .collect(),
            Some(_) => {
                let mut shown = vec![wire.blank.unwrap_or(0); (wire.cols * wire.rows) as usize];
                for cell in wire.cells {
                    let WireCell::Run(run) = cell else { continue };
                    let start = (run.y * wire.cols + run.x) as usize;
                    let end = (start + run.n.unwrap_or(1) as usize).min(shown.len());
                    shown
                        .get_mut(start..end)
                        .into_iter()
                        .flatten()
                        .for_each(|s| *s = run.style);
                }
                shown
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i, style)| {
                        let (x, y) = (i as u32 % wire.cols, i as u32 / wire.cols);
                        let style = wire.styles.get(style)?.clone();
                        Some(style.at(x, y, Some(square_id(x, y))))
                    })
                    .collect()
            }
        };
        Self {
            head: wire.head,
            cols: wire.cols,
            rows: wire.rows,
            cursor: wire.cursor,
            cells,
            looks: wire.looks,
            role: wire.role,
            drag: wire.drag,
            drag_begin: wire.drag_begin,
            drag_end: wire.drag_end,
            block: wire.block,
            regions: wire.regions,
            region_drag: wire.region_drag,
            secondary: wire.secondary,
            secondary_drag: wire.secondary_drag,
            refusals: wire.refusals,
            blank: blank.map(|style| style.at(0, 0, None)),
            stamp: wire.stamp,
        }
    }
}

/// A grid with `blank` goes back as the engine sends it: the blank style first, the others as they first show
/// from the north-west, and the other squares as runs east.
impl From<Grid> for GridWire {
    fn from(grid: Grid) -> Self {
        let (blank, styles, cells) = match grid.blank {
            None => (
                None,
                vec![],
                grid.cells
                    .into_iter()
                    .map(|c| WireCell::Whole(Box::new(c)))
                    .collect(),
            ),
            Some(blank) => {
                let mut styles = vec![GridStyle::of(blank)];
                let mut runs: Vec<GridRun> = vec![];
                let mut cells = grid.cells;
                cells.sort_by_key(|c| (c.y, c.x));
                for cell in cells {
                    let (x, y) = (cell.x, cell.y);
                    let style = GridStyle::of(cell);
                    let index = match styles.iter().position(|s| *s == style) {
                        Some(0) => continue,
                        Some(i) => i,
                        None => {
                            styles.push(style);
                            styles.len() - 1
                        }
                    };
                    match runs.last_mut() {
                        Some(run)
                            if run.y == y
                                && run.style == index
                                && run.x + run.n.unwrap_or(1) == x =>
                        {
                            run.n = Some(run.n.unwrap_or(1) + 1)
                        }
                        _ => runs.push(GridRun {
                            x,
                            y,
                            style: index,
                            n: None,
                        }),
                    }
                }
                (
                    Some(0),
                    styles,
                    runs.into_iter().map(WireCell::Run).collect(),
                )
            }
        };
        Self {
            head: grid.head,
            cols: grid.cols,
            rows: grid.rows,
            cursor: grid.cursor,
            cells,
            blank,
            styles,
            refusals: grid.refusals,
            looks: grid.looks,
            role: grid.role,
            drag: grid.drag,
            drag_begin: grid.drag_begin,
            drag_end: grid.drag_end,
            block: grid.block,
            regions: grid.regions,
            region_drag: grid.region_drag,
            secondary: grid.secondary,
            secondary_drag: grid.secondary_drag,
            stamp: grid.stamp,
        }
    }
}

/// A square of a canvas grid, counted from the same origin as its cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridSquare {
    pub x: i32,
    pub y: i32,
}

/// A rectangle of squares from `from` to `to`, both included.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridRegion {
    pub id: String,
    pub from: GridSquare,
    pub to: GridSquare,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridBlock {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GridDrag {
    /// Every square the pointer crosses runs it, from the press on.
    Trail,
    /// The squares under the press and under the release run it.
    Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GridRole {
    /// Squares the player draws on, which take whatever room there is.
    Canvas,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridCursor {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridCell {
    pub x: u32,
    pub y: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The affordance that takes this cell as its `row`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affordance: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denial: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    /// What the cell shows from the bottom up, as indexes into the grid's `looks`, for a client that draws
    /// sprites; `glyph` stands for it otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub looks: Vec<usize>,
    /// The cell shows another level, such as the one below seen through open air, which a client draws faded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faded: Option<bool>,
    /// Why the tool in hand cannot act on the cell, as an index into the grid's `refusals`; the cell stays
    /// enabled, and a client draws it dimmed and says why on hover.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<usize>,
}
