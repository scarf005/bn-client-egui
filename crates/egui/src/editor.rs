//! Value editors for menu rows the engine lets a client set directly (the options menu) and the
//! key prompt of the keybindings menu. Each draws one control and returns the value to send.

use bn_client_core::protocol::Editor;
use eframe::egui::{self, accesskit};

/// What the user typed so far, and the engine value it was started from.
#[derive(Clone)]
struct Draft {
    seen: String,
    text: String,
}

/// Draws the control for `editor` next to `label`; returns the new value once it is committed.
pub fn value_editor(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    editor: &Editor,
    enabled: bool,
) -> Option<String> {
    let key = egui::Id::new(("editor", id));
    let mut draft = ui
        .data(|d| d.get_temp::<Draft>(key))
        .filter(|d| d.seen == editor.value)
        .unwrap_or_else(|| Draft {
            seen: editor.value.clone(),
            text: editor.value.clone(),
        });
    let mut committed = None;
    ui.horizontal(|ui| {
        ui.add_enabled_ui(enabled, |ui| match editor.kind.as_str() {
            "bool" => {
                let mut on = editor.value == "true";
                if ui.checkbox(&mut on, label).changed() {
                    committed = Some(on.to_string());
                }
            }
            "select" => {
                ui.label(label);
                let shown = editor
                    .values
                    .iter()
                    .find(|v| v.id == editor.value)
                    .map_or(editor.value.as_str(), |v| v.label.as_str());
                egui::ComboBox::from_id_salt(key)
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for v in &editor.values {
                            if ui
                                .selectable_label(v.id == editor.value, &v.label)
                                .clicked()
                            {
                                committed = Some(v.id.clone());
                            }
                        }
                    });
            }
            _ => {
                ui.label(label);
                let edit = ui.add(egui::TextEdit::singleline(&mut draft.text));
                edit.widget_info(|| {
                    egui::WidgetInfo::text_edit(true, draft.text.clone(), draft.text.clone(), label)
                });
                if edit.lost_focus() && draft.text != editor.value {
                    committed = Some(draft.text.clone());
                }
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(key, draft));
    committed
}

/// The engine asks for a key: the next key press names it. Escape is left to the cancel button.
pub fn key_prompt(ui: &mut egui::Ui) -> Option<String> {
    let label = ui.label("Press the new key");
    ui.ctx()
        .accesskit_node_builder(label.id, |b| b.set_live(accesskit::Live::Assertive));
    ui.input(|i| {
        i.events.iter().find_map(|event| match event {
            egui::Event::Text(text) if !text.trim().is_empty() => Some(text.clone()),
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => engine_key(*key, *modifiers),
            _ => None,
        })
    })
}

/// The engine's name for keys that produce no text, as keybinding files spell them.
fn engine_key(key: egui::Key, modifiers: egui::Modifiers) -> Option<String> {
    use egui::Key::*;
    let name = match key {
        ArrowUp => "UP",
        ArrowDown => "DOWN",
        ArrowLeft => "LEFT",
        ArrowRight => "RIGHT",
        Enter => "RETURN",
        Tab => "TAB",
        Backspace => "BACKSPACE",
        Delete => "DELETE",
        Home => "HOME",
        End => "END",
        PageUp => "PPAGE",
        PageDown => "NPAGE",
        Insert => "INSERT",
        F1 | F2 | F3 | F4 | F5 | F6 | F7 | F8 | F9 | F10 | F11 | F12 => key.name(),
        letter if modifiers.ctrl && letter.name().len() == 1 => {
            return Some(format!("CTRL+{}", letter.name()));
        }
        _ => return None,
    };
    Some(name.to_string())
}
