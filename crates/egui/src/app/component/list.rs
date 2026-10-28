//! A list component: its rows as a `kit::table` and its count editors.

use super::*;
use crate::kit::table;
use bn_client_core::localise::tr;

/// Rows left below the last one shown before the next page is asked for.
const PREFETCH: usize = 20;

/// The fewest rows a list keeps room for, however little the window has left.
const MIN_ROWS: f32 = 3.0;

/// A list row's item and the square it is drawn in.
pub(super) struct RowIcon {
    pub(super) painter: egui::Painter,
    pub(super) rect: egui::Rect,
    pub(super) cell: Cell,
}

/// How this frame lays out a list's rows.
struct RowLayout<'a> {
    widths: &'a table::Widths,
    /// The table's body, which a category row's band spans.
    across: egui::Rect,
    /// The list is the one of a top screen with no detail beside it, whose rows a single click answers; a panel's
    /// rows open the full screen only on a double click.
    answers: bool,
    /// The screen is a list filter's prompt: a row it shows is a result, and a click on one confirms the text and
    /// then chooses that row of the list.
    filter: bool,
    /// The engine chose a row of the list and the focus does not move its choice: the chosen row is shown
    /// selected, and the focus only outlined.
    chosen: bool,
}

impl<M: GameModel> App<M> {
    /// The room a list has here: what the window has left, less the `below` lines it draws under itself (the
    /// refusal and the buttons), and no more than `list_rows` rows of `step` when the list is drawn so, nor more
    /// than the `held` height of the rows of a list that has all its rows, past `MIN_ROWS`.
    fn list_height(&self, ui: &egui::Ui, step: f32, below: usize, held: Option<f32>) -> f32 {
        let spacing = ui.spacing();
        let line = spacing.interact_size.y + spacing.item_spacing.y;
        let room = match self.screens.in_column.get() {
            true => f32::INFINITY,
            false => (ui.available_height() - below as f32 * line).max(MIN_ROWS * line),
        };
        self.screens.list_rows.get().map_or(room, |rows| {
            let cap = rows as f32 * step;
            room.min(held.map_or(cap, |held| cap.min(held.max(MIN_ROWS * step))))
        })
    }

    /// The row's label in its colour.
    pub(super) fn row_label(row: &Row) -> String {
        tagged(
            row.color.as_deref(),
            &row.markup.clone().unwrap_or_else(|| row.label.shown()),
        )
    }

    /// The engine's mark and the favourite, as the pick column shows them beside the checkbox.
    pub(super) fn row_marks(row: &Row) -> Option<String> {
        let mark = row
            .mark
            .as_ref()
            .map(|m| tagged(m.color.as_deref(), &m.glyph));
        let favorite = row.favorite.filter(|&f| f).map(|_| "*".to_string());
        let marks = [mark, favorite].into_iter().flatten().collect::<Vec<_>>();
        (!marks.is_empty()).then(|| marks.join(" "))
    }

    /// Whether the row's checkbox is ticked: picked in a list that picks, else switched on; none for a row that
    /// is neither.
    pub(super) fn row_check(list: &List, row: &Row, picked: bool) -> Option<bool> {
        match list.selection {
            Selection::Multi | Selection::Count => Some(picked),
            _ => row.toggled,
        }
    }

    /// A screen of one list with no detail beside it asks a question: each row is an answer.
    fn asks(screen: &Screen) -> bool {
        let parts = screen.components();
        let lists = parts.iter().filter(|c| matches!(c, Component::List(_)));
        let details = parts
            .iter()
            .filter(|c| matches!(c, Component::Split(s) if s.relation == Relation::Detail));
        lists.count() == 1 && details.count() == 0
    }

    pub(super) fn list(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        list: &List,
        out: &mut Vec<Intent>,
    ) {
        let id = &list.head.id;
        let slot = (screen.screen_id.clone(), id.clone());
        let interaction = &self.screens.interaction;
        let rows = self.known_rows(list);
        if let Some(search) = list.search(screen).filter(|_| !self.screens.passive) {
            self.search_box(ui, screen, (list, search), &rows, out);
        }
        let focus = interaction.focus();
        let cursor = focus
            .row_id
            .as_deref()
            .filter(|_| focus.component_id.as_deref() == Some(id));
        let spacing = ui.spacing().item_spacing;
        // The rows sit in a scroll area, which keeps room for its bar.
        let room = ui.available_width() - ui.spacing().scroll.allocated_width();
        let widths = self.list_widths(ui, list, &rows);
        // Without columns to keep beside it, the label takes the room.
        // Where names come first, a long value is cut before the label wraps.
        let kept = match self.screens.in_column.get() {
            true => table::VALUE_KEPT,
            false => f32::INFINITY,
        };
        let label = match list.columns.is_empty() {
            true => widths.wrapped_label_in(room, spacing.x),
            false => widths.label_in(room, spacing.x, kept),
        };
        let widths = widths.fitted(room, spacing.x, label, kept);
        let height = match self.squares(&rows) {
            true => tokens::ROW_ITEM,
            false => tokens::ROW,
        };
        let captions = list
            .columns
            .iter()
            .map(|c| markup::plain(&c.label.shown()))
            .collect::<Vec<_>>();
        // The captions head the columns, when some column has one.
        let header = captions
            .iter()
            .any(|c| !c.is_empty())
            .then(|| table::header_height(ui));
        let own = list
            .head
            .affordances
            .iter()
            .filter_map(|a| find(screen, a))
            .filter(|a| !searched(screen, list, a))
            .chain(row_actions(screen, id))
            .collect::<Vec<_>>();
        // The rows run the list's `activate`, on a double click or Enter; in a workspace's column, beside other
        // buttons that act on a row, it is one too, so every action on an editor's row is in sight.
        let activate = |a: &Affordance| list.activate.as_deref() == Some(a.id.as_str());
        let on_row = |a: &Affordance| {
            a.args
                .as_ref()
                .and_then(|args| args.row.as_ref())
                .is_some_and(|row| &row.component_id == id)
        };
        let beside = self.screens.in_column.get()
            && own
                .iter()
                .any(|a| !activate(a) && on_row(a) && drawn_button(a));
        let own = own
            .into_iter()
            .filter(|a| beside || !activate(a))
            .collect::<Vec<_>>();
        let step = height + spacing.y;
        // Room is kept for a refusal and the buttons only when some row or the list has them, so a list with
        // neither reaches the bottom.
        let refused = rows
            .iter()
            .any(|r| !r.enabled && !r.denial.shown().is_empty());
        let buttons = own.iter().any(|a| drawn_button(a));
        let below = usize::from(refused) + 2 * usize::from(buttons);
        // A label too long for its width wraps between words and makes its row taller.
        let heights = rows
            .iter()
            .map(|row| self.wrapped_height(ui, row, label).max(height))
            .collect::<Vec<_>>();
        let starts = heights
            .iter()
            .scan(0.0, |y, h| {
                let start = *y;
                *y += h + spacing.y;
                Some(start)
            })
            .collect::<Vec<_>>();
        // A list that holds every row it has is no taller than they are, under its captions.
        let held = (rows.len() as u64 >= list.total).then(|| {
            let end = starts.last().zip(heights.last());
            end.map_or(0.0, |(start, h)| start + h) + header.map_or(0.0, |h| h + spacing.y)
        });
        let list_height =
            self.list_height(ui, step, below, held) - header.map_or(0.0, |h| h + spacing.y);
        // In a column, the rows take the height the column gives them, so it holds all its parts.
        let wanted = starts
            .last()
            .zip(heights.last())
            .map_or(0.0, |(start, h)| start + h);
        let cap = match self.screens.in_column.get() {
            true => self.screens.column_caps.borrow().get(id).copied(),
            false => None,
        };
        let list_height = cap.map_or(list_height, |cap| cap.min(wanted).max(MIN_ROWS * step));
        // A page is the rows the list has room for.
        let page = ((list_height + spacing.y) / step) as usize;
        self.screens
            .list_pages
            .borrow_mut()
            .insert(slot.clone(), page.max(1));
        let at = cursor.and_then(|c| rows.iter().position(|r| r.id == c));
        // The engine may put its cursor on a row past the pages held: they are asked for until it is among them.
        if cursor.is_some() && at.is_none() {
            out.push(Intent::More(id.clone()));
        }
        // A focus new to the list and out of its sight scrolls the focused row to the middle, with the rows
        // around it in view, the top row whole; without a focus in the list, so does a newly selected row.
        let selected = rows.iter().position(|r| r.selected);
        let shown = at.or(selected.filter(|_| cursor.is_none()));
        let focus = shown.map(|i| rows[i].id.clone());
        let mut offset = None;
        let (mut pending, mut moved) = (false, false);
        if let Some(i) = shown {
            let mut scrolls = self.screens.list_scroll.borrow_mut();
            let scroll = scrolls.entry(slot.clone()).or_default();
            let y = starts[i];
            let step = heights[i] + spacing.y;
            // The last row has no gap under it in the viewport.
            let seen = y >= scroll.top && y + heights[i] <= scroll.top + list_height + 0.5;
            if scroll.shown != focus && !seen {
                let middle = y - (list_height - step) / 2.0;
                let top = starts.iter().copied().take_while(|s| *s <= middle).last();
                offset = Some(top.unwrap_or(0.0));
            }
            pending = scroll.shown != focus;
            moved = scroll.shown.is_some();
        }
        // The viewport is a whole number of rows tall: the most rows from the one at its top that fit, so rows of
        // a wrapped label do not leave the last row cut. An empty list, or one whose top row does not fit, keeps its
        // room.
        let top = offset.unwrap_or_else(|| {
            let scrolls = self.screens.list_scroll.borrow();
            scrolls.get(&slot).map_or(0.0, |s| s.top)
        });
        let first = starts.iter().rposition(|s| *s <= top + 0.5).unwrap_or(0);
        let list_height = starts[first..]
            .iter()
            .zip(&heights[first..])
            .map(|(start, h)| start + h - starts[first])
            .take_while(|end| *end <= list_height)
            .last()
            .unwrap_or(list_height);
        if self.screens.in_column.get() {
            let fit = ListFit {
                id: id.clone(),
                wanted,
                taken: list_height,
            };
            self.screens.column_lists.borrow_mut().push(fit);
        }
        let mut drawn = 0;
        let body = |body: egui_extras::TableBody| {
            let layout = RowLayout {
                widths: &widths,
                across: body.max_rect(),
                answers: !self.screens.passive && Self::asks(screen),
                filter: form::filter_submit(screen).is_some(),
                chosen: rows.iter().any(|r| r.selected) && !follows_focus(screen, id),
            };
            body.heterogeneous_rows(heights.iter().copied(), |row| {
                let at = row.index();
                drawn = drawn.max(at + 1);
                self.list_row(row, list, rows[at], cursor, &layout, out);
            });
        };
        let top = ui.cursor().top();
        let scrolled = ui
            .scope(|ui| {
                table::row_style(ui);
                // A row is as high as its line; the label's button fills it.
                ui.spacing_mut().button_padding.y = 0.0;
                let mut table = table::table(ui, &widths, label)
                    .id_salt(("list", id))
                    .max_scroll_height(list_height)
                    // A column scrolls as a whole, so a list low in it is as tall as below the column's fold.
                    .min_scrolled_height(match self.screens.in_column.get() {
                        true => list_height.max(height),
                        false => height,
                    })
                    // An empty list keeps its room, so its pane does not collapse.
                    .auto_shrink([false, !rows.is_empty()]);
                if let Some(offset) = offset {
                    table = table.vertical_scroll_offset(offset);
                }
                match header {
                    Some(h) => table
                        .header(h, |row| table::header(row, &widths, &captions))
                        .body(body),
                    None => table.body(body),
                }
            })
            .inner;
        windows::note_hidden(ui, &scrolled);
        // The row is shown once it is in sight in the list and in what holds it: a list that changes its height as
        // it scrolls scrolls again, and what holds it scrolls too when the row moved there from another.
        if let (Some(i), true) = (shown, pending) {
            let y = scrolled.inner_rect.top() + starts[i] - scrolled.state.offset.y;
            let row =
                egui::Rect::from_x_y_ranges(scrolled.inner_rect.x_range(), y..=y + heights[i]);
            let within = |r: egui::Rect| {
                r.y_range().contains(row.top()) && r.y_range().contains(row.bottom())
            };
            match (
                within(scrolled.inner_rect),
                within(ui.clip_rect()) || !moved,
            ) {
                (true, true) => {
                    let mut scrolls = self.screens.list_scroll.borrow_mut();
                    scrolls.entry(slot.clone()).or_default().shown = focus;
                }
                (true, false) => ui.scroll_to_rect(row, None),
                (false, _) => {}
            }
        }
        // A list with no rows keeps its pane, outlined, as tall as its rows would have been.
        if rows.is_empty() {
            let pane = egui::Rect::from_x_y_ranges(
                scrolled.inner_rect.x_range(),
                top..=scrolled.inner_rect.bottom(),
            );
            let stroke = egui::Stroke::new(1.0, tokens::BORDER);
            ui.painter()
                .rect_stroke(pane, 0, stroke, egui::StrokeKind::Inside);
        }
        if drawn + PREFETCH >= rows.len() {
            out.push(Intent::More(id.clone()));
        }
        let mut scrolls = self.screens.list_scroll.borrow_mut();
        scrolls.entry(slot).or_default().top = scrolled.state.offset.y;
        drop(scrolls);
        let shown = cursor.and_then(|c| rows.iter().find(|r| r.id == c));
        self.affordance_buttons(ui, &own, shown.copied(), out);
    }

    /// The box above a list that sets the list's filter as it is typed: each text goes to the engine as the
    /// `search` affordance, one at a time with the newest last. Return or Down put the focus on the first row.
    fn search_box(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        (list, search): (&List, &Affordance),
        rows: &[&Row],
        out: &mut Vec<Intent>,
    ) {
        let id = format!("{}/{}/search", screen.screen_id, list.head.id);
        let typed = editor::search_box(
            ui,
            &editor::Search {
                screen: &screen.screen_id,
                id: &id,
                label: &tr("Search"),
                value: list.filter.as_deref().unwrap_or_default(),
                busy: self.model.busy(),
                // The focus asked for is the top screen's box, never that of a workspace under it.
                focus: !self.screens.under.get() && self.screens.focus_search.take(),
            },
        );
        if let Some(text) = typed.changed.or(typed.submit) {
            out.push(Intent::Search {
                affordance: search.id.clone(),
                text,
            });
        }
        let first = rows.iter().find(|r| r.enabled && r.selectable);
        if let (true, Some(row)) = (typed.left, first) {
            out.push(Intent::Focus {
                component: list.head.id.clone(),
                row: row.id.clone(),
            });
        }
    }

    /// How high a row of a list without columns is with its label wrapped at `label`.
    fn wrapped_height(&self, ui: &egui::Ui, row: &Row, label: f32) -> f32 {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let indent = f32::from(u8::from(row.parent_id.is_some())) * tokens::INDENT;
        let mut job = text::job(
            &self.model.styled(&Self::row_label(row)),
            &font,
            egui::Color32::WHITE,
        );
        job.wrap.max_width = label - indent - 2.0 * ui.spacing().button_padding.x;
        ui.fonts_mut(|f| f.layout_job(job)).size().y
    }

    /// One row: its checkbox and marks, its item, its key, its label as a button, its column values and its
    /// count editor. A click focuses the row and marks it in a picking list; a double click, or a single click on the
    /// choices of a question, runs the list's `activate`, as Enter does.
    fn list_row(
        &self,
        mut cells: egui_extras::TableRow,
        list: &List,
        row: &Row,
        cursor: Option<&str>,
        layout: &RowLayout,
        out: &mut Vec<Intent>,
    ) {
        let id = &list.head.id;
        let widths = layout.widths;
        if row.header == Some(true) {
            let title = markup::plain(&row.label.shown());
            return table::section(cells, widths, layout.across, title);
        }
        let pick = self
            .screens
            .interaction
            .picks(id)
            .iter()
            .find(|p| p.id == row.id);
        let result = layout.filter && row.enabled && !row.selectable;
        let on = row.enabled && (row.selectable || result);
        let at = cursor == Some(row.id.as_str());
        let highlighted = match layout.chosen {
            true => row.selected,
            false => at || (cursor.is_none() && row.selected),
        };
        cells.set_selected(highlighted);
        // The row's text is drawn on the selection bar when it is highlighted.
        let ground = |ui: &egui::Ui| match highlighted {
            true => tokens::SELECTION,
            false => tokens::ground(ui),
        };
        let line = |ui: &egui::Ui, markup: &str| self.line_on(ui, markup, ground(ui));
        table::cell(&mut cells, widths.pick, |ui| {
            if let Some(ticked) = Self::row_check(list, row, pick.is_some()) {
                table::checkbox(ui, ticked);
            }
            if let Some(marks) = Self::row_marks(row) {
                ui.add(egui::Label::new(line(ui, &marks)).selectable(false));
            }
        });
        table::cell(&mut cells, widths.item, |ui| {
            let look = row.look.as_ref();
            let cell = look
                .filter(|_| self.display.tiles())
                .and_then(|look| self.model.look_cell(look));
            match (cell, look) {
                (Some(cell), _) => {
                    let (column, _) = ui.allocate_exact_size(
                        egui::vec2(tokens::ITEM, ui.available_height()),
                        egui::Sense::hover(),
                    );
                    let rect = egui::Rect::from_center_size(
                        column.center(),
                        egui::Vec2::splat(tokens::ITEM),
                    );
                    self.screens.icons.borrow_mut().push(RowIcon {
                        painter: ui.painter_at(rect),
                        rect,
                        cell,
                    });
                }
                (None, Some(look)) => {
                    ui.add(
                        egui::Label::new(line(ui, &tagged(Some(&look.color), &look.glyph)))
                            .selectable(false),
                    );
                }
                (None, None) => {}
            }
        });
        table::cell(&mut cells, widths.key, |ui| {
            if let Some(key) = &row.hotkey {
                let color = row.key_color.as_deref().map_or(tokens::WEAK, |c| {
                    tokens::readable(self.model.colors().resolve(c).0, ground(ui))
                });
                table::key(ui, egui::RichText::new(key).color(color));
            }
        });
        let mut button = None;
        cells.col(|ui| {
            if row.parent_id.is_some() {
                ui.add_space(tokens::INDENT);
            }
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            let label = Self::row_label(row);
            // A refused row is plain dim text: the game's colours would hide that it is.
            let text = match row.enabled {
                true => line(ui, &label),
                false => egui::RichText::new(markup::plain(&label))
                    .color(ui.visuals().weak_text_color())
                    .into(),
            };
            let justified = egui::Layout::top_down_justified(egui::Align::Min);
            button = Some(
                ui.with_layout(justified, |ui| {
                    self.control(ui, text, &row.id, on, highlighted)
                })
                .inner,
            );
        });
        for (column, w) in list.columns.iter().zip(&widths.values) {
            table::cell(&mut cells, *w, |ui| {
                let value = row
                    .columns
                    .iter()
                    .find(|c| c.key.as_deref() == Some(column.key.as_str()))
                    .map(|c| tagged(c.color.as_deref(), &c.value.shown()))
                    .unwrap_or_default();
                table::right_aligned(ui, line(ui, &value));
            });
        }
        table::cell(&mut cells, widths.count, |ui| {
            if let (Selection::Count, Some(pick), Some(max)) = (list.selection, pick, row.max) {
                let mut count = pick.count.unwrap_or(max);
                let drag = ui.add(egui::DragValue::new(&mut count).range(0..=max));
                if drag.changed() {
                    out.push(Intent::MarkCount {
                        component: id.clone(),
                        row: row.id.clone(),
                        count,
                    });
                }
            }
        });
        table::spare(&mut cells);
        let across = cells.response();
        if at && !highlighted {
            let stroke = egui::Stroke::new(1.0, tokens::SELECTION);
            across.ctx.layer_painter(across.layer_id).rect_stroke(
                across.rect,
                0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        let Some(button) = button else {
            return;
        };
        a11y::selected(&button.ctx, button.id, highlighted);
        button.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::SelectableLabel,
                button.enabled(),
                highlighted,
                markup::plain(&row.label.shown()),
            )
        });
        let denial = row.denial.shown();
        let button = match (on, denial.is_empty()) {
            (false, false) => button.on_disabled_hover_text(denial),
            _ => button,
        };
        let clicked = button.clicked() || across.clicked();
        if clicked && result {
            out.push(Intent::Reach(row.id.clone()));
        } else if clicked {
            out.push(Intent::Focus {
                component: id.clone(),
                row: row.id.clone(),
            });
            match (
                list.selection,
                &list.activate,
                layout.answers || button.double_clicked() || across.double_clicked(),
            ) {
                (Selection::Multi | Selection::Count, _, _) => out.push(Intent::Mark {
                    component: id.clone(),
                    row: row.id.clone(),
                }),
                (_, Some(activate), true) => out.push(Intent::Run(activate.clone())),
                (_, None, true) => out.push(Intent::Open),
                _ => {}
            }
        }
    }

    /// The rows of `list` that are known: a panel's lists hold all of theirs, the top screen's have their first
    /// page and the pages fetched.
    pub(super) fn known_rows<'a>(&'a self, list: &'a List) -> Vec<&'a Row> {
        match self.screens.passive {
            true => list.rows.iter().collect(),
            false => self.model.rows(list),
        }
    }

    /// Whether the rows draw their items in squares: some row has an item and the map is drawn in tiles.
    pub(super) fn squares(&self, rows: &[&Row]) -> bool {
        self.display.tiles() && rows.iter().any(|r| r.look.is_some())
    }
}

/// Whether the engine's cursor is in the list `id` of `screen`, so its chosen row is where its cursor is, or a
/// `select`-role affordance names the list, so moving the focus in it moves the engine's choice.
fn follows_focus(screen: &Screen, id: &str) -> bool {
    let cursor = screen
        .focus
        .as_ref()
        .and_then(|f| f.component_id.as_deref());
    cursor == Some(id)
        || screen.affordances.iter().any(|a| {
            a.role == Some(AffordanceRole::Select)
                && a.args
                    .as_ref()
                    .and_then(|args| args.row.as_ref())
                    .is_some_and(|row| row.component_id == id)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(toggled: Option<bool>) -> Row {
        serde_json::from_value(serde_json::json!({
            "id": "r", "label": "Lighting", "description": "", "denial": "", "enabled": true,
            "selectable": true, "selected": false, "columns": [], "toggled": toggled
        }))
        .unwrap()
    }

    fn list() -> List {
        serde_json::from_value(serde_json::json!({
            "id": "l", "kind": "list", "columns": [], "rows": [], "total": 0, "selection": "single"
        }))
        .unwrap()
    }

    #[test]
    fn a_switch_row_shows_whether_it_is_on_and_a_plain_row_shows_nothing() {
        let check =
            |toggled| App::<bn_client_core::EngineModel>::row_check(&list(), &row(toggled), false);
        assert_eq!(check(Some(true)), Some(true));
        assert_eq!(check(Some(false)), Some(false));
        assert_eq!(check(None), None);
    }

    #[test]
    fn a_row_of_a_picking_list_shows_whether_it_is_picked() {
        let mut list = list();
        list.selection = Selection::Multi;
        let check =
            |picked| App::<bn_client_core::EngineModel>::row_check(&list, &row(None), picked);
        assert_eq!(check(true), Some(true));
        assert_eq!(check(false), Some(false));
    }
}
