//! The client-local state of the top screen: the focus, the scroll of each list and the marked rows.

use super::*;

/// How much of a list the view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub total: usize,
    pub visible: usize,
}

/// The client-local state of the top screen. It starts afresh when another screen opens, follows the
/// engine's cursor when that moves, and gathers the arguments of an affordance from the focus, the marked
/// rows and what the player types.
#[derive(Debug, Default)]
pub struct Interaction {
    screen_id: Option<String>,
    /// The cursor the engine last reported, to tell when it moves.
    pub(super) native: ScreenFocus,
    pub(super) focus: ScreenFocus,
    scroll: HashMap<String, usize>,
    picks: HashMap<String, Vec<RowPick>>,
    pub(super) ask: Option<Ask>,
    /// The row the engine's cursor is known to be on, by list: the focus when the engine put it there or a
    /// `select` invoke moved it.
    followed: Option<(String, String)>,
    /// The `select` invoke that the focus moving calls for, until the host takes it.
    select: Option<(String, InvokeArgs)>,
}

impl Interaction {
    /// Takes the stack the engine published. Another top screen drops everything local; the same one keeps
    /// it, except that a moved engine cursor takes the focus, and a draft whose affordance went ends. A focus
    /// that moved to a row of a list that a `select`-role affordance names is left for `take_select`.
    pub fn sync(&mut self, stack: &[Screen]) {
        self.track(stack);
        self.select = stack.last().and_then(|top| self.select_for(top));
    }

    /// The invoke that moves the engine's cursor to the focused row, once, when the focus moved in a list
    /// that a `select`-role affordance of the screen names.
    pub fn take_select(&mut self) -> Option<(String, InvokeArgs)> {
        self.select.take()
    }

    fn select_for(&mut self, top: &Screen) -> Option<(String, InvokeArgs)> {
        let at = (self.focus.component_id.clone()?, self.focus.row_id.clone()?);
        if self.followed.as_ref() == Some(&at) {
            return None;
        }
        let select = top.affordances.iter().find(|a| {
            a.enabled
                && a.role == Some(AffordanceRole::Select)
                && a.args
                    .as_ref()
                    .and_then(|shape| shape.row.as_ref())
                    .is_some_and(|row| row.component_id == at.0)
        })?;
        let call = (
            select.id.clone(),
            InvokeArgs {
                row: Some(at.1.clone()),
                ..InvokeArgs::default()
            },
        );
        self.followed = Some(at);
        Some(call)
    }

    fn track(&mut self, stack: &[Screen]) {
        let top = stack.last();
        let native = top.and_then(|s| s.focus.clone()).unwrap_or_default();
        let known =
            |focus: &ScreenFocus| Some((focus.component_id.clone()?, focus.row_id.clone()?));
        if top.map(|s| s.screen_id.as_str()) != self.screen_id.as_deref() {
            let focus = match native == ScreenFocus::default() {
                true => input::selected_focus(top),
                false => native.clone(),
            };
            *self = Self {
                screen_id: top.map(|s| s.screen_id.clone()),
                followed: known(&focus),
                focus,
                native,
                ..Self::default()
            };
            return;
        }
        if native != self.native {
            self.followed = known(&native);
            self.focus = native.clone();
            self.native = native;
        }
        let gone = self.ask.as_ref().is_some_and(|ask| {
            top.is_none_or(|screen| screen.affordances.iter().all(|a| a.id != ask.affordance_id))
        });
        if gone {
            self.ask = None;
        }
    }

    pub fn focus(&self) -> &ScreenFocus {
        &self.focus
    }

    /// The focused row, when the focus is in that component.
    pub(super) fn focused_in(&self, component_id: &str) -> Option<&str> {
        self.focus
            .row_id
            .as_deref()
            .filter(|_| self.focus.component_id.as_deref() == Some(component_id))
    }

    pub fn focus_row(&mut self, component_id: &str, row_id: &str) {
        self.focus = ScreenFocus {
            component_id: Some(component_id.into()),
            row_id: Some(row_id.into()),
        };
    }

    /// Moves the focus `by` rows among the ones to choose (headings and rows that cannot be selected are
    /// skipped), stopping at the ends. A focus elsewhere starts at the first row, or at the last going up.
    pub fn move_focus(&mut self, component_id: &str, rows: &[&Row], by: isize) -> bool {
        let choosable = rows
            .iter()
            .filter(|row| row.selectable && row.header != Some(true))
            .collect::<Vec<_>>();
        let Some(last) = choosable.len().checked_sub(1) else {
            return false;
        };
        let at = self
            .focused_in(component_id)
            .and_then(|id| choosable.iter().position(|row| row.id == id));
        let next = match (at, by < 0) {
            (Some(at), _) => at.saturating_add_signed(by).min(last),
            (None, true) => last,
            (None, false) => 0,
        };
        self.focus_row(component_id, &choosable[next].id);
        true
    }

    /// The first row of the list the view shows, which the client scrolls on its own.
    pub fn scroll(&self, component_id: &str) -> usize {
        self.scroll.get(component_id).copied().unwrap_or(0)
    }

    pub fn scroll_by(&mut self, component_id: &str, by: isize, window: Window) {
        self.scroll_to(
            component_id,
            self.scroll(component_id).saturating_add_signed(by),
            window,
        );
    }

    /// Scrolls the least that shows the row at `index`.
    pub fn reveal(&mut self, component_id: &str, index: usize, window: Window) {
        let visible = window.visible.max(1);
        let top = self.scroll(component_id);
        let top = match index {
            i if i < top => i,
            i if i >= top + visible => i + 1 - visible,
            _ => top,
        };
        self.scroll_to(component_id, top, window);
    }

    /// Never past the point where the last row is the last one shown.
    fn scroll_to(&mut self, component_id: &str, top: usize, window: Window) {
        let last = window.total.saturating_sub(window.visible);
        self.scroll.insert(component_id.into(), top.min(last));
    }

    /// The rows marked for an affordance that takes several.
    pub fn picks(&self, component_id: &str) -> &[RowPick] {
        self.picks.get(component_id).map_or(&[], Vec::as_slice)
    }

    pub fn toggle_pick(&mut self, component_id: &str, row_id: &str) {
        let picks = self.picks.entry(component_id.into()).or_default();
        match picks.iter().position(|pick| pick.id == row_id) {
            Some(at) => {
                picks.remove(at);
            }
            None => picks.push(RowPick {
                id: row_id.into(),
                count: None,
            }),
        }
    }

    /// Sets how much of a marked row to take.
    pub fn set_pick_count(&mut self, component_id: &str, row_id: &str, count: Option<u64>) {
        let marked = self
            .picks
            .get_mut(component_id)
            .and_then(|picks| picks.iter_mut().find(|pick| pick.id == row_id));
        if let Some(pick) = marked {
            pick.count = count;
        }
    }

    pub fn clear_picks(&mut self, component_id: &str) {
        self.picks.remove(component_id);
    }
}
