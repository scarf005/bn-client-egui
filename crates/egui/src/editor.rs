//! Value editors for form fields and the key prompt of the keybindings screen. Each draws one control
//! and returns the value to send.

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
                let edit = ui.add(if editor.multiline {
                    egui::TextEdit::multiline(&mut draft.text)
                } else {
                    egui::TextEdit::singleline(&mut draft.text)
                });
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
    key_wait(ui);
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

/// The line that asks for the key, read out as soon as it shows.
pub fn key_wait(ui: &mut egui::Ui) {
    let label = ui.label("Press the new key");
    ui.ctx()
        .accesskit_node_builder(label.id, |b| b.set_live(accesskit::Live::Assertive));
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

/// The state of a typing prompt between frames.
#[derive(Clone, Default)]
struct Typing {
    /// The screen the prompt is on: a prompt on another screen starts anew.
    screen: String,
    /// The value the engine last showed.
    engine: String,
    text: String,
    /// The text last handed out, so it goes out once.
    handed: String,
    /// The texts handed out that the engine has not echoed yet, to tell an echo of one from news.
    sent: Vec<String>,
    composing: bool,
    entered: bool,
    submit: bool,
}

/// What a typing prompt hands out this frame.
#[derive(Default)]
pub struct Typed {
    /// The newest text, once the engine can take it.
    pub changed: Option<String>,
    /// The text that Return submits, once the engine can take it.
    pub submit: Option<String>,
}

/// A text box whose every change goes out as it is typed and whose Return submits, such as the prompt of a
/// list filter. The engine's value replaces the box only when it is news: a new prompt, or a value this box
/// did not send. The echo of an older send, or any change while the engine is busy, must not undo the newer
/// text, whose cursor would then land in the wrong place. `screen` tells one prompt from the next.
pub fn typing_prompt(
    ui: &mut egui::Ui,
    screen: &str,
    id: &str,
    label: &str,
    value: &str,
    busy: bool,
) -> Typed {
    // One state per field id, so the prompts of closed screens leave nothing behind.
    let key = egui::Id::new(("typing", id));
    let kept = ui
        .data(|d| d.get_temp::<Typing>(key))
        .filter(|t| t.screen == screen);
    let fresh = kept.is_none();
    let mut state = kept.unwrap_or_else(|| Typing {
        screen: screen.into(),
        engine: value.into(),
        text: value.into(),
        handed: value.into(),
        ..Typing::default()
    });
    if !fresh && state.engine != value {
        let echoed = state.sent.iter().position(|sent| sent == value);
        if !(echoed.is_some() || busy) {
            state.text = value.into();
            state.handed = value.into();
        }
        // The sends before an echoed one are answered too.
        if let Some(at) = echoed {
            state.sent.drain(..=at);
        }
        state.engine = value.into();
    }
    let edit = egui::TextEdit::singleline(&mut state.text).desired_width(f32::INFINITY);
    let response = ui.add(edit.hint_text(label));
    let name = label.to_string();
    ui.ctx()
        .accesskit_node_builder(response.id, move |b| b.set_label(name));
    if !state.entered {
        response.request_focus();
        state.entered = response.has_focus();
    }
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    state.submit |= (response.lost_focus() || response.has_focus()) && enter;
    // Preedit text sits in the box while composing; only the committed text is typed.
    state.composing = ui.input(|i| {
        i.events
            .iter()
            .fold(state.composing, |composing, event| match event {
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => !text.is_empty(),
                egui::Event::Ime(egui::ImeEvent::Commit(_)) => false,
                _ => composing,
            })
    });
    let mut typed = Typed::default();
    if state.submit && !busy {
        state.submit = false;
        state.handed = state.text.clone();
        state.sent.push(state.text.clone());
        typed.submit = Some(state.text.clone());
    } else if state.text != state.handed && !state.composing && !busy {
        state.handed = state.text.clone();
        state.sent.push(state.text.clone());
        typed.changed = Some(state.text.clone());
    }
    ui.data_mut(|d| d.insert_temp(key, state));
    typed
}
