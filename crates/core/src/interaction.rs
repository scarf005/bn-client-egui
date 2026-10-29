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

mod ask;
mod input;
mod pages;
mod refusal;
mod state;

pub use ask::*;
pub use input::{Outcome, form_fill};
pub use pages::*;
pub use refusal::*;
pub use state::*;

/// Whether `stack` is the title screen's: it holds screens and none of them is the running game's map.
pub fn titled(stack: &[Screen]) -> bool {
    !stack.is_empty() && stack.iter().all(|s| s.role != ScreenRole::Map)
}

/// Whether the running game is out of view: no map is in `stack`, or a workspace covers it.
pub fn game_hidden(stack: &[Screen]) -> bool {
    titled(stack) || stack.iter().any(|s| s.role == ScreenRole::Workspace)
}

/// The top screen when the engine describes it by components and it is not the map, which the map view draws.
pub fn described(stack: &[Screen]) -> Option<&Screen> {
    stack
        .last()
        .filter(|s| s.role != ScreenRole::Map && s.root.is_some())
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
        module: "crates/egui/src/app/menu.rs",
        screen: "context dispatch and the 0.1 generic menu",
        slice: "S0",
    },
    Override {
        module: "crates/egui/src/editor.rs",
        screen: "the key prompt",
        slice: "S0",
    },
    Override {
        module: "crates/tui/src/lib.rs",
        screen: "context dispatch, the 0.1 generic menu and action rows",
        slice: "S0",
    },
    Override {
        module: "crates/tui/src/settings.rs",
        screen: "the key prompt and text fields",
        slice: "S0",
    },
];
