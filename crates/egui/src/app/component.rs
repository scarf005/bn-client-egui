//! The top screen of the engine's stack drawn from its components (protocol 0.2): lists, tabs, splits,
//! panels, forms, text, grids, the map and canvases, laid out the same way for every screen.
//!
//! Drawing reads the engine's data and collects what the player did as `Intent`s. Applying an intent only
//! calls the interaction core (`bn_client_core::interaction`), which decides what is sent.

use super::App;
use crate::{a11y, editor, text};
use bn_client_core::colors::ColorMap;
use bn_client_core::interaction::{self, ArgKind, InvokeError, Reason};
use bn_client_core::protocol::{
    Affordance, CanvasComponent, Component, Editor, EditorValue, FieldKind, Form, FormField, Grid,
    List, MapComponent, MapMode, PanelStat, Row, Screen, ScreenRole, Selection, Text, TextBlock,
    TextRole, Tone,
};
use bn_client_core::{Canvas, CanvasRun, GameModel, markup};
use eframe::egui::{self, WidgetInfo, WidgetType};
use std::collections::HashSet;

/// Share of a list's width its label takes when the list has columns.
const LABEL_SHARE: f32 = 0.5;
/// Rows left below the last one shown before the next page is asked for.
const PREFETCH: usize = 20;
/// Height the description under a list may take.
const DESCRIPTION_HEIGHT: f32 = 96.0;
/// Side of a grid cell in points.
const GRID_CELL: f32 = 26.0;
const GOOD: egui::Color32 = egui::Color32::from_rgb(110, 210, 110);
const BAD: egui::Color32 = egui::Color32::from_rgb(235, 95, 95);

/// The client-local state of the top screen: what the interaction core keeps, and what the player is asked.
#[derive(Default)]
pub(super) struct Screens {
    interaction: bn_client_core::interaction::Interaction,
    /// Why the last refused affordance is off, in the engine's words.
    denial: Option<String>,
    /// The list the keys just moved the focus in, to be scrolled to it.
    reveal: Option<String>,
}

/// What the player did while the screen was drawn.
enum Intent {
    Focus {
        component: String,
        row: String,
    },
    Mark {
        component: String,
        row: String,
    },
    MarkCount {
        component: String,
        row: String,
        count: u64,
    },
    Run(String),
    Fill {
        affordance: String,
        form: String,
        field: String,
        value: String,
    },
    More(String),
    Text(String),
    Pane(String),
    Confirm,
    Cancel,
}

/// The screen on top, when the engine describes it by components. The map is drawn by the map view.
pub(super) fn sliced(screens: &[Screen]) -> Option<&Screen> {
    screens
        .last()
        .filter(|s| s.role != ScreenRole::Map && s.root.is_some())
}

/// `component` and every component inside it, in reading order.
fn walk(component: &Component) -> Vec<&Component> {
    std::iter::once(component)
        .chain(component.children().into_iter().flat_map(walk))
        .collect()
}

fn find<'a>(screen: &'a Screen, id: &str) -> Option<&'a Affordance> {
    screen.affordances.iter().find(|a| a.id == id)
}

/// The ids of the affordances some component names.
fn claimed(root: &Component) -> HashSet<&str> {
    walk(root)
        .into_iter()
        .flat_map(|c| {
            let extra = match c {
                Component::Tabs(t) => t.select.as_deref(),
                Component::Form(f) => f.submit.as_deref(),
                _ => None,
            };
            c.head().affordances.iter().map(String::as_str).chain(extra)
        })
        .collect()
}

/// Whether a component's own widgets run the affordance, so it needs no button.
fn driven(component: &Component, affordance: &Affordance) -> bool {
    match component {
        Component::Tabs(t) => t.select.as_deref() == Some(affordance.id.as_str()),
        Component::Form(_) => affordance.args.as_ref().is_some_and(|a| a.text.is_some()),
        Component::Grid(g) => g
            .cells
            .iter()
            .any(|cell| cell.affordance.as_deref() == Some(affordance.id.as_str())),
        _ => false,
    }
}

/// The form field as the value editors take it.
fn editor_of(field: &FormField) -> Editor {
    let kind = match field.kind {
        FieldKind::Text => "text",
        FieldKind::Integer => "integer",
        FieldKind::Float => "float",
        FieldKind::Key => "key",
        FieldKind::Bool => "bool",
        FieldKind::Select => "select",
    };
    Editor {
        kind: kind.into(),
        value: field.value.clone(),
        values: field
            .options
            .iter()
            .flatten()
            .map(|o| EditorValue {
                id: o.id.clone(),
                label: o.label.shown(),
            })
            .collect(),
        minimum: field.min.clone(),
        maximum: field.max.clone(),
        max_length: field.max_length.unwrap_or(-1),
    }
}

/// The engine's window with its curses colours resolved from the built-in palette.
fn canvas_of(canvas: &CanvasComponent, colors: &ColorMap) -> Canvas {
    Canvas {
        cols: canvas.cols,
        rows: canvas
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|(text, fg, bg)| CanvasRun {
                        text: text.clone(),
                        color: colors.index(*fg),
                        bg: (*bg != 0).then(|| colors.index(*bg)),
                    })
                    .collect()
            })
            .collect(),
    }
}

/// The rows of `list` that its own screen holds.
fn held(list: &List) -> Vec<&Row> {
    list.rows.iter().collect()
}

/// The screen with the rows fetched past each list's first page folded in, which is what the core's key
/// entry takes: its row lookup cannot borrow them from the model.
fn loaded<M: GameModel>(model: &M, screen: &Screen) -> Screen {
    fn fold<M: GameModel>(model: &M, component: &mut Component) {
        match component {
            Component::List(list) => {
                let rows = model.rows(list).into_iter().cloned().collect();
                list.rows = rows;
            }
            Component::Tabs(t) => t.child.iter_mut().for_each(|c| fold(model, c)),
            Component::Panel(p) => p.child.iter_mut().for_each(|c| fold(model, c)),
            Component::Split(s) => s.children.iter_mut().for_each(|c| fold(model, c)),
            _ => {}
        }
    }
    let mut screen = screen.clone();
    screen.root.iter_mut().for_each(|root| fold(model, root));
    screen
}

/// The client's words for a refusal the engine gave no reason for.
fn reason_words(reason: Reason) -> String {
    match reason {
        Reason::NoScreen | Reason::Unknown | Reason::BadArgs => "Not available.",
        Reason::Disabled | Reason::RowDisabled | Reason::NotOnThisRow => "Not available now.",
        Reason::Missing(_) => "Choose something first.",
        Reason::Busy => "Busy.",
    }
    .into()
}

/// `text` in the game colour, as the native markup writes it.
fn tagged(color: Option<&str>, text: &str) -> String {
    match color {
        Some(color) => format!("<color_{color}>{text}</color>"),
        None => text.into(),
    }
}

impl<M: GameModel> App<M> {
    /// The screen on top drawn from its components when the engine describes it so, else the 0.1 menu.
    pub(super) fn menu_or_screen(&mut self, ctx: &egui::Context) {
        self.screens.interaction.sync(self.model.screens());
        let Some(top) = sliced(self.model.screens()) else {
            return self.menu(ctx);
        };
        let mut out = vec![];
        self.screen_window(ctx, top, &mut out);
        self.screens.reveal = None;
        out.into_iter().for_each(|intent| self.apply(intent));
    }

    /// The keys pressed this frame go to the interaction core, which decides what each one does.
    pub(super) fn component_keys(&mut self, ctx: &egui::Context) {
        self.keys_consumed = false;
        self.screens.interaction.sync(self.model.screens());
        let page = self.page_rows(ctx);
        let keys = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Text(t) => Some(if t == " " { "SPACE".into() } else { t.clone() }),
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => super::key_name(*key, *modifiers),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });
        // A text box has the typing; its keys are not the screen's.
        let keys = keys.into_iter().filter(|_| !ctx.text_edit_focused());
        for key in keys {
            let Some(top) = sliced(self.model.screens()) else {
                return;
            };
            let known = loaded(&self.model, top);
            let out = self.screens.interaction.press(&known, &key, page, &held);
            self.settle(out);
        }
    }

    /// Shows what the core decided and sends the call it built.
    fn settle(&mut self, out: interaction::Outcome) {
        self.keys_consumed |= out.consumed;
        self.screens.denial = out
            .denial
            .as_ref()
            .map(Text::shown)
            .or_else(|| out.reason.map(reason_words));
        if out.revealed.is_some() {
            self.screens.reveal = out.revealed;
        }
        if let Some((id, args)) = out.invoke
            && let Err(InvokeError::Disabled(denial)) = self.model.invoke_affordance(&id, args)
        {
            self.screens.denial = denial.as_ref().map(Text::shown);
        }
    }

    /// The room a list has on the screen.
    fn list_height(ctx: &egui::Context) -> f32 {
        (super::dialog_scroll_height(ctx) - DESCRIPTION_HEIGHT).max(DESCRIPTION_HEIGHT)
    }

    /// How many rows a list shows at once.
    fn page_rows(&self, ctx: &egui::Context) -> usize {
        let spacing = &ctx.global_style().spacing;
        let row = spacing.interact_size.y + spacing.item_spacing.y;
        ((Self::list_height(ctx) / row) as usize).max(1)
    }

    /// Hands what the player did to the interaction core.
    fn apply(&mut self, intent: Intent) {
        if let Intent::More(component) = &intent {
            return self.model.more_rows(component);
        }
        let interaction = &mut self.screens.interaction;
        let Some(top) = sliced(self.model.screens()) else {
            return;
        };
        let out = match intent {
            Intent::Focus { component, row } => return interaction.focus_row(&component, &row),
            Intent::Mark { component, row } => return interaction.toggle_pick(&component, &row),
            Intent::MarkCount {
                component,
                row,
                count,
            } => return interaction.set_pick_count(&component, &row, Some(count)),
            Intent::More(_) => return,
            Intent::Text(typed) => return interaction.answer_text(&typed),
            Intent::Cancel => return interaction.cancel(),
            Intent::Run(id) => interaction.activate(top, &id),
            Intent::Fill {
                affordance,
                form,
                field,
                value,
            } => interaction.fill(top, &affordance, &form, &field, &value),
            Intent::Pane(pane) => interaction.answer_pane(top, &pane),
            Intent::Confirm => interaction.confirm(top),
        };
        self.settle(out);
    }

    fn screen_window(&self, ctx: &egui::Context, screen: &Screen, out: &mut Vec<Intent>) {
        let title = markup::plain(&screen.title.as_ref().map(Text::shown).unwrap_or_default());
        let frame = super::menu::list_frame(ctx);
        let max_width = (ctx.content_rect().width() * 0.8).max(520.0);
        egui::Window::new(&title)
            .id(a11y::dialog_id(&title))
            .title_bar(!title.is_empty())
            .anchor(egui::Align2::CENTER_TOP, super::dialog_offset(ctx))
            .collapsible(false)
            .resizable(false)
            .default_width(max_width + frame.total_margin().sum().x)
            .default_height(super::dialog_scroll_height(ctx))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_min_width(520.0);
                ui.set_max_width(max_width);
                let Some(root) = &screen.root else {
                    return;
                };
                self.component(ui, screen, root, out);
                self.question(ui, screen, out);
                if let Some(denial) = &self.screens.denial {
                    ui.colored_label(BAD, markup::plain(denial));
                }
                let named = claimed(root);
                let rest = screen
                    .affordances
                    .iter()
                    .filter(|a| !named.contains(a.id.as_str()))
                    .collect::<Vec<_>>();
                self.affordance_buttons(ui, &rest, None, out);
            });
    }

    /// One line of the engine's text with its colour tags; what does not fit is cut.
    fn line(&self, ui: &egui::Ui, markup: &str) -> egui::WidgetText {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let mut job = text::job(&self.model.styled(markup), &font);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.into()
    }

    /// The game colour as a colour of the screen.
    fn tint(&self, name: &str) -> Option<egui::Color32> {
        Some(super::rgb(self.model.colors().resolve(name).0))
    }

    /// Buttons for the affordances: enabled as the affordance (and the row, when it overrides) says, with the
    /// engine's reason on hover when off.
    fn affordance_buttons(
        &self,
        ui: &mut egui::Ui,
        affordances: &[&Affordance],
        row: Option<&Row>,
        out: &mut Vec<Intent>,
    ) {
        if affordances.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for a in affordances {
                let availability = interaction::row_availability(a, row);
                let (enabled, denial) =
                    (availability.enabled, availability.denial.map(|d| d.shown()));
                let key = a
                    .keys
                    .first()
                    .map(|k| format!(" [{k}]"))
                    .unwrap_or_default();
                let label = format!("{}{key}", markup::plain(&a.label.shown()));
                let button = self.control(ui, label, &a.id, enabled, false);
                let button = match (enabled, denial) {
                    (false, Some(why)) => button.on_disabled_hover_text(why),
                    _ => button,
                };
                if button.clicked() {
                    out.push(Intent::Run(a.id.clone()));
                }
            }
        });
    }

    /// The affordances a component names that its own widgets do not run.
    fn component_buttons(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        component: &Component,
        row: Option<&Row>,
        out: &mut Vec<Intent>,
    ) {
        let own = component
            .head()
            .affordances
            .iter()
            .filter_map(|id| find(screen, id))
            .filter(|a| !driven(component, a))
            .collect::<Vec<_>>();
        self.affordance_buttons(ui, &own, row, out);
    }

    fn component(&self, ui: &mut egui::Ui, screen: &Screen, c: &Component, out: &mut Vec<Intent>) {
        if let (Some(label), false) = (&c.head().label, matches!(c, Component::Panel(_))) {
            ui.label(egui::RichText::new(markup::plain(&label.shown())).strong());
        }
        match c {
            Component::List(list) => self.list(ui, screen, list, out),
            Component::Tabs(tabs) => {
                let select = tabs.select.as_deref().and_then(|id| find(screen, id));
                ui.horizontal_wrapped(|ui| {
                    for tab in &tabs.tabs {
                        let on = tab.enabled && select.is_some_and(|a| a.enabled);
                        let label = self.line(ui, &tab.label.shown());
                        let button = self.control(ui, label, &tab.id, on, tab.selected);
                        a11y::tab(ui.ctx(), button.id, tab.selected);
                        if let (true, Some(a)) = (button.clicked(), select) {
                            out.push(Intent::Focus {
                                component: tabs.head.id.clone(),
                                row: tab.id.clone(),
                            });
                            out.push(Intent::Run(a.id.clone()));
                        }
                    }
                });
                if let Some(child) = &tabs.child {
                    self.component(ui, screen, child, out);
                }
            }
            Component::Split(split) => {
                ui.columns(split.children.len(), |columns| {
                    for (ui, child) in columns.iter_mut().zip(&split.children) {
                        let active = split.active.as_deref() == Some(child.id());
                        let stroke = if active {
                            ui.visuals().selection.stroke
                        } else {
                            egui::Stroke::NONE
                        };
                        egui::Frame::NONE
                            .stroke(stroke)
                            .inner_margin(2)
                            .show(ui, |ui| self.component(ui, screen, child, out));
                    }
                });
            }
            Component::Panel(panel) => {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new(markup::plain(&panel.title.shown())).strong());
                    if let Some(subtitle) = &panel.subtitle {
                        ui.label(self.line(ui, &subtitle.shown()));
                    }
                    for stat in panel.stats.iter().flatten() {
                        self.stat(ui, stat);
                    }
                    if let Some(filter) = &panel.filter {
                        ui.weak(format!("/{filter}"));
                    }
                    if let Some(child) = &panel.child {
                        self.component(ui, screen, child, out);
                    }
                });
            }
            Component::Form(form) => self.form(ui, screen, form, out),
            Component::Text(block) => self.text_block(ui, block),
            Component::Grid(grid) => self.grid(ui, grid, out),
            Component::Map(map) => self.map_slot(ui, map),
            Component::Canvas(canvas) => self.canvas(ui, &canvas_of(canvas, self.model.colors())),
        }
        if !matches!(c, Component::List(_)) {
            self.component_buttons(ui, screen, c, None, out);
        }
    }

    fn stat(&self, ui: &mut egui::Ui, stat: &PanelStat) {
        let max = stat.max.as_ref().map(|m| format!("/{}", m.shown()));
        let unit = stat.unit.as_ref().map(|u| format!(" {}", u.shown()));
        let shown = format!(
            "{}: {}{}{}",
            markup::plain(&stat.label.shown()),
            markup::plain(&stat.value.shown()),
            max.unwrap_or_default(),
            unit.unwrap_or_default()
        );
        let tone = stat.tone.map(|t| if t == Tone::Good { GOOD } else { BAD });
        ui.label(egui::RichText::new(shown).color(tone.unwrap_or(ui.visuals().text_color())));
    }

    fn text_block(&self, ui: &mut egui::Ui, block: &TextBlock) {
        let (font, wrap) = match block.role {
            Some(TextRole::TerminalOutput | TextRole::Banner) => (
                egui::TextStyle::Monospace.resolve(ui.style()),
                egui::TextWrapMode::Extend,
            ),
            None => (
                egui::TextStyle::Body.resolve(ui.style()),
                egui::TextWrapMode::Wrap,
            ),
        };
        let job = text::job(&self.model.styled(&block.markup.shown()), &font);
        ui.scope(|ui| {
            if let Some(tone) = block.tone {
                ui.visuals_mut().override_text_color =
                    Some(if tone == Tone::Good { GOOD } else { BAD });
            }
            ui.add(egui::Label::new(job).wrap_mode(wrap));
        });
    }

    fn form(&self, ui: &mut egui::Ui, screen: &Screen, form: &Form, out: &mut Vec<Intent>) {
        for field in &form.fields {
            let label = markup::plain(&field.label.shown());
            if let Some(description) = &field.description {
                ui.weak(markup::plain(&description.shown()));
            }
            let fill = interaction::form_fill(screen, form, &field.id);
            let on = fill.is_some_and(|a| a.enabled);
            let id = format!("{}/{}", form.head.id, field.id);
            let committed = match field.kind {
                FieldKind::Key => {
                    ui.label(&label);
                    on.then(|| editor::key_prompt(ui)).flatten()
                }
                _ => editor::value_editor(ui, &id, &label, &editor_of(field), on),
            };
            if let (Some(value), Some(a)) = (committed, fill) {
                out.push(Intent::Fill {
                    affordance: a.id.clone(),
                    form: form.head.id.clone(),
                    field: field.id.clone(),
                    value,
                });
            }
        }
    }

    fn grid(&self, ui: &mut egui::Ui, grid: &Grid, out: &mut Vec<Intent>) {
        let size = egui::vec2(GRID_CELL, GRID_CELL);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            for y in 0..grid.rows {
                ui.horizontal(|ui| {
                    for x in 0..grid.cols {
                        let Some(cell) = grid.cells.iter().find(|c| (c.x, c.y) == (x, y)) else {
                            ui.allocate_space(size);
                            continue;
                        };
                        let glyph = cell.glyph.clone().unwrap_or_else(|| " ".into());
                        let name = cell.label.as_ref().map_or(glyph.clone(), Text::shown);
                        let color = cell.color.as_deref().and_then(|c| self.tint(c));
                        let at_cursor = grid
                            .cursor
                            .is_some_and(|c| (c.x, c.y) == (x as i32, y as i32));
                        let actionable = cell.id.is_some() && cell.affordance.is_some();
                        let mut shown = egui::RichText::new(&glyph).monospace();
                        if let Some(color) = color {
                            shown = shown.color(color);
                        }
                        let marked = cell.selected == Some(true) || at_cursor;
                        if !actionable {
                            let shown = match marked {
                                true => shown.background_color(super::inventory::BAR),
                                false => shown,
                            };
                            ui.add_sized(size, egui::Label::new(shown));
                            continue;
                        }
                        let button = ui.add_enabled(
                            cell.enabled,
                            egui::Button::new(shown).min_size(size).selected(marked),
                        );
                        button.widget_info(|| {
                            WidgetInfo::labeled(
                                WidgetType::Button,
                                button.enabled(),
                                markup::plain(&name),
                            )
                        });
                        let button = match &cell.denial {
                            Some(why) if !cell.enabled => {
                                button.on_disabled_hover_text(why.shown())
                            }
                            _ => button,
                        };
                        if let (true, Some(id), Some(a)) =
                            (button.clicked(), &cell.id, &cell.affordance)
                        {
                            out.push(Intent::Focus {
                                component: grid.head.id.clone(),
                                row: id.clone(),
                            });
                            out.push(Intent::Run(a.clone()));
                        }
                    }
                });
            }
        });
    }

    /// The map is the map view's; here it is the place it would take.
    fn map_slot(&self, ui: &mut egui::Ui, map: &MapComponent) {
        let name = match (&map.head.label, map.mode) {
            (Some(label), _) => markup::plain(&label.shown()),
            (None, MapMode::World) => "Map".into(),
            (None, MapMode::Overmap) => "Overmap".into(),
        };
        let size = egui::vec2(ui.available_width().min(480.0), 120.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
        let color = ui.visuals().weak_text_color();
        ui.painter().rect_stroke(
            rect,
            0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &name,
            egui::TextStyle::Body.resolve(ui.style()),
            color,
        );
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &name));
    }

    /// The row's text with its marks: the picking mark, the engine's mark, the key, the name in its colour.
    fn row_markup(list: &List, row: &Row, picked: bool) -> String {
        let look = row.look.as_ref();
        let parts = [
            row.parent_id.is_some().then(|| "  ".to_string()),
            matches!(list.selection, Selection::Multi | Selection::Count)
                .then(|| if picked { "[x]" } else { "[ ]" }.to_string()),
            row.mark
                .as_ref()
                .map(|m| tagged(m.color.as_deref(), &m.glyph)),
            look.map(|l| tagged(Some(&l.color), &l.glyph)),
            row.favorite.filter(|&f| f).map(|_| "*".to_string()),
            row.hotkey
                .as_ref()
                .map(|k| tagged(row.key_color.as_deref(), k)),
            Some(tagged(
                row.color.as_deref(),
                &row.markup.clone().unwrap_or_else(|| row.label.shown()),
            )),
        ];
        parts.into_iter().flatten().collect::<Vec<_>>().join(" ")
    }

    fn list(&self, ui: &mut egui::Ui, screen: &Screen, list: &List, out: &mut Vec<Intent>) {
        let id = &list.head.id;
        let interaction = &self.screens.interaction;
        let rows = self.model.rows(list);
        let focus = interaction.focus();
        let cursor = focus
            .row_id
            .as_deref()
            .filter(|_| focus.component_id.as_deref() == Some(id));
        let spacing = ui.spacing().item_spacing.x;
        let width = ui.available_width();
        let columns = list.columns.len();
        let label_w = if columns == 0 {
            width
        } else {
            width * LABEL_SHARE
        };
        let column_w = ((width - label_w) / columns.max(1) as f32 - spacing).max(0.0);
        let height = ui.spacing().interact_size.y;
        if columns > 0 {
            ui.horizontal(|ui| {
                ui.allocate_space(egui::vec2(label_w, height));
                for column in &list.columns {
                    let caption = markup::plain(&column.label.shown());
                    ui.add_sized(
                        [column_w, height],
                        egui::Label::new(egui::RichText::new(caption).weak()).truncate(),
                    );
                }
            });
        }
        let mut area = egui::ScrollArea::vertical()
            .id_salt(("list", id))
            .auto_shrink([false, true])
            .max_height(Self::list_height(ui.ctx()));
        if self.screens.reveal.as_deref() == Some(id.as_str()) {
            let step = height + ui.spacing().item_spacing.y;
            area = area.vertical_scroll_offset(interaction.scroll(id) as f32 * step);
        }
        area.show_rows(ui, height, rows.len(), |ui, range| {
            for row in &rows[range.clone()] {
                ui.horizontal(|ui| {
                    if row.header == Some(true) {
                        let heading = markup::plain(&row.label.shown());
                        ui.label(egui::RichText::new(heading).strong());
                        return;
                    }
                    let pick = interaction.picks(id).iter().find(|p| p.id == row.id);
                    let text = self.line(ui, &Self::row_markup(list, row, pick.is_some()));
                    let on = row.enabled && row.selectable;
                    let highlighted =
                        cursor == Some(row.id.as_str()) || (cursor.is_none() && row.selected);
                    let layout = egui::Layout::top_down_justified(egui::Align::Min);
                    let button = ui
                        .allocate_ui_with_layout(egui::vec2(label_w, height), layout, |ui| {
                            self.control(ui, text, &row.id, on, highlighted)
                        })
                        .inner;
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
                    if button.clicked() {
                        out.push(Intent::Focus {
                            component: id.clone(),
                            row: row.id.clone(),
                        });
                        match (list.selection, &list.activate) {
                            (Selection::Multi | Selection::Count, _) => out.push(Intent::Mark {
                                component: id.clone(),
                                row: row.id.clone(),
                            }),
                            (_, Some(activate)) => out.push(Intent::Run(activate.clone())),
                            _ => {}
                        }
                    }
                    if let (Selection::Count, Some(pick), Some(max)) =
                        (list.selection, pick, row.max)
                    {
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
                    for column in &list.columns {
                        let value = row
                            .columns
                            .iter()
                            .find(|c| c.key.as_deref() == Some(column.key.as_str()))
                            .map(|c| tagged(c.color.as_deref(), &c.value.shown()))
                            .unwrap_or_default();
                        ui.add_sized(
                            [column_w, height],
                            egui::Label::new(self.line(ui, &value)).truncate(),
                        );
                    }
                });
            }
            if range.end + PREFETCH >= rows.len() {
                out.push(Intent::More(id.clone()));
            }
        });
        let shown = cursor.and_then(|c| rows.iter().find(|r| r.id == c));
        if let Some(row) = shown {
            let description = row.description.shown();
            if !description.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt(("description", id))
                    .max_height(DESCRIPTION_HEIGHT)
                    .show(ui, |ui| ui.label(self.rich(ui, &description)));
            }
            if !row.enabled && !row.denial.shown().is_empty() {
                ui.colored_label(BAD, markup::plain(&row.denial.shown()));
            }
        }
        let own = list
            .head
            .affordances
            .iter()
            .filter_map(|a| find(screen, a))
            .collect::<Vec<_>>();
        self.affordance_buttons(ui, &own, shown.copied(), out);
    }

    /// The count, text or pane the affordance being started still asks the player for.
    fn question(&self, ui: &mut egui::Ui, screen: &Screen, out: &mut Vec<Intent>) {
        let Some(ask) = self.screens.interaction.ask() else {
            return;
        };
        let Some(aff) = find(screen, &ask.affordance_id) else {
            return;
        };
        ui.group(|ui| {
            ui.label(egui::RichText::new(markup::plain(&aff.label.shown())).strong());
            if let (Some(count), Some(ArgKind::Count)) = (ask.count, ask.asking) {
                ui.label(egui::RichText::new(count.to_string()).monospace());
            }
            if let Some(select) = &ask.select {
                ui.horizontal_wrapped(|ui| {
                    for option in &select.options {
                        let label = markup::plain(&option.label.shown());
                        if ui.button(label).clicked() {
                            out.push(Intent::Text(option.id.clone()));
                            out.push(Intent::Confirm);
                        }
                    }
                });
            } else if ask.capture {
                ui.label("Press the new key");
            } else if let (Some(typed), Some(ArgKind::Text)) = (&ask.text, ask.asking) {
                let mut shown = typed.clone();
                let edit = ui.text_edit_singleline(&mut shown);
                if edit.changed() {
                    out.push(Intent::Text(shown));
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    out.push(Intent::Confirm);
                }
            }
            ui.horizontal_wrapped(|ui| {
                for pane in &ask.panes {
                    let found = screen.root.as_ref().and_then(|root| root.find(pane));
                    if let Some(Component::Panel(p)) = found
                        && ui.button(markup::plain(&p.title.shown())).clicked()
                    {
                        out.push(Intent::Pane(p.head.id.clone()));
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    out.push(Intent::Confirm);
                }
                if ui.button("Cancel").clicked() {
                    out.push(Intent::Cancel);
                }
            });
        });
    }
}
