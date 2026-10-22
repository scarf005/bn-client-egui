//! The generic menu a 0.1 interaction shows.

use super::{EngineModel, Paged, Pending};
use crate::model::{Menu, MenuChoice, MenuField, MenuModel, MenuPane, PaneTotals};
use crate::protocol::{self, Choice, Interaction, Operation};
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
        // The cursor travels as `compat.focus`.
        let focus = i
            .compat
            .as_ref()
            .and_then(|c| c["focus"]["choice_id"].as_str());
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
                .map(|c| row(c, tab(c), Some(c.id.as_str()) == focus))
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

impl MenuModel for EngineModel {
    fn menu(&self) -> Option<Menu> {
        profiling::scope!("model.menu");
        let i = self.replica.boundary()?.interaction.as_ref()?;
        // A cursor the engine moves (look mode) is drawn on the map, not listed.
        (i.kind != "target").then(|| self.menu_of(i))
    }

    fn choose(&mut self, choice_id: &str) {
        let enabled = self
            .menu()
            .is_some_and(|m| m.choices.iter().any(|c| c.id == choice_id && c.enabled));
        if enabled && !self.busy {
            self.submit(Operation::Choose {
                choice_id: choice_id.into(),
            });
        }
    }

    fn can_scroll_menu(&self) -> bool {
        self.replica.boundary().is_some_and(|b| {
            b.interaction.is_some() && b.actions.iter().any(|a| a.id == "SCROLL_UP")
        })
    }

    fn scroll_menu(&mut self, up: bool) -> bool {
        let id = if up { "SCROLL_UP" } else { "SCROLL_DOWN" };
        self.can_scroll_menu() && !self.busy && self.invoke(&[id.into()])
    }

    fn more_choices(&mut self) {
        let Some(b) = self.replica.boundary() else {
            return;
        };
        let Some(i) = b.interaction.as_ref() else {
            return;
        };
        let key = (b.boundary_id.clone(), i.schema_id.clone());
        if self.paged.key != key {
            self.paged = Paged {
                key,
                ..Paged::default()
            };
        }
        let loaded = i.choices.len() + self.paged.more.len();
        if self.paged.waiting || self.paged.closed || loaded as u64 >= i.choice_total {
            return;
        }
        let (epoch, boundary_id) = (
            self.replica.at().map(|a| a.epoch.clone()),
            b.boundary_id.clone(),
        );
        let Some(epoch) = epoch else {
            return;
        };
        self.paged.waiting = true;
        let params = protocol::ChoicesParams {
            epoch,
            boundary_id,
            component_id: None,
            offset: loaded as u64,
            limit: protocol::PAGE_ROWS,
        };
        self.call(
            "bn.interaction.choices",
            serde_json::to_value(params).expect("choices params serialize"),
            Pending::Choices,
        );
    }

    fn cancel_menu(&mut self) {
        if self.menu().is_some_and(|m| m.can_cancel) && !self.busy {
            self.submit(Operation::Cancel);
        }
    }

    fn fill(&mut self, field_id: &str, value: &str, submit: bool) {
        // An editable row is filled by its own id, like an option of the options menu.
        let has_field = self.menu().is_some_and(|m| {
            m.field.is_some_and(|f| f.id == field_id)
                || m.choices
                    .iter()
                    .any(|c| c.id == field_id && c.enabled && c.editor.is_some())
        });
        if has_field && !self.busy {
            self.submit(Operation::Fill {
                field_id: field_id.into(),
                value: value.into(),
                submit,
            });
        }
    }

    fn set_count(&mut self, choice_id: &str, count: u64) {
        let allowed = self.menu().is_some_and(|m| {
            m.can_set_count && m.choices.iter().any(|c| c.id == choice_id && c.enabled)
        });
        if allowed && !self.busy {
            self.submit(Operation::SetCount {
                choice_id: choice_id.into(),
                count,
            });
        }
    }
}
