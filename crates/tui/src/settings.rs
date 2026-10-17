//! Settings screens in the terminal: the key prompt and text fields of the 0.1 prompts.

use crate::TuiApp;
use bn_client_core::{GameModel, Menu};
use crossterm::event::{KeyCode, KeyEvent};

/// A value being typed for a row or field.
pub(super) struct Edit {
    pub(super) id: String,
    text: String,
    numeric: bool,
    /// The field is the whole screen (a text prompt): Esc cancels the prompt, not just the edit.
    prompt: bool,
    /// Commit ends the interaction; a filter beside a list only updates it.
    submit: bool,
    /// The text last sent as a filter, so the newest is sent once, as soon as the engine can take it.
    sent: Option<String>,
    /// Enter was pressed: the text goes out as the final value as soon as the engine can take it.
    entered: bool,
}

impl Edit {
    pub(super) fn shown(&self) -> String {
        format!("{}_", self.text)
    }
}

impl<M: GameModel> TuiApp<M> {
    /// Handles the keys settings screens add; false leaves the key to the plain menu.
    pub(super) fn settings_key(&mut self, menu: &Menu, key: KeyEvent) -> bool {
        let field = menu.field.as_ref();
        if let Some(prompt) = field.filter(|f| f.kind == "key") {
            // The next key names the binding; Esc is left to cancel the prompt.
            if key.code == KeyCode::Esc {
                return false;
            }
            if let Some(name) = super::token(key) {
                self.model.fill(&prompt.id, &name, true);
            }
            return true;
        }
        let entries = menu.entries();
        let text_field = field.filter(|f| matches!(f.kind.as_str(), "text" | "integer"));
        if self.edit.is_none() {
            match (text_field, entries.is_empty(), key.code) {
                (Some(f), true, _) => {
                    self.edit = Some(Edit {
                        id: f.id.clone(),
                        text: f.value.clone(),
                        numeric: f.kind == "integer",
                        prompt: true,
                        submit: true,
                        sent: None,
                        entered: false,
                    });
                }
                (Some(f), false, KeyCode::Char('/')) => {
                    self.edit = Some(Edit {
                        id: f.id.clone(),
                        text: f.value.clone(),
                        numeric: false,
                        prompt: false,
                        submit: false,
                        sent: None,
                        entered: false,
                    });
                    return true;
                }
                _ => {}
            }
        }
        if let Some(edit) = self.edit.take() {
            self.edit_key(edit, key);
            return true;
        }
        // A screen that binds Tab (the next tab) gets it; the client's action list takes it otherwise.
        if key.code == KeyCode::Tab && !self.model.key_bound("TAB") {
            self.actions_open = Some(0);
            return true;
        }
        false
    }

    fn edit_key(&mut self, mut edit: Edit, key: KeyEvent) {
        match key.code {
            _ if edit.entered => self.edit = Some(edit),
            KeyCode::Esc if edit.prompt => self.model.cancel_menu(),
            KeyCode::Esc => {}
            // The value is sent by `send_filter`, at once or when the engine is free.
            KeyCode::Enter => {
                edit.entered = true;
                self.edit = Some(edit);
            }
            KeyCode::Backspace => {
                edit.text.pop();
                self.edit = Some(edit);
            }
            KeyCode::Char(c) if !edit.numeric || c.is_ascii_digit() || matches!(c, '-' | '.') => {
                edit.text.push(c);
                self.edit = Some(edit);
            }
            _ => self.edit = Some(edit),
        }
        self.send_filter();
    }

    /// A filter shows the engine's state as it is typed: the newest text goes out whenever the engine is
    /// free, and a text it was too busy for waits. A value submitted with Enter waits the same way.
    pub(super) fn send_filter(&mut self) {
        if self.model.busy() {
            return;
        }
        if self.edit.as_ref().is_some_and(|e| e.entered)
            && let Some(edit) = self.edit.take()
        {
            self.model.fill(&edit.id, &edit.text, edit.submit);
        } else if let Some(edit) = self
            .edit
            .as_mut()
            .filter(|e| !e.submit && e.sent.as_ref() != Some(&e.text))
        {
            edit.sent = Some(edit.text.clone());
            self.model.fill(&edit.id, &edit.text, false);
        }
    }
}

impl<M: GameModel> TuiApp<M> {
    /// The prompt lines a settings screen adds to its header.
    pub(super) fn settings_hint(&self, menu: &Menu) -> Vec<String> {
        let Some(field) = &menu.field else {
            return vec![];
        };
        match (field.kind.as_str(), &self.edit) {
            ("key", _) => vec!["Press the new key (Esc cancels)".into()],
            (_, Some(edit)) => vec![format!("{}: {}", field.label, edit.shown())],
            _ if !menu.entries().is_empty() => {
                vec![format!("/ filter: {}", field.value)]
            }
            _ => vec![format!("{}: {}_", field.label, field.value)],
        }
    }
}
