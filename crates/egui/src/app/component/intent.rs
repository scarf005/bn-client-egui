//! What the player did while a screen was drawn, as `Intent`s, and how each is handed to the interaction core:
//! the keys of the frame, the clicks and drags the widgets collected, and the engine's answers to them.

use super::*;

/// What the player did while the screen was drawn.
pub(super) enum Intent {
    Focus {
        component: String,
        row: String,
    },
    Mark {
        component: String,
        row: String,
    },
    MarkCount {
        component: String,
        row: String,
        count: u64,
    },
    Run(String),
    /// A drag crossed a square of a canvas grid.
    Paint(Paint),
    /// Runs the affordance that puts the cursor on a square.
    Move {
        affordance: String,
        at: WorldPos,
    },
    Fill {
        affordance: String,
        form: String,
        field: String,
        value: String,
    },
    More(String),
    /// A row of a list that runs nothing was double-clicked.
    Open,
    /// A result row of a list filter's prompt was clicked: confirm the text, then choose the row.
    Reach(String),
    /// The text a list's search box holds now, for the affordance that sets the list's filter.
    Search {
        affordance: String,
        text: String,
    },
    Text(String),
    Pane(String),
    Confirm,
    Cancel,
}

/// A square of a canvas grid a drag runs its affordance on.
pub(super) struct Paint {
    pub(super) component: String,
    /// The square's row; none for the affordance that opens or closes a drag.
    pub(super) row: Option<String>,
    pub(super) affordance: String,
    /// The text the affordance takes with the row, when it takes one.
    pub(super) text: Option<String>,
}

/// Which form `intent` acts on, when it runs something: none for an action of no form; nothing to say for one
/// that runs nothing.
fn acted_form(screen: &Screen, intent: &Intent) -> Option<Option<String>> {
    let form_of = |id: &str| {
        screen.components().into_iter().find_map(|c| match c {
            Component::Form(form)
                if form.submit.as_deref() == Some(id)
                    || form.head.affordances.iter().any(|a| a == id) =>
            {
                Some(form.head.id.clone())
            }
            _ => None,
        })
    };
    match intent {
        Intent::Fill { form, .. } => Some(Some(form.clone())),
        Intent::Run(id) => Some(form_of(id)),
        Intent::Paint(_)
        | Intent::Move { .. }
        | Intent::Search { .. }
        | Intent::Reach(_)
        | Intent::Pane(_)
        | Intent::Confirm => Some(None),
        _ => None,
    }
}

impl<M: GameModel> App<M> {
    /// Takes the stack the engine published, and moves the engine's cursor where the player's focus went.
    pub(super) fn sync_screens(&mut self) {
        self.screens.interaction.sync(self.model.screens());
        // A form sent on a screen that closed is no place for the news of one opened after it.
        let screens = self.model.screens();
        if let Some((at, _)) = &self.screens.sent_form
            && !screens.iter().any(|s| &s.screen_id == at)
        {
            self.screens.sent_form = None;
        }
        let top = screens.last().map(|s| s.screen_id.clone());
        if let Some((id, args)) = self.screens.interaction.take_select() {
            self.screens.select = top.clone().map(|at| (at, id, args));
        }
        // A select waiting for the engine belongs to the screen it was made on, not one opened since.
        if let Some((at, id, args)) = self.screens.select.take()
            && Some(&at) == top.as_ref()
            && let Err(InvokeError::Busy) = self.model.invoke_affordance(&id, args.clone())
        {
            self.screens.select = Some((at, id, args));
        }
        self.reach_row();
    }

    /// Chooses the row a filter's result click named once the prompt is gone and the list has it; a list that
    /// lacks it, a screen that is not a list, or one that is not described ends the wait.
    fn reach_row(&mut self) {
        let Some(row) = self.screens.reach.clone() else {
            return;
        };
        let Some(top) = interaction::described(self.model.screens()) else {
            self.screens.reach = None;
            return;
        };
        if form::filter_submit(top).is_some() {
            return;
        }
        self.screens.reach = None;
        let found = top.components().into_iter().find_map(|c| match c {
            Component::List(list) if self.model.rows(list).iter().any(|r| r.id == row) => {
                Some((list.head.id.clone(), list.activate.clone()))
            }
            _ => None,
        });
        if let Some((component, Some(activate))) = found {
            self.screens.interaction.focus_row(&component, &row);
            let out = self.screens.interaction.activate(top, &activate);
            self.settle(out);
        }
    }

    /// Runs the squares a drag crossed in order, as many as the engine takes now; the rest wait for a later frame.
    pub(super) fn send_paints(&mut self, ctx: &egui::Context) {
        while let Some(paint) = self.screens.paints.front() {
            let Some(top) = interaction::described(self.model.screens()) else {
                return self.screens.paints.clear();
            };
            let interaction = &mut self.screens.interaction;
            if let Some(row) = &paint.row {
                interaction.focus_row(&paint.component, row);
            }
            // An affordance that takes the drag's text is called with it as it is; the core only builds the others.
            let invoke = match &paint.text {
                Some(text) => Some((
                    paint.affordance.clone(),
                    InvokeArgs {
                        row: paint.row.clone(),
                        text: Some(text.clone()),
                        ..InvokeArgs::default()
                    },
                )),
                None => {
                    let out = interaction.activate(top, &paint.affordance);
                    self.screens.denial = out.note();
                    out.invoke
                }
            };
            if let Some((id, args)) = invoke
                && !self.invoke(&id, args)
            {
                return ctx.request_repaint();
            }
            self.screens.paints.pop_front();
        }
    }

    /// The keys pressed this frame go to the interaction core, which decides what each one does.
    pub(in crate::app) fn component_keys(&mut self, ctx: &egui::Context) {
        self.keys_consumed = false;
        canvas::keep_space(ctx);
        self.sync_screens();
        let pages = self.screens.list_pages.borrow().clone();
        // A text box has the typing; its keys are not the screen's, except Escape, which closes the screen (a filled filter box clears first).
        let typing = ctx.text_edit_focused();
        self.search_keys(ctx, typing);
        let keys = input::frame_keys(ctx)
            .into_iter()
            .filter(|k| !self.settings_key(k))
            .map(|k| k.token);
        let clears = crate::editor::clears_on_escape(ctx);
        let keys = keys
            .filter(|key| !typing || (key == "ESC" && !clears))
            .collect::<Vec<_>>();
        // A Tab the screen takes is its own: egui must not also walk its focus onto a widget. Any other Tab
        // walks egui's focus over the buttons.
        let takes = |key: &String| {
            interaction::described(self.model.screens()).is_some_and(|top| {
                top.affordances
                    .iter()
                    .any(|a| a.enabled && a.keys.contains(key))
            })
        };
        if keys
            .iter()
            .any(|k| (k == "TAB" || k == "BACKTAB") && takes(k))
        {
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        }
        for key in keys {
            let Some(top) = interaction::described(self.model.screens()) else {
                return;
            };
            // An enabled key field takes the key as its text, and the screen does not see it; Escape still
            // closes the screen.
            if let Some(fill) = key_fill(top, &key).filter(|_| key != "ESC") {
                self.keys_consumed = true;
                return self.apply(fill);
            }
            let page = |id: &str| {
                let at = (top.screen_id.clone(), id.to_string());
                pages.get(&at).copied().unwrap_or(1)
            };
            let model = &self.model;
            let out = self
                .screens
                .interaction
                .press(top, &key, &page, &|list| model.rows(list));
            if out.invoke.is_some() {
                self.screens.sent_form = None;
            }
            self.settle(out);
        }
    }

    /// The keys that take the screen's search box: Ctrl+F, and the keys the engine binds to its filter prompt, which
    /// the box replaces. They are the box's, not the screen's, and are not typed into it.
    fn search_keys(&mut self, ctx: &egui::Context, typing: bool) {
        let Some(top) = interaction::described(self.model.screens()) else {
            return;
        };
        let searched = top
            .components()
            .into_iter()
            .any(|c| matches!(c, Component::List(list) if list.search(top).is_some()));
        if !searched || typing {
            return;
        }
        let prompt = find(top, FILTER).map_or(vec![], |a| a.keys.clone());
        let takes = |e: &egui::Event| {
            let ctrl_f = matches!(e, egui::Event::Key { key: egui::Key::F, pressed: true, modifiers, .. }
                if modifiers.command && !modifiers.alt);
            ctrl_f || input::frame_key(e).is_some_and(|k| prompt.contains(&k.token))
        };
        if ctx.input(|i| i.events.iter().any(takes)) {
            self.screens.focus_search.set(true);
            ctx.input_mut(|i| i.events.retain(|e| !takes(e)));
        }
    }

    /// Shows what the core decided and sends the call it built.
    fn settle(&mut self, out: interaction::Outcome) {
        self.keys_consumed |= out.consumed;
        self.screens.denial = out.note();
        if let Some((id, args)) = out.invoke {
            self.invoke(&id, args);
        }
    }

    /// Sends the affordance `id` with `args`, showing the engine's reason when it is off; false while the engine is
    /// too busy to take it.
    fn invoke(&mut self, id: &str, args: InvokeArgs) -> bool {
        match self.model.invoke_affordance(id, args) {
            Err(InvokeError::Busy) => false,
            Err(InvokeError::Disabled(denial)) => {
                self.screens.denial = denial.as_ref().map(Text::shown);
                true
            }
            _ => true,
        }
    }

    /// Hands what the player did to the interaction core.
    pub(super) fn apply(&mut self, intent: Intent) {
        let Some(top) = interaction::described(self.model.screens()) else {
            return;
        };
        // The form the player sends takes the engine's news once it goes out; an act outside the forms gives the
        // news back to the status line.
        let acted = acted_form(top, &intent);
        if let Some(None) = acted {
            self.screens.sent_form = None;
        }
        let sent_form = acted.flatten().map(|form| (top.screen_id.clone(), form));
        let filled = match &intent {
            Intent::Fill { form, field, .. } => {
                Some((top.screen_id.clone(), form.clone(), field.clone()))
            }
            _ => None,
        };
        let interaction = &mut self.screens.interaction;
        let out = match intent {
            Intent::More(component) => return self.model.more_rows(&component),
            Intent::Focus { component, row } => return interaction.focus_row(&component, &row),
            Intent::Mark { component, row } => return interaction.toggle_pick(&component, &row),
            Intent::MarkCount {
                component,
                row,
                count,
            } => return interaction.set_pick_count(&component, &row, Some(count)),
            Intent::Text(typed) => return interaction.answer_text(&typed),
            Intent::Cancel => return interaction.cancel(),
            Intent::Open => return,
            Intent::Reach(row) => {
                let Some(confirm) = form::filter_submit(top) else {
                    return;
                };
                self.screens.reach = Some(row);
                return self.apply(confirm);
            }
            Intent::Search { affordance, text } => {
                let args = InvokeArgs {
                    text: Some(text),
                    ..InvokeArgs::default()
                };
                self.invoke(&affordance, args);
                return;
            }
            Intent::Run(id) => interaction.activate(top, &id),
            Intent::Paint(paint) => return self.screens.paints.push_back(paint),
            Intent::Move { affordance, at } => {
                let args = InvokeArgs {
                    pos: Some(at),
                    ..InvokeArgs::default()
                };
                self.invoke(&affordance, args);
                return;
            }
            Intent::Fill {
                affordance,
                form,
                field,
                value,
            } => interaction.fill(top, &affordance, &form, &field, &value),
            Intent::Pane(pane) => interaction.answer_pane(top, &pane),
            Intent::Confirm => interaction.confirm(top),
        };
        if out.invoke.is_some() && sent_form.is_some() {
            self.screens.sent_form = sent_form;
        }
        // A value the client refused stays by its field, and only there; one sent takes the refusal away.
        if let Some((screen, form, field)) = filled {
            let refusal = out.invoke.is_none().then(|| out.note()).flatten();
            let refused = refusal.is_some();
            let sent = out.invoke.as_ref().map(|(id, _)| id.clone());
            self.screens.field_refusal.replace(Some(FieldRefusal {
                screen,
                form,
                field,
                text: refusal,
                sent,
            }));
            if refused {
                self.keys_consumed |= out.consumed;
                self.screens.denial = None;
                return;
            }
        }
        self.settle(out);
    }
}
