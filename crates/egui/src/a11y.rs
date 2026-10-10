//! Screen-reader metadata that egui's widgets do not provide themselves.

use eframe::egui::{self, accesskit};

/// Names the application window.
pub fn name_window(ctx: &egui::Context, title: &str) {
    ctx.accesskit_node_builder(egui::accesskit_root_id(), |b| b.set_label(title));
}

/// The id a dialog window is given, so its accessibility nodes can be addressed.
pub fn dialog_id(title: &str) -> egui::Id {
    egui::Id::new(("dialog", title))
}

/// Announces a window made with `dialog_id` as a dialog, reads its message and names its otherwise
/// anonymous title bar.
pub fn dialog(ctx: &egui::Context, title: &str, message: &str) {
    let id = dialog_id(title);
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

/// Keeps Tab and Shift+Tab inside `ids` (a modal's controls, in order): focus wraps at either end
/// and returns from outside. Call every frame the modal is open, before it is drawn.
pub fn trap_tab(ctx: &egui::Context, ids: &[egui::Id]) {
    let focused = ctx.memory(|m| m.focused());
    let at = focused.and_then(|f| ids.iter().position(|id| *id == f));
    if let Some(f) = focused.filter(|_| at.is_some()) {
        // Leave Tab to us instead of letting egui walk to the widgets behind the modal.
        ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                f,
                egui::EventFilter {
                    tab: true,
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
