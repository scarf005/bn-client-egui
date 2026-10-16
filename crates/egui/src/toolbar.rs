//! The shortcut bar at the map's left edge: one icon button per shortcut the engine publishes. The engine
//! decides which buttons exist, in which order, what each is called and which key it is bound to; the
//! client draws them and sends the click as the registered action it names.
use bn_client_core::GameModel;
use bn_client_core::markup;
use bn_client_core::protocol::Shortcut;
use eframe::egui::{self, accesskit};

/// The side of one button.
pub const BUTTON: f32 = 40.0;

/// The tooltip: the engine's name for the action, then the key the player has bound to it.
pub fn tip(shortcut: &Shortcut) -> String {
    let label = markup::plain(&shortcut.label.shown());
    match shortcut.keys.first() {
        Some(key) => format!("{label} [{key}]"),
        None => label,
    }
}

/// The bar, or nothing where the engine publishes no shortcuts (the title screen, a model without a game).
pub fn show(ui: &mut egui::Ui, model: &mut impl GameModel) {
    let shortcuts = model.shortcuts();
    if shortcuts.is_empty() {
        return;
    }
    let mut run = None;
    egui::Panel::left("shortcut-bar")
        .resizable(false)
        .show_separator_line(false)
        .exact_size(BUTTON)
        .frame(egui::Frame::NONE.fill(egui::Color32::from_gray(18)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                // A command on its way does not grey the bar: the model refuses what the engine cannot take yet.
                for shortcut in &shortcuts {
                    if button(ui, shortcut, shortcut.enabled).clicked() {
                        run = Some(shortcut.affordance_id.clone());
                    }
                }
            });
        });
    if let Some(id) = run {
        model.invoke_action(&id);
    }
}

fn button(ui: &mut egui::Ui, shortcut: &Shortcut, enabled: bool) -> egui::Response {
    let glyph = shortcut.glyph.as_deref().unwrap_or(&shortcut.icon);
    let text = egui::RichText::new(glyph).size(22.0);
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(text)
            .fill(egui::Color32::TRANSPARENT)
            .min_size(egui::vec2(BUTTON, BUTTON)),
    );
    let name = tip(shortcut);
    ui.ctx().accesskit_node_builder(response.id, |b| {
        b.set_role(accesskit::Role::Button);
        b.set_label(markup::plain(&shortcut.label.shown()));
        b.set_description(name.clone());
    });
    response.on_hover_text(name)
}
