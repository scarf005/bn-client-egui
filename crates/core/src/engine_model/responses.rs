//! What the engine answers: command stages and the response to each kind of request.

use super::{EngineModel, Pending, RESIZE_DEBOUNCE};
use crate::localise::{self, Catalogue, Value as Arg, tr_format};
use crate::model::{Failure, FailureKind};
use crate::protocol::{self, RpcError, Snapshot};
use serde_json::{Value, json};
use std::sync::Arc;

/// JSON-RPC code of a request whose params the engine does not accept.
const INVALID_PARAMS: i64 = -32602;
/// JSON-RPC code of a method the engine does not know.
const METHOD_NOT_FOUND: i64 = -32601;

impl EngineModel {
    /// A command's stage, from a notification or from `bn.command.result`.
    pub(super) fn command_notice(&mut self, n: protocol::CommandNotice) {
        self.mark(match n.stage.as_str() {
            "validated" => "validated",
            "executing" => "executing",
            "completed" => "completed",
            _ => "stage",
        });
        if self.command_id.as_deref() == Some(&n.command_id) {
            let reason = n.error.as_ref().map(error_kind);
            let swap = reason.as_deref() == Some("stale_epoch");
            let stage = match n.stage.as_str() {
                "validated" => "accepted",
                "completed" => "done",
                _ if swap => "done",
                other => other,
            };
            // The world swap is no failure to tell.
            let text = match (&n.error_text, swap) {
                (_, true) => String::new(),
                (Some(why), false) => format!("{} {why}", n.stage_text).trim().into(),
                (None, false) => n.stage_text.clone(),
            };
            if let Some(s) = &mut self.status {
                s.set_stage(stage, text, n.is_terminal());
            }
        }
        let latest = self.command_id.as_deref() == Some(&n.command_id);
        let outstanding = n.is_terminal() && self.running.remove(&n.command_id);
        if outstanding {
            self.busy = self.unanswered > 0 || !self.running.is_empty();
        }
        if n.is_terminal() && (latest || outstanding) {
            if latest {
                self.command_id = None;
            }
            // The world swap after choosing the Tutorial ends its command with stale_epoch.
            let swap =
                n.stage == "interrupted" && n.error.as_ref().is_some_and(|e| e == "stale_epoch");
            if n.stage != "completed" && !swap {
                let reason = n.error.as_ref().map(error_kind);
                // A kept resend is the latest command; an earlier queued one is only reported.
                if !(latest && self.stale_resend(reason.as_deref())) {
                    self.report_failure(n.error_text.as_deref().unwrap_or(&n.stage));
                }
            }
        }
    }

    /// Hands a response to the handler of the request it answers; each handler deals with its own errors.
    pub(super) fn response(&mut self, id: u64, result: Result<Value, RpcError>) {
        let Some(kind) = self.pending.remove(&id) else {
            return;
        };
        match kind {
            Pending::Hello => self.hello_answered(result),
            Pending::Subscribe => self.subscribed(result),
            Pending::Submit => self.submitted(result),
            Pending::Viewport => {
                if let Err(e) = result {
                    self.report_failure(&reason_of(&e));
                }
            }
            // Held at the wrong moment (an activity, a popup): the key must go up and down again.
            Pending::Hold => {
                if result.is_err() {
                    self.holding = None;
                }
            }
            Pending::Interrupt => self.interrupt_answered(result),
            Pending::Outcome => {
                if let Some(n) = result.ok().and_then(|v| serde_json::from_value(v).ok()) {
                    self.command_notice(n);
                }
            }
            Pending::Retile => self.retiled(result),
            Pending::Describe => self.described(result),
            Pending::Choices => self.choices_page(result),
            Pending::Rows => self.rows_page(id, result),
        }
    }

    fn hello_answered(&mut self, result: Result<Value, RpcError>) {
        let value = match result {
            Ok(v) => v,
            Err(e) if incompatible(&e) => {
                self.failure = Some(Failure {
                    kind: FailureKind::Incompatible,
                    message: reason_of(&e),
                    stderr: self.transport.stderr_tail(),
                    engine: self.engine.clone(),
                });
                return;
            }
            Err(e) => return self.report_failure(&reason_of(&e)),
        };
        self.display = value["engine"]["display"].as_object().and_then(|d| {
            Some((
                d.get("tiles")?.as_bool()?,
                d.get("tileset")?.as_str()?.to_string(),
            ))
        });
        self.soundpack = value["engine"]["sound"]["soundpack"]
            .as_str()
            .map(std::path::PathBuf::from);
        self.mark("hello");
        self.catalogue = self
            .data_root
            .as_deref()
            .and_then(|root| Catalogue::load(root, value["locale"].as_str()?).map(Arc::new));
        self.chrome = value["locale"].as_str().and_then(Catalogue::client);
        localise::install(self.catalogue.clone());
        localise::install_client(self.chrome.clone());
        self.screen_queue = serde_json::from_value::<protocol::Hello>(value.clone())
            .map_or(1, |hello| hello.screen_queue());
        self.set_credits(&value);
        self.subscribe();
    }

    fn subscribed(&mut self, result: Result<Value, RpcError>) {
        self.mark("subscribed");
        self.subscribing = false;
        match result.map(serde_json::from_value::<Snapshot>) {
            Ok(Ok(s)) => self.replica.begin(s),
            Ok(Err(e)) => self.tell(tr_format("Bad snapshot: %s", &[Arg::Text(e.to_string())])),
            Err(e) => self.report_failure(&reason_of(&e)),
        }
    }

    fn submitted(&mut self, result: Result<Value, RpcError>) {
        self.unanswered = self.unanswered.saturating_sub(1);
        match result {
            Ok(value) => {
                self.mark("received");
                self.command_id = value["command_id"].as_str().map(String::from);
                self.running.extend(self.command_id.clone());
                if let Some(s) = self.status.as_mut().filter(|s| s.stage == "sent") {
                    let text = value["stage_text"].as_str().unwrap_or_default();
                    s.set_stage("accepted", text.into(), false);
                }
            }
            Err(e) => {
                let kind = e.data.as_ref().map_or(e.message.clone(), error_kind);
                self.stale_resend(Some(&kind));
                if let Some(s) = &mut self.status {
                    s.set_stage("rejected", reason_of(&e), true);
                }
                self.report_failure(&reason_of(&e));
            }
        }
        self.busy = self.unanswered > 0 || !self.running.is_empty();
    }

    /// The engine had no activity running; say so once, not per key press.
    fn interrupt_answered(&mut self, result: Result<Value, RpcError>) {
        let Err(e) = result else {
            return;
        };
        self.report_failure(&reason_of(&e));
        // An engine with nothing to interrupt may have finished the command without our hearing of it.
        if let (Some(at), Some(command_id)) = (self.replica.at(), self.command_id.clone()) {
            let params = protocol::ResultParams {
                epoch: at.epoch.clone(),
                command_id,
            };
            self.call("bn.command.result", json!(params), Pending::Outcome);
        }
    }

    fn retiled(&mut self, result: Result<Value, RpcError>) {
        match result {
            Ok(v) => self.set_credits(&v),
            // The engine refused (a command was running): ask again a moment later.
            Err(_) => {
                self.tiles_told = !self.tiles;
                self.tiles_retry = self.clock.now() + RESIZE_DEBOUNCE;
            }
        }
    }

    /// A square outside the engine's window has no description; the cell's own name stands.
    fn described(&mut self, result: Result<Value, RpcError>) {
        let described = result
            .ok()
            .and_then(|v| serde_json::from_value::<protocol::DescribeResult>(v).ok());
        if let Some(h) = self.hovered.as_mut() {
            let (parts, actions) =
                described.map_or_else(Default::default, |d| (d.parts, d.actions));
            h.parts = Some(parts);
            h.actions = actions;
        }
    }

    /// A page of a long 0.1 menu, for the boundary it was asked in; a refused one closes paging for it.
    fn choices_page(&mut self, result: Result<Value, RpcError>) {
        self.paged.waiting = false;
        let Ok(value) = result else {
            self.paged.closed = true;
            return;
        };
        if let Ok(page) = serde_json::from_value::<protocol::ChoicesResult>(value)
            && self.paged.key.0 == page.boundary_id
        {
            self.paged.more.extend(page.choices);
        }
    }

    /// A page of a list of the top screen; a refused one closes that list for its boundary.
    fn rows_page(&mut self, id: u64, result: Result<Value, RpcError>) {
        let asked = self.row_requests.remove(&id);
        let reply = result
            .ok()
            .and_then(|v| serde_json::from_value::<protocol::ChoicesReply>(v).ok());
        match (reply, asked) {
            (Some(protocol::ChoicesReply::Rows(page)), _) => self.pages.accept(page),
            (_, Some(component_id)) => self.pages.failed(&component_id),
            _ => {}
        }
    }

    /// Keeps a command rejected as `stale_boundary` for the next boundary: a timed screen redraws on its
    /// own, so a key sent as a tick lands is rejected though the player did nothing wrong.
    fn stale_resend(&mut self, reason: Option<&str>) -> bool {
        let timed = self
            .replica
            .boundary()
            .and_then(|b| b.interaction.as_ref())
            .is_some_and(|i| i.refresh_ms.is_some());
        let sent = self.sent.take();
        let keep = reason == Some("stale_boundary") && timed && sent.is_some();
        if keep {
            self.resend = sent;
        }
        keep
    }

    /// Logs a failed command's reason.
    fn report_failure(&mut self, text: &str) {
        self.tell(text.to_string());
    }

    /// Adds a client line; a repeat of the last one (a click held on an unreachable square) only keeps it longer.
    pub(super) fn tell(&mut self, text: String) {
        let now = self.clock.now();
        match (self.messages.last() == Some(&text), self.told.last_mut()) {
            (true, Some(at)) => *at = now,
            _ => {
                self.messages.push(text);
                self.told.push(now);
            }
        }
    }
}

/// The engine's localized reason for a failed call: its `message` in the error data, else the RPC message.
fn reason_of(e: &RpcError) -> String {
    e.data
        .as_ref()
        .and_then(|d| d["message"].as_str())
        .map_or_else(|| e.message.clone(), String::from)
}

/// A hello the engine cannot accept from this client at all: another protocol version, none, or a language
/// it cannot word.
fn incompatible(e: &RpcError) -> bool {
    e.code == METHOD_NOT_FOUND
        || e.code == INVALID_PARAMS
        || matches!(
            e.data.as_ref().map(error_kind).as_deref(),
            Some("negotiation_failed" | "unavailable_locale")
        )
}

fn error_kind(e: &Value) -> String {
    e.as_str()
        .or_else(|| e["kind"].as_str())
        .map_or_else(|| e.to_string(), String::from)
}
