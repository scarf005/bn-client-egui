//! The top screen of the engine's stack drawn from its components (protocol 0.2): lists, tabs, splits,
//! panels, forms, text, grids, the map and canvases, laid out the same way for every screen.
//!
//! Drawing reads the engine's data and collects what the player did as `Intent`s. Applying an intent only
//! calls the interaction core (`bn_client_core::interaction`), which decides what is sent.

mod footer;
mod form;
mod list;
mod measure;
mod overmap;
mod placement;

use super::{App, sidebar};
use crate::layout::{self, Extent};
use crate::windows::{self, Place, Shown, WindowSpec};
use crate::{a11y, editor, text};
use bn_client_core::interaction::{self, ArgKind, InvokeError, Reason};
use bn_client_core::protocol::{
    Affordance, AffordanceRole, CanvasComponent, Component, Editor, EditorValue, FieldKind, Form,
    FormField, Grid, InvokeArgs, List, MapComponent, MapMode, OvermapTile, OvermapWire, PanelStat,
    Row, Screen, ScreenDock, ScreenRole, Selection, SidebarSide, Text, TextBlock, TextRole, Tone,
    WorldPos,
};
use bn_client_core::{Cell, GameModel, Pos, Rgb, markup};
use eframe::egui::{self, WidgetInfo, WidgetType};
use footer::*;
use form::*;
use list::*;
use measure::*;
use overmap::*;
use placement::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// The margin around each pane of a split, on every side.
const PANE_MARGIN: i8 = 2;
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
    /// The height each list had in the last frame, by component id, for the rows a page key moves in it.
    list_heights: RefCell<HashMap<String, f32>>,
    /// The squares of this frame's list rows left for their item's sprite, painted once the window is drawn.
    icons: RefCell<Vec<RowIcon>>,
    /// What this frame's overmap draws, painted once the window is drawn, so that a tileset's sprites come
    /// before the marks over them.
    overmap: RefCell<Vec<OvermapPaint>>,
}

fn tone_color(tone: Tone) -> egui::Color32 {
    if tone == Tone::Good { GOOD } else { BAD }
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
    /// Runs the affordance that puts the cursor on a square.
    Move {
        affordance: String,
        at: WorldPos,
    },
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
pub(super) fn tagged(color: Option<&str>, text: &str) -> String {
    match color {
        Some(color) => format!("<color_{color}>{text}</color>"),
        None => text.into(),
    }
}

impl<M: GameModel> App<M> {
    /// Takes the stack the engine published, and moves the engine's cursor where the player's focus went.
    fn sync_screens(&mut self) {
        self.screens.interaction.sync(self.model.screens());
        if let Some((id, args)) = self.screens.interaction.take_select() {
            let _ = self.model.invoke_affordance(&id, args);
        }
    }

    /// The screen on top drawn from its components when the engine describes it so, else the 0.1 menu.
    pub(super) fn menu_or_screen(&mut self, ctx: &egui::Context) {
        self.sync_screens();
        let Some(top) = sliced(self.model.screens()) else {
            return self.menu(ctx);
        };
        let mut out = vec![];
        let cancel = interaction::with_role(top, AffordanceRole::Cancel).map(|a| a.id.clone());
        let kind = screen_kind(&markup::plain(
            &top.title.as_ref().map(Text::shown).unwrap_or_default(),
        ));
        // Only the lists drawn this frame have a page.
        self.screens.list_heights.borrow_mut().clear();
        let shown = self.screen_window(ctx, top, &mut out);
        if self.keep_window(ctx, &kind, &shown)
            && let Some(cancel) = cancel
        {
            out.push(Intent::Run(cancel));
        }
        self.screens.reveal = None;
        let season = self.model.season();
        for icon in self.screens.icons.take() {
            let display = &mut self.display;
            sidebar::paint_cell(
                &icon.painter,
                icon.rect,
                display,
                &icon.cell,
                season.as_deref(),
            );
        }
        self.paint_overmap(ctx);
        out.into_iter().for_each(|intent| self.apply(intent));
    }

    /// The keys pressed this frame go to the interaction core, which decides what each one does.
    pub(super) fn component_keys(&mut self, ctx: &egui::Context) {
        self.keys_consumed = false;
        self.sync_screens();
        let spacing = &ctx.global_style().spacing;
        let row = spacing.interact_size.y + spacing.item_spacing.y;
        let heights = self.screens.list_heights.borrow().clone();
        let page = |id: &str| heights.get(id).map_or(1, |h| ((h / row) as usize).max(1));
        let keys = super::input::frame_keys(ctx).into_iter().map(|k| k.token);
        // A text box has the typing; its keys are not the screen's, except Escape, which closes the screen.
        let typing = ctx.text_edit_focused();
        let keys = keys.filter(|key| !typing || key == "ESC");
        for key in keys {
            let Some(top) = sliced(self.model.screens()) else {
                return;
            };
            // An enabled key field takes the key as its text, and the screen does not see it; Escape still
            // closes the screen.
            if let Some(fill) = key_fill(top, &key).filter(|_| key != "ESC") {
                self.keys_consumed = true;
                return self.apply(fill);
            }
            let model = &self.model;
            let out = self
                .screens
                .interaction
                .press(top, &key, &page, &|list| model.rows(list));
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

    /// Hands what the player did to the interaction core.
    fn apply(&mut self, intent: Intent) {
        let interaction = &mut self.screens.interaction;
        let Some(top) = sliced(self.model.screens()) else {
            return;
        };
        let out = match intent {
            Intent::More(component) => return self.model.more_rows(&component),
            Intent::Focus { component, row } => return interaction.focus_row(&component, &row),
            Intent::Mark { component, row } => return interaction.toggle_pick(&component, &row),
            Intent::MarkCount {
                component,
                row,
                count,
            } => return interaction.set_pick_count(&component, &row, Some(count)),
            Intent::Text(typed) => return interaction.answer_text(&typed),
            Intent::Cancel => return interaction.cancel(),
            Intent::Run(id) => interaction.activate(top, &id),
            Intent::Move { affordance, at } => {
                let args = InvokeArgs {
                    pos: Some(at),
                    ..InvokeArgs::default()
                };
                if let Err(InvokeError::Disabled(denial)) =
                    self.model.invoke_affordance(&affordance, args)
                {
                    self.screens.denial = denial.as_ref().map(Text::shown);
                }
                return;
            }
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

    fn screen_window(
        &self,
        ctx: &egui::Context,
        screen: &Screen,
        out: &mut Vec<Intent>,
    ) -> Shown<()> {
        let title = markup::plain(&screen.title.as_ref().map(Text::shown).unwrap_or_default());
        let frame = super::menu::list_frame(ctx);
        let region = self.region.unwrap_or_else(|| ctx.content_rect());
        // The squares to keep clear: the engine's cursor and the avatar, a square around each.
        let avoid = self.map_view.map_or(vec![], |(origin, cell)| {
            let square = |at: Pos| {
                egui::Rect::from_min_size(
                    origin + egui::vec2(at.x as f32, at.y as f32) * cell,
                    egui::Vec2::splat(cell),
                )
                .expand(KEEP_CLEAR * cell)
            };
            self.model
                .focus()
                .into_iter()
                .chain([self.model.player()])
                .map(square)
                .collect()
        });
        let place = placement(screen, region, self.sidebar, &avoid);
        // A window that is not the whole room is as wide as its content, within the room.
        let width = match (place, &screen.root) {
            (Place::Fill(_), _) | (_, None) => None,
            (_, Some(root)) => {
                let builder = egui::UiBuilder::new().max_rect(region);
                let ui = egui::Ui::new(ctx.clone(), egui::Id::new("measure"), builder);
                let content = self.extent(&ui, root, region.width()).natural;
                Some(content + frame.total_margin().sum().x + ui.spacing().window_margin.sum().x)
            }
        };
        let base = self.window_spec(ctx, &screen_kind(&title), &title);
        // The help sits in the title bar, or last in the footer of a window that has none.
        let titled = interaction::with_role(screen, AffordanceRole::Help)
            .filter(|a| a.enabled && base.title_bar);
        let title_bar = base.title_bar;
        let spec = WindowSpec {
            closable: interaction::with_role(screen, AffordanceRole::Cancel).is_some(),
            frame: Some(frame),
            place: Some(place),
            movable: !keeps_off_cursor(screen),
            width,
            help: titled.map(|a| windows::TitleHelp {
                label: markup::plain(&a.label.shown()),
                tip: format!("{}{}", markup::plain(&a.label.shown()), key_hint(a)),
            }),
            ..base
        };
        let shown = windows::show(ctx, spec, |ui| {
            let Some(root) = &screen.root else {
                return;
            };
            let named = claimed(root);
            let rest = screen
                .affordances
                .iter()
                .filter(|a| !named.contains(a.id.as_str()))
                .collect::<Vec<_>>();
            let footer = |ui: &mut egui::Ui, out: &mut Vec<Intent>| {
                self.question(ui, screen, out);
                if let Some(denial) = &self.screens.denial {
                    ui.colored_label(BAD, markup::plain(denial));
                }
                self.footer(ui, &rest, title_bar, out);
            };
            // A screen that fills the room keeps its footer under its body, whatever the body's height.
            if matches!(place, Place::Fill(_)) {
                egui::Panel::bottom(egui::Id::new(("footer", &screen.screen_id)))
                    .resizable(false)
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| footer(ui, out));
                egui::CentralPanel::no_frame().show(ui, |ui| self.component(ui, screen, root, out));
            } else {
                self.component(ui, screen, root, out);
                footer(ui, out);
            }
        });
        if let Some(a) = titled.filter(|_| shown.help) {
            out.push(Intent::Run(a.id.clone()));
        }
        shown
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
    fn tint(&self, name: &str) -> egui::Color32 {
        super::rgb(self.model.colors().resolve(name).0)
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
                        let marked = format!(
                            "{}{}",
                            tab.label.shown(),
                            if tab.unread { " •" } else { "" }
                        );
                        let label = self.line(ui, &marked);
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
                // A banner is the screen's heading: it sits above the parts, whatever their number.
                let (banners, parts) = split.children.iter().partition::<Vec<_>, _>(
                    |c| matches!(c, Component::Text(t) if t.role == Some(TextRole::Banner)),
                );
                for banner in banners {
                    self.component(ui, screen, banner, out);
                }
                self.split(
                    ui,
                    screen,
                    &split.head.id,
                    &parts,
                    split.active.as_deref(),
                    out,
                );
            }
            Component::Panel(panel) => {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    // A panel the screen's own title already names has none.
                    let title = markup::plain(&panel.title.shown());
                    if !title.is_empty() {
                        ui.label(egui::RichText::new(title).strong());
                    }
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
            Component::Map(map) => self.map_slot(ui, screen, map, out),
            Component::Canvas(canvas) => self.canvas(ui, canvas),
        }
        if !matches!(c, Component::List(_)) {
            self.component_buttons(ui, screen, c, None, out);
        }
    }

    /// The panes side by side, each as wide as its content needs (`layout::split_widths`); when even the
    /// minimums do not fit, the split scrolls sideways instead of breaking words.
    fn split(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        id: &str,
        parts: &[&Component],
        active: Option<&str>,
        out: &mut Vec<Intent>,
    ) {
        let gap = ui.spacing().item_spacing.x;
        let room = ui.available_width();
        let extents = parts
            .iter()
            .map(|c| with_margin(self.extent(ui, c, room)))
            .collect::<Vec<_>>();
        let widths = layout::split_widths(&extents, room, gap);
        let mut panes = |ui: &mut egui::Ui| {
            ui.horizontal_top(|ui| {
                for (child, width) in parts.iter().zip(&widths) {
                    let stroke = if active == Some(child.id()) {
                        ui.visuals().selection.stroke
                    } else {
                        egui::Stroke::NONE
                    };
                    let size = egui::vec2(*width, ui.available_height());
                    let down = egui::Layout::top_down(egui::Align::Min);
                    // The active pane's stroke is inside its width too, or each frame it widens the
                    // window that the next frame's room comes from.
                    let frame = egui::Frame::NONE.stroke(stroke).inner_margin(PANE_MARGIN);
                    ui.allocate_ui_with_layout(size, down, |ui| {
                        frame.show(ui, |ui| {
                            ui.set_width(width - frame.total_margin().sum().x);
                            self.component(ui, screen, child, out)
                        });
                    });
                }
            });
        };
        match layout::overflows(&widths, room, gap) {
            true => {
                egui::ScrollArea::horizontal()
                    .id_salt(("split", id))
                    .auto_shrink([false, true])
                    .show(ui, panes);
            }
            false => panes(ui),
        }
    }

    fn stat(&self, ui: &mut egui::Ui, stat: &PanelStat) {
        let shown = stat_text(stat);
        let label = markup::plain(&stat.label.shown());
        let color = stat.tone.map_or(ui.visuals().text_color(), tone_color);
        if shown.is_empty() {
            ui.label(egui::RichText::new(label).color(color));
            return;
        }
        // The label and the value are parts of their own; the engine's text carries any punctuation.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(label);
            ui.label(egui::RichText::new(shown).color(color));
        });
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
        let show = |ui: &mut egui::Ui| {
            ui.scope(|ui| {
                if let Some(tone) = block.tone {
                    ui.visuals_mut().override_text_color = Some(tone_color(tone));
                }
                ui.add(egui::Label::new(job).wrap_mode(wrap));
            });
        };
        // A banner is the screen's heading and keeps its height; any other text scrolls past a window's.
        if block.role == Some(TextRole::Banner) {
            show(ui);
        } else {
            egui::ScrollArea::vertical()
                .max_height(ui.available_height())
                .auto_shrink(true)
                .show(ui, show);
        }
    }

    /// The engine's own window, a row of colored cells at a time, in a fixed-width font as large as
    /// lets the whole grid fit the window, so every cell keeps its column and row at any size.
    fn canvas(&self, ui: &mut egui::Ui, canvas: &CanvasComponent) {
        // The room is what the dialog offers its list; it does not depend on the dialog's own size,
        // which follows the canvas.
        // A row's height rounds up to a whole pixel, which a row of slack each pays for.
        let room = egui::vec2(
            ui.ctx().content_rect().width() * 0.9,
            ui.available_height() - canvas.rows.len() as f32,
        );
        let unit = 100.0;
        let cell = ui
            .painter()
            .layout_no_wrap(
                "0".into(),
                egui::FontId::monospace(unit),
                egui::Color32::WHITE,
            )
            .size()
            / unit;
        let fit = (room.x / (canvas.cols.max(1) as f32 * cell.x))
            .min(room.y / (canvas.rows.len().max(1) as f32 * cell.y));
        let font = egui::FontId::monospace(fit.clamp(10.0, 40.0));
        let colors = self.model.colors();
        let paint = |index: u8| {
            let c = colors.index(index);
            egui::Color32::from_rgb(c.0, c.1, c.2)
        };
        // The rows touch; what follows the canvas keeps the theme's spacing.
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in &canvas.rows {
                let mut job = egui::text::LayoutJob::default();
                for (text, fg, bg) in row {
                    job.append(
                        text,
                        0.0,
                        egui::TextFormat {
                            color: paint(*fg),
                            background: if *bg == 0 {
                                egui::Color32::TRANSPARENT
                            } else {
                                paint(*bg)
                            },
                            font_id: font.clone(),
                            ..Default::default()
                        },
                    );
                }
                // An empty row keeps its height.
                if row.is_empty() {
                    job.append(
                        " ",
                        0.0,
                        egui::TextFormat::simple(font.clone(), egui::Color32::BLACK),
                    );
                }
                ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
            }
        });
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
                        let color = cell.color.as_deref().map(|c| self.tint(c));
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
                                true => shown.background_color(super::menu::BAR),
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
}
