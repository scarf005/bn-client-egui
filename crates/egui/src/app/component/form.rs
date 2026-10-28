//! Form fields and the arguments an affordance still asks the player for.

use super::*;
use bn_client_core::localise::tr;

/// The form field as the value editors take it.
fn editor_of(field: &FormField) -> Editor {
    let kind = match field.kind {
        FieldKind::Text => "text",
        FieldKind::Integer => "integer",
        FieldKind::Float => "float",
        FieldKind::Key => "key",
        FieldKind::Bool => "bool",
        FieldKind::Select => "select",
    };
    Editor {
        kind: kind.into(),
        value: field.value.clone(),
        values: field
            .options
            .iter()
            .flatten()
            .map(|o| EditorValue {
                id: o.id.clone(),
                label: o.label.shown(),
            })
            .collect(),
        minimum: field.min.clone(),
        maximum: field.max.clone(),
        max_length: field.max_length.unwrap_or(-1),
        multiline: field.multiline.unwrap_or(false),
    }
}

/// The fill of the screen's enabled key field with `key` as its text.
pub(super) fn key_fill(screen: &Screen, key: &str) -> Option<Intent> {
    screen.components().into_iter().find_map(|c| {
        let Component::Form(form) = c else {
            return None;
        };
        let field = form.fields.iter().find(|f| f.kind == FieldKind::Key)?;
        let fill = interaction::form_fill(screen, form, &field.id).filter(|a| a.enabled)?;
        Some(Intent::Fill {
            affordance: fill.id.clone(),
            form: form.head.id.clone(),
            field: field.id.clone(),
            value: key.into(),
        })
    })
}

/// The call that confirms the text the screen's list filter holds, when the screen is one.
pub(super) fn filter_submit(screen: &Screen) -> Option<Intent> {
    screen.components().into_iter().find_map(|c| {
        let Component::Form(form) = c else {
            return None;
        };
        let submit = find(screen, form.submit.as_deref()?)
            .filter(|a| a.enabled && a.role == Some(AffordanceRole::Confirm))?;
        let field = form.fields.iter().find(|f| f.kind == FieldKind::Text)?;
        Some(Intent::Fill {
            affordance: submit.id.clone(),
            form: form.head.id.clone(),
            field: field.id.clone(),
            value: field.value.clone(),
        })
    })
}

impl<M: GameModel> App<M> {
    pub(super) fn form(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        form: &Form,
        out: &mut Vec<Intent>,
    ) {
        for field in &form.fields {
            let label = markup::plain(&field.label.shown());
            if let Some(description) = &field.description {
                ui.weak(markup::plain(&description.shown()));
            }
            let fill = interaction::form_fill(screen, form, &field.id);
            let on = fill.is_some_and(|a| a.enabled);
            let id = format!("{}/{}", form.head.id, field.id);
            // A prompt the engine confirms with Return takes each change as it is typed, so the screen can
            // follow it (a list filter).
            let submit = form
                .submit
                .as_deref()
                .and_then(|submit| find(screen, submit))
                .filter(|a| a.role == Some(AffordanceRole::Confirm));
            if let (FieldKind::Text, Some(fill), Some(submit)) = (field.kind, fill, submit) {
                let typed = editor::typing_prompt(
                    ui,
                    &screen.screen_id,
                    &id,
                    &label,
                    &field.value,
                    self.model.busy(),
                );
                let (affordance, value) = match (typed.submit, typed.changed) {
                    (Some(value), _) => (submit, value),
                    (None, Some(value)) => (fill, value),
                    _ => continue,
                };
                out.push(Intent::Fill {
                    affordance: affordance.id.clone(),
                    form: form.head.id.clone(),
                    field: field.id.clone(),
                    value,
                });
                continue;
            }
            let committed = match field.kind {
                // The key itself is taken with the screen's keys (`key_fill`).
                // The screen's message says what the key is for, and its any-key button asks for one.
                FieldKind::Key => {
                    if !label.is_empty() {
                        ui.label(&label);
                    }
                    let asked = screen
                        .affordances
                        .iter()
                        .any(|a| a.role == Some(AffordanceRole::AnyKey));
                    if on && !asked {
                        editor::key_wait(ui);
                    }
                    None
                }
                _ => {
                    let at = (
                        screen.screen_id.as_str(),
                        form.head.id.as_str(),
                        field.id.as_str(),
                    );
                    // The client's refusal of the value, else the engine's of the command that sent it.
                    let command = self.model.command();
                    let rejected = |sent: &Option<String>| {
                        command
                            .as_ref()
                            .filter(|c| c.finished && c.stage == "rejected")
                            .filter(|c| c.target.is_some() && &c.target == sent)
                            .map(|c| c.text.clone())
                    };
                    let refusal = self
                        .screens
                        .field_refusal
                        .borrow()
                        .as_ref()
                        .filter(|r| (r.screen.as_str(), r.form.as_str(), r.field.as_str()) == at)
                        .and_then(|r| r.text.clone().or_else(|| rejected(&r.sent)));
                    let edit = editor::value_editor(
                        ui,
                        &editor::ValueField {
                            id: &id,
                            label: &label,
                            editor: &editor_of(field),
                            enabled: on,
                            refusal: refusal.as_deref(),
                        },
                    );
                    // Editing the refused value takes its refusal away.
                    if edit.edited && refusal.is_some() {
                        self.screens.field_refusal.replace(None);
                    }
                    edit.committed
                }
            };
            if let (Some(value), Some(a)) = (committed, fill) {
                out.push(Intent::Fill {
                    affordance: a.id.clone(),
                    form: form.head.id.clone(),
                    field: field.id.clone(),
                    value,
                });
            }
        }
    }

    /// The count, text or pane the affordance being started still asks the player for.
    pub(super) fn question(&self, ui: &mut egui::Ui, screen: &Screen, out: &mut Vec<Intent>) {
        let focused = egui::Id::new("question focused");
        let Some(ask) = self.screens.interaction.ask() else {
            ui.data_mut(|d| d.remove::<egui::Id>(focused));
            return;
        };
        let Some(aff) = find(screen, &ask.affordance_id) else {
            return;
        };
        ui.group(|ui| {
            ui.label(egui::RichText::new(markup::plain(&aff.label.shown())).strong());
            if let (Some(count), Some(ArgKind::Count)) = (ask.count, ask.asking) {
                ui.label(egui::RichText::new(count.to_string()).monospace());
            }
            if let Some(select) = &ask.select {
                ui.horizontal_wrapped(|ui| {
                    for option in &select.options {
                        let label = markup::plain(&option.label.shown());
                        if ui.button(label).clicked() {
                            out.push(Intent::Text(option.id.clone()));
                            out.push(Intent::Confirm);
                        }
                    }
                });
            } else if ask.capture {
                ui.label(tr("Press the new key"));
            } else if let (Some(typed), Some(ArgKind::Text)) = (&ask.text, ask.asking) {
                let mut shown = typed.clone();
                let edit = ui.text_edit_singleline(&mut shown);
                // The box a question opens with has the typing, as in a game's own prompt, once its window is shown.
                let opened = egui::Id::new(&ask.affordance_id);
                if ui.is_enabled() && ui.data(|d| d.get_temp::<egui::Id>(focused)) != Some(opened) {
                    edit.request_focus();
                    ui.data_mut(|d| d.insert_temp(focused, opened));
                    // What it opens with, such as the engine's suggestion, is selected: typing replaces it.
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), edit.id) {
                        let all = egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(shown.chars().count()),
                        );
                        state.cursor.set_char_range(Some(all));
                        state.store(ui.ctx(), edit.id);
                    }
                }
                if edit.changed() {
                    out.push(Intent::Text(shown));
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    out.push(Intent::Confirm);
                }
            }
            ui.horizontal_wrapped(|ui| {
                for pane in &ask.panes {
                    let found = screen.root.as_ref().and_then(|root| root.find(pane));
                    if let Some(Component::Panel(p)) = found
                        && ui.button(markup::plain(&p.title.shown())).clicked()
                    {
                        out.push(Intent::Pane(p.head.id.clone()));
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button(tr("OK")).clicked() {
                    out.push(Intent::Confirm);
                }
                if ui.button(tr("Cancel")).clicked() {
                    out.push(Intent::Cancel);
                }
            });
        });
    }
}
