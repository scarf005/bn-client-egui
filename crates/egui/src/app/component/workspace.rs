//! A `workspace` screen laid out as an editor: a menu bar from the affordances' groups, the tools on the left, the
//! canvas in the middle with the log under it (on the left without tools), the palette, outline and inspector on the
//! right and the status line at the bottom, each part by the slot the engine gave it.

use super::*;
use bn_client_core::localise::tr;

/// The width the tools' column, the log's column and the right column first have, which the player can change.
const TOOLS_WIDTH: f32 = 200.0;
const LOG_WIDTH: f32 = 240.0;
/// The right column's width beside a canvas without a grid: this share of the window, within these bounds; beside
/// a grid, the room its whole grid leaves, from the least of these bounds to half the room.
const SIDE_SHARE: f32 = 0.22;
const SIDE_WIDTH: (f32, f32) = (360.0, 480.0);
/// The least of the right column beside a grid, where its palette's tabs and the inspector's actions need the width
/// to take fewer lines, which gives the rows under them the height.
const GRID_SIDE_MIN: f32 = 440.0;
/// The fewest rows a list of a column is fitted to, so that the column holds as many of its parts as it can.
const FIT_ROWS: f32 = 8.0;
/// The rows of a list a section of a column is sure to show of its first part: lists shrink to these before the
/// heads of the other sections leave the view.
const HEAD_ROWS: f32 = 3.0;
/// The rows of the list worked in, the first with a search box over it, a section of a column keeps in view.
const WORKED_ROWS: f32 = 5.0;
/// The rows of its first part a section of a column keeps in view whatever the others hold.
const REACH_ROWS: f32 = 1.0;
/// The height the log's strip first has, which the player can change.
const LOG_HEIGHT: f32 = 160.0;

/// The height the top of a column takes, and how many lists it holds.
#[derive(Clone, Copy, Default)]
struct Head {
    height: f32,
    lists: usize,
}

/// Three tops of a column: its first part, through its first part that holds rows to pick from, and through its
/// first part that holds rows with a search box over them, which is the one worked in.
#[derive(Clone, Copy, Default)]
struct First {
    part: Head,
    listed: Option<Head>,
    searched: Option<Head>,
}

/// How tall a column is with every row of its lists, and how tall each of its tops is with its lists at their
/// fewest rows; a top it has none of is not as tall.
#[derive(Clone, Copy)]
struct ColumnHeight {
    whole: f32,
    part: f32,
    listed: f32,
    searched: Option<f32>,
}

impl ColumnHeight {
    /// A column not drawn yet.
    const UNKNOWN: Self = Self {
        whole: f32::INFINITY,
        part: 0.0,
        listed: 0.0,
        searched: None,
    };
}

/// The parts of `root` in `slot`; a part with none is the inspector.
fn parts(root: &Split, slot: Slot) -> Vec<&Component> {
    let slotted = |c: &&Component| c.head().slot.unwrap_or(Slot::Inspector) == slot;
    root.children.iter().filter(slotted).collect()
}

/// The right column's width in the room `ui` has left: what the canvas's whole grid leaves at the cell size its
/// height gives, else a share of the window.
fn side_width(ui: &egui::Ui, root: &Split) -> f32 {
    let room = ui.available_rect_before_wrap();
    let grid = parts(root, Slot::Canvas)
        .into_iter()
        .flat_map(Component::nested)
        .find_map(|c| match c {
            Component::Grid(grid) => Some(grid),
            _ => None,
        });
    let Some(grid) = grid else {
        return (ui.ctx().content_rect().width() * SIDE_SHARE).clamp(SIDE_WIDTH.0, SIDE_WIDTH.1);
    };
    let (xs, ys) = drawn_span(grid);
    // The canvas shares its height with a row of tools above it.
    let height = room.height() - 2.0 * ui.spacing().interact_size.y;
    let cell = (height / ys.len().max(1) as f32).clamp(GRID_CELL, GRID_MAX_CELL);
    let gap = 2.0 * ui.spacing().item_spacing.x;
    (room.width() - xs.len() as f32 * cell - gap)
        .clamp(GRID_SIDE_MIN, (room.width() / 2.0).max(GRID_SIDE_MIN))
}

/// The heights of `room` the lists that want `wanted` get: each what it wants up to an even share of what the
/// others leave, the smallest served first, and up to `least` however little is left.
fn shares(room: f32, wanted: &[f32], least: f32) -> Vec<f32> {
    let mut order = (0..wanted.len()).collect::<Vec<_>>();
    order.sort_by(|a, b| wanted[*a].total_cmp(&wanted[*b]));
    let mut out = vec![0.0; wanted.len()];
    let mut left = room.max(0.0);
    for (served, i) in order.into_iter().enumerate() {
        out[i] = wanted[i].min((left / (wanted.len() - served) as f32).max(least));
        left -= out[i];
    }
    out
}

/// Keeps the panel `id` `width` wide as that changes, until the player resizes it: then it keeps the player's.
fn follow(ctx: &egui::Context, id: egui::Id, width: f32) {
    use egui::containers::panel::PanelState;
    let (asked, player) = (id.with("asked width"), id.with("resized by the player"));
    if ctx.data(|d| d.get_temp::<bool>(player)).unwrap_or_default() {
        return;
    }
    let Some(state) = PanelState::load(ctx, id) else {
        ctx.data_mut(|d| d.insert_temp(asked, width));
        return;
    };
    let shown = state.size().x;
    match ctx.data(|d| d.get_temp::<f32>(asked)) {
        Some(was) if (shown - was).abs() > 0.5 => {
            ctx.data_mut(|d| d.insert_temp(player, true));
        }
        _ => {
            let outer_rect = state.outer_rect.with_min_x(state.outer_rect.max.x - width);
            ctx.data_mut(|d| {
                d.insert_persisted(id, PanelState { outer_rect });
                d.insert_temp(asked, width);
            });
        }
    }
}

/// The groups of `screen`'s affordances, in the order they first appear.
fn groups(screen: &Screen) -> Vec<String> {
    screen
        .affordances
        .iter()
        .filter_map(|a| a.group.as_ref().map(Text::shown))
        .fold(vec![], |mut groups, group| {
            if !groups.contains(&group) {
                groups.push(group);
            }
            groups
        })
}

/// The split of a workspace screen that has a canvas part: the layout needs one, and any other is laid out as a
/// screen.
pub(super) fn laid_out(screen: &Screen) -> Option<&Split> {
    match (screen.role, &screen.root) {
        (ScreenRole::Workspace, Some(Component::Split(root)))
            if !parts(root, Slot::Canvas).is_empty() =>
        {
            Some(root)
        }
        _ => None,
    }
}

impl<M: GameModel> App<M> {
    /// One menu per group of the screen's affordances, in the order the groups first appear, in a bar over its
    /// body. A workspace's are in the top bar instead (`workspace_bar`).
    pub(super) fn menu_bar(&self, ui: &mut egui::Ui, screen: &Screen, out: &mut Vec<Intent>) {
        if groups(screen).is_empty() || laid_out(screen).is_some() {
            return;
        }
        egui::Panel::top(egui::Id::new(("menu bar", &screen.screen_id))).show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| self.menus(ui, screen, out));
        });
    }

    /// The top bar while a workspace is the top screen: its menus, the client's buttons and status, and its title,
    /// help and leave at the end, which a window has in its title bar. False when no workspace is up.
    pub(in crate::app) fn workspace_bar(&mut self, ui: &mut egui::Ui) -> bool {
        // A workspace under the top screen keeps its bar: its title, and its menus, which it has none of there.
        let screens = self.model.screens();
        let Some(screen) = screens
            .iter()
            .rev()
            .find(|s| laid_out(s).is_some())
            .cloned()
        else {
            return false;
        };
        let title = title(&screen);
        // Under another screen its menus and ends are drawn off: what they run, and the keys an open menu holds,
        // would go to the screen on top.
        let top = screens.last().map(|s| &s.screen_id) == Some(&screen.screen_id);
        let mut out = vec![];
        egui::MenuBar::new().ui(ui, |ui| {
            ui.add_enabled_ui(top, |ui| self.menus(ui, &screen, &mut out));
            ui.separator();
            self.client_buttons(ui);
            let ends = [AffordanceRole::Cancel, AffordanceRole::Help]
                .into_iter()
                .filter_map(|role| interaction::with_role(&screen, role));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(top, |ui| {
                    ends.for_each(|a| self.affordance_button(ui, a, None, &mut out))
                });
                ui.label(egui::RichText::new(title).strong());
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    self.status(ui)
                });
            });
        });
        out.into_iter().for_each(|intent| self.apply(intent));
        true
    }

    /// The menus of `screen`'s groups.
    fn menus(&self, ui: &mut egui::Ui, screen: &Screen, out: &mut Vec<Intent>) {
        for group in &groups(screen) {
            ui.menu_button(markup::plain(group), |ui| {
                let items = screen
                    .affordances
                    .iter()
                    .filter(|a| a.group.as_ref().map(Text::shown).as_ref() == Some(group));
                for a in items {
                    let availability = interaction::row_availability(a, None);
                    let mut item = egui::Button::new(markup::plain(&a.label.shown()));
                    if let Some(key) = a.keys.first() {
                        item = item.shortcut_text(key);
                    }
                    // An item that is off looks off, yet takes a click and Return, which ask all the same for the
                    // core to show the engine's reason where a refusal shows.
                    let button = ui
                        .scope(|ui| {
                            if !availability.enabled {
                                ui.multiply_opacity(ui.visuals().disabled_alpha());
                            }
                            ui.add(item)
                        })
                        .inner;
                    if !availability.enabled {
                        ui.ctx()
                            .accesskit_node_builder(button.id, |b| b.set_disabled());
                    }
                    let button = described(ui.ctx(), button, a);
                    let button = match (availability.enabled, availability.denial) {
                        (false, Some(why)) => button.on_hover_text(why.shown()),
                        _ => button,
                    };
                    if button.clicked() {
                        out.push(Intent::Run(a.id.clone()));
                        ui.close();
                    }
                }
            });
        }
    }

    /// The parts of `root` by slot, around the canvas that takes the room left, and the screen's `footer` buttons
    /// at the end of the status line.
    pub(super) fn workspace(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        (root, footer): (&Split, &[&Affordance]),
        out: &mut Vec<Intent>,
    ) {
        let id = |name: &str| egui::Id::new((name, &screen.screen_id));
        // The columns' ids are strings, which `label_resize_handle` names the drag handles by.
        let (tools_id, side_id, log_id) = (
            format!("workspace-tools {}", screen.screen_id),
            format!("workspace-side {}", screen.screen_id),
            format!("workspace-log {}", screen.screen_id),
        );
        let status = parts(root, Slot::Status);
        // The engine's news, a toned text of the log, is shown where the player acted: under the form they sent,
        // else by the status line.
        let (news, log) = parts(root, Slot::Log)
            .into_iter()
            .partition::<Vec<_>, _>(|c| matches!(c, Component::Text(t) if t.tone.is_some()));
        // What a described row already says is not said again.
        let said = self.said_by_rows(screen);
        let news = news
            .into_iter()
            .filter_map(|c| match c {
                Component::Text(t) => Some(t.clone()),
                _ => None,
            })
            .filter(|t| !said.contains(&t.markup.shown()))
            .collect::<Vec<_>>();
        let sent = self
            .screens
            .sent_form
            .as_ref()
            .filter(|(at, form)| {
                at == &screen.screen_id && screen.root.as_ref().and_then(|r| r.find(form)).is_some()
            })
            .map(|(_, form)| form.clone());
        let strip_news = match (&sent, news.is_empty()) {
            (Some(form), false) => {
                self.screens.news.replace(Some((form.clone(), news)));
                vec![]
            }
            _ => {
                self.screens.news.replace(None);
                news
            }
        };
        let footer_width = Self::footer_width(ui, footer, true);
        // The question and the refusal are the top screen's, which a workspace under it does not show.
        let under = self.screens.under.get();
        let asking =
            !under && (self.screens.interaction.ask().is_some() || self.screens.denial.is_some());
        // No strip at the bottom when it would be empty.
        if !status.is_empty() || footer_width > 0.0 || asking || !strip_news.is_empty() {
            egui::Panel::bottom(id("status"))
                .resizable(false)
                .show(ui, |ui| {
                    // The news line is kept when there is none, so the canvas does not move as the news comes and goes.
                    let before = ui.cursor().top();
                    strip_news.iter().for_each(|t| self.text_block(ui, t));
                    let line =
                        ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
                    ui.add_space((line - (ui.cursor().top() - before)).max(0.0));
                    if !under {
                        self.question(ui, screen, out);
                        if let Some(denial) = &self.screens.denial {
                            ui.colored_label(BAD, markup::plain(denial));
                        }
                    }
                    // The buttons end the line, at most half of it; the facts wrap in what they leave.
                    let line = ui.available_rect_before_wrap();
                    let gap = ui.spacing().item_spacing.x;
                    let split = line.right() - footer_width.min(line.width() / 2.0);
                    // Beside buttons, the facts' first line sits level with their words.
                    let lift = match footer_width > 0.0 {
                        true => ui.spacing().button_padding.y,
                        false => 0.0,
                    };
                    let mut facts = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(line.with_max_x(split - gap).with_min_y(line.top() + lift)),
                    );
                    status
                        .iter()
                        .for_each(|c| self.status_line(&mut facts, screen, c, out));
                    let mut buttons =
                        ui.new_child(egui::UiBuilder::new().max_rect(line.with_min_x(split)));
                    self.footer(&mut buttons, footer, true, out);
                    ui.advance_cursor_after_rect(facts.min_rect().union(buttons.min_rect()));
                });
        }
        // Without tools the log takes the left column, and the canvas the whole height.
        let tools = parts(root, Slot::Tools);
        let left_is_log = tools.is_empty();
        let (left, left_width, log) = match left_is_log {
            true => (log, LOG_WIDTH, vec![]),
            false => (tools, TOOLS_WIDTH, log),
        };
        if !left.is_empty() {
            egui::Panel::left(egui::Id::new(&tools_id))
                .default_size(left_width)
                .show(ui, |ui| match left_is_log {
                    true => self.log_column(ui, screen, &left, out),
                    false => {
                        self.column(ui, screen, (&left, FIT_ROWS), out);
                    }
                });
            crate::app::label_resize_handle(ui.ctx(), &tools_id, &tr("Resize workspace column"));
        }
        let (palette, outline, inspector) = (
            parts(root, Slot::Palette),
            parts(root, Slot::Outline),
            parts(root, Slot::Inspector),
        );
        // The right column and the log under the canvas take their room now and are filled after the canvas, so
        // that the keyboard walks the canvas before them, as they are read.
        let lists = !palette.is_empty() || !outline.is_empty();
        let side = (lists || !inspector.is_empty()).then(|| {
            let width = side_width(ui, root);
            follow(ui.ctx(), egui::Id::new(&side_id), width);
            let room = egui::Panel::right(egui::Id::new(&side_id))
                .default_size(width)
                .show(ui, reserve)
                .inner;
            crate::app::label_resize_handle(ui.ctx(), &side_id, &tr("Resize workspace column"));
            room
        });
        let below = (!log.is_empty()).then(|| {
            let room = egui::Panel::bottom(egui::Id::new(&log_id))
                .default_size(LOG_HEIGHT)
                .show(ui, reserve)
                .inner;
            crate::app::label_resize_handle(ui.ctx(), &log_id, &tr("Resize workspace log"));
            room
        });
        // The canvas keeps clear of the columns' edges, so a press there grabs the edge, not a square.
        let grab = ui.style().interaction.resize_grab_radius_side.ceil() as i8;
        let clear = egui::Frame::NONE.inner_margin(egui::Margin::same(grab));
        egui::CentralPanel::no_frame().frame(clear).show(ui, |ui| {
            parts(root, Slot::Canvas)
                .into_iter()
                .for_each(|c| self.component(ui, screen, c, out));
        });
        self.canvas_stop(ui, screen, root, out);
        let mut filled = |room: egui::Rect, name: &str| {
            let mut column = ui.new_child(egui::UiBuilder::new().id_salt(name).max_rect(room));
            column.set_clip_rect(room.intersect(ui.clip_rect()));
            column
        };
        if let Some(room) = below {
            self.log_column(&mut filled(room, &log_id), screen, &log, out);
        }
        if let Some(room) = side {
            let ui = &mut filled(room, &side_id);
            // Room for two columns, each as wide as the right column least is beside a grid, puts the inspector
            // beside the palette and the outline, both in view.
            match ui.available_width() >= 2.0 * GRID_SIDE_MIN && lists && !inspector.is_empty() {
                true => ui.columns(2, |halves| {
                    // The halves are told apart by name: a column's own id is its parent's.
                    halves[0].push_id("lists", |ui| {
                        self.sections(ui, screen, &[&palette, &outline], out)
                    });
                    halves[1].push_id("inspector", |ui| {
                        self.sections(ui, screen, &[&inspector], out)
                    });
                }),
                false => self.sections(ui, screen, &[&palette, &outline, &inspector], out),
            }
        }
    }

    /// A status part on one line: a panel's title and its facts side by side; any other part as it is drawn
    /// elsewhere.
    fn status_line(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        c: &Component,
        out: &mut Vec<Intent>,
    ) {
        let Component::Panel(panel) = c else {
            return self.component(ui, screen, c, out);
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(markup::plain(&panel.title.shown())).strong());
            let font = egui::TextStyle::Body.resolve(ui.style());
            let (gap, right) = (ui.spacing().item_spacing.x, ui.max_rect().right());
            for stat in panel.stats.iter().flatten() {
                let (label, shown) = (markup::plain(&stat.label.shown()), stat_text(stat));
                // A fact wraps to the next line whole, its value beside its label, before it would run past the
                // line's end.
                let width = text_width(ui, &label, &font) + gap + text_width(ui, &shown, &font);
                match right - ui.cursor().left() < width + 3.0 * gap {
                    true => ui.end_row(),
                    false => {
                        ui.separator();
                    }
                }
                ui.horizontal(|ui| {
                    ui.weak(label);
                    if !shown.is_empty() {
                        let color = stat.tone.map_or(ui.visuals().text_color(), tone_color);
                        ui.label(egui::RichText::new(shown).color(color));
                    }
                });
            }
        });
        if let Some(child) = &panel.child {
            self.component(ui, screen, child, out);
        }
    }

    /// The canvas as one stop of the keyboard's walk, where it was drawn the frame before: while it has the
    /// keyboard, the arrows and Return are its squares' (the interaction core's focus is kept on it, at the
    /// engine's cursor until the arrows move it).
    fn canvas_stop(&self, ui: &mut egui::Ui, screen: &Screen, root: &Split, out: &mut Vec<Intent>) {
        let grid = parts(root, Slot::Canvas)
            .into_iter()
            .flat_map(Component::nested)
            .find_map(|c| match c {
                Component::Grid(grid) if grid.role == Some(GridRole::Canvas) => Some(grid),
                _ => None,
            });
        let Some(grid) = grid else {
            return;
        };
        let view_key = egui::Id::new(("grid view", &screen.screen_id, &grid.head.id));
        let Some(view) = ui.data(|d| d.get_temp::<canvas::View>(view_key)) else {
            return;
        };
        let id = view_key.with("keyboard");
        let stop = ui.interact(view.rect, id, egui::Sense::FOCUSABLE);
        stop.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, tr("Canvas")));
        if !stop.has_focus() {
            return;
        }
        let arrows = egui::EventFilter {
            horizontal_arrows: true,
            vertical_arrows: true,
            ..Default::default()
        };
        ui.memory_mut(|m| m.set_focus_lock_filter(id, arrows));
        let focus = self.screens.interaction.focus();
        if focus.component_id.as_deref() == Some(&grid.head.id) {
            return;
        }
        let at = |c: &&bn_client_core::protocol::GridCell| {
            grid.cursor
                .is_some_and(|at| (at.x, at.y) == (c.x as i32, c.y as i32))
        };
        let cells = grid.cells.iter().filter(|c| c.id.is_some());
        let first = cells.clone().find(at).or(cells.clone().next());
        if let Some(row) = first.and_then(|c| c.id.clone()) {
            out.push(Intent::Focus {
                component: grid.head.id.clone(),
                row,
            });
        }
    }

    /// The log's `parts`: its words, the engine's latest news, over the column of the rest, so that no history
    /// however long scrolls them out of view.
    fn log_column(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        parts: &[&Component],
        out: &mut Vec<Intent>,
    ) {
        let (words, rest): (Vec<&Component>, Vec<&Component>) =
            parts.iter().partition(|c| matches!(c, Component::Text(_)));
        words
            .iter()
            .for_each(|c| self.component(ui, screen, c, out));
        // A log's list takes the room the log has, however little: the log does not scroll it out of view.
        self.column(ui, screen, (&rest, 1.0), out);
    }

    /// `parts` one above the other in a column that scrolls, whole lines in view, each list at least `fit_rows`
    /// rows tall. The column is as wide as the player left it: what is wider is cut at its edge rather than widen
    /// the column a little more each frame.
    fn column(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        (parts, fit_rows): (&[&Component], f32),
        out: &mut Vec<Intent>,
    ) -> ColumnHeight {
        let rect = ui.available_rect_before_wrap();
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        inner.set_clip_rect(rect.intersect(ui.clip_rect()));
        let scroll = egui::ScrollArea::vertical()
            .id_salt(ui.id().with("column"))
            .auto_shrink([false, false]);
        let room = rect.height();
        // The lists share what the other parts leave of the column, as they took it the frame before, each at
        // least `fit_rows` rows; until then a list is at most `PEER_ROWS` rows tall, so the parts under a long one
        // stay in reach.
        let key = ui.id().with("lists");
        let (content, fits, _) = ui
            .data(|d| d.get_temp::<(f32, Vec<ListFit>, First)>(key))
            .unwrap_or_default();
        let left = room - (content - fits.iter().map(|f| f.taken).sum::<f32>());
        let least = fit_rows * (tokens::ROW + ui.spacing().item_spacing.y);
        let wanted = fits.iter().map(|f| f.wanted).collect::<Vec<_>>();
        let ids = fits.iter().map(|f| f.id.clone());
        let caps = self
            .screens
            .column_caps
            .replace(ids.zip(shares(left, &wanted, least)).collect());
        let lists = self.screens.column_lists.take();
        body_scroll(&mut inner, scroll, room, |ui| {
            // A column's lists are palettes, read by their names, as tall as their rows wherever they are.
            let was = self.screens.in_column.replace(true);
            let top = ui.cursor().top();
            let first = self.with_list_rows(Some(PEER_ROWS), || self.stack(ui, screen, parts, out));
            self.screens.in_column.set(was);
            let content = ui.min_rect().bottom() - top;
            let fits = self.screens.column_lists.take();
            ui.data_mut(|d| d.insert_temp(key, (content, fits, first)));
        });
        self.screens.column_caps.replace(caps);
        self.screens.column_lists.replace(lists);
        ui.allocate_rect(rect, egui::Sense::hover());
        let (content, fits, first) = ui
            .data(|d| d.get_temp::<(f32, Vec<ListFit>, First)>(key))
            .unwrap_or_default();
        // As tall as its parts are with every row of their lists, and as its first part is with its lists at their
        // fewest rows.
        let fewest = HEAD_ROWS * (tokens::ROW + ui.spacing().item_spacing.y);
        let worked = WORKED_ROWS * (tokens::ROW + ui.spacing().item_spacing.y);
        let shrunk = |rows: f32| move |f: &ListFit| f.wanted.min(rows) - f.taken;
        let head_to = |h: Head, rows: f32| {
            h.height + fits.iter().take(h.lists).map(shrunk(rows)).sum::<f32>()
        };
        let head = |h: Head| head_to(h, fewest);
        ColumnHeight {
            whole: content + fits.iter().map(|f| f.wanted - f.taken).sum::<f32>(),
            part: head(first.part),
            listed: first.listed.map_or(0.0, head),
            searched: first.searched.map(|h| head_to(h, worked)),
        }
    }

    /// `groups` one above the other in the room `ui` has, each a column of its own that scrolls. Each first gets
    /// the height that keeps its first part reachable, then its top through its first rows to pick from and, for
    /// the first list with a search box of them all, through that list; then what its parts want up to an even
    /// share of what the others leave.
    fn sections(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        groups: &[&Vec<&Component>],
        out: &mut Vec<Intent>,
    ) {
        let groups = groups.iter().filter(|g| !g.is_empty()).collect::<Vec<_>>();
        let rect = ui.available_rect_before_wrap();
        let gap = ui.spacing().item_spacing.y;
        let room = rect.height() - gap * groups.len().saturating_sub(1) as f32;
        let key = ui.id().with("sections");
        // Until a section is drawn, what it wants is not known: they share the room evenly.
        let heights = ui
            .data(|d| d.get_temp::<Vec<ColumnHeight>>(key))
            .filter(|h| h.len() == groups.len())
            .unwrap_or_else(|| vec![ColumnHeight::UNKNOWN; groups.len()]);
        // Each section is given, in turn, evenly up to what the sections want of each step: what keeps its head
        // reachable; its top through the first rows to pick from, and through `WORKED_ROWS` rows of the first list with
        // a search box of them all; then what the parts of the others want; then what its parts want.
        let reach = REACH_ROWS * (tokens::ROW + gap);
        let worked = heights.iter().position(|h| h.searched.is_some());
        let ends = [
            heights
                .iter()
                .map(|h| h.part.min(reach))
                .collect::<Vec<_>>(),
            heights
                .iter()
                .enumerate()
                .map(|(i, h)| h.searched.filter(|_| Some(i) == worked).unwrap_or(h.listed))
                .collect(),
            // What the parts under the worked-in list want, its form fields among them, before it gets more rows.
            heights
                .iter()
                .enumerate()
                .map(|(i, h)| match Some(i) == worked {
                    true => h.searched.unwrap_or(h.listed),
                    false => h.whole,
                })
                .collect(),
            heights.iter().map(|h| h.whole).collect(),
        ];
        let mut given = vec![0.0; heights.len()];
        for end in ends {
            let wanted = end
                .iter()
                .zip(&given)
                .map(|(e, g)| (e - g).max(0.0))
                .collect::<Vec<_>>();
            let left = room - given.iter().sum::<f32>();
            given
                .iter_mut()
                .zip(shares(left, &wanted, 0.0))
                .for_each(|(g, s)| *g += s);
        }
        let mut top = rect.top();
        let mut drawn = vec![];
        for (i, (group, height)) in groups.iter().zip(given).enumerate() {
            let slice = egui::Rect::from_min_size(
                egui::pos2(rect.left(), top),
                egui::vec2(rect.width(), height),
            );
            let mut section = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("section", i))
                    .max_rect(slice),
            );
            drawn.push(self.column(&mut section, screen, (group, FIT_ROWS), out));
            top += height + gap;
        }
        ui.data_mut(|d| d.insert_temp(key, drawn));
        ui.allocate_rect(rect, egui::Sense::hover());
    }

    /// `parts` one above the other, each under its label as a heading that collapses it. A part that was not there
    /// the frame before, such as a result the engine just sent, is scrolled into view.
    fn stack(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        parts: &[&Component],
        out: &mut Vec<Intent>,
    ) -> First {
        let (key, follow) = (ui.id().with("parts"), ui.id().with("arrival"));
        let was = ui.data(|d| d.get_temp::<Vec<String>>(key));
        // The part that arrived and where it was drawn, followed until the column around it settles.
        let following = ui.data(|d| d.get_temp::<(String, egui::Rect)>(follow));
        let mut arrived = None;
        let mut first = First::default();
        let start = ui.cursor().top();
        for (i, c) in parts.iter().enumerate() {
            let top = ui.cursor().top();
            match &c.head().label {
                Some(label) => {
                    // Open as the player last left it on this screen, else as the engine has it.
                    let chosen =
                        egui::Id::new(("part folded by the player", &screen.screen_id, c.id()));
                    let open = ui
                        .data(|d| d.get_temp::<bool>(chosen))
                        .unwrap_or(!c.head().collapsed);
                    let header = egui::CollapsingHeader::new(markup::plain(&label.shown()))
                        .id_salt(c.id())
                        .open(Some(open))
                        .show(ui, |ui| self.component_body(ui, screen, c, out));
                    if header.header_response.clicked() {
                        ui.data_mut(|d| d.insert_temp(chosen, !open));
                    }
                }
                None => self.component(ui, screen, c, out),
            }
            let head = || Head {
                height: ui.min_rect().bottom() - start,
                lists: self.screens.column_lists.borrow().len(),
            };
            let rows = |n: &&Component| matches!(n, Component::List(l) if !l.rows.is_empty());
            let nested = c.nested();
            if i == 0 {
                first.part = head();
            }
            if first.listed.is_none() && nested.iter().any(rows) {
                first.listed = Some(head());
            }
            let search = |n: &&Component| matches!(n, Component::List(l) if !l.rows.is_empty() && l.search(screen).is_some());
            if first.searched.is_none() && nested.iter().any(search) {
                first.searched = Some(head());
            }
            let new = was
                .as_ref()
                .is_some_and(|was| !was.iter().any(|id| id == c.id()));
            let followed = following.as_ref().is_some_and(|(id, _)| id == c.id());
            if new || followed {
                arrived = arrived.or(Some((c.id().to_string(), ui.min_rect().with_min_y(top))));
            }
        }
        // Asked for after the parts, whose own lists would take the request for themselves.
        match arrived {
            Some((id, drawn)) if following.as_ref().map(|(_, was)| *was) != Some(drawn) => {
                let at_once = egui::style::ScrollAnimation::none();
                ui.scroll_to_rect_animation(drawn, Some(egui::Align::Min), at_once);
                ui.data_mut(|d| d.insert_temp(follow, (id, drawn)));
            }
            _ => ui.data_mut(|d| d.remove::<(String, egui::Rect)>(follow)),
        }
        let ids = parts.iter().map(|c| c.id().to_string()).collect::<Vec<_>>();
        ui.data_mut(|d| d.insert_temp(key, ids));
        first
    }
}

/// Takes a panel's whole room without drawing in it, for what fills it later.
fn reserve(ui: &mut egui::Ui) -> egui::Rect {
    ui.expand_to_include_rect(ui.max_rect());
    ui.max_rect()
}
