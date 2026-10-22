//! Where a screen's window sits over the map, and how wide it is, by the class of window the screen is.

use super::*;

/// The room a window leaves the map's squares around the avatar and the engine's cursor, in whole squares.
pub(super) const KEEP_CLEAR: f32 = 1.0;

/// The share of the map region a window beside the cursor, or docked to a side, takes in width.
pub(super) const SIDE_SHARE: f32 = 0.4;

/// The share of the map region a dialog takes at most, and a screen at least, in width.
const SCREEN_SHARE: f32 = 0.6;

/// The narrowest and the widest a dialog is.
const DIALOG_WIDTH: (f32, f32) = (320.0, 720.0);

/// The widest the column of a full screen with no map or canvas of its own is.
pub(super) const COLUMN_WIDTH: f32 = 720.0;

/// The space kept between a window and the edge of the map region.
const REGION_MARGIN: f32 = 8.0;

/// The kinds of window a screen is shown in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Class {
    /// A question or a short list: as large as its content, centred in the region, a readable column wide but
    /// for a table, which keeps its columns beside its labels, and panes side by side, which keep their lines.
    Dialog,
    /// A screen read beside the map: as large as its content and at least most of the region wide, centred, so
    /// the map stays visible around it.
    Screen,
    /// A screen of its own map or canvas, or one shown with no game under it: the whole region.
    Full,
    /// A window docked to a side or kept off the cursor: as large as its content and at least its footer on one
    /// row (unless that would cover the cursor), at a side of the region.
    Side,
}

/// What a screen's window sits in.
pub(super) struct Room<'a> {
    /// The map panel, or what the top bar leaves where no game runs.
    pub(super) region: egui::Rect,
    pub(super) sidebar: Option<(SidebarSide, egui::Rect)>,
    /// The rectangles on the map that must stay uncovered.
    pub(super) avoid: &'a [egui::Rect],
    /// A game's map lies under the window.
    pub(super) map: bool,
}

/// A screen's window class and where it sits.
pub(super) struct Placement {
    pub(super) class: Class,
    pub(super) place: Place,
    /// The width of the region inside its margin, which the class's shares are of.
    pub(super) room: f32,
    /// The window holds a list with columns, which a dialog's reading width would leave no room for its labels.
    pub(super) table: bool,
    /// The window holds panes side by side, which a dialog's reading width would wrap word by word.
    pub(super) panes: bool,
    /// The window fills its place with a banner and no map or canvas (the title screen): its content is one centred
    /// column, which a selection bar would otherwise stretch across.
    pub(super) column: bool,
}

/// A window at the left or the right side of `region`, no wider than `share` of the region nor taller than it,
/// and kept inside it.
pub(super) fn side(region: egui::Rect, left: bool, share: f32) -> Place {
    let inner = region.shrink(REGION_MARGIN);
    let (pivot, pos) = match left {
        true => (egui::Align2::LEFT_TOP, inner.left_top()),
        false => (egui::Align2::RIGHT_TOP, inner.right_top()),
    };
    Place::Side {
        pivot,
        pos,
        max: egui::vec2(inner.width() * share, inner.height()),
        within: inner,
    }
}

/// How a screen of the stack sits in the room (the sidebar stays visible): a dialog and a screen are centred, a
/// screen of a map or canvas fills the region, a window that asked for a side sits there, and a screen that puts
/// a cursor on the map sits on the side away from it.
pub(super) fn placement(screen: &Screen, room: &Room) -> Placement {
    let inner = room.region.shrink(REGION_MARGIN);
    let beside = |left: bool| {
        let width = inner.width() * SIDE_SHARE;
        let (x0, x1) = match left {
            true => (inner.left(), inner.left() + width),
            false => (inner.right() - width, inner.right()),
        };
        egui::Rect::from_x_y_ranges(x0..=x1, inner.y_range())
    };
    // Away from the cursor: the window's side is the one whose share of the room has none of `avoid`, and the
    // window never takes more than that share, whatever its footer needs.
    let away = || {
        let covered = |left: bool| room.avoid.iter().any(|r| beside(left).intersects(*r));
        let cursor_left = room
            .avoid
            .first()
            .is_some_and(|r| r.center().x < inner.center().x);
        let left = match (covered(!cursor_left), covered(cursor_left)) {
            (true, false) => cursor_left,
            _ => !cursor_left,
        };
        side(room.region, left, SIDE_SHARE)
    };
    let (class, place) = match (screen.dock, screen.role) {
        (Some(ScreenDock::Left), _) => (Class::Side, side(room.region, true, 1.0)),
        (Some(ScreenDock::Right), _) => (Class::Side, side(room.region, false, 1.0)),
        // Over the sidebar: its own place, level with the map's top.
        (Some(ScreenDock::Overlapping), _) => (
            Class::Side,
            match room.sidebar {
                Some((_, rect)) => Place::Side {
                    pivot: egui::Align2::LEFT_TOP,
                    pos: egui::pos2(rect.left(), inner.top()),
                    max: egui::vec2(rect.width(), inner.height()),
                    within: inner.union(rect),
                },
                None => side(room.region, true, 1.0),
            },
        ),
        (Some(ScreenDock::Cursor), _) => (Class::Side, away()),
        _ if keeps_off_cursor(screen) => (Class::Side, away()),
        (None, ScreenRole::Dialog) => (Class::Dialog, Place::Center(inner)),
        _ if !room.map || draws_world(screen) => (Class::Full, Place::Fill(inner)),
        _ => (Class::Screen, Place::Center(inner)),
    };
    Placement {
        class,
        place,
        room: inner.width(),
        column: class == Class::Full
            && !draws_world(screen)
            && screen.components().into_iter().any(|c| {
                matches!(c, Component::Text(t) if t.role == Some(TextRole::Banner))
            }),
        table: screen
            .components()
            .into_iter()
            .any(|c| matches!(c, Component::List(list) if !list.columns.is_empty())),
        panes: screen.components().into_iter().any(|c| {
            matches!(c, Component::Split(split) if split.children.iter().filter(|c| !heading(c)).count() > 1)
        }),
    }
}

impl Placement {
    /// The widest the class lets the window be, which its content is measured in.
    pub(super) fn widest(&self) -> f32 {
        let most = match self.class {
            Class::Full | Class::Screen => self.room,
            Class::Dialog if self.table || self.panes => self.room,
            Class::Dialog => DIALOG_WIDTH.1.min(SCREEN_SHARE * self.room),
            Class::Side => SIDE_SHARE * self.room,
        };
        most.min(self.place.room().x)
    }

    /// The window's width for content and a one-row footer `footer` wide (both with the window's margins): as
    /// wide as both up to `widest`, but never narrower than the class's least, the content's least, nor a side
    /// window narrower than its footer, nor wider than its place; none for a window that fills its place.
    pub(super) fn width(&self, content: Extent, footer: f32) -> Option<f32> {
        let least = match self.class {
            Class::Full => return None,
            Class::Dialog => DIALOG_WIDTH.0,
            Class::Screen => SCREEN_SHARE * self.room,
            Class::Side => footer,
        };
        let wanted = content.natural.max(footer);
        let width = wanted.min(self.widest().max(content.min)).max(least);
        Some(width.min(self.place.room().x))
    }
}

/// A screen that holds a map or a canvas of its own, which take whatever room they get.
fn draws_world(screen: &Screen) -> bool {
    screen.components().into_iter().any(takes_room)
}

/// A map or a canvas, which take whatever room they get: the engine's canvas, or a grid it marks as one (the map
/// editor's); a grid that only shows or picks squares (a vehicle's, the direction compass) is not.
pub(super) fn takes_room(c: &Component) -> bool {
    match c {
        Component::Map(_) | Component::Canvas(_) => true,
        Component::Grid(grid) => grid.role == Some(GridRole::Canvas),
        _ => false,
    }
}

/// A screen that puts a cursor on the map (look, aim), or asks to sit away from it, must not hide what the
/// cursor is on: it moves as the cursor does rather than where the player drags it.
pub(super) fn keeps_off_cursor(screen: &Screen) -> bool {
    screen.dock == Some(ScreenDock::Cursor)
        || screen
            .root
            .as_ref()
            .is_some_and(|r| r.cursor_map().is_some())
}

/// What a window body painted, from the top of its content: the lines of text, as (top, bottom).
#[derive(Clone, Default)]
struct Painted(Vec<(f32, f32)>);

/// The lines of text painted in `layer` since its shape number `from`, in view, as (top, bottom) from `top`.
fn painted_lines(
    ctx: &egui::Context,
    layer: egui::LayerId,
    from: usize,
    top: f32,
) -> Vec<(f32, f32)> {
    fn walk(clip: egui::Rect, shape: &egui::Shape, top: f32, out: &mut Vec<(f32, f32)>) {
        match shape {
            egui::Shape::Text(text) => out.extend(text.galley.rows.iter().filter_map(|row| {
                let at = row.rect().translate(text.pos.to_vec2());
                clip.intersect(at)
                    .is_positive()
                    .then_some((at.top() - top, at.bottom() - top))
            })),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(clip, s, top, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    ctx.graphics(|g| {
        let shapes = g
            .get(layer)
            .into_iter()
            .flat_map(|l| l.all_entries().skip(from));
        shapes.for_each(|s| walk(s.clip_rect, &s.shape, top, &mut out));
    });
    out
}

/// Draws a window body in `scroll`, a vertical scroll area at most `room` tall that ends between lines of text
/// instead of across one: the lines painted so far, those past the viewport as last seen, say where.
pub(super) fn body_scroll(
    ui: &mut egui::Ui,
    scroll: egui::ScrollArea,
    room: f32,
    draw: impl FnOnce(&mut egui::Ui),
) {
    let ctx = ui.ctx().clone();
    let key = ui.id().with("body lines");
    let seen = ctx.data(|d| d.get_temp::<Painted>(key)).unwrap_or_default();
    let height = split::whole_rows(&seen.0, room);
    let layer = ui.layer_id();
    scroll.max_height(height).show(ui, |ui| {
        let top = ui.cursor().top();
        let from = ctx.graphics(|g| g.get(layer).map_or(0, |l| l.all_entries().len()));
        draw(ui);
        // The lines in view are as painted now; those past the viewport stay as they were last seen.
        let past = ui.clip_rect().top() - top + height;
        let kept = seen.0.iter().filter(|(line, _)| *line >= past - 0.5);
        let lines = painted_lines(&ctx, layer, from, top)
            .into_iter()
            .chain(kept.copied())
            .collect::<Vec<_>>();
        if split::whole_rows(&lines, room) != height {
            ctx.request_repaint();
        }
        ctx.data_mut(|d| d.insert_temp(key, Painted(lines)));
    });
}
