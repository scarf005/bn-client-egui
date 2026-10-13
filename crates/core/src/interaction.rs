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
    Affordance, AffordanceRole, ArgShape, At, Boundary, BoundaryExpect, Expect, InteractionExpect,
    InvokeArgs, List, Operation, PAGE_ROWS, Row, RowPick, RowsResult, Scope, Screen, ScreenExpect,
    ScreenFocus, Submit, Text, WorldPos,
};
use std::collections::HashMap;

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
    /// A command is running and the affordance is boundary-scoped, which the engine refuses meanwhile.
    Busy,
    Args(ArgError),
}

/// What a command is built against.
pub struct Basis<'a> {
    pub at: &'a At,
    pub boundary: &'a Boundary,
    /// A submitted command has not reached a terminal stage.
    pub busy: bool,
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
        Scope::Screen => Ok(Expect::Screen(ScreenExpect {
            screen_id: screen.screen_id.clone(),
        })),
        Scope::Boundary if basis.busy => Err(InvokeError::Busy),
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

/// What the player is typing for an affordance whose arguments are being collected.
#[derive(Debug, Default)]
struct Draft {
    affordance: Option<String>,
    count: Option<String>,
    text: Option<String>,
    pos: Option<WorldPos>,
    pane: Option<String>,
}

static NO_DRAFT: Draft = Draft {
    affordance: None,
    count: None,
    text: None,
    pos: None,
    pane: None,
};

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
    draft: Draft,
}

impl Interaction {
    /// Takes the stack the engine published. Another top screen drops everything local; the same one keeps
    /// it, except that a moved engine cursor takes the focus, and a draft whose affordance went ends.
    pub fn sync(&mut self, stack: &[Screen]) {
        let top = stack.last();
        let native = top.and_then(|s| s.focus.clone()).unwrap_or_default();
        if top.map(|s| s.screen_id.as_str()) != self.screen_id.as_deref() {
            *self = Self {
                screen_id: top.map(|s| s.screen_id.clone()),
                focus: native.clone(),
                native,
                ..Self::default()
            };
            return;
        }
        if native != self.native {
            self.focus = native.clone();
            self.native = native;
        }
        let gone = self.draft.affordance.as_ref().is_some_and(|id| {
            top.is_none_or(|screen| !screen.affordances.iter().any(|a| &a.id == id))
        });
        if gone {
            self.draft = Draft::default();
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
    /// for: the focus and the marked rows already answer `row` and `rows`.
    pub fn begin(&mut self, affordance: &Affordance) -> Vec<ArgKind> {
        self.draft = Draft {
            affordance: Some(affordance.id.clone()),
            ..Draft::default()
        };
        self.args(affordance).err().unwrap_or_default()
    }

    /// The draft of the affordance being collected, if any.
    fn draft_mut(&mut self) -> Option<&mut Draft> {
        self.draft.affordance.is_some().then_some(&mut self.draft)
    }

    /// Types a digit (or a leading minus) of the count.
    pub fn push_count(&mut self, c: char) -> bool {
        let Some(draft) = self.draft_mut() else {
            return false;
        };
        let typed = draft.count.get_or_insert_default();
        let fits = c.is_ascii_digit() || (c == '-' && typed.is_empty());
        if fits {
            typed.push(c);
        }
        fits
    }

    pub fn pop_count(&mut self) {
        if let Some(typed) = self.draft_mut().and_then(|d| d.count.as_mut()) {
            typed.pop();
        }
    }

    pub fn push_text(&mut self, text: &str) {
        if let Some(draft) = self.draft_mut() {
            draft.text.get_or_insert_default().push_str(text);
        }
    }

    pub fn pop_text(&mut self) {
        if let Some(typed) = self.draft_mut().and_then(|d| d.text.as_mut()) {
            typed.pop();
        }
    }

    pub fn set_text(&mut self, text: &str) {
        if let Some(draft) = self.draft_mut() {
            draft.text = Some(text.into());
        }
    }

    pub fn set_pos(&mut self, pos: WorldPos) {
        if let Some(draft) = self.draft_mut() {
            draft.pos = Some(pos);
        }
    }

    pub fn set_pane(&mut self, pane_id: &str) {
        if let Some(draft) = self.draft_mut() {
            draft.pane = Some(pane_id.into());
        }
    }

    /// Ends the collection without invoking.
    pub fn cancel(&mut self) {
        self.draft = Draft::default();
    }

    /// The arguments `affordance` asks for, from the focus, the marked rows and the draft; else the members
    /// still missing. A count is held to the shape's bounds, and defaults to the shape's default.
    pub fn args(&self, affordance: &Affordance) -> Result<InvokeArgs, Vec<ArgKind>> {
        let none = ArgShape::default();
        let shape = affordance.args.as_ref().unwrap_or(&none);
        let draft = match self.draft.affordance.as_ref() == Some(&affordance.id) {
            true => &self.draft,
            false => &NO_DRAFT,
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
        if let Some(bounds) = &shape.count {
            let typed = draft
                .count
                .as_deref()
                .and_then(|text| text.parse::<i64>().ok());
            match typed.or(bounds.default) {
                Some(n) => {
                    let (min, max) = (
                        bounds.min.unwrap_or(i64::MIN),
                        bounds.max.unwrap_or(i64::MAX),
                    );
                    args.count = Some(n.clamp(min, max.max(min)));
                }
                None => missing.push(ArgKind::Count),
            }
        }
        if shape.text.is_some() {
            args.text = draft.text.clone();
            if args.text.is_none() {
                missing.push(ArgKind::Text);
            }
        }
        if shape.pos.is_some() {
            args.pos = draft.pos.clone();
            if args.pos.is_none() {
                missing.push(ArgKind::Pos);
            }
        }
        if shape.pane.is_some() {
            args.pane = draft.pane.clone();
            if args.pane.is_none() {
                missing.push(ArgKind::Pane);
            }
        }
        match missing.is_empty() {
            true => Ok(args),
            false => Err(missing),
        }
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
        self.draft = Draft::default();
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
        module: "crates/egui/src/app.rs",
        screen: "map key queue (run_queued) and the main menu rules",
        slice: "S1",
    },
    Override {
        module: "crates/egui/src/app/aim.rs",
        screen: "ADVANCED_INVENTORY",
        slice: "S2",
    },
    Override {
        module: "crates/egui/src/app/character.rs",
        screen: "PLAYER_INFO, BIONICS, MUTATIONS",
        slice: "S9",
    },
    Override {
        module: "crates/egui/src/app/construction.rs",
        screen: "CONSTRUCTION",
        slice: "S5",
    },
    Override {
        module: "crates/egui/src/app/crafting.rs",
        screen: "CRAFTING",
        slice: "S5",
    },
    Override {
        module: "crates/egui/src/app/inventory.rs",
        screen: "INVENTORY",
        slice: "S3",
    },
    Override {
        module: "crates/egui/src/app/menu.rs",
        screen: "context dispatch and the 0.1 generic menu",
        slice: "S0",
    },
    Override {
        module: "crates/egui/src/app/palette.rs",
        screen: "action palette and its queued map keys",
        slice: "S1",
    },
    Override {
        module: "crates/egui/src/app/pickup.rs",
        screen: "PICKUP",
        slice: "S3",
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
        module: "crates/egui/src/app/terminal.rs",
        screen: "COMPUTER_TERMINAL and the extended description",
        slice: "S10",
    },
    Override {
        module: "crates/egui/src/app/trade.rs",
        screen: "NPC_TRADE",
        slice: "S4",
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
        module: "crates/tui/src/aim.rs",
        screen: "ADVANCED_INVENTORY",
        slice: "S2",
    },
    Override {
        module: "crates/tui/src/canvas.rs",
        screen: "engine-drawn windows (software mini-games)",
        slice: "S10",
    },
    Override {
        module: "crates/tui/src/crafting.rs",
        screen: "CRAFTING",
        slice: "S5",
    },
    Override {
        module: "crates/tui/src/lib.rs",
        screen: "context dispatch, the 0.1 generic menu and action rows",
        slice: "S0",
    },
    Override {
        module: "crates/tui/src/pickup.rs",
        screen: "PICKUP",
        slice: "S3",
    },
    Override {
        module: "crates/tui/src/settings.rs",
        screen: "option, key and rule screens",
        slice: "S0",
    },
    Override {
        module: "crates/tui/src/terminal.rs",
        screen: "COMPUTER_TERMINAL and the extended description",
        slice: "S10",
    },
    Override {
        module: "crates/tui/src/trade.rs",
        screen: "NPC_TRADE",
        slice: "S4",
    },
];
