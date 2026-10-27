//! The view of a canvas grid: how far it is zoomed and where it is scrolled, kept while its screen stays.

use eframe::egui;

/// How far one point of wheel travel zooms, as the exponent of the scale.
const WHEEL_ZOOM: f32 = 1.0 / 200.0;

/// A canvas as it was drawn last frame.
#[derive(Clone, Copy)]
pub(super) struct View {
    /// Columns and rows drawn: a grid of another size is another document, shown fitted anew.
    pub span: (usize, usize),
    /// The cells' scale from the size that fills the room.
    pub zoom: f32,
    pub cell: f32,
    /// The part of the screen the canvas shows its squares in.
    pub rect: egui::Rect,
    pub offset: egui::Vec2,
    /// The room left before the first cell, which centres a grid narrower than the room.
    pub lead: egui::Vec2,
    /// The offset a zoom asked for, which the scrolled area may have clamped to the content of the frame before.
    pub pending: Option<egui::Vec2>,
}

/// The wheel's hold on the grid: the point `under` the pointer, in cells, and where the pointer is in the room.
pub(super) struct Anchor {
    pub zoom: f32,
    pub under: egui::Vec2,
    pub pointer: egui::Vec2,
}

/// Where the cells sit in the scrolled area, and the offset a zoom asks of it.
pub(super) struct Placement {
    /// The room before the first cell. A grid smaller than the room is centred, and once zoomed it is moved
    /// wherever the point under the pointer needs it.
    pub lead: egui::Vec2,
    /// The room after the last cell: a zoomed grid scrolls until its far edge is the last thing in view.
    pub tail: egui::Vec2,
    pub offset: Option<egui::Vec2>,
}

/// The placement of a grid of `grid` size in `room` with cells of `cell`, `zoomed` past the whole grid fitting.
pub(super) fn place(
    (view, anchor): (Option<View>, Option<&Anchor>),
    zoomed: bool,
    (room, grid): (egui::Vec2, egui::Vec2),
    cell: f32,
) -> Placement {
    let centred = egui::vec2(((room.x - grid.x) / 2.0).max(0.0), 0.0);
    if !zoomed {
        return Placement {
            lead: centred,
            tail: egui::Vec2::ZERO,
            offset: anchor.map(|_| egui::Vec2::ZERO),
        };
    }
    let tail = (room - egui::vec2(cell, cell)).max(egui::Vec2::ZERO);
    let lead = view.map_or(centred, |v| v.lead);
    let Some(anchor) = anchor else {
        return Placement {
            lead,
            tail,
            offset: view.and_then(|v| v.pending),
        };
    };
    // The offset cannot be negative: the grid moves in the room instead.
    let wanted = anchor.under * cell + lead - anchor.pointer;
    Placement {
        lead: lead - wanted.min(egui::Vec2::ZERO),
        tail,
        offset: Some(wanted.max(egui::Vec2::ZERO)),
    }
}

/// The zoom after this frame's wheel with the pointer over `view`, and the point of the grid under the pointer, which
/// stays under it. `None` when the pointer is elsewhere or the zoom does not change.
pub(super) fn zoomed(ui: &egui::Ui, view: View, limits: (f32, f32)) -> Option<Anchor> {
    let pointer = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| view.rect.contains(*p))?;
    let wheel = ui.input_mut(|i| std::mem::take(&mut i.smooth_scroll_delta).y);
    let factor = ui.input(|i| i.zoom_delta()) * (wheel * WHEEL_ZOOM).exp();
    let zoom = (view.zoom * factor).clamp(limits.0, limits.1);
    if zoom == view.zoom {
        return None;
    }
    let pointer = pointer - view.rect.min;
    Some(Anchor {
        zoom,
        under: (pointer - view.lead + view.offset) / view.cell,
        pointer,
    })
}

/// Whether the primary button drags the canvas about instead of painting: Space is held.
pub(super) fn panning(ui: &egui::Ui) -> bool {
    ui.input(|i| i.key_down(egui::Key::Space))
}

/// The clips of the canvases drawn in this pass: their text scrolls on its own, so it is no line of the body.
pub(super) fn clips(ctx: &egui::Context) -> Vec<egui::Rect> {
    let pass = ctx.cumulative_pass_nr();
    let key = egui::Id::new("canvas clips");
    let (at, clips) = ctx
        .data(|d| d.get_temp::<(u64, Vec<egui::Rect>)>(key))
        .unwrap_or_default();
    if at == pass { clips } else { vec![] }
}

/// Notes the clip of a canvas drawn in this pass.
pub(super) fn note_clip(ctx: &egui::Context, clip: egui::Rect) {
    let pass = ctx.cumulative_pass_nr();
    let mut clips = clips(ctx);
    clips.push(clip);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("canvas clips"), (pass, clips)));
}
