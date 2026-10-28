//! Value editors for form fields and the key prompt of the keybindings screen. Each draws one control
//! and returns the value to send.

use bn_client_core::localise::tr;
use bn_client_core::protocol::Editor;
use eframe::egui::{self, accesskit};

/// What the user typed so far, and the engine value it was started from.
#[derive(Clone)]
struct Draft {
    seen: String,
    text: String,
}

/// The space between a field's frame and its text, on each side.
const FIELD_MARGIN: egui::Margin = egui::Margin::symmetric(4, 2);

/// The rows a multiline field shows before its text scrolls.
const MULTILINE_ROWS: usize = 6;

/// The width a one-line field takes to show the whole of `value`.
pub fn field_width(ui: &egui::Ui, value: &str) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text = ui
        .painter()
        .layout_no_wrap(value.into(), font, egui::Color32::WHITE)
        .size()
        .x;
    text + FIELD_MARGIN.sum().x
}

/// A one-line field's text wrapped at `wrap`: a one-line field lays its text out unwrapped, and a value wider than
/// the field would run past it or be cut.
fn wrapped_text(ui: &egui::Ui, text: &str, wrap: f32) -> std::sync::Arc<egui::Galley> {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let color = ui
        .visuals()
        .override_text_color
        .unwrap_or_else(|| ui.visuals().widgets.inactive.text_color());
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, color, wrap);
    job.keep_trailing_whitespace = true;
    ui.fonts_mut(|f| f.layout_job(job))
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
                // A one-line field widens to its whole value rather than cut it, up to the room it has.
                let edit = match editor.multiline {
                    // A long text scrolls in a box of a few lines, as wide as the room left.
                    true => {
                        let line = ui.text_style_height(&egui::TextStyle::Body);
                        egui::ScrollArea::vertical()
                            .id_salt(key)
                            .max_height(MULTILINE_ROWS as f32 * line)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut draft.text)
                                        .id_salt(key)
                                        .desired_rows(MULTILINE_ROWS)
                                        .desired_width(ui.available_width())
                                        .margin(FIELD_MARGIN),
                                )
                            })
                            .inner
                    }
                    // Wider than the room, it wraps its value onto further lines instead of running past the
                    // room's edge.
                    false => {
                        let room = ui.available_width();
                        let width = field_width(ui, &draft.text)
                            .max(ui.spacing().text_edit_width)
                            .min(room);
                        let wrap = width - FIELD_MARGIN.sum().x;
                        let mut wrapped = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _: f32| {
                            wrapped_text(ui, text.as_str(), wrap)
                        };
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.text)
                                .desired_width(width)
                                .layouter(&mut wrapped)
                                .clip_text(false)
                                .margin(FIELD_MARGIN),
                        )
                    }
                };
                edit.widget_info(|| {
                    egui::WidgetInfo::text_edit(true, draft.text.clone(), draft.text.clone(), label)
                });
                // The box is named by its field, as its label beside it reads.
                ui.ctx()
                    .accesskit_node_builder(edit.id, |b| b.set_label(label));
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
            } if *key != egui::Key::Escape => crate::app::key_name(*key, *modifiers),
            _ => None,
        })
    })
}

/// The line that asks for the key, read out as soon as it shows.
pub fn key_wait(ui: &mut egui::Ui) {
    let label = ui.label(tr("Press the new key"));
    ui.ctx()
        .accesskit_node_builder(label.id, |b| b.set_live(accesskit::Live::Assertive));
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

/// Where the id of the typing prompt's box is kept while it holds text, as of its last frame.
const FILLED: egui::Id = egui::Id::NULL;

/// Whether the focused text box is a typing prompt holding text, whose Escape empties it instead of closing.
pub fn clears_on_escape(ctx: &egui::Context) -> bool {
    let filled = ctx.data(|d| d.get_temp::<Option<egui::Id>>(FILLED).flatten());
    filled.is_some_and(|id| ctx.memory(|m| m.has_focus(id)))
}

/// What a typing prompt hands out this frame.
#[derive(Default)]
pub struct Typed {
    /// The newest text, once the engine can take it.
    pub changed: Option<String>,
    /// The text that Return submits, once the engine can take it.
    pub submit: Option<String>,
    /// A search box was left by Return or Down, for the results.
    pub left: bool,
}

/// A search box above a list; see `search_box`.
pub struct Search<'a> {
    pub screen: &'a str,
    pub id: &'a str,
    pub label: &'a str,
    /// The filter text the engine holds.
    pub value: &'a str,
    pub busy: bool,
    /// The box takes the keyboard this frame.
    pub focus: bool,
}

/// A text box above a list whose every change goes out as it is typed, the newest text last. It has the keyboard
/// only when asked: Escape empties it, then leaves it, and Return or Down leave it for the results.
pub fn search_box(ui: &mut egui::Ui, search: &Search) -> Typed {
    let Search {
        screen,
        id,
        label,
        value,
        busy,
        focus,
    } = *search;
    typing(ui, (screen, id, label, value, busy), Some(focus))
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
    typing(ui, (screen, id, label, value, busy), None)
}

/// A typing prompt, or a search box when `search` holds whether it takes the keyboard now.
fn typing(
    ui: &mut egui::Ui,
    (screen, id, label, value, busy): (&str, &str, &str, &str, bool),
    search: Option<bool>,
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
    // Escape empties a filled box first; the screen's own Escape waits for an empty one.
    let box_id = egui::Id::new(("typing-box", id));
    let focused = ui.memory(|m| m.has_focus(box_id));
    let escaped = focused
        && (search.is_some() || !state.text.is_empty())
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let cleared = escaped && !state.text.is_empty();
    if cleared {
        state.text.clear();
    }
    // Down is no focus walk in a search box: it goes to the results.
    let down = search.is_some()
        && focused
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
    ui.memory_mut(|m| match (search, (escaped && !cleared) || down) {
        (_, true) => m.surrender_focus(box_id),
        (Some(true), false) => m.request_focus(box_id),
        _ => {}
    });
    // As wide as the room, wrapping a longer text.
    let wrap = ui.available_width() - 2.0 * ui.spacing().button_padding.x;
    let mut wrapped =
        |ui: &egui::Ui, text: &dyn egui::TextBuffer, _: f32| wrapped_text(ui, text.as_str(), wrap);
    let edit = egui::TextEdit::singleline(&mut state.text)
        .id(box_id)
        .desired_width(f32::INFINITY)
        .layouter(&mut wrapped)
        .clip_text(false);
    let response = ui.add(edit.hint_text(label));
    // egui drops the focus of a box on Escape unless the box's event filter says it handles it.
    if response.has_focus() && (search.is_some() || !state.text.is_empty()) {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                box_id,
                egui::EventFilter {
                    escape: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            )
        });
    }
    let name = label.to_string();
    ui.ctx()
        .accesskit_node_builder(response.id, move |b| b.set_label(name));
    // A window that is not shown yet cannot take it.
    if search.is_none() && !state.entered && ui.is_enabled() {
        response.request_focus();
        state.entered = response.has_focus();
    }
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    state.submit |= search.is_none() && (response.lost_focus() || response.has_focus()) && enter;
    let left = search.is_some() && (down || (response.lost_focus() && enter));
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
    ui.data_mut(|d| {
        d.insert_temp(
            FILLED,
            (cleared || search.is_some() || !state.text.is_empty()).then_some(box_id),
        )
    });
    let mut typed = Typed {
        left,
        ..Typed::default()
    };
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
