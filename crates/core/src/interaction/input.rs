//! Keys and clicks on the top screen: one entry point each, deciding what to send, what to ask the player
//! and what only moves the client's own focus. A renderer passes key names and ids and draws what comes back.

use super::*;
use crate::protocol::FieldKind;

/// What a key or a click did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The invoke to send: the affordance id and its arguments.
    pub invoke: Option<(String, InvokeArgs)>,
    /// The engine's reason for a refusal, to show.
    pub denial: Option<Text>,
    /// Why it was refused, as data.
    pub reason: Option<Reason>,
    /// The row, cell or field the focus moved to, which the view scrolls into sight.
    pub revealed: Option<String>,
    /// The client used the key or click; the renderer must not.
    pub consumed: bool,
}

impl Outcome {
    /// What to tell the player of a refusal: the engine's `denial`, else the client's words for its own.
    pub fn note(&self) -> Option<String> {
        self.denial
            .as_ref()
            .map(Text::shown)
            .or_else(|| self.reason.and_then(Reason::words))
    }

    fn handled() -> Self {
        Self {
            consumed: true,
            ..Self::default()
        }
    }

    fn revealing(id: &str) -> Self {
        Self {
            revealed: Some(id.into()),
            ..Self::handled()
        }
    }

    fn refused(refusal: Refusal) -> Self {
        Self {
            denial: refusal.denial,
            reason: Some(refusal.reason),
            ..Self::handled()
        }
    }

    fn because(reason: Reason) -> Self {
        Self::refused(Refusal {
            reason,
            denial: None,
        })
    }

    fn off(availability: Availability) -> Self {
        Self::refused(Refusal {
            reason: availability.reason.unwrap_or(Reason::Disabled),
            denial: availability.denial,
        })
    }
}

impl Filter {
    /// Whether a field of this kind takes the character.
    pub(super) fn takes(self, c: char) -> bool {
        match self {
            Filter::Any => true,
            Filter::Integer => c.is_ascii_digit() || c == '-',
            Filter::Float => c.is_ascii_digit() || c == '-' || c == '.',
        }
    }
}

/// The affordance that sets `field_id` of the form: the one that acts on the form and takes a `text`, for
/// that field by name or, naming none, for the field its `row` is.
pub fn form_fill<'a>(screen: &'a Screen, form: &Form, field_id: &str) -> Option<&'a Affordance> {
    screen
        .affordances
        .iter()
        .filter(|a| form.head.affordances.contains(&a.id))
        .find(|a| {
            let shape = a.args.as_ref();
            shape
                .and_then(|shape| shape.text.as_ref())
                .is_some_and(|text| match &text.field_id {
                    Some(id) => id == field_id,
                    None => shape
                        .and_then(|shape| shape.row.as_ref())
                        .is_some_and(|of| of.component_id == form.head.id),
                })
        })
}

/// The cursor a new screen starts with when the engine names none: the first row a list marks selected.
pub(super) fn selected_focus(screen: Option<&Screen>) -> ScreenFocus {
    let selected = screen
        .into_iter()
        .flat_map(Screen::components)
        .find_map(|c| match c {
            Component::List(list) => list
                .rows
                .iter()
                .find(|row| row.selected && row.header != Some(true))
                .map(|row| (list.head.id.clone(), row.id.clone())),
            _ => None,
        });
    ScreenFocus {
        component_id: selected.as_ref().map(|(component, _)| component.clone()),
        row_id: selected.map(|(_, row)| row),
    }
}

/// The key as a typed character: a space or a single one.
fn typed(key: &str) -> Option<&str> {
    match key {
        "SPACE" => Some(" "),
        _ => (key.chars().count() == 1).then_some(key),
    }
}

impl Interaction {
    /// One key pressed on the top screen. In order: an open ask takes it; Return on a focus the player moved
    /// runs its row; an affordance the engine binds to it runs; Escape and Return run the `cancel` and `confirm` affordances, or a list's `activate` on its
    /// focused row; a row's hotkey chooses the row; the arrows, Home, End, Page keys and Tab move the focus
    /// (or the tab, or the grid cursor); else the `any_key` affordance runs. `page` is the rows a page key
    /// moves in the list with that id; `rows` gives the rows of a list that are loaded.
    pub fn press<'r>(
        &mut self,
        screen: &'r Screen,
        key: &str,
        page: &dyn Fn(&str) -> usize,
        rows: &dyn Fn(&'r List) -> Vec<&'r Row>,
    ) -> Outcome {
        if self.ask.is_some() {
            return self.press_ask(screen, key);
        }
        // A focus the player moved off the engine's cursor is what Return confirms; the engine's own Confirm
        // acts on its cursor, which the move did not touch.
        if key == "RETURN"
            && self.native != ScreenFocus::default()
            && self.focus != self.native
            && let Some(outcome) = self.activate_focused(screen)
        {
            return outcome;
        }
        let bound = screen
            .affordances
            .iter()
            .filter(|a| a.keys.iter().any(|k| k == key))
            .collect::<Vec<_>>();
        if let Some(first) = bound.first() {
            return match bound.iter().find(|a| a.enabled) {
                Some(a) => self.activate(screen, &a.id),
                None => Outcome::off(row_availability(first, None)),
            };
        }
        let outcome = match key {
            "ESC" => {
                with_role(screen, AffordanceRole::Cancel).map(|a| self.activate(screen, &a.id))
            }
            "RETURN" => with_role(screen, AffordanceRole::Confirm)
                .map(|a| self.activate(screen, &a.id))
                .or_else(|| self.activate_focused(screen)),
            _ => None,
        };
        outcome
            .or_else(|| self.press_hotkey(screen, key, rows))
            .or_else(|| self.press_move(screen, key, page, rows))
            .or_else(|| {
                with_role(screen, AffordanceRole::AnyKey).map(|a| self.activate(screen, &a.id))
            })
            .unwrap_or_default()
    }

    /// A click on a button, or on a row, tab or cell once the focus is there: the invoke is gated by the
    /// affordance and the rows it acts on, and either goes out now or starts asking for its arguments.
    pub fn activate(&mut self, screen: &Screen, affordance_id: &str) -> Outcome {
        let Some(affordance) = screen.affordances.iter().find(|a| a.id == affordance_id) else {
            return Outcome::because(Reason::Unknown);
        };
        let availability = row_availability(affordance, None);
        if !availability.enabled {
            return Outcome::off(availability);
        }
        let asked = self.open(affordance, Some(screen));
        let missing = asked
            .iter()
            .find(|kind| matches!(kind, ArgKind::Row | ArgKind::Rows));
        if let Some(&kind) = missing {
            self.ask = None;
            return Outcome::because(Reason::Missing(kind));
        }
        let (picked, _) = self.collect(affordance);
        if let Err(refusal) = gate(screen, affordance, &picked) {
            self.ask = None;
            return Outcome::refused(*refusal);
        }
        match self.ask.as_ref().is_some_and(|ask| ask.asking.is_some()) {
            true => Outcome::handled(),
            false => self.complete(screen, affordance),
        }
    }

    /// Sets `field_id` of a form to `value` with `affordance_id` (see `form_fill`).
    pub fn fill(
        &mut self,
        screen: &Screen,
        affordance_id: &str,
        form_id: &str,
        field_id: &str,
        value: &str,
    ) -> Outcome {
        let affordance = screen.affordances.iter().find(|a| a.id == affordance_id);
        let known = screen.components().into_iter().any(|c| match c {
            Component::Form(form) => {
                form.head.id == form_id && form.fields.iter().any(|f| f.id == field_id)
            }
            _ => false,
        });
        let (Some(affordance), true) = (affordance, known) else {
            return Outcome::because(Reason::Unknown);
        };
        let shape = affordance.args.as_ref();
        let args = InvokeArgs {
            row: shape
                .and_then(|shape| shape.row.as_ref())
                .map(|_| field_id.into()),
            text: Some(value.into()),
            ..InvokeArgs::default()
        };
        match gate(screen, affordance, &args) {
            Ok(()) => Outcome {
                invoke: Some((affordance.id.clone(), args)),
                ..Outcome::handled()
            },
            Err(refusal) => Outcome::refused(*refusal),
        }
    }

    /// Answers the pane argument being asked for, and sends when that was the last one.
    pub fn answer_pane(&mut self, screen: &Screen, pane_id: &str) -> Outcome {
        self.set_pane(pane_id);
        self.advance(screen, ArgKind::Pane)
    }

    /// Sends what the ask has gathered; refuses with the member still missing.
    pub fn confirm(&mut self, screen: &Screen) -> Outcome {
        let affordance = self.ask.as_ref().and_then(|ask| {
            screen
                .affordances
                .iter()
                .find(|a| a.id == ask.affordance_id)
        });
        match affordance {
            Some(affordance) => self.complete(screen, affordance),
            None => Outcome::default(),
        }
    }

    /// The invoke with every argument, or why not; the ask ends unless an argument is missing.
    fn complete(&mut self, screen: &Screen, affordance: &Affordance) -> Outcome {
        let args = match self.args(affordance) {
            Ok(args) => args,
            Err(missing) => return Outcome::because(Reason::Missing(missing[0])),
        };
        if let Err(refusal) = gate(screen, affordance, &args) {
            self.ask = None;
            return Outcome::refused(*refusal);
        }
        let args = self.finish(affordance).unwrap_or(args);
        Outcome {
            invoke: Some((affordance.id.clone(), args)),
            ..Outcome::handled()
        }
    }

    /// The member `kind` is answered: asks for the next one, or sends after the last.
    fn advance(&mut self, screen: &Screen, kind: ArgKind) -> Outcome {
        let Some(ask) = self.ask.as_mut() else {
            return Outcome::default();
        };
        let answered = match kind {
            ArgKind::Pos => ask.pos.is_some(),
            ArgKind::Pane => ask.pane.is_some(),
            _ => true,
        };
        if !answered {
            return Outcome::because(Reason::Missing(kind));
        }
        ask.todo.retain(|asked| *asked != kind);
        ask.asking = ask.todo.first().copied();
        match ask.asking {
            Some(_) => Outcome::handled(),
            None => self.confirm(screen),
        }
    }

    /// A key while an ask is open: it types, moves among the options, answers or ends the ask.
    fn press_ask(&mut self, screen: &Screen, key: &str) -> Outcome {
        let Some(ask) = self.ask.as_mut() else {
            return Outcome::default();
        };
        if key == "ESC" {
            self.ask = None;
            return Outcome::handled();
        }
        if std::mem::take(&mut ask.capture) {
            ask.text = Some(key.into());
            return self.advance(screen, ArgKind::Text);
        }
        let (asking, step) = (
            ask.asking,
            match key {
                "UP" => -1,
                "DOWN" => 1,
                _ => 0,
            },
        );
        match (key, asking) {
            ("RETURN", Some(ArgKind::Pane)) => {
                let pane = ask.panes.get(ask.pane_at).cloned();
                return match pane {
                    Some(pane) => self.answer_pane(screen, &pane),
                    None => Outcome::because(Reason::Missing(ArgKind::Pane)),
                };
            }
            ("RETURN", Some(kind)) => return self.advance(screen, kind),
            ("RETURN", None) => return self.confirm(screen),
            ("BACKSPACE", Some(ArgKind::Count)) => self.pop_count(),
            ("BACKSPACE", Some(ArgKind::Text)) => self.pop_text(),
            _ if step != 0 => match (asking, ask.select.as_mut()) {
                (Some(ArgKind::Text), Some(select)) => {
                    select.at = select
                        .at
                        .saturating_add_signed(step)
                        .min(select.options.len().saturating_sub(1));
                    ask.text = select.options.get(select.at).map(|o| o.id.clone());
                }
                (Some(ArgKind::Pane), _) => {
                    ask.pane_at = ask
                        .pane_at
                        .saturating_add_signed(step)
                        .min(ask.panes.len().saturating_sub(1));
                }
                _ => {}
            },
            _ => match (asking, typed(key)) {
                (Some(ArgKind::Count), Some(c)) => {
                    c.chars().for_each(|c| {
                        self.push_count(c);
                    });
                }
                (Some(ArgKind::Text), Some(c)) if ask.select.is_none() => self.push_text(c),
                _ => {}
            },
        }
        Outcome::handled()
    }

    /// Return on the focused row: runs its list's `activate`, the affordance the focused grid cell names, or
    /// starts editing the focused form field.
    fn activate_focused(&mut self, screen: &Screen) -> Option<Outcome> {
        let parts = screen.components();
        let focused = self.focus.component_id.as_deref()?;
        match parts.iter().find(|c| c.id() == focused)? {
            Component::List(list) => {
                let activate = list.activate.as_deref()?;
                self.focused_in(&list.head.id)?;
                Some(self.activate(screen, activate))
            }
            Component::Form(form) => {
                let field = self.focused_in(&form.head.id)?.to_string();
                let fill = form_fill(screen, form, &field)?.id.clone();
                Some(self.activate(screen, &fill))
            }
            Component::Grid(grid) => {
                let id = self.focused_in(&grid.head.id)?;
                let cell = grid.cells.iter().find(|c| c.id.as_deref() == Some(id))?;
                let affordance = cell.affordance.as_deref()?;
                Some(self.activate(screen, affordance))
            }
            _ => None,
        }
    }

    /// A key that is a row's hotkey chooses that row.
    fn press_hotkey<'r>(
        &mut self,
        screen: &'r Screen,
        key: &str,
        rows: &dyn Fn(&'r List) -> Vec<&'r Row>,
    ) -> Option<Outcome> {
        let focused = self.focus.component_id.clone();
        let mut lists = screen
            .components()
            .into_iter()
            .filter_map(|c| match c {
                Component::List(list) => Some(list),
                _ => None,
            })
            .collect::<Vec<_>>();
        lists.sort_by_key(|list| Some(&list.head.id) != focused.as_ref());
        let (list, row) = lists.into_iter().find_map(|list| {
            let row = rows(list).into_iter().find(|row| {
                row.hotkey.as_deref() == Some(key) && row.selectable && row.header != Some(true)
            })?;
            Some((list, row))
        })?;
        self.focus_row(&list.head.id, &row.id);
        let chosen = match (&list.activate, row.enabled) {
            (Some(activate), _) => self.activate(screen, activate),
            (None, true) => Outcome::handled(),
            (None, false) => Outcome::off(Availability::off(
                Reason::RowDisabled,
                Some(row.denial.clone()),
            )),
        };
        Some(Outcome {
            revealed: Some(row.id.clone()),
            consumed: true,
            ..chosen
        })
    }

    /// The arrows, Home, End and the Page keys move the focus in the list, form or grid that has it; Left
    /// and Right switch tab; Tab cycles the components that can hold the focus.
    fn press_move<'r>(
        &mut self,
        screen: &'r Screen,
        key: &str,
        page: &dyn Fn(&str) -> usize,
        rows: &dyn Fn(&'r List) -> Vec<&'r Row>,
    ) -> Option<Outcome> {
        let parts = screen.components();
        let holders = parts
            .iter()
            .copied()
            .filter(|c| !self.items(c, rows).is_empty())
            .collect::<Vec<_>>();
        let focused = self.focus.component_id.clone();
        let current = holders
            .iter()
            .copied()
            .find(|c| Some(c.id()) == focused.as_deref())
            .or_else(|| holders.first().copied());
        if matches!(key, "TAB" | "BACKTAB") {
            let n = holders.len();
            let at = holders
                .iter()
                .position(|c| Some(c.id()) == focused.as_deref());
            let next = match (at, key == "TAB") {
                (None, _) => 0,
                (Some(at), true) => (at + 1) % n,
                (Some(at), false) => (at + n - 1) % n,
            };
            if n == 0 || (n == 1 && at.is_some()) {
                return None;
            }
            let first = self.items(holders[next], rows).into_iter().next()?;
            self.focus_row(holders[next].id(), &first);
            return Some(Outcome::revealing(&first));
        }
        let arrow = matches!(key, "UP" | "DOWN" | "LEFT" | "RIGHT");
        let vertical = match key {
            "UP" => Some(-1),
            "DOWN" => Some(1),
            "PPAGE" | "NPAGE" => {
                let by = current.map_or(1, |c| page(c.id()).max(1)) as isize;
                Some(if key == "PPAGE" { -by } else { by })
            }
            _ => None,
        };
        let horizontal = match key {
            "LEFT" => Some(-1),
            "RIGHT" => Some(1),
            _ => None,
        };
        match (current, vertical, horizontal) {
            (Some(Component::Grid(grid)), ..) if arrow => {
                let (dx, dy) = (horizontal.unwrap_or(0), vertical.unwrap_or(0));
                self.move_cell(grid, dx as i64, dy as i64)
            }
            (Some(holder), Some(by), _) => {
                let items = self.items(holder, rows);
                self.step(holder.id(), &items, by)
            }
            (Some(holder), None, None) if matches!(key, "HOME" | "END") => {
                let items = self.items(holder, rows);
                let id = match key == "HOME" {
                    true => items.first(),
                    false => items.last(),
                }?;
                self.focus_row(holder.id(), id);
                Some(Outcome::revealing(id))
            }
            (_, None, Some(by)) => self.switch_tab(screen, &parts, by),
            _ => None,
        }
    }

    /// The ids that can take the focus in a component, in reading order.
    fn items<'r>(
        &self,
        component: &'r Component,
        rows: &dyn Fn(&'r List) -> Vec<&'r Row>,
    ) -> Vec<String> {
        match component {
            Component::List(list) => rows(list)
                .into_iter()
                .filter(|row| row.selectable && row.header != Some(true))
                .map(|row| row.id.clone())
                .collect(),
            Component::Form(form) => form.fields.iter().map(|f| f.id.clone()).collect(),
            Component::Grid(grid) => {
                let mut cells = grid
                    .cells
                    .iter()
                    .filter(|cell| cell.id.is_some())
                    .collect::<Vec<_>>();
                cells.sort_by_key(|cell| (cell.y, cell.x));
                cells
                    .into_iter()
                    .filter_map(|cell| cell.id.clone())
                    .collect()
            }
            _ => vec![],
        }
    }

    /// Moves the focus `by` among `items` and says which it landed on.
    fn step(&mut self, component_id: &str, items: &[String], by: isize) -> Option<Outcome> {
        let last = items.len().checked_sub(1)?;
        let at = self
            .focused_in(component_id)
            .and_then(|id| items.iter().position(|item| item == id));
        let next = match (at, by < 0) {
            (Some(at), _) => at.saturating_add_signed(by).min(last),
            (None, true) => last,
            (None, false) => 0,
        };
        self.focus_row(component_id, &items[next]);
        Some(Outcome::revealing(&items[next]))
    }

    /// Moves the grid cursor to the neighbouring cell; the first move from elsewhere lands on the cursor.
    fn move_cell(&mut self, grid: &Grid, dx: i64, dy: i64) -> Option<Outcome> {
        let cell_of = |id: &str| grid.cells.iter().find(|c| c.id.as_deref() == Some(id));
        let current = self
            .focused_in(&grid.head.id)
            .and_then(cell_of)
            .map(|cell| (cell.x as i64, cell.y as i64));
        let target = match current {
            Some((x, y)) => (x + dx, y + dy),
            None => grid.cursor.map(|c| (c.x as i64, c.y as i64)).or_else(|| {
                grid.cells
                    .iter()
                    .find(|c| c.id.is_some())
                    .map(|c| (c.x as i64, c.y as i64))
            })?,
        };
        let cell = grid
            .cells
            .iter()
            .find(|c| (c.x as i64, c.y as i64) == target && c.id.is_some());
        match cell.and_then(|c| c.id.as_deref()) {
            Some(id) => {
                self.focus_row(&grid.head.id, id);
                Some(Outcome::revealing(id))
            }
            None => Some(Outcome::handled()),
        }
    }

    /// Selects the neighbouring enabled tab with the tabs' `select` affordance.
    fn switch_tab(&mut self, screen: &Screen, parts: &[&Component], by: isize) -> Option<Outcome> {
        let (tabs, select) = parts.iter().find_map(|c| match c {
            Component::Tabs(tabs) => tabs.select.as_deref().map(|select| (tabs, select)),
            _ => None,
        })?;
        let at = tabs.tabs.iter().position(|tab| tab.selected)?;
        let next = match by < 0 {
            true => tabs.tabs[..at].iter().rposition(|tab| tab.enabled),
            false => tabs.tabs[at + 1..]
                .iter()
                .position(|tab| tab.enabled)
                .map(|n| at + 1 + n),
        };
        let Some(next) = next else {
            return Some(Outcome::handled());
        };
        self.focus_row(&tabs.head.id, &tabs.tabs[next].id);
        Some(self.activate(screen, select))
    }

    /// Reads from the screen what an affordance's arguments are chosen among.
    pub(super) fn seed(&self, ask: &mut Ask, screen: &Screen, shape: &ArgShape) {
        let parts = screen.components();
        if let Some(of) = &shape.pane {
            ask.panes = parts
                .iter()
                .find(|c| c.id() == of.component_id)
                .into_iter()
                .flat_map(|c| c.children())
                .filter(|c| matches!(c, Component::Panel(_)))
                .map(|c| c.id().to_string())
                .collect();
        }
        let Some(text) = &shape.text else {
            return;
        };
        let field_id = text.field_id.as_deref().or_else(|| {
            let of = shape.row.as_ref()?;
            self.focused_in(&of.component_id)
        });
        let field = parts
            .iter()
            .filter_map(|c| match c {
                Component::Form(form) => Some(form),
                _ => None,
            })
            .flat_map(|form| form.fields.iter())
            .find(|field| Some(field.id.as_str()) == field_id);
        let Some(field) = field else {
            return;
        };
        ask.text = Some(field.value.clone());
        ask.max_len = field.max_length.filter(|n| *n > 0).map(|n| n as usize);
        match field.kind {
            FieldKind::Integer => ask.filter = Filter::Integer,
            FieldKind::Float => ask.filter = Filter::Float,
            FieldKind::Key => {
                ask.capture = true;
                ask.text = None;
            }
            FieldKind::Select => {
                let options = field.options.clone().unwrap_or_default();
                let at = options
                    .iter()
                    .position(|o| o.id == field.value)
                    .unwrap_or(0);
                ask.text = options.get(at).map(|o| o.id.clone());
                ask.select = Some(Select { options, at });
            }
            FieldKind::Text | FieldKind::Bool => {}
        }
    }
}
