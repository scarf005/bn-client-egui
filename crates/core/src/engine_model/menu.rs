//! The generic menu a 0.1 interaction shows.

use super::EngineModel;
use crate::model::{Menu, MenuChoice, MenuField, MenuPane, PaneTotals};
use crate::protocol::{Choice, Interaction};
use serde::Deserialize;

/// A pane the engine names in `compat.panes`.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Pane {
    id: String,
    label: String,
    role: String,
    area_label: String,
    area_description: String,
    storage_kind: String,
    filter: String,
    sort: String,
    totals: Option<PaneTotals>,
    page: Option<Page>,
}

#[derive(Deserialize)]
struct Page {
    number: u64,
    count: u64,
}

impl Pane {
    /// The pane as a menu shows it, when it is on an area.
    fn shown(&self) -> Option<MenuPane> {
        (!self.area_label.is_empty()).then(|| MenuPane {
            id: self.id.clone(),
            label: self.label.clone(),
            role: self.role.clone(),
            area: self.area_label.clone(),
            storage: self.storage_kind.clone(),
            description: self.area_description.clone(),
            filter: self.filter.clone(),
            sort: self.sort.clone(),
            totals: self.totals.clone(),
            page: self.page.as_ref().map(|p| (p.number, p.count)),
        })
    }
}

/// The panes the engine names, in its order; none when it names none.
fn panes(i: &Interaction) -> Vec<Pane> {
    i.compat
        .as_ref()
        .and_then(|c| Vec::<Pane>::deserialize(&c["panes"]).ok())
        .unwrap_or_default()
}

/// The text field of the interaction, when it has one.
fn field(i: &Interaction) -> Option<MenuField> {
    let text = |k: &str| {
        let v = i.field.as_ref()?.get(k)?.as_str();
        v.map(String::from)
    };
    text("id").map(|id| MenuField {
        id,
        label: text("label").unwrap_or_default(),
        value: text("value").unwrap_or_default(),
        kind: text("type").unwrap_or_default(),
    })
}

/// A row as the menu lists it; `tab` says it is a tab header, `highlighted` that the engine's cursor is on it.
fn row(c: &Choice, tab: bool, highlighted: bool) -> MenuChoice {
    MenuChoice {
        id: c.id.clone(),
        label: c.label.clone(),
        enabled: c.enabled && c.selectable,
        denial: c.denial.clone(),
        hotkey: c.hotkey.clone().unwrap_or_default(),
        selected: c.selected,
        group: c.pane_id.clone(),
        tab,
        highlighted,
        selected_count: c.selected_count,
        available_count: c.available_count,
        description: c.description.clone(),
        columns: c.columns.clone(),
        favorite: c.favorite,
        color: c.color.clone().unwrap_or_default(),
        key_color: c.key_color.clone().unwrap_or_default(),
        markup: c.markup.clone().unwrap_or_default(),
        action: c.action_id.clone().unwrap_or_default(),
        nest_color: c.nest_color.clone().unwrap_or_default(),
        mark: c.mark.clone(),
        editor: c.editor.clone(),
    }
}

impl EngineModel {
    /// The generic menu an interaction shows, with the pages fetched after its first.
    pub(super) fn menu_of(&self, i: &Interaction) -> Menu {
        let panes = panes(i);
        let pane_of = |label: &str| panes.iter().find(|p| p.label == label).map(|p| &p.id);
        // Without the engine's pane list, the rows that belong to no pane are the tabs of a paned menu.
        let more = self.more_of(i);
        let all = || i.choices.iter().chain(more);
        let paned = all().any(|c| c.pane_id.is_some());
        let tab = |c: &Choice| {
            c.pane_id.is_none() && (pane_of(&c.label).is_some() || (panes.is_empty() && paned))
        };
        // The cursor travels as `compat.focus`; a popup row's description is its action id, for tools.
        let focus = i
            .compat
            .as_ref()
            .and_then(|c| c["focus"]["choice_id"].as_str());
        let actions = self.replica.boundary().map_or(&[][..], |b| &b.actions);
        let described = |text: &str| match actions.iter().any(|a| a.id == text) {
            true => String::new(),
            false => text.to_string(),
        };
        Menu {
            context: i.context.clone(),
            schema_id: i.schema_id.clone(),
            field: field(i),
            can_set_count: i.allow_set_count,
            title: i.title.clone(),
            message: i.message.clone(),
            message_tone: i.message_tone.clone().unwrap_or_default(),
            category_mode: i.category_mode,
            total: i.choice_total as usize,
            choices: all()
                .map(|c| MenuChoice {
                    description: described(&c.description),
                    ..row(c, tab(c), Some(c.id.as_str()) == focus)
                })
                .collect(),
            panes: panes.iter().filter_map(Pane::shown).collect(),
            can_cancel: i.allow_cancel,
            pane: all()
                .find(|c| tab(c) && c.selected)
                .and_then(|c| pane_of(&c.label).cloned()),
            screen: i.kind == "custom",
            focus: focus.map(String::from),
            sections: panes
                .iter()
                .map(|p| (p.id.clone(), p.label.clone()))
                .collect(),
        }
    }
}
