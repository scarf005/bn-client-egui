//! The lines the log shows: the client's own for a while, the game's log and the sounds heard.

use super::*;

/// How long a client line stays in the log.
const MESSAGE_LIFE: Duration = Duration::from_secs(20);
/// Heard sounds kept; older ones have long faded.
const MAX_SOUNDS: usize = 50;
/// The log lines kept: the engine contract keeps the newest 100 (`messages` in the snapshot).
const MAX_LOG_LINES: usize = 100;

impl EngineModel {
    /// Drops the client's lines told `MESSAGE_LIFE` or longer before `now`.
    pub(super) fn expire_messages(&mut self, now: Duration) {
        // The client's lines stand beside the game's log for a while; they are not part of its history.
        let expired = self
            .told
            .iter()
            .take_while(|at| now.saturating_sub(**at) >= MESSAGE_LIFE)
            .count();
        self.messages.drain(..expired);
        self.told.drain(..expired);
    }

    /// Takes the sounds and the game's log lines the engine sent; coming back to the main menu clears the lines
    /// kept.
    pub(super) fn take_log(&mut self) {
        let now = self.clock.now();
        self.sounds
            .extend(self.replica.take_sounds().into_iter().map(|s| Heard {
                text: s.text,
                direction: s.direction,
                distance: s.distance,
                at: now,
            }));
        self.sounds
            .drain(..self.sounds.len().saturating_sub(MAX_SOUNDS));
        for line in self.replica.take_lines() {
            let line = LogLine {
                color: line.color.map(|name| self.colors.resolve(&name).0),
                id: line.id,
                text: line.text,
                count: line.count,
                turn: line.turn,
                label: line.label,
            };
            // Ids only grow and are never reused. A line past the newest one is new; any other is a
            // repeat, or a line a fresh snapshot carries again, and replaces its own line.
            let id = line.id.parse::<u64>().ok();
            let newest = self.native.last().and_then(|l| l.id.parse::<u64>().ok());
            if id.is_some() && id > newest {
                self.native.push(line);
            } else if let Some(old) = self
                .native
                .iter_mut()
                .rev()
                .find(|l| !line.id.is_empty() && l.id == line.id)
            {
                *old = line;
            } else if id.is_none() || newest.is_none() {
                self.native.push(line);
            }
        }
        self.native
            .drain(..self.native.len().saturating_sub(MAX_LOG_LINES));
        let at_main_menu = interaction::titled(self.replica.stack());
        if at_main_menu && !std::mem::replace(&mut self.at_main_menu, true) {
            self.messages.clear();
            self.told.clear();
            self.sounds.clear();
            self.native.clear();
        }
        self.at_main_menu = at_main_menu;
    }
}
