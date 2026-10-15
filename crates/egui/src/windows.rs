//! The one window every dialog is shown in: moved by its title bar, resized from its edges, collapsed and
//! expanded from the title bar, and closed where the engine offers cancel. Where each kind of window sits,
//! how large it is and whether it is collapsed is kept between runs.
use eframe::egui;
use serde::{Deserialize, Serialize};

/// How a window was first placed, kept while it is shown.
#[derive(Clone, Copy)]
struct Placed {
    kept: Option<WindowState>,
}

/// What the player did to a kind of window, as the settings file keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// The window's top left corner.
    pub pos: [f32; 2],
    /// The size of the whole window, which is what egui's default size sets.
    pub size: [f32; 2],
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

/// Where a window sits by the screen's layout rather than where the player left it: such a window cannot be
/// moved, is not remembered and never leaves the screen.
#[derive(Clone, Copy, Debug)]
pub enum Place {
    /// The whole of `rect`.
    Fill(egui::Rect),
    /// The middle of `rect`, as large as its content and no larger than `rect`.
    Center(egui::Rect),
    /// `pivot` of the window at `pos`, as large as its content and no larger than `max`.
    Side {
        pivot: egui::Align2,
        pos: egui::Pos2,
        max: egui::Vec2,
    },
}

pub struct WindowSpec {
    /// The kind of window whose place is kept: every window of a kind shares it.
    pub kind: String,
    pub title: String,
    pub id: egui::Id,
    pub saved: Option<WindowState>,
    pub origin: Origin,
    pub place: Option<Place>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub max_width: Option<f32>,
    pub frame: Option<egui::Frame>,
    pub title_bar: bool,
    /// The title bar offers a close button.
    pub closable: bool,
}

pub struct Shown<R> {
    /// What the contents returned; none while the window is collapsed.
    pub inner: Option<R>,
    pub closed: bool,
    pub state: Option<WindowState>,
}

/// `window` with `width` as its default width, when there is one.
fn widened(window: egui::Window<'_>, width: Option<f32>) -> egui::Window<'_> {
    match width {
        Some(width) => window.default_width(width),
        None => window,
    }
}

/// Shows the window `spec` describes around `add`.
pub fn show<R>(
    ctx: &egui::Context,
    spec: WindowSpec,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Shown<R> {
    let mut open = true;
    let mut window = egui::Window::new(&spec.title)
        .id(spec.id)
        .title_bar(spec.title_bar)
        .collapsible(spec.title_bar);
    // How the window was first placed holds for as long as egui remembers it, whatever the settings say
    // after that, so its pivot never changes under it.
    let key = spec.id.with("placed");
    let placed = ctx.data_mut(|d| {
        d.get_temp::<Placed>(key)
            .unwrap_or(Placed { kept: spec.saved })
    });
    ctx.data_mut(|d| d.insert_temp(key, placed));
    window = match (spec.place, placed.kept) {
        (Some(Place::Fill(rect)), _) => window
            .collapsible(false)
            .fixed_pos(rect.min)
            .fixed_size(rect.size()),
        (Some(Place::Center(rect)), _) => widened(window, spec.width)
            .collapsible(false)
            .anchor(
                egui::Align2::CENTER_CENTER,
                rect.center() - ctx.content_rect().center(),
            )
            .max_size(rect.size()),
        (Some(Place::Side { pivot, pos, max }), _) => widened(window, spec.width)
            .collapsible(false)
            .pivot(pivot)
            .fixed_pos(pos)
            .max_size(max),
        (None, Some(s)) => window
            .pivot(egui::Align2::LEFT_TOP)
            .default_pos(s.pos)
            .default_size(s.size)
            .default_open(!s.collapsed),
        (None, None) => {
            // With its width known the window starts from its top left corner, so that resizing it from an
            // edge moves only that edge; without, egui centres it on the origin once it has a size.
            let window = match spec.width {
                Some(width) => {
                    let x = spec.origin.pos.x - spec.origin.pivot.x().to_factor() * width;
                    window
                        .pivot(egui::Align2::LEFT_TOP)
                        .default_pos(egui::pos2(x, spec.origin.pos.y))
                }
                None => window.pivot(spec.origin.pivot).default_pos(spec.origin.pos),
            };
            let size = egui::vec2(spec.width.unwrap_or(0.0), spec.height.unwrap_or(0.0));
            match (spec.width, spec.height) {
                (Some(_), Some(_)) => window.default_size(size),
                (Some(w), None) => window.default_width(w),
                (None, Some(h)) => window.default_height(h),
                (None, None) => window,
            }
        }
    };
    if let Some(w) = spec.max_width {
        window = window.max_width(w);
    }
    if let Some(frame) = spec.frame {
        window = window.frame(frame);
    }
    if spec.closable {
        window = window.open(&mut open);
    }
    let shown = window.show(ctx, add);
    let collapsed = shown.as_ref().is_some_and(|s| s.inner.is_none());
    // A collapsed window is only its title bar, so it keeps the size it had expanded.
    let state = shown
        .as_ref()
        .filter(|_| spec.place.is_none())
        .and_then(|s| {
            let pos = s.response.rect.min;
            let size = match collapsed {
                true => spec.saved.map(|p| p.size),
                false => Some([s.response.rect.width(), s.response.rect.height()]),
            };
            size.filter(|s| s.iter().all(|n| *n > 1.0))
                .map(|size| WindowState {
                    pos: [pos.x, pos.y],
                    size,
                    collapsed,
                })
        });
    Shown {
        inner: shown.and_then(|s| s.inner),
        closed: !open,
        state,
    }
}
