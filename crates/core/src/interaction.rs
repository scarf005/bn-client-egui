//! The one interaction core of the clients (protocol 0.2, D6): the egui and terminal clients only draw
//! what this module decides.
//!
//! - `submit` is the single path from an operation to a command. An `invoke` takes the expectation its
//!   affordance's scope names: `screen` expects the top screen, `boundary` the boundary and revision.
//! - `Interaction` is the client-local state of the top screen: focus, scroll, marked rows and the arguments
//!   being collected. It decides nothing about the game; it only gathers what an affordance's `args` shape asks.
//! - `Pages` fetches the rows of a long list past its first page, by component id.
//! - `OVERRIDES` lists the bespoke screen modules that still draw a screen themselves.

use crate::protocol::{
    Affordance, AffordanceRole, ArgShape, At, Boundary, BoundaryExpect, Component, CountShape,
    Expect, FieldOption, Form, Grid, InteractionExpect, InvokeArgs, List, Operation, PAGE_ROWS,
    Row, RowPick, RowsResult, Scope, Screen, ScreenExpect, ScreenFocus, ScreenRole, Submit, Text,
    WorldPos,
};
use std::collections::HashMap;

mod input;
pub use input::{Outcome, form_fill};

/// Whether `stack` is the title screen's: it holds screens and none of them is the running game's map.
pub fn titled(stack: &[Screen]) -> bool {
    !stack.is_empty() && stack.iter().all(|s| s.role != ScreenRole::Map)
}

/// One member of an invoke's arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    Row,
    Rows,
    Count,
    Text,
    Pos,
    Pane,
}

/// Why arguments do not fit an affordance's shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArgError {
    /// The shape names the member and the invoke has none.
    Missing(ArgKind),
    /// The invoke carries a member the shape does not name.
    Unexpected(ArgKind),
    EmptyRows,
    /// A row carries a count where the shape takes none.
    RowCount,
    OutOfRange {
        min: Option<i64>,
        max: Option<i64>,
    },
}

/// Why an operation was not made into a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvokeError {
    /// The boundary has no screen to take the input.
    NoScreen,
    /// The top screen offers no affordance with this id.
    Unknown(String),
    /// The affordance is off; the engine's reason, when it gave one.
    Disabled(Option<Text>),
    /// A command is running and the affordance is boundary-scoped, which the engine refuses meanwhile, or
    /// a screen-scoped one finds the engine's queue full.
    Busy,
    Args(ArgError),
}

/// Why the client did not send an invoke, as data: the renderer maps it to its own words when the engine
/// gave no `denial`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    NoScreen,
    Unknown,
    /// The affordance is off.
    Disabled,
    /// The row (or cell, or tab) it acts on is off, or the row does not allow this affordance now.
    RowDisabled,
    /// The row does not list the affordance among those it allows.
    NotOnThisRow,
    /// An argument is still to be chosen or typed.
    Missing(ArgKind),
    Busy,
    /// The arguments do not fit the affordance's shape.
    BadArgs,
}

/// A refusal with the engine's own reason for it, when it gave one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub reason: Reason,
    /// The engine's `denial` text; never empty.
    pub denial: Option<Text>,
}

/// Whether an affordance may run now, and the engine's reason when not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Availability {
    pub enabled: bool,
    pub denial: Option<Text>,
    /// Why it may not, as data; none when it may.
    pub reason: Option<Reason>,
}

impl Availability {
    const ON: Self = Self {
        enabled: true,
        denial: None,
        reason: None,
    };

    fn off(reason: Reason, denial: Option<Text>) -> Self {
        Self {
            enabled: false,
            denial: denial.filter(|text| !text.shown().is_empty()),
            reason: Some(reason),
        }
    }
}

/// Whether `affordance` may run on `row`: it is on, the row is on, and the row allows it. Without a row,
/// only the affordance itself counts.
pub fn row_availability(affordance: &Affordance, row: Option<&Row>) -> Availability {
    if !affordance.enabled {
        return Availability::off(Reason::Disabled, affordance.denial.clone());
    }
    let Some(row) = row else {
        return Availability::ON;
    };
    if !row.enabled {
        return Availability::off(Reason::RowDisabled, Some(row.denial.clone()));
    }
    match row
        .affordances
        .as_ref()
        .map(|allowed| allowed.iter().find(|a| a.id == affordance.id))
    {
        Some(None) => Availability::off(Reason::NotOnThisRow, None),
        Some(Some(allowed)) if !allowed.enabled => {
            Availability::off(Reason::RowDisabled, allowed.denial.clone().map(Text::Plain))
        }
        _ => Availability::ON,
    }
}

/// Whether the affordance may run on the row, cell or tab `row_id` of the component.
fn available_on(
    screen: &Screen,
    affordance: &Affordance,
    component_id: &str,
    row_id: &str,
) -> Availability {
    match screen
        .root
        .as_ref()
        .and_then(|root| root.find(component_id))
    {
        Some(Component::List(list)) => {
            row_availability(affordance, list.rows.iter().find(|row| row.id == row_id))
        }
        Some(Component::Grid(grid)) => grid
            .cells
            .iter()
            .find(|cell| cell.id.as_deref() == Some(row_id))
            .filter(|cell| !cell.enabled)
            .map_or(Availability::ON, |cell| {
                Availability::off(Reason::RowDisabled, cell.denial.clone())
            }),
        Some(Component::Tabs(tabs)) => tabs
            .tabs
            .iter()
            .find(|tab| tab.id == row_id)
            .filter(|tab| !tab.enabled)
            .map_or(Availability::ON, |_| {
                Availability::off(Reason::RowDisabled, None)
            }),
        _ => Availability::ON,
    }
}

/// Whether `affordance` may be invoked with `args` on `screen`: it is on, and so is every row it acts on.
pub fn gate(screen: &Screen, affordance: &Affordance, args: &InvokeArgs) -> Result<(), Refusal> {
    let shape = affordance.args.as_ref();
    let one = shape
        .and_then(|shape| shape.row.as_ref())
        .zip(args.row.as_ref())
        .map(|(of, id)| (of.component_id.as_str(), id.as_str()));
    let many = shape
        .and_then(|shape| shape.rows.as_ref())
        .zip(args.rows.as_ref())
        .into_iter()
        .flat_map(|(of, picks)| {
            picks
                .iter()
                .map(move |pick| (of.component_id.as_str(), pick.id.as_str()))
        });
    std::iter::once(row_availability(affordance, None))
        .chain(
            one.into_iter().chain(many).map(|(component_id, row_id)| {
                available_on(screen, affordance, component_id, row_id)
            }),
        )
        .find(|check| !check.enabled)
        .map_or(Ok(()), |check| {
            Err(Refusal {
                reason: check.reason.unwrap_or(Reason::Disabled),
                denial: check.denial,
            })
        })
}

/// What a command is built against.
pub struct Basis<'a> {
    pub at: &'a At,
    pub boundary: &'a Boundary,
    /// The commands submitted that have not reached a terminal stage.
    pub outstanding: usize,
    /// How many commands the engine holds behind the running one (`limits.screen_queue`).
    pub queue: usize,
}

/// Whether `args` are what `shape` asks for: every member it names, no other, within its bounds.
pub fn check_args(shape: &ArgShape, args: &InvokeArgs) -> Result<(), ArgError> {
    let members = [
        (ArgKind::Row, shape.row.is_some(), args.row.is_some()),
        (ArgKind::Rows, shape.rows.is_some(), args.rows.is_some()),
        (ArgKind::Count, shape.count.is_some(), args.count.is_some()),
        (ArgKind::Text, shape.text.is_some(), args.text.is_some()),
        (ArgKind::Pos, shape.pos.is_some(), args.pos.is_some()),
        (ArgKind::Pane, shape.pane.is_some(), args.pane.is_some()),
    ];
    members
        .iter()
        .try_for_each(|&(kind, wanted, given)| match (wanted, given) {
            (true, false) => Err(ArgError::Missing(kind)),
            (false, true) => Err(ArgError::Unexpected(kind)),
            _ => Ok(()),
        })?;
    if let (Some(rows), Some(shape)) = (&args.rows, &shape.rows) {
        if rows.is_empty() {
            return Err(ArgError::EmptyRows);
        }
        if shape.counts != Some(true) && rows.iter().any(|row| row.count.is_some()) {
            return Err(ArgError::RowCount);
        }
    }
    if let (Some(count), Some(bounds)) = (args.count, &shape.count) {
        let inside =
            bounds.min.is_none_or(|min| count >= min) && bounds.max.is_none_or(|max| count <= max);
        if !inside {
            return Err(ArgError::OutOfRange {
                min: bounds.min,
                max: bounds.max,
            });
        }
    }
    Ok(())
}

/// The command for `operation`. An `invoke` is checked against the top screen's affordance and takes the
/// expectation of its scope; every other operation takes the 0.1 expectation.
pub fn submit(basis: &Basis, operation: Operation) -> Result<Submit, InvokeError> {
    let expect = match &operation {
        Operation::Invoke {
            affordance_id,
            args,
        } => invoke_expect(basis, affordance_id, args.as_ref())?,
        _ => Expect::Interaction(InteractionExpect {
            revision: basis.at.revision.clone(),
            boundary_id: basis.boundary.boundary_id.clone(),
            schema_id: basis
                .boundary
                .interaction
                .as_ref()
                .map(|i| i.schema_id.clone()),
        }),
    };
    Ok(Submit {
        epoch: basis.at.epoch.clone(),
        expect,
        operation,
    })
}

fn invoke_expect(
    basis: &Basis,
    affordance_id: &str,
    args: Option<&InvokeArgs>,
) -> Result<Expect, InvokeError> {
    let screen = basis.boundary.stack.last().ok_or(InvokeError::NoScreen)?;
    let affordance = screen
        .affordances
        .iter()
        .find(|a| a.id == affordance_id)
        .ok_or_else(|| InvokeError::Unknown(affordance_id.into()))?;
    if !affordance.enabled {
        return Err(InvokeError::Disabled(affordance.denial.clone()));
    }
    let (none, no_args) = (ArgShape::default(), InvokeArgs::default());
    check_args(
        affordance.args.as_ref().unwrap_or(&none),
        args.unwrap_or(&no_args),
    )
    .map_err(InvokeError::Args)?;
    match affordance.expect {
        Scope::Screen if basis.outstanding > basis.queue => Err(InvokeError::Busy),
        Scope::Screen => Ok(Expect::Screen(ScreenExpect {
            screen_id: screen.screen_id.clone(),
        })),
        Scope::Boundary if basis.outstanding > 0 => Err(InvokeError::Busy),
        Scope::Boundary => Ok(Expect::Boundary(BoundaryExpect {
            boundary_id: basis.boundary.boundary_id.clone(),
            revision: basis.at.revision.clone(),
        })),
    }
}

/// The affordance a key runs: the enabled one that binds it; none when only disabled ones do; else the
/// `any_key` affordance, which runs on a key nothing binds.
pub fn affordance_for_key<'a>(screen: &'a Screen, key: &str) -> Option<&'a Affordance> {
    let bound = screen
        .affordances
        .iter()
        .filter(|a| a.keys.iter().any(|k| k == key))
        .collect::<Vec<_>>();
    match bound.is_empty() {
        false => bound.into_iter().find(|a| a.enabled),
        true => with_role(screen, AffordanceRole::AnyKey),
    }
}

/// Whether the screen binds the key, enabled or not. A navigation key the client handles itself (moving the
/// focus, scrolling) goes to the engine when this is true, and to the client only when it is false.
pub fn binds_key(screen: &Screen, key: &str) -> bool {
    screen
        .affordances
        .iter()
        .any(|a| a.keys.iter().any(|k| k == key))
}

/// The enabled affordance with the role, such as the one Escape takes.
pub fn with_role(screen: &Screen, role: AffordanceRole) -> Option<&Affordance> {
    screen
        .affordances
        .iter()
        .find(|a| a.enabled && a.role == Some(role))
}

/// How much of a list the view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub total: usize,
    pub visible: usize,
}

/// A text field's input: what it takes, from the form field it edits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Filter {
    #[default]
    Any,
    Integer,
    Float,
}

/// The options of a `select` field and the one highlighted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Select {
    pub options: Vec<FieldOption>,
    pub at: usize,
}

/// What the player is being asked for: the arguments of one affordance being collected. A renderer only
/// shows it; every change goes through `Interaction`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub affordance_id: String,
    /// What the count would be sent as: the typed one, else the shape's default, held to its bounds.
    pub count: Option<i64>,
    /// The text typed, or a field's current value to start from.
    pub text: Option<String>,
    /// The panels that can answer a `pane` argument.
    pub panes: Vec<String>,
    /// The panel highlighted among `panes`.
    pub pane_at: usize,
    /// The options when the text is a `select` field's choice.
    pub select: Option<Select>,
    /// The next key pressed is the answer, as for a `key` field.
    pub capture: bool,
    /// The square or tile chosen, for a `pos` argument.
    pub pos: Option<WorldPos>,
    pub pane: Option<String>,
    /// The member being asked for now; none when every one is answered.
    pub asking: Option<ArgKind>,
    /// The count digits typed; none until typed.
    digits: Option<String>,
    /// The count is the shape's default, which the first digit typed replaces.
    fresh: bool,
    bounds: Option<CountShape>,
    /// The members still to answer, in the order they are asked.
    todo: Vec<ArgKind>,
    filter: Filter,
    max_len: Option<usize>,
}

impl Ask {
    fn new(affordance: &Affordance) -> Self {
        let bounds = affordance
            .args
            .as_ref()
            .and_then(|shape| shape.count.clone());
        let default = bounds.as_ref().and_then(|bounds| bounds.default);
        let mut ask = Self {
            affordance_id: affordance.id.clone(),
            count: None,
            text: None,
            panes: vec![],
            pane_at: 0,
            select: None,
            capture: false,
            pos: None,
            pane: None,
            asking: None,
            digits: default.map(|n| n.to_string()),
            fresh: default.is_some(),
            bounds,
            todo: vec![],
            filter: Filter::Any,
            max_len: None,
        };
        ask.recount();
        ask
    }

    /// The count as it would be sent: typed digits, else the default, held to the bounds.
    fn recount(&mut self) {
        let Some(bounds) = &self.bounds else {
            return;
        };
        let typed = self
            .digits
            .as_deref()
            .and_then(|digits| digits.parse::<i64>().ok());
        self.count = typed.or(bounds.default).map(|n| {
            let (min, max) = (
                bounds.min.unwrap_or(i64::MIN),
                bounds.max.unwrap_or(i64::MAX),
            );
            n.clamp(min, max.max(min))
        });
    }

    fn set_todo(&mut self, todo: Vec<ArgKind>) {
        self.asking = todo.first().copied();
        self.todo = todo;
    }
}

/// The client-local state of the top screen. It starts afresh when another screen opens, follows the
/// engine's cursor when that moves, and gathers the arguments of an affordance from the focus, the marked
/// rows and what the player types.
#[derive(Debug, Default)]
pub struct Interaction {
    screen_id: Option<String>,
    /// The cursor the engine last reported, to tell when it moves.
    native: ScreenFocus,
    focus: ScreenFocus,
    scroll: HashMap<String, usize>,
    picks: HashMap<String, Vec<RowPick>>,
    ask: Option<Ask>,
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
        let known = |focus: &ScreenFocus| Some((focus.component_id.clone()?, focus.row_id.clone()?));
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
    fn focused_in(&self, component_id: &str) -> Option<&str> {
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

    /// Starts collecting the arguments of `affordance` and returns what the client must still ask the player
    /// for: the focus and the marked rows already answer `row` and `rows`. A count or a text is always asked,
    /// starting from the shape's default; a position or a pane until it is answered.
    pub fn begin(&mut self, affordance: &Affordance) -> Vec<ArgKind> {
        self.open(affordance, None)
    }

    /// `begin`, which also reads from `screen` what the arguments are chosen among: the field a text edits and
    /// the panels a pane is one of.
    fn open(&mut self, affordance: &Affordance, screen: Option<&Screen>) -> Vec<ArgKind> {
        let shape = affordance.args.clone().unwrap_or_default();
        let mut ask = Ask::new(affordance);
        let todo = [
            (shape.count.is_some(), ArgKind::Count),
            (shape.text.is_some(), ArgKind::Text),
            (shape.pos.is_some(), ArgKind::Pos),
            (shape.pane.is_some(), ArgKind::Pane),
        ]
        .into_iter()
        .filter_map(|(asked, kind)| asked.then_some(kind))
        .collect::<Vec<_>>();
        ask.text = shape.text.as_ref().map(|_| String::new());
        if let Some(screen) = screen {
            self.seed(&mut ask, screen, &shape);
        }
        let rows = self
            .collect(affordance)
            .1
            .into_iter()
            .filter(|kind| matches!(kind, ArgKind::Row | ArgKind::Rows));
        let asked = rows.chain(todo.iter().copied()).collect();
        ask.set_todo(todo);
        self.ask = Some(ask);
        asked
    }

    /// Types a digit (or a leading minus) of the count.
    pub fn push_count(&mut self, c: char) -> bool {
        let Some(ask) = self.ask.as_mut() else {
            return false;
        };
        if std::mem::take(&mut ask.fresh) {
            ask.digits = None;
        }
        let typed = ask.digits.get_or_insert_default();
        let fits = c.is_ascii_digit() || (c == '-' && typed.is_empty());
        if fits {
            typed.push(c);
        }
        ask.recount();
        fits
    }

    pub fn pop_count(&mut self) {
        if let Some(ask) = self.ask.as_mut() {
            ask.fresh = false;
            ask.digits.as_mut().map(String::pop);
            ask.recount();
        }
    }

    /// Types text, keeping the characters the field takes and as many as it holds.
    pub fn push_text(&mut self, text: &str) {
        let Some(ask) = self.ask.as_mut() else {
            return;
        };
        let (filter, max_len) = (ask.filter, ask.max_len);
        let typed = ask.text.get_or_insert_default();
        for c in text.chars().filter(|&c| filter.takes(c)) {
            if max_len.is_none_or(|max| typed.chars().count() < max) {
                typed.push(c);
            }
        }
    }

    pub fn pop_text(&mut self) {
        if let Some(typed) = self.ask.as_mut().and_then(|a| a.text.as_mut()) {
            typed.pop();
        }
    }

    /// Replaces the text typed.
    pub fn answer_text(&mut self, text: &str) {
        if let Some(ask) = self.ask.as_mut() {
            ask.text = Some(text.into());
        }
    }

    pub fn set_pos(&mut self, pos: WorldPos) {
        if let Some(ask) = self.ask.as_mut() {
            ask.pos = Some(pos);
        }
    }

    pub fn set_pane(&mut self, pane_id: &str) {
        if let Some(ask) = self.ask.as_mut() {
            ask.pane = Some(pane_id.into());
        }
    }

    /// What the player is being asked for, if an affordance is collecting its arguments.
    pub fn ask(&self) -> Option<&Ask> {
        self.ask.as_ref()
    }

    /// Ends the collection without invoking.
    pub fn cancel(&mut self) {
        self.ask = None;
    }

    /// The arguments `affordance` asks for, from the focus, the marked rows and the ask; else the members
    /// still missing. A count is held to the shape's bounds, and defaults to the shape's default.
    pub fn args(&self, affordance: &Affordance) -> Result<InvokeArgs, Vec<ArgKind>> {
        match self.collect(affordance) {
            (args, missing) if missing.is_empty() => Ok(args),
            (_, missing) => Err(missing),
        }
    }

    /// The arguments gathered so far and the members still missing.
    fn collect(&self, affordance: &Affordance) -> (InvokeArgs, Vec<ArgKind>) {
        let none = ArgShape::default();
        let shape = affordance.args.as_ref().unwrap_or(&none);
        let unasked;
        let ask = match self
            .ask
            .as_ref()
            .filter(|ask| ask.affordance_id == affordance.id)
        {
            Some(ask) => ask,
            None => {
                unasked = Ask::new(affordance);
                &unasked
            }
        };
        let mut missing = vec![];
        let mut args = InvokeArgs::default();
        if let Some(of) = &shape.row {
            args.row = self.focused_in(&of.component_id).map(String::from);
            if args.row.is_none() {
                missing.push(ArgKind::Row);
            }
        }
        if let Some(of) = &shape.rows {
            let counted = of.counts == Some(true);
            let picks = self.picks(&of.component_id);
            let rows = match (picks.is_empty(), self.focused_in(&of.component_id)) {
                (false, _) => picks
                    .iter()
                    .map(|pick| RowPick {
                        id: pick.id.clone(),
                        count: pick.count.filter(|_| counted),
                    })
                    .collect(),
                (true, Some(id)) => vec![RowPick {
                    id: id.into(),
                    count: None,
                }],
                (true, None) => vec![],
            };
            match rows.is_empty() {
                true => missing.push(ArgKind::Rows),
                false => args.rows = Some(rows),
            }
        }
        if shape.count.is_some() {
            args.count = ask.count;
            if args.count.is_none() {
                missing.push(ArgKind::Count);
            }
        }
        if shape.text.is_some() {
            args.text = ask.text.clone();
            if args.text.is_none() {
                missing.push(ArgKind::Text);
            }
        }
        if shape.pos.is_some() {
            args.pos = ask.pos.clone();
            if args.pos.is_none() {
                missing.push(ArgKind::Pos);
            }
        }
        if shape.pane.is_some() {
            args.pane = ask.pane.clone();
            if args.pane.is_none() {
                missing.push(ArgKind::Pane);
            }
        }
        (args, missing)
    }

    /// The arguments to invoke `affordance` with, ending the collection and clearing the rows it used.
    pub fn finish(&mut self, affordance: &Affordance) -> Result<InvokeArgs, Vec<ArgKind>> {
        let args = self.args(affordance)?;
        if let Some(of) = affordance
            .args
            .as_ref()
            .and_then(|shape| shape.rows.as_ref())
        {
            self.clear_picks(&of.component_id);
        }
        self.ask = None;
        Ok(args)
    }
}

/// A page of a list to ask `bn.interaction.choices` for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageRequest {
    pub component_id: String,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Default)]
struct Fetched {
    rows: Vec<Row>,
    waiting: bool,
    /// Nothing more is asked for until the boundary changes: a page was refused or came back empty.
    closed: bool,
}

/// The rows of long lists past their first page, fetched by component id for one boundary.
#[derive(Debug, Default)]
pub struct Pages {
    boundary_id: String,
    lists: HashMap<String, Fetched>,
}

impl Pages {
    /// The next page of `list` to ask for, once: none when every row is held or the page is on its way.
    pub fn next(&mut self, boundary_id: &str, list: &List) -> Option<PageRequest> {
        if self.boundary_id != boundary_id {
            *self = Self {
                boundary_id: boundary_id.into(),
                ..Self::default()
            };
        }
        let held = self.lists.entry(list.head.id.clone()).or_default();
        let loaded = list.offset.unwrap_or(0) + (list.rows.len() + held.rows.len()) as u64;
        if held.waiting || held.closed || loaded >= list.total {
            return None;
        }
        held.waiting = true;
        Some(PageRequest {
            component_id: list.head.id.clone(),
            offset: loaded,
            limit: PAGE_ROWS,
        })
    }

    /// Adds an answer to the rows held; an answer for another boundary or a list not asked for is dropped.
    pub fn accept(&mut self, page: RowsResult) {
        let held = self.lists.get_mut(&page.component_id);
        if let (true, Some(held)) = (page.boundary_id == self.boundary_id, held) {
            held.waiting = false;
            held.closed = page.rows.is_empty();
            held.rows.extend(page.rows);
        }
    }

    /// A page was refused; the list is not asked again for this boundary.
    pub fn failed(&mut self, component_id: &str) {
        if let Some(held) = self.lists.get_mut(component_id) {
            held.waiting = false;
            held.closed = true;
        }
    }

    /// The rows of the list that are known: its first page, then the pages fetched.
    pub fn rows<'a>(&'a self, boundary_id: &str, list: &'a List) -> Vec<&'a Row> {
        let held = self
            .lists
            .get(&list.head.id)
            .filter(|_| self.boundary_id == boundary_id);
        list.rows
            .iter()
            .chain(held.into_iter().flat_map(|held| held.rows.iter()))
            .collect()
    }
}

/// A bespoke screen module that still draws a screen itself instead of the generic components.
#[derive(Debug)]
pub struct Override {
    /// The path from the repository root.
    pub module: &'static str,
    /// What the module draws or decides, by the 0.1 context or the role it serves.
    pub screen: &'static str,
    /// The slice of the protocol plan that replaces it (S0 is the generic path).
    pub slice: &'static str,
}

/// The deletion backlog: a slice deletes its modules, their dispatch and their lines here. Empty when the
/// 0.1 `interaction` is gone.
pub const OVERRIDES: &[Override] = &[
    Override {
        module: "crates/egui/src/app/character.rs",
        screen: "PLAYER_INFO, BIONICS, MUTATIONS",
        slice: "S9",
    },
    Override {
        module: "crates/egui/src/app/menu.rs",
        screen: "context dispatch and the 0.1 generic menu",
        slice: "S0",
    },
    Override {
        module: "crates/egui/src/app/spells.rs",
        screen: "SPELL_SELECT",
        slice: "S9",
    },
    Override {
        module: "crates/egui/src/app/target.rs",
        screen: "target panes (map editor tile and actions)",
        slice: "S6",
    },
    Override {
        module: "crates/egui/src/app/vehicle.rs",
        screen: "VEH_INTERACT",
        slice: "S8",
    },
    Override {
        module: "crates/egui/src/editor.rs",
        screen: "option editors and the key prompt",
        slice: "S0",
    },
    Override {
        module: "crates/tui/src/lib.rs",
        screen: "context dispatch, the 0.1 generic menu and action rows",
        slice: "S0",
    },
    Override {
        module: "crates/tui/src/settings.rs",
        screen: "option, key and rule screens",
        slice: "S0",
    },
];
