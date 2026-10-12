//! The native pickup list: `PICK Wgt/Vol` header, one row per stack as its letter, nesting slash, mark
//! and name in the item's color, the cursor bar, the count box of a marked stack, the key hints under
//! the list, and the cursor row's item description. Letters mark rows; Enter confirms, Escape cancels.
use super::inventory::{column, key_hints, selection_mark, tinted, typed_hotkey};
use super::menu::{list_frame, list_row_style};
use super::{App, a11y};
use bn_client_core::{GameModel, Menu, MenuChoice};
use eframe::egui::{self, accesskit};

#[derive(Default)]
pub struct Pickup {
    /// The row the cursor was last on, so the list scrolls only when it moves.
    cursor: Option<String>,
}

/// The native nesting slash of a row: gray under an unmarked parent, yellow under a marked one that
/// still has an unmarked child, light blue when all its children are marked; none for a plain row.
fn nesting(rows: &[&MenuChoice], row: &MenuChoice) -> Option<&'static str> {
    let parent_id = column(row, "Nesting").strip_prefix("child of ")?;
    let parent = rows.iter().find(|r| r.id == parent_id);
    let mark = format!("child of {parent_id}");
    let all = rows
        .iter()
        .filter(|r| column(r, "Nesting") == mark)
        .all(|r| r.selected);
    Some(match parent {
        Some(p) if p.selected && all => "c_light_blue",
        Some(p) if p.selected => "c_yellow",
        _ => "c_dark_gray",
    })
}

impl<M: GameModel> App<M> {
    /// Draws the menu; true when it took keyboard focus (on `entered`) this frame.
    pub(super) fn pickup(&mut self, ctx: &egui::Context, menu: &Menu, entered: bool) -> bool {
        a11y::trap_tab(
            ctx,
            self.trapped_by_menu(),
            super::menu_forwards_keys(Some(menu)),
        );
        let mut ids = Vec::new();
        let mut focused = false;
        let mut chosen = typed_hotkey(ctx, &self.model, &menu.choices);
        let mut count = None;
        let mut confirm = false;
        let any = menu.choices.iter().any(|c| c.selected);
        let cancel = self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && menu.can_cancel;
        let rows = menu.choices.iter().filter(|c| !c.tab).collect::<Vec<_>>();
        let title = bn_client_core::markup::plain(&menu.title);
        let mut hovered = None;
        egui::Window::new(&title)
            .id(a11y::dialog_id(&title))
            .title_bar(false)
            // The native list sits at the left edge of the screen.
            .anchor(egui::Align2::LEFT_TOP, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .frame(list_frame(ctx))
            .show(ctx, |ui| {
                // The native header: predicted against capacity, red where over, then the filter.
                let header = match menu.panes.first().and_then(|p| p.totals.as_ref()) {
                    Some(t) => {
                        let tone = |over| if over { "c_red" } else { "c_white" };
                        format!(
                            "PICK Wgt {}/{}  Vol {}/{}",
                            tinted(tone(t.over_weight), &t.weight),
                            t.weight_max,
                            tinted(tone(t.over_volume), &t.volume),
                            t.volume_max
                        )
                    }
                    None => menu.message.clone(),
                };
                ui.label(self.rich(ui, &tinted("c_white", &header)));
                if let Some(filter) = menu
                    .panes
                    .first()
                    .map(|p| &p.filter)
                    .filter(|f| !f.is_empty())
                {
                    ui.label(self.rich(ui, &format!("Filter: {filter}")));
                }
                egui::ScrollArea::vertical()
                    .max_height(ctx.content_rect().height() * 0.45)
                    .show(ui, |ui| {
                        for (i, row) in rows.iter().enumerate() {
                            ui.horizontal(|ui| {
                                list_row_style(ui);
                                ui.spacing_mut().item_spacing.x = 0.0;
                                let color = column(row, "Row color");
                                let key = match row.hotkey.as_str() {
                                    "" => " ".to_string(),
                                    k => tinted(color, k),
                                };
                                let slash = nesting(&rows, row)
                                    .map_or(" ".to_string(), |c| tinted(c, "\\"));
                                let mark = match selection_mark(row) {
                                    '-' => "- ".to_string(),
                                    m => tinted("c_light_blue", &format!("{m} ")),
                                };
                                let name = match column(row, "Name markup") {
                                    "" => tinted(color, &row.label),
                                    markup => tinted(color, markup),
                                };
                                let text = self.rich(ui, &format!("{key}{slash}{mark}{name}"));
                                let button =
                                    self.control(ui, text, &row.id, row.enabled, row.highlighted);
                                ids.extend(button.enabled().then_some(button.id));
                                ui.ctx().accesskit_node_builder(button.id, |b| {
                                    b.set_role(accesskit::Role::MenuItem);
                                    b.set_label(bn_client_core::markup::plain(&row.label));
                                    b.set_toggled(if row.selected {
                                        accesskit::Toggled::True
                                    } else {
                                        accesskit::Toggled::False
                                    });
                                });
                                if entered && i == 0 {
                                    button.request_focus();
                                    focused = button.has_focus();
                                }
                                if row.highlighted && self.pickup.cursor.as_deref() != Some(&row.id)
                                {
                                    button.scroll_to_me(None);
                                    self.pickup.cursor = Some(row.id.clone());
                                }
                                if button.hovered() {
                                    hovered = Some(*row);
                                }
                                // Enter on a focused row is the native confirm key; a click marks.
                                if button.clicked() {
                                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                        confirm = any;
                                    } else {
                                        chosen = Some(row.id.clone());
                                    }
                                }
                                if let Some(c) = self.count_box(ui, row, &mut ids) {
                                    count = Some(c);
                                }
                                if !row.enabled && !row.denial.is_empty() {
                                    ui.label(self.rich(ui, &tinted("c_red", &row.denial)));
                                }
                            });
                        }
                    });
                let actions = self.model.actions();
                for hints in [
                    &[
                        ("LEFT", "Unmark"),
                        ("HELP_KEYBINDINGS", "Help"),
                        ("RIGHT", "Mark"),
                    ],
                    &[
                        ("PREV_TAB", "Prev"),
                        ("SELECT_ALL", "All"),
                        ("NEXT_TAB", "Next"),
                    ],
                ] {
                    ui.label(self.rich(ui, &tinted("c_light_gray", &key_hints(&actions, hints))));
                }
                let shown = hovered.or_else(|| rows.iter().copied().find(|r| r.highlighted));
                self.item_info(
                    ui,
                    &shown
                        .map(|r| tinted(column(r, "Row color"), &r.label))
                        .unwrap_or_default(),
                    shown.map(|r| r.description.as_str()),
                );
            });
        self.menu_ids = ids;
        a11y::dialog(ctx, &title, &menu.message);
        if let Some((id, n)) = count {
            ctx.request_repaint();
            self.model.set_count(&id, n);
        } else if confirm {
            ctx.request_repaint();
            self.model.invoke_action("CONFIRM");
        } else if let Some(id) = chosen {
            ctx.request_repaint();
            self.model.choose(&id);
        } else if cancel {
            self.model.cancel_menu();
        }
        focused
    }
}
