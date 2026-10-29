//! Which part of a screen runs or shows each of its affordances: a widget of its own, a row, a drag, a
//! component's button, or the footer.

use super::*;

/// Whether the rows of one of the screen's lists run the affordance, on a double click or Enter.
fn run_by_rows(screen: &Screen, affordance: &Affordance) -> bool {
    let id = Some(affordance.id.as_str());
    let runs = |c: &&Component| matches!(c, Component::List(l) if l.activate.as_deref() == id);
    screen.components().iter().any(runs)
}

/// The ids of the affordances some component names.
pub(super) fn claimed(root: &Component) -> HashSet<&str> {
    root.nested()
        .into_iter()
        .flat_map(|c| {
            let extra = match c {
                Component::Tabs(t) => vec![t.select.as_deref()],
                Component::Form(f) => vec![f.submit.as_deref()],
                // A grid's drags run these, so no button does.
                Component::Grid(g) => vec![
                    g.drag_begin.as_deref(),
                    g.drag_end.as_deref(),
                    g.region_drag.as_deref(),
                ],
                // Its rows run it on a double click or Enter, so it is no button.
                Component::List(l) => vec![l.activate.as_deref()],
                _ => vec![],
            };
            // A row that does what an affordance does offers it, so it is no button either.
            let rows = match c {
                Component::List(l) => l
                    .rows
                    .iter()
                    .filter_map(|r| r.affordance.as_deref())
                    .collect(),
                _ => vec![],
            };
            c.head()
                .affordances
                .iter()
                .map(String::as_str)
                .chain(extra.into_iter().flatten())
                .chain(rows)
        })
        .collect()
}

/// The affordances of `screen` that take a row of its list `id` and that no part names: the list offers them under
/// its rows as its own.
pub(super) fn row_actions<'a>(
    screen: &'a Screen,
    id: &str,
) -> impl Iterator<Item = &'a Affordance> {
    let named = screen.root.as_ref().map_or_else(HashSet::new, claimed);
    screen.affordances.iter().filter(move |a| {
        !named.contains(a.id.as_str())
            && a.args
                .as_ref()
                .and_then(|args| args.row.as_ref())
                .is_some_and(|row| row.component_id == id)
    })
}

/// The affordances of `screen` that its footer offers: those no part names nor that take a row of one of its lists,
/// which the list offers under its rows, or a square of one of its grids, which runs from the squares whose cells
/// name it.
pub(super) fn footer_affordances(screen: &Screen) -> Vec<&Affordance> {
    let named = screen.root.as_ref().map_or_else(HashSet::new, claimed);
    let rowed = |c: &&Component| matches!(c, Component::List(_) | Component::Grid(_));
    let rows = screen.components().into_iter().filter(rowed);
    let rows = rows.map(Component::id).collect::<HashSet<_>>();
    let on_row = |a: &Affordance| {
        let row = a.args.as_ref().and_then(|args| args.row.as_ref());
        row.is_some_and(|row| rows.contains(row.component_id.as_str()))
    };
    let shown = |a: &&Affordance| !named.contains(a.id.as_str()) && !on_row(a);
    screen.affordances.iter().filter(shown).collect()
}

/// The engine's affordance that opens the prompt of a list's filter.
pub(super) const FILTER: &str = "FILTER";

/// Whether `a` is the filter of `component`, a list with a search box: the box sets the text, and its keys
/// take the box instead of opening the prompt.
pub(super) fn searched(screen: &Screen, list: &List, a: &Affordance) -> bool {
    list.search(screen)
        .is_some_and(|search| search.id == a.id || a.id == FILTER)
}

/// Whether a drag over one of the screen's grids runs the affordance, so no button does.
fn gesture(screen: &Screen, affordance: &Affordance) -> bool {
    let id = Some(affordance.id.as_str());
    screen.components().into_iter().any(|c| match c {
        Component::Grid(g) => [&g.drag_begin, &g.drag_end, &g.region_drag]
            .into_iter()
            .any(|gesture| gesture.as_deref() == id),
        _ => false,
    })
}

/// Whether a component's own widgets run the affordance, so it needs no button; a click on the map moves
/// its cursor.
fn driven(component: &Component, affordance: &Affordance) -> bool {
    match component {
        Component::Map(_) => affordance.args.as_ref().is_some_and(|a| a.pos.is_some()),
        Component::Tabs(t) => t.select.as_deref() == Some(affordance.id.as_str()),
        Component::Form(_) => affordance.args.as_ref().is_some_and(|a| a.text.is_some()),
        Component::Grid(g) => g
            .cells
            .iter()
            .any(|cell| cell.affordance.as_deref() == Some(affordance.id.as_str())),
        _ => false,
    }
}

/// The buttons a component draws for the affordances it names that its own widgets do not run.
pub(super) fn own_buttons<'a>(screen: &'a Screen, component: &Component) -> Vec<&'a Affordance> {
    component
        .head()
        .affordances
        .iter()
        .filter_map(|id| find(screen, id))
        // A list's rows run its `activate`, also where a panel around the list names it.
        .filter(|a| {
            !driven(component, a)
                && !gesture(screen, a)
                && !run_by_rows(screen, a)
                && !matches!(component, Component::List(list) if searched(screen, list, a))
                && drawn_button(a)
        })
        .collect()
}

/// Whether `a` is drawn as a button: picking, moving, digits and text edits run from their rows and keys instead, and what
/// only rearranges the native window changes nothing here. A grouped one is also an item of the workspace's menu bar.
pub(super) fn drawn_button(a: &Affordance) -> bool {
    !matches!(
        a.role,
        Some(
            AffordanceRole::Select
                | AffordanceRole::Navigate
                | AffordanceRole::Digit
                | AffordanceRole::Layout
                | AffordanceRole::Edit
        )
    )
}
