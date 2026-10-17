//! Screen-reader metadata that egui's widgets do not provide themselves.

use eframe::egui::{self, accesskit};

/// Names the application window.
pub fn name_window(ctx: &egui::Context, title: &str) {
    ctx.accesskit_node_builder(egui::accesskit_root_id(), |b| b.set_label(title));
}

/// The id the dialog window `key` names is given, so its accessibility nodes can be addressed.
pub fn dialog_id(key: &str) -> egui::Id {
    egui::Id::new(("dialog", key))
}

/// Announces the window `id` as a dialog, reads its message and names its otherwise anonymous title bar.
pub fn dialog(ctx: &egui::Context, id: egui::Id, title: &str, message: &str) {
    // egui registers the window under its area's `move` widget.
    ctx.accesskit_node_builder(id.with("move"), |b| {
        b.set_role(accesskit::Role::Dialog);
        b.set_modal();
        if !message.is_empty() {
            b.set_description(message);
        }
    });
    ctx.accesskit_node_builder(id.with("__title_click"), |b| {
        b.set_role(accesskit::Role::TitleBar);
        b.set_label(title);
    });
}

/// A choice between sibling pages, not a toggle.
pub fn tab(ctx: &egui::Context, id: egui::Id, selected: bool) {
    ctx.accesskit_node_builder(id, |b| {
        b.set_role(accesskit::Role::Tab);
        b.clear_toggled();
        b.set_selected(selected);
    });
}

/// A button that reads as pressed only while `pressed`; egui reports every button drawn `selected`
/// as a toggle, pressed or not.
pub fn button(ctx: &egui::Context, id: egui::Id, pressed: bool) {
    ctx.accesskit_node_builder(id, |b| {
        b.set_role(accesskit::Role::Button);
        if !pressed {
            b.clear_toggled();
        }
    });
}

/// A button whose drawn text is a symbol, read as `label`.
pub fn named_button(ctx: &egui::Context, id: egui::Id, label: &str) {
    ctx.accesskit_node_builder(id, |b| {
        b.set_role(accesskit::Role::Button);
        b.set_label(label);
    });
}

/// Keeps Tab and Shift+Tab inside `ids` (a modal's controls, in order): focus wraps at either end
/// and returns from outside; `arrows` also keeps the arrow keys from moving focus, for a modal whose
/// arrows belong to the game. Call every frame the modal is open, before it is drawn. A text field
/// among `ids` must also be passed to `text_field` after it is drawn.
pub fn trap_tab(ctx: &egui::Context, ids: &[egui::Id], arrows: bool) {
    let focused = ctx.memory(|m| m.focused());
    let at = focused.and_then(|f| ids.iter().position(|id| *id == f));
    if let Some(f) = focused.filter(|_| at.is_some()) {
        // Leave Tab to us instead of letting egui walk to the widgets behind the modal.
        ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                f,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: arrows,
                    vertical_arrows: arrows,
                    ..Default::default()
                },
            )
        });
    }
    let (pressed, back) = ctx.input(|i| (i.key_pressed(egui::Key::Tab), i.modifiers.shift));
    let n = ids.len();
    let next = match (at, back) {
        _ if !pressed || n == 0 => return,
        (Some(i), false) => (i + 1) % n,
        (Some(i), true) => (i + n - 1) % n,
        (None, false) => 0,
        (None, true) => n - 1,
    };
    ctx.memory_mut(|m| m.request_focus(ids[next]));
}

/// Keeps a drawn text field's own Tab from moving focus past `trap_tab`; a text field sets its filter
/// while it is drawn, which replaces the one `trap_tab` set.
pub fn text_field(ctx: &egui::Context, id: egui::Id) {
    ctx.memory_mut(|m| {
        if m.has_focus(id) {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: false,
                },
            )
        }
    });
}

/// A polite live region that paints nothing: the text is announced when it changes but is not shown,
/// for content the screen already displays elsewhere.
pub fn live_region(ui: &mut egui::Ui, text: &str) {
    let region = ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover());
    ui.ctx().accesskit_node_builder(region.id, |b| {
        b.set_role(accesskit::Role::Label);
        b.set_value(text);
        b.set_live(accesskit::Live::Polite);
    });
}

/// Turns an append-only log into the text of one polite live region: every line added since the
/// last frame, joined in order, so a burst is read whole and history is never re-read.
#[derive(Default)]
pub struct Announcer {
    changed: bool,
    sep: &'static str,
    seen: Option<usize>,
    last: String,
    text: String,
}

impl Announcer {
    /// Joins a burst with `sep`.
    pub fn new(sep: &'static str) -> Self {
        Self {
            sep,
            ..Self::default()
        }
    }

    /// Whether the last `update` found new lines.
    pub fn changed(&self) -> bool {
        self.changed
    }

    pub fn update(&mut self, lines: &[String]) -> &str {
        self.changed = true;
        let Some(last) = lines.last() else {
            self.seen = Some(0);
            self.changed = false;
            return &self.text;
        };
        let first = match self.seen {
            None => lines.len() - 1,
            Some(seen) if seen < lines.len() => seen,
            // A repeat counter changed the newest line in place.
            Some(seen) if seen == lines.len() && *last != self.last => seen - 1,
            Some(seen) if seen > lines.len() => 0,
            Some(_) => {
                self.changed = false;
                return &self.text;
            }
        };
        self.text = lines[first..].join(self.sep);
        self.seen = Some(lines.len());
        self.last.clone_from(last);
        &self.text
    }
}
