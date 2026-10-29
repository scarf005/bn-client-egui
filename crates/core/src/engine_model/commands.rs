//! The commands the client sends: what a key runs, and the one way a command leaves the client.

use super::{EngineModel, Pending};
use crate::interaction::{self, InvokeError};
use crate::keys;
use crate::localise::tr;
use crate::markup;
use crate::model::{ActionEntry, CommandStatus, MenuChoice, MenuModel};
use crate::protocol::{self, Operation};

impl EngineModel {
    /// Sends the kept command once the engine has published a newer boundary of a timed screen; a screen
    /// that no longer redraws on its own drops it.
    pub(super) fn send_resend(&mut self) {
        let Some((_, stale)) = &self.resend else {
            return;
        };
        let Some(b) = self.replica.boundary().filter(|b| &b.boundary_id != stale) else {
            return;
        };
        let timed = b
            .interaction
            .as_ref()
            .is_some_and(|i| i.refresh_ms.is_some());
        if let Some((operation, _)) = self.resend.take().filter(|_| timed) {
            self.submit(operation);
            self.sent = None;
        }
    }

    /// The action the game key runs. The engine names the keys of its active context, the player's rebinds
    /// included; the client knows none. A digit with no binding may be a numpad key (terminals and windows do
    /// not tell them apart).
    pub(super) fn action_for(&self, key: &str) -> Option<String> {
        self.bound_action(key).or_else(|| {
            self.row_for(key)
                .is_none()
                .then(|| self.numpad_action(key))
                .flatten()
        })
    }

    fn bound_action(&self, key: &str) -> Option<String> {
        let b = self.replica.boundary()?;
        let bound = b.actions.iter().find(|a| {
            a.keys
                .as_ref()
                .is_some_and(|keys| keys.iter().any(|k| k == key))
        });
        bound.map(|a| a.id.clone())
    }

    /// The action of the numpad key a typed digit may have come from; a menu row showing the digit as its
    /// hotkey wins, as in a native list.
    fn numpad_action(&self, key: &str) -> Option<String> {
        keys::numpad(key).and_then(|k| self.bound_action(&k))
    }

    /// The enabled row of a menu without a text field that shows `key` as its hotkey.
    pub(super) fn row_for(&self, key: &str) -> Option<MenuChoice> {
        self.menu()
            .filter(|m| m.field.is_none())
            .and_then(|m| m.choices.into_iter().find(|c| c.enabled && c.hotkey == key))
    }

    pub(super) fn submit(&mut self, operation: Operation) -> bool {
        self.send(operation).is_ok()
    }

    /// The one way a command leaves the client: `interaction::submit` builds it with the expectation its
    /// operation takes, or says why there is none.
    pub(super) fn send(&mut self, operation: Operation) -> Result<(), InvokeError> {
        // Until the new snapshot lands the mirror is the old world's, whose epoch the engine refuses.
        if self.subscribing {
            return Err(InvokeError::Busy);
        }
        let (Some(at), Some(b)) = (self.replica.at(), self.replica.boundary()) else {
            return Err(InvokeError::NoScreen);
        };
        let basis = interaction::Basis {
            at,
            boundary: b,
            outstanding: self.unanswered as usize + self.running.len(),
            queue: self.screen_queue,
        };
        let submit = interaction::submit(&basis, operation)?;
        let boundary_id = b.boundary_id.clone();
        self.status = Some(self.describe(&submit.operation));
        self.sent = Some((submit.operation.clone(), boundary_id));
        self.unanswered += 1;
        self.busy = true;
        let params = serde_json::to_value(submit).expect("submit serializes");
        self.mark("submit");
        self.call("bn.command.submit", params, Pending::Submit);
        Ok(())
    }

    /// What the user pressed, named from the boundary the command was sent against.
    fn describe(&self, operation: &Operation) -> CommandStatus {
        let boundary = self.replica.boundary();
        let (label, target) = match operation {
            Operation::Choose { choice_id } => {
                let choices = boundary
                    .and_then(|b| b.interaction.as_ref())
                    .map(|i| &i.choices);
                let found = choices.and_then(|c| c.iter().find(|c| &c.id == choice_id));
                (
                    found.map(|c| markup::plain(&c.label)),
                    Some(choice_id.clone()),
                )
            }
            Operation::Action { action_id } => {
                let found = boundary.and_then(|b| b.actions.iter().find(|a| &a.id == action_id));
                (
                    found.map(|a| markup::plain(&a.name)),
                    Some(action_id.clone()),
                )
            }
            Operation::Invoke { affordance_id, .. } => {
                let found = boundary
                    .and_then(|b| b.stack.last())
                    .and_then(|s| s.affordances.iter().find(|a| &a.id == affordance_id));
                (
                    found.map(|a| markup::plain(&a.label.shown())),
                    Some(affordance_id.clone()),
                )
            }
            Operation::Travel { .. } => (Some(tr("Travel")), None),
            Operation::Context { .. } => (Some(tr("Right click")), None),
            Operation::TileAction { action_id, pos } => {
                let found = self
                    .hovered
                    .as_ref()
                    .filter(|h| &h.pos == pos)
                    .and_then(|h| h.actions.iter().find(|a| &a.id == action_id));
                (
                    found.map(|a| markup::plain(&a.label.shown())),
                    Some(action_id.clone()),
                )
            }
            Operation::Cancel => (Some(tr("Cancel")), None),
            _ => (None, None),
        };
        CommandStatus {
            label: label.unwrap_or_else(|| tr("Command")),
            target,
            stage: "sent".into(),
            text: String::new(),
            finished: false,
            history: vec![],
        }
    }

    /// Submits a registered action whose id is one of `ids`; false when the engine offers none.
    pub(super) fn invoke(&mut self, ids: &[String]) -> bool {
        let found = self.replica.boundary().and_then(|b| {
            b.actions
                .iter()
                .find(|a| ids.iter().any(|i| a.id.eq_ignore_ascii_case(i)))
        });
        let Some(action_id) = found.map(|a| a.id.clone()) else {
            return false;
        };
        self.submit(Operation::Action { action_id })
    }

    /// The enabled affordance of the map that is the registered action `id`.
    pub(super) fn map_affordance(&self, id: &str) -> Option<&protocol::Affordance> {
        let map = self
            .replica
            .stack()
            .last()
            .filter(|s| s.role == protocol::ScreenRole::Map)?;
        map.affordances
            .iter()
            .find(|a| a.enabled && a.id.eq_ignore_ascii_case(id))
    }

    pub(super) fn refresh_actions(&mut self) {
        let offered = self.replica.boundary().map_or(&[][..], |b| &b.actions);
        if !offered.is_empty() || !self.busy {
            // An action with no keys is offered only where the map's affordances run it.
            self.shown = offered
                .iter()
                .filter(|a| {
                    a.keys.as_ref().is_none_or(|keys| !keys.is_empty())
                        || self.map_affordance(&a.id).is_some()
                })
                .map(|a| ActionEntry {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    keys: a.keys.clone().unwrap_or_default(),
                })
                .collect();
        }
    }
}
