//! Where a screen's window sits over the map.

use super::*;

/// The room a window leaves the map's squares around the avatar and the engine's cursor, in whole squares.
pub(super) const KEEP_CLEAR: f32 = 1.0;

/// The share of the map region a window beside the cursor, or docked to a side, takes in width.
const SIDE_SHARE: f32 = 0.4;

/// The space kept between a window and the edge of the map region.
const REGION_MARGIN: f32 = 8.0;

/// How a screen of the stack sits in `region` (the map panel; the sidebar stays visible): a full screen fills it, a
/// dialog is centred, a window that asked for a side sits there, and a screen that puts a cursor on the map sits
/// on the side away from it. `avoid` are the rectangles on the map that must stay uncovered.
pub(super) fn placement(
    screen: &Screen,
    region: egui::Rect,
    sidebar: Option<(SidebarSide, egui::Rect)>,
    avoid: &[egui::Rect],
) -> Place {
    let inner = region.shrink(REGION_MARGIN);
    let max = egui::vec2(inner.width() * SIDE_SHARE, inner.height());
    let side = |left: bool| Place::Side {
        pivot: if left {
            egui::Align2::LEFT_TOP
        } else {
            egui::Align2::RIGHT_TOP
        },
        pos: if left {
            inner.left_top()
        } else {
            inner.right_top()
        },
        max,
    };
    let beside = |left: bool| {
        let (x0, x1) = if left {
            (inner.left(), inner.left() + max.x)
        } else {
            (inner.right() - max.x, inner.right())
        };
        egui::Rect::from_x_y_ranges(x0..=x1, inner.y_range())
    };
    // Away from the cursor: the window's side is the one whose half of the room has none of `avoid`.
    let away = || {
        let covered = |left: bool| avoid.iter().any(|r| beside(left).intersects(*r));
        let cursor_left = avoid
            .first()
            .is_some_and(|r| r.center().x < inner.center().x);
        match (covered(!cursor_left), covered(cursor_left)) {
            (false, _) => side(!cursor_left),
            (true, false) => side(cursor_left),
            (true, true) => side(!cursor_left),
        }
    };
    match (screen.dock, screen.role) {
        (Some(ScreenDock::Left), _) => side(true),
        (Some(ScreenDock::Right), _) => side(false),
        // Over the sidebar: its own place, level with the map's top.
        (Some(ScreenDock::Overlapping), _) => match sidebar {
            Some((_, rect)) => Place::Side {
                pivot: egui::Align2::LEFT_TOP,
                pos: egui::pos2(rect.left(), inner.top()),
                max: egui::vec2(rect.width(), inner.height()),
            },
            None => side(true),
        },
        (Some(ScreenDock::Cursor), _) => away(),
        _ if keeps_off_cursor(screen) => away(),
        (None, ScreenRole::Dialog) => Place::Center(inner),
        (None, _) => Place::Fill(inner),
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
