//! The one window every dialog is shown in. Its size changes only when the player drags an edge or a corner
//! (which moves that edge and no other) or maximizes and restores it, never with its content; it is moved by its
//! title bar, minimized, maximized and closed from there, and where each kind of window sits and how large it is
//! is kept between runs.
use bn_client_core::localise::tr;
use eframe::egui;
use serde::{Deserialize, Serialize};

/// What the player did to a kind of window, as the settings file keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// The window's top left corner.
    pub pos: [f32; 2],
    /// The size of the whole window, minimized or not.
    pub size: [f32; 2],
    /// Only the title bar is shown.
    pub collapsed: bool,
}

/// Where a window of a kind the player never moved first appears: `pivot` of the window sits at `pos`.
#[derive(Clone, Copy, Debug)]
pub struct Origin {
    pub pivot: egui::Align2,
    pub pos: egui::Pos2,
}

impl Origin {
    /// Centred under the top of the screen, as the dialogs always were.
    pub fn top_center(ctx: &egui::Context) -> Self {
        let screen = ctx.content_rect();
        Self {
            pivot: egui::Align2::CENTER_TOP,
            pos: egui::pos2(screen.center().x, screen.top() + screen.height() * 0.12),
        }
    }
}

/// Where a window sits by the screen's layout rather than where the player left it. Unless the spec lets the
/// player move it, such a window cannot be moved and is not remembered; moved or not, it never leaves its room.
#[derive(Clone, Copy, Debug)]
pub enum Place {
    /// The whole of `rect`.
    Fill(egui::Rect),
    /// The middle of `rect`.
    Center(egui::Rect),
    /// `pivot` of the window at `pos`, no larger than `max`; moved, it stays in `within`.
    Side {
        pivot: egui::Align2,
        pos: egui::Pos2,
        max: egui::Vec2,
        within: egui::Rect,
    },
}

impl Place {
    /// The most a window this first puts may take.
    pub fn room(self) -> egui::Vec2 {
        match self {
            Place::Fill(rect) | Place::Center(rect) => rect.size(),
            Place::Side { max, .. } => max,
        }
    }

    /// Where a window this puts first lies.
    fn bounds(self) -> egui::Rect {
        match self {
            Place::Fill(rect) | Place::Center(rect) => rect,
            Place::Side { within, .. } => within,
        }
    }
}

pub struct WindowSpec {
    pub title: String,
    pub id: egui::Id,
    pub saved: Option<WindowState>,
    pub origin: Origin,
    pub place: Option<Place>,
    /// A placed window is only first placed by its layout: the player moves and resizes it, and where they
    /// leave it is kept as for any other window.
    pub movable: bool,
    /// The width a window first has, none for half the room.
    pub width: Option<f32>,
    pub frame: Option<egui::Frame>,
    pub title_bar: bool,
    /// The title bar offers a close button.
    pub closable: bool,
    /// The body is a vertical scroll area; a body that lays itself out in the window's height is not.
    pub scrolls: bool,
    /// The title bar offers a help button.
    pub help: Option<TitleHelp>,
}

/// The help button of a title bar: read as `label`, with `tip` on hover.
pub struct TitleHelp {
    pub label: String,
    pub tip: String,
}

pub struct Shown<R> {
    /// What the contents returned; none while the window is minimized.
    pub inner: Option<R>,
    pub closed: bool,
    /// The title bar's help button was pressed.
    pub help: bool,
    pub state: Option<WindowState>,
}

/// The share of its room a window is first as tall and as wide as, but for a width the spec names.
const FIRST_HEIGHT: f32 = 0.7;
const FIRST_WIDTH: f32 = 0.5;
/// The smallest a window may be dragged to.
const LEAST: egui::Vec2 = egui::vec2(240.0, 120.0);
/// How far inside a window's edge its resize handles reach, and how far along it a corner's does.
const GRIP: f32 = 6.0;
const CORNER: f32 = 8.0;

/// Where a window is, kept by its id.
#[derive(Clone, Copy)]
struct Geometry {
    /// The whole window as the player left it, minimized or not.
    rect: egui::Rect,
    /// What `rect` was before it was maximized.
    restore: Option<egui::Rect>,
    minimized: bool,
}

/// The edges a resize handle moves.
#[derive(Clone, Copy)]
struct Grip {
    left: bool,
    right: bool,
    top: bool,
    bottom: bool,
}

const fn grip(left: bool, right: bool, top: bool, bottom: bool) -> Grip {
    Grip {
        left,
        right,
        top,
        bottom,
    }
}

const GRIPS: [Grip; 8] = [
    grip(true, false, false, false),
    grip(false, true, false, false),
    grip(false, false, true, false),
    grip(false, false, false, true),
    grip(true, false, true, false),
    grip(false, true, true, false),
    grip(true, false, false, true),
    grip(false, true, false, true),
];

impl Grip {
    fn cursor(self) -> egui::CursorIcon {
        match (self.left || self.right, self.top || self.bottom) {
            (true, true) if self.left == self.top => egui::CursorIcon::ResizeNwSe,
            (true, true) => egui::CursorIcon::ResizeNeSw,
            (true, false) => egui::CursorIcon::ResizeHorizontal,
            _ => egui::CursorIcon::ResizeVertical,
        }
    }

    /// The handle's strip of `rect`: an edge, or a square at a corner.
    fn strip(self, rect: egui::Rect) -> egui::Rect {
        let corner = (self.left || self.right) && (self.top || self.bottom);
        let reach = if corner { CORNER } else { GRIP };
        let along = |near: bool, far: bool, lo: f32, hi: f32| match (near, far) {
            (true, _) => lo..=lo + reach,
            (_, true) => hi - reach..=hi,
            _ => lo + CORNER..=hi - CORNER,
        };
        egui::Rect::from_x_y_ranges(
            along(self.left, self.right, rect.left(), rect.right()),
            along(self.top, self.bottom, rect.top(), rect.bottom()),
        )
    }

    /// `rect` with the edges of this handle moved by `delta`, no smaller than `LEAST` and inside `bounds`.
    fn drag(self, rect: egui::Rect, delta: egui::Vec2, bounds: egui::Rect) -> egui::Rect {
        // Not clamps: by the screen's edge, a window already smaller than `LEAST` has the two bounds crossed.
        let mut out = rect;
        if self.left {
            out.min.x = (rect.min.x + delta.x)
                .min(rect.max.x - LEAST.x)
                .max(bounds.min.x);
        }
        if self.right {
            out.max.x = (rect.max.x + delta.x)
                .max(rect.min.x + LEAST.x)
                .min(bounds.max.x);
        }
        if self.top {
            out.min.y = (rect.min.y + delta.y)
                .min(rect.max.y - LEAST.y)
                .max(bounds.min.y);
        }
        if self.bottom {
            out.max.y = (rect.max.y + delta.y)
                .max(rect.min.y + LEAST.y)
                .min(bounds.max.y);
        }
        out
    }
}

/// `rect` moved, if need be, to lie in `bounds`, and no larger than them.
fn inside(rect: egui::Rect, bounds: egui::Rect) -> egui::Rect {
    let size = rect.size().min(bounds.size());
    // A room's far edge less its own width can round to just before its near edge.
    let far = (bounds.max - size).max(bounds.min);
    egui::Rect::from_min_size(rect.min.clamp(bounds.min, far), size)
}

/// The corner a window of `size` has when `pivot` of it sits at `pos`.
fn pivoted(pivot: egui::Align2, pos: egui::Pos2, size: egui::Vec2) -> egui::Pos2 {
    pos - egui::vec2(pivot.x().to_factor(), pivot.y().to_factor()) * size
}

/// Where a window the player has not touched first lies.
fn first_rect(spec: &WindowSpec, screen: egui::Rect) -> egui::Rect {
    if let Some(Place::Fill(rect)) = spec.place {
        return rect;
    }
    let room = spec.place.map_or(screen, Place::bounds);
    let cap = spec
        .place
        .map_or(screen.size(), Place::room)
        .min(room.size());
    let size = egui::vec2(
        spec.width.unwrap_or(FIRST_WIDTH * room.width()),
        FIRST_HEIGHT * room.height(),
    )
    .min(cap);
    let (pivot, pos) = match spec.place {
        Some(Place::Center(rect)) => (egui::Align2::CENTER_CENTER, rect.center()),
        Some(Place::Side { pivot, pos, .. }) => (pivot, pos),
        _ => (spec.origin.pivot, spec.origin.pos),
    };
    inside(
        egui::Rect::from_min_size(pivoted(pivot, pos, size), size),
        room,
    )
}

type Symbol<'a> = &'a dyn Fn(&egui::Painter, egui::Rect, egui::Stroke);

/// A title bar button at `rect`, named `label`, with a symbol drawn from strokes so that no font has to have it.
fn title_button(ui: &mut egui::Ui, rect: egui::Rect, label: &str, draw: Symbol) -> bool {
    let id = ui.id().with(("title button", label));
    let response = ui.interact(rect, id, egui::Sense::click());
    crate::a11y::named_button(ui.ctx(), id, label);
    let visuals = ui.style().interact(&response);
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, visuals.bg_fill);
    }
    draw(
        ui.painter(),
        rect.shrink(rect.width() * 0.28),
        visuals.fg_stroke,
    );
    response.on_hover_text(label).clicked()
}

/// The buttons the player pressed in a title bar.
#[derive(Default)]
struct Pressed {
    close: bool,
    maximize: bool,
    minimize: bool,
    help: bool,
}

/// The title bar of `spec` over `strip`: its title, and from the right close, maximize or restore, minimize and
/// help. A press on the bar and a drag of it are `bar`'s.
fn title_bar(
    ui: &mut egui::Ui,
    spec: &WindowSpec,
    strip: egui::Rect,
    geometry: &Geometry,
    free: bool,
    fills: bool,
) -> Pressed {
    let side = strip.height();
    let gap = ui.spacing().item_spacing.x;
    let mut pressed = Pressed::default();
    let mut right = strip.right();
    let mut cell = || {
        let rect = egui::Rect::from_min_size(
            egui::pos2(right - side, strip.top()),
            egui::Vec2::splat(side),
        );
        right -= side + gap;
        rect
    };
    if spec.closable {
        pressed.close = title_button(ui, cell(), &tr("Close window"), &|p, r, s| {
            p.line_segment([r.left_top(), r.right_bottom()], s);
            p.line_segment([r.right_top(), r.left_bottom()], s);
        });
    }
    if free {
        let restoring = geometry.restore.is_some();
        let name = if restoring {
            tr("Restore")
        } else {
            tr("Maximize")
        };
        pressed.maximize = title_button(ui, cell(), &name, &|p, r, s| {
            let step = r.width() * 0.25;
            let front = egui::Rect::from_min_max(
                r.min + egui::vec2(0.0, step),
                r.max - egui::vec2(step, 0.0),
            );
            let behind = front.translate(egui::vec2(step, -step));
            if restoring {
                p.rect_stroke(front, 0.0, s, egui::StrokeKind::Inside);
                p.rect_stroke(behind, 0.0, s, egui::StrokeKind::Inside);
            } else {
                p.rect_stroke(r, 0.0, s, egui::StrokeKind::Inside);
            }
        });
    }
    if !fills {
        let name = if geometry.minimized {
            tr("Expand")
        } else {
            tr("Minimize")
        };
        pressed.minimize = title_button(ui, cell(), &name, &|p, r, s| {
            p.line_segment([r.left_bottom(), r.right_bottom()], s);
        });
    }
    if let Some(help) = &spec.help {
        let rect = cell();
        let id = ui.id().with("help");
        let button = ui.interact(rect, id, egui::Sense::click());
        crate::a11y::named_button(ui.ctx(), id, &help.label);
        let font = egui::TextStyle::Heading.resolve(ui.style());
        let color = ui.visuals().text_color();
        ui.painter()
            .text(rect.center(), egui::Align2::CENTER_CENTER, "?", font, color);
        pressed.help = button.on_hover_text(&help.tip).clicked();
    }
    // Painted rather than a label, which the window's own accessible name already is.
    let mut job = egui::text::LayoutJob::simple_singleline(
        spec.title.clone(),
        egui::TextStyle::Heading.resolve(ui.style()),
        ui.visuals().strong_text_color(),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((right + gap - strip.left()).max(0.0));
    let galley = ui.painter().layout_job(job);
    let at = egui::pos2(strip.left(), strip.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, ui.visuals().text_color());
    pressed
}

/// Shows the window `spec` describes around `add`.
pub fn show<R>(
    ctx: &egui::Context,
    spec: WindowSpec,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Shown<R> {
    let screen = ctx.content_rect();
    let style = ctx.global_style();
    let frame = spec.frame.unwrap_or_else(|| egui::Frame::window(&style));
    let margin = frame.total_margin();
    let free = spec.place.is_none() || spec.movable;
    let fills = matches!(spec.place, Some(Place::Fill(_))) && !spec.movable;
    let key = spec.id.with("geometry");
    let mut geometry = ctx
        .data_mut(|d| d.get_temp::<Geometry>(key))
        .unwrap_or_else(|| match spec.saved {
            Some(s) => Geometry {
                rect: egui::Rect::from_min_size(s.pos.into(), s.size.into()),
                restore: None,
                minimized: s.collapsed,
            },
            None => Geometry {
                rect: first_rect(&spec, screen),
                restore: None,
                minimized: false,
            },
        });
    // A window its layout puts is where the layout says; a free one stays where the player left it, inside the
    // screen, however the screen changed.
    geometry.rect = match (spec.place, free) {
        (Some(Place::Fill(rect)), false) => rect,
        (
            Some(Place::Side {
                pivot, pos, within, ..
            }),
            false,
        ) => {
            let size = geometry.rect.size().min(within.size());
            inside(
                egui::Rect::from_min_size(pivoted(pivot, pos, size), size),
                within,
            )
        }
        (Some(Place::Center(rect)), false) => {
            egui::Rect::from_center_size(rect.center(), geometry.rect.size().min(rect.size()))
        }
        _ => inside(geometry.rect, screen),
    };
    // Shown on whole points, so that the text in the window is laid out on the same pixels whatever the room;
    // kept as it is, so that a window maximized on a screen of fractional points is still the screen.
    let whole = egui::Rect::from_min_max(geometry.rect.min.round(), geometry.rect.max.round());
    let side = ctx
        .fonts_mut(|f| f.row_height(&egui::TextStyle::Heading.resolve(&style)))
        .ceil();
    let bar = if spec.title_bar {
        side + style.spacing.item_spacing.y.ceil()
    } else {
        0.0
    };
    let shown_rect = match geometry.minimized {
        true => {
            egui::Rect::from_min_size(whole.min, egui::vec2(whole.width(), margin.sum().y + bar))
        }
        false => whole,
    };
    let (mut pressed, mut moved, mut dragged, mut inner) =
        (Pressed::default(), egui::Vec2::ZERO, None, None);
    let area = egui::Area::new(spec.id)
        .order(egui::Order::Middle)
        .fixed_pos(shown_rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            let background = ui.painter().add(egui::Shape::Noop);
            // The area is exactly the window, whatever the contents ask for, so a press on any of it raises it.
            ui.allocate_rect(shown_rect, egui::Sense::hover());
            let content = egui::Rect::from_min_max(
                shown_rect.min + egui::vec2(margin.left, margin.top),
                shown_rect.max - egui::vec2(margin.right, margin.bottom),
            );
            let mut body = content;
            if spec.title_bar {
                let strip =
                    egui::Rect::from_min_size(content.min, egui::vec2(content.width(), side));
                let id = spec.id.with("__title_click");
                let drag = ui.interact(strip, id, egui::Sense::click_and_drag());
                ui.ctx().accesskit_node_builder(id, |b| {
                    b.set_role(egui::accesskit::Role::TitleBar);
                    b.set_label(tr("Move window"));
                });
                pressed = title_bar(ui, &spec, strip, &geometry, free, fills);
                if free {
                    moved = drag.drag_delta();
                    pressed.maximize |= drag.double_clicked();
                }
                body.min.y += bar;
            }
            if !geometry.minimized {
                // The body is laid out in what the window has, and what does not fit is cut off.
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
                // What overflows by a margin of the body, such as a line taller than its row, is still drawn.
                let reach = egui::vec2(margin.left, margin.top);
                child.set_clip_rect(body.expand2(reach).intersect(shown_rect));
                inner = Some(match spec.scrolls {
                    true => {
                        egui::ScrollArea::vertical()
                            .id_salt(spec.id.with("body"))
                            .auto_shrink(false)
                            .show(&mut child, add)
                            .inner
                    }
                    false => add(&mut child),
                });
                if free {
                    for g in GRIPS {
                        let id = spec.id.with(("grip", g.left, g.right, g.top, g.bottom));
                        let handle = ui.interact(g.strip(shown_rect), id, egui::Sense::drag());
                        ui.ctx().accesskit_node_builder(id, |b| {
                            b.set_role(egui::accesskit::Role::Splitter);
                            b.set_label(tr("Resize window"));
                        });
                        if handle.hovered() || handle.dragged() {
                            ui.ctx().set_cursor_icon(g.cursor());
                        }
                        if handle.dragged() {
                            dragged = Some((g, handle.drag_delta()));
                        }
                    }
                }
            }
            ui.painter().set(background, frame.paint(content));
        });
    area.response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Window, true, &spec.title));
    let rect = match dragged {
        Some((g, delta)) => g.drag(geometry.rect, delta, screen),
        None => inside(geometry.rect.translate(moved), screen),
    };
    if rect != geometry.rect {
        geometry.restore = None;
        geometry.rect = rect;
    }
    if pressed.maximize {
        geometry.rect = match geometry.restore.take() {
            Some(before) => inside(before, screen),
            None => {
                geometry.restore = Some(geometry.rect);
                screen
            }
        };
        geometry.minimized = false;
    }
    geometry.minimized ^= pressed.minimize;
    ctx.data_mut(|d| d.insert_temp(key, geometry));
    let state = free.then(|| WindowState {
        pos: [geometry.rect.min.x, geometry.rect.min.y],
        size: [geometry.rect.width(), geometry.rect.height()],
        collapsed: geometry.minimized,
    });
    Shown {
        inner,
        closed: pressed.close,
        help: pressed.help,
        state,
    }
}
