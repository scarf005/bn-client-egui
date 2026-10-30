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
    /// What a span drag draws from its press square to the square under the pointer.
    pub span: Option<GridSpan>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    span: Option<GridSpan>,
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
            span: wire.span,
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
            span: grid.span,
        }
    }
}

/// A square of a canvas grid, counted from the same origin as its cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridSquare {
    pub x: i32,
    pub y: i32,
}

/// What a span drag draws, as the engine's map editor draws it on release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridSpan {
    pub shape: SpanShape,
    /// The side of the square of squares each square of the shape stands for.
    pub size: u32,
    /// Whether it is also drawn mirrored across the middle column, or row, of the squares `from` to `to`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror_x: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror_y: bool,
    /// The squares it may draw on, both included.
    pub from: GridSquare,
    pub to: GridSquare,
    /// What it draws, bottom up, as indexes into the grid's looks; `fill` inside a room.
    pub looks: Vec<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fill: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanShape {
    /// The squares joining the two.
    Line,
    /// The outline of the rectangle they span.
    Rect,
    RectFilled,
    /// The ellipse inscribed in that rectangle.
    Ellipse,
    /// The outline in `looks` around its inside in `fill`.
    Room,
}

/// The squares a span covers: its outline or whole shape, and the inside of a room.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpanSquares {
    pub drawn: Vec<(i32, i32)>,
    pub fill: Vec<(i32, i32)>,
}

impl GridSpan {
    /// The squares it covers from `from` to `to`, each once, in reading order, as the engine's stroke draws them.
    pub fn squares(&self, from: (i32, i32), to: (i32, i32)) -> SpanSquares {
        let (low, high) = (
            (from.0.min(to.0), from.1.min(to.1)),
            (from.0.max(to.0), from.1.max(to.1)),
        );
        let boxed = |keep: &dyn Fn(i32, i32) -> bool| {
            (low.1..=high.1)
                .flat_map(|y| (low.0..=high.0).map(move |x| (x, y)))
                .filter(|&(x, y)| keep(x, y))
                .collect::<Vec<_>>()
        };
        let shape = match self.shape {
            SpanShape::Line => [vec![from], line_to(from, to)].concat(),
            SpanShape::Rect | SpanShape::Room => {
                boxed(&|x, y| x == low.0 || x == high.0 || y == low.1 || y == high.1)
            }
            SpanShape::RectFilled => boxed(&|_, _| true),
            SpanShape::Ellipse => {
                // Twice the centre and the radii, so that an even span has its centre between squares.
                let centre = (low.0 + high.0, low.1 + high.1);
                let radii = ((high.0 - low.0 + 1) as f64, (high.1 - low.1 + 1) as f64);
                boxed(&|x, y| {
                    let dx = (2 * x - centre.0) as f64 / radii.0;
                    let dy = (2 * y - centre.1) as f64 / radii.1;
                    dx * dx + dy * dy <= 1.0
                })
            }
        };
        // The inside of a room, filled before its outline.
        let inside = match self.shape {
            SpanShape::Room => (low.1 + 1..high.1)
                .flat_map(|y| (low.0 + 1..high.0).map(move |x| (x, y)))
                .collect(),
            _ => vec![],
        };
        SpanSquares {
            drawn: self.widened(&shape),
            fill: self.widened(&inside),
        }
    }

    /// `squares` each widened to the size, mirrored, kept inside `from` to `to`, once each in reading order.
    fn widened(&self, squares: &[(i32, i32)]) -> Vec<(i32, i32)> {
        let (west, north, east, south) = (self.from.x, self.from.y, self.to.x, self.to.y);
        let reach = self.size.max(1) as i32;
        let mut out = std::collections::BTreeSet::new();
        for &(x, y) in squares {
            for dy in 0..reach {
                for dx in 0..reach {
                    let q = (x + dx - (reach - 1) / 2, y + dy - (reach - 1) / 2);
                    let mirrored = (west + east - q.0, north + south - q.1);
                    out.insert((q.1, q.0));
                    if self.mirror_x {
                        out.insert((q.1, mirrored.0));
                    }
                    if self.mirror_y {
                        out.insert((mirrored.1, q.0));
                    }
                    if self.mirror_x && self.mirror_y {
                        out.insert((mirrored.1, mirrored.0));
                    }
                }
            }
        }
        out.into_iter()
            .map(|(y, x)| (x, y))
            .filter(|&(x, y)| west <= x && x <= east && north <= y && y <= south)
            .collect()
    }
}

/// The engine's `line_to`: the squares after `from` up to `to`, by its Bresenham walk; `from` alone when they meet.
fn line_to(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    if from == to {
        return vec![from];
    }
    let d = (to.0 - from.0, to.1 - from.1);
    let s = (d.0.signum(), d.1.signum());
    let a = (d.0.abs() * 2, d.1.abs() * 2);
    let (mut cur, mut t) = (from, 0);
    let mut line = vec![];
    if a.0 == a.1 {
        while cur.0 != to.0 {
            cur = (cur.0 + s.0, cur.1 + s.1);
            line.push(cur);
        }
    } else if a.0 > a.1 {
        while cur.0 != to.0 {
            if t > 0 {
                cur.1 += s.1;
                t -= a.0;
            }
            cur.0 += s.0;
            t += a.1;
            line.push(cur);
        }
    } else {
        while cur.1 != to.1 {
            if t > 0 {
                cur.0 += s.0;
                t -= a.1;
            }
            cur.1 += s.1;
            t += a.0;
            line.push(cur);
        }
    }
    line
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
