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

/// A plain button; egui reports every button drawn `selected` as a toggle.
pub fn button(ctx: &egui::Context, id: egui::Id) {
    ctx.accesskit_node_builder(id, |b| {
        b.set_role(accesskit::Role::Button);
        b.clear_toggled();
    });
}
