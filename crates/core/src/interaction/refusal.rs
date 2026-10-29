//! Why the client does not send an invoke: arguments that do not fit an affordance's shape, an affordance or a
//! row that is off, and the words for each.

use super::*;

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

impl InvokeError {
    /// What to tell the player: the engine's reason for what it turned off, else the client's words.
    pub fn note(&self) -> Option<String> {
        match self {
            InvokeError::Disabled(denial) => denial.as_ref().map(Text::shown),
            InvokeError::NoScreen => Reason::NoScreen.words(),
            InvokeError::Unknown(_) => Reason::Unknown.words(),
            InvokeError::Busy => Reason::Busy.words(),
            InvokeError::Args(_) => Reason::BadArgs.words(),
        }
    }
}

/// Why the client did not send an invoke, as data.
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
    /// A whole-number field's text is no whole number within its bounds.
    Range {
        min: Option<i64>,
        max: Option<i64>,
    },
}

impl Reason {
    /// The client's words for a refusal of its own; none for what the engine turned off, whose `denial` says why.
    pub fn words(self) -> Option<String> {
        use crate::localise::{Value, tr, tr_format};
        Some(match self {
            Reason::Disabled | Reason::RowDisabled => return None,
            Reason::NoScreen | Reason::Unknown | Reason::BadArgs => tr("Not available."),
            Reason::NotOnThisRow => tr("Not for this row."),
            Reason::Missing(ArgKind::Row | ArgKind::Rows) => tr("Choose a row first."),
            Reason::Missing(ArgKind::Count) => tr("Enter a count first."),
            Reason::Missing(ArgKind::Text) => tr("Enter text first."),
            Reason::Missing(ArgKind::Pos) => tr("Choose a square first."),
            Reason::Missing(ArgKind::Pane) => tr("Choose a pane first."),
            Reason::Busy => tr("Busy."),
            Reason::Range { min, max } => match (min, max) {
                (Some(min), Some(max)) => tr_format(
                    "Enter a whole number from %d to %d.",
                    &[Value::Int(min), Value::Int(max)],
                ),
                (Some(min), None) => {
                    tr_format("Enter a whole number of at least %d.", &[Value::Int(min)])
                }
                (None, Some(max)) => {
                    tr_format("Enter a whole number of at most %d.", &[Value::Int(max)])
                }
                (None, None) => tr("Enter a whole number."),
            },
        })
    }
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

    pub(super) fn off(reason: Reason, denial: Option<Text>) -> Self {
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
            .map_or(Availability::ON, |tab| {
                Availability::off(Reason::RowDisabled, tab.denial.clone())
            }),
        _ => Availability::ON,
    }
}

/// Whether `affordance` may be invoked with `args` on `screen`: it is on, and so is every row it acts on.
pub fn gate(
    screen: &Screen,
    affordance: &Affordance,
    args: &InvokeArgs,
) -> Result<(), Box<Refusal>> {
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
            Err(Box::new(Refusal {
                reason: check.reason.unwrap_or(Reason::Disabled),
                denial: check.denial,
            }))
        })
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
