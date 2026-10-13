//! Settings screens in the terminal: editable option rows, the key prompt, text fields and the
//! registered actions of rule screens.

use crate::TuiApp;
use bn_client_core::protocol::Editor;
use bn_client_core::{GameModel, Menu};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A value being typed for a row or field.
pub(super) struct Edit {
    id: String,
    text: String,
    numeric: bool,
    /// The field is the whole screen (a text prompt): Esc cancels the prompt, not just the edit.
    prompt: bool,
    /// Commit ends the interaction; a filter beside a list only updates it.
    submit: bool,
    /// The text last sent as a filter, so the newest is sent once, as soon as the engine can take it.
    sent: Option<String>,
}

impl Edit {
    pub(super) fn shown(&self) -> String {
        format!("{}_", self.text)
    }
}

/// The engine's name for a pressed key, as keybinding files spell it.
pub(super) fn engine_key(key: KeyEvent) -> Option<String> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    Some(match key.code {
        KeyCode::Char(' ') => "SPACE".into(),
        KeyCode::Char(c) if ctrl => format!("CTRL+{}", c.to_ascii_uppercase()),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::F(n) => format!("F{n}"),
        KeyCode::Up => "UP".into(),
        KeyCode::Down => "DOWN".into(),
        KeyCode::Left => "LEFT".into(),
        KeyCode::Right => "RIGHT".into(),
        KeyCode::Home => "HOME".into(),
        KeyCode::End => "END".into(),
        KeyCode::PageUp => "PPAGE".into(),
        KeyCode::PageDown => "NPAGE".into(),
        KeyCode::Enter => "RETURN".into(),
        KeyCode::Tab => "TAB".into(),
        KeyCode::Backspace => "BACKSPACE".into(),
        KeyCode::Delete => "DELETE".into(),
        KeyCode::Insert => "INSERT".into(),
        _ => return None,
    })
}

/// The row's value as the screen shows it: a select by its label.
pub(super) fn shown_value(editor: &Editor) -> String {
    editor
        .values
        .iter()
        .find(|v| v.id == editor.value)
        .map_or_else(|| editor.value.clone(), |v| v.label.clone())
}

fn numeric(editor: &Editor) -> bool {
    matches!(editor.kind.as_str(), "integer" | "float")
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
            if let Some(name) = engine_key(key) {
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
        if key.code == KeyCode::Tab {
            self.actions_open = Some(0);
            return true;
        }
        let Some(row) = entries.get(self.selected).filter(|c| c.enabled) else {
            return false;
        };
        let Some(editor) = &row.editor else {
            return false;
        };
        let step = |forward: bool| {
            let at = editor.values.iter().position(|v| v.id == editor.value);
            let count = editor.values.len();
            at.and_then(|i| match forward {
                true => editor.values.get((i + 1) % count),
                false => editor.values.get((i + count - 1) % count),
            })
            .map(|v| v.id.clone())
        };
        match (editor.kind.as_str(), key.code) {
            ("bool", KeyCode::Enter | KeyCode::Char(' ')) => {
                let next = (editor.value != "true").to_string();
                self.model.fill(&row.id, &next, true);
            }
            ("select", KeyCode::Enter | KeyCode::Char(' ')) => {
                if let Some(next) = step(true) {
                    self.model.fill(&row.id, &next, true);
                }
            }
            ("select", KeyCode::Backspace) => {
                if let Some(next) = step(false) {
                    self.model.fill(&row.id, &next, true);
                }
            }
            (_, KeyCode::Enter) => {
                self.edit = Some(Edit {
                    id: row.id.clone(),
                    text: editor.value.clone(),
                    numeric: numeric(editor),
                    prompt: false,
                    submit: true,
                    sent: None,
                });
            }
            _ => return false,
        }
        true
    }

    fn edit_key(&mut self, mut edit: Edit, key: KeyEvent) {
        match key.code {
            KeyCode::Esc if edit.prompt => self.model.cancel_menu(),
            KeyCode::Esc => {}
            KeyCode::Enter => self.model.fill(&edit.id, &edit.text, edit.submit),
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
    /// free, and a text it was too busy for waits.
    pub(super) fn send_filter(&mut self) {
        if let Some(edit) = self
            .edit
            .as_mut()
            .filter(|e| !e.submit && e.sent.as_ref() != Some(&e.text))
            && !self.model.busy()
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
