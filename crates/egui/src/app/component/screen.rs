//! The top screen's window: where it sits over the map, its body and footer, and the workspace kept in view under
//! a dialog.

use super::*;

/// What a window kept off the cursor holds while its screen is open: its side, and the pass it was last shown in.
#[derive(Clone, Copy)]
struct Anchor {
    left: bool,
    pass: u64,
}

impl<M: GameModel> App<M> {
    /// What the window of `screen` is remembered as: a list filter's prompt is the window of the screen it filters,
    /// so typing in it neither moves nor resizes that window.
    fn window_kind(&self, screen: &Screen) -> Option<String> {
        let stack = self.model.screens();
        let below = stack.len().checked_sub(2).and_then(|at| stack.get(at));
        match (form::filter_submit(screen), below) {
            (Some(_), Some(below)) => crate::app::screen_kind(below),
            _ => crate::app::screen_kind(screen),
        }
    }

    /// The screen on top drawn from its components when the engine describes it so, else the 0.1 menu.
    pub(in crate::app) fn menu_or_screen(&mut self, ctx: &egui::Context) {
        self.sync_screens();
        self.backdrop(ctx);
        self.paint_icons();
        self.paint_overmap();
        // Only the lists of the top screen drawn this frame have a page: the backdrop's take no keys.
        self.screens.list_pages.borrow_mut().clear();
        let Some(top) = interaction::described(self.model.screens()) else {
            return self.menu(ctx);
        };
        let mut out = vec![];
        let cancel = interaction::with_role(top, AffordanceRole::Cancel).map(|a| a.id.clone());
        let kind = self.window_kind(top);
        let shown = self.screen_window(ctx, top, &mut out);
        if self.keep_window(ctx, kind.as_deref(), &shown)
            && let Some(cancel) = cancel
        {
            out.push(Intent::Run(cancel));
        }
        self.paint_icons();
        self.paint_overmap();
        out.into_iter().for_each(|intent| self.apply(intent));
        self.send_paints(ctx);
    }

    /// The topmost workspace under a top screen that is not one, drawn behind it and taking no input, as an
    /// application stays in view under its dialogs.
    fn backdrop(&self, ctx: &egui::Context) {
        let Some((top, below)) = self.model.screens().split_last() else {
            return;
        };
        let under = below
            .iter()
            .rev()
            .find(|s| workspace::laid_out(s).is_some());
        let Some(under) = under.filter(|_| workspace::laid_out(top).is_none()) else {
            return;
        };
        self.screens.under.set(true);
        // What the player does on it is nothing: it has no affordances, and its widgets are off.
        self.screen_window(ctx, under, &mut vec![]);
        self.screens.under.set(false);
    }

    /// Paints the items of the list rows drawn, now that their window is drawn.
    pub(super) fn paint_icons(&mut self) {
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
        for (painter, rules) in self.screens.rules.take() {
            painter.extend(rules);
        }
    }

    fn screen_window(
        &self,
        ctx: &egui::Context,
        screen: &Screen,
        out: &mut Vec<Intent>,
    ) -> Shown<()> {
        let title = title(screen);
        // A workspace fills the view edge to edge; its panels keep their own margins.
        let frame = match workspace::laid_out(screen) {
            Some(_) => egui::Frame::NONE.fill(tokens::DIALOG),
            None => crate::app::menu::list_frame(ctx),
        };
        let region = self.region.unwrap_or_else(|| ctx.content_rect());
        // The squares to keep clear: the engine's cursor and the avatar, a square around each.
        let avoid = self.map_view.map_or(vec![], |view| {
            let square = |at: Pos| view.square(at).expand(KEEP_CLEAR * view.cell);
            self.model
                .focus()
                .into_iter()
                .chain([self.model.player()])
                .map(square)
                .collect()
        });
        // A window kept off the cursor holds its side while its screen is open; its size is the player's.
        let anchored = keeps_off_cursor(screen);
        let anchor_key = egui::Id::new(("anchored", &screen.screen_id));
        // One not shown in the last pass belongs to an earlier opening of the screen.
        let pass = ctx.cumulative_pass_nr();
        let anchor = ctx
            .data(|d| d.get_temp::<Anchor>(anchor_key))
            .filter(|a| anchored && a.pass + 1 >= pass);
        let room = Room {
            region,
            sidebar: self.sidebar,
            avoid: &avoid,
            map: !interaction::game_hidden(self.model.screens()),
            kept_left: anchor.map(|a| a.left),
        };
        let placement = placement(screen, &room);
        let place = placement.place;
        let base = self.window_spec(ctx, self.window_kind(screen).as_deref(), &title);
        // A workspace is the whole view, with no title bar: its title, help and leave end its menu bar.
        let workspace = workspace::laid_out(screen).is_some();
        let title_bar = base.title_bar || workspace;
        let rest = footer_affordances(screen);
        // The help sits in the title bar, or last in the footer of a window that has none.
        let titled = interaction::with_role(screen, AffordanceRole::Help)
            .filter(|a| a.enabled && base.title_bar && !workspace);
        // Under a screen kept as its window, such as its list filter's prompt, it is a window of its own.
        let under = self.screens.under.get();
        let shared = under
            && self.model.screens().last().is_some_and(|top| {
                let kind = self.window_kind(top);
                a11y::dialog_id(kind.as_deref().unwrap_or(&super::title(top))) == base.id
            });
        let spec = WindowSpec {
            id: match shared {
                true => base.id.with("under"),
                false => base.id,
            },
            inert: under,
            title_bar: base.title_bar && !workspace,
            closable: interaction::with_role(screen, AffordanceRole::Cancel).is_some(),
            frame: Some(frame),
            place: Some(place),
            movable: !keeps_off_cursor(screen) && placement.class != Class::Full,
            width: placement.first_width(),
            scrolls: false,
            help: titled.map(|a| windows::TitleHelp {
                label: markup::plain(&a.label.shown()),
                tip: glyph_tip(a),
            }),
            ..base
        };
        let shown = windows::show(ctx, spec, |ui| {
            let Some(root) = &screen.root else {
                return;
            };
            let footer = |ui: &mut egui::Ui, out: &mut Vec<Intent>| {
                self.question(ui, screen, out);
                if let Some(denial) = &self.screens.denial {
                    ui.colored_label(BAD, markup::plain(denial));
                }
                self.footer(ui, &rest, title_bar, out);
            };
            // A screen that fills the room keeps its footer under its body, whatever the body's height.
            if placement.class == Class::Full {
                if let Some(split) = workspace::laid_out(screen) {
                    return self.workspace(ui, screen, (split, &rest), out);
                }
                egui::Panel::bottom(egui::Id::new(("footer", &screen.screen_id)))
                    .resizable(false)
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| footer(ui, out));
                self.menu_bar(ui, screen, out);
                // A body taller than the room, a full-size grid over further parts, scrolls instead of being cut.
                egui::CentralPanel::no_frame().show(ui, |ui| {
                    let scroll = egui::ScrollArea::vertical()
                        .id_salt(("body", &screen.screen_id))
                        .auto_shrink(false);
                    let room = ui.available_height();
                    body_scroll(ui, scroll, room, |ui| match placement.column {
                        true => {
                            // A row's bar reaches half a gap past each side of its table.
                            let width = ui.available_width().min(COLUMN_WIDTH)
                                - ui.spacing().item_spacing.x;
                            ui.vertical_centered(|ui| {
                                ui.scope(|ui| {
                                    ui.set_width(width);
                                    ui.with_layout(
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| self.component(ui, screen, root, out),
                                    );
                                })
                            });
                        }
                        false => self.component(ui, screen, root, out),
                    });
                });
            } else {
                // A body taller than the room scrolls above the footer, which keeps the height it had last frame,
                // by the window's kind.
                self.menu_bar(ui, screen, out);
                let kept = ui.id().with("footer height");
                let reserve = ctx.data(|d| d.get_temp::<f32>(kept)).unwrap_or(0.0);
                let scroll = egui::ScrollArea::vertical()
                    .id_salt("body")
                    .auto_shrink([false, true]);
                let room = ui.available_height() - reserve;
                body_scroll(ui, scroll, room, |ui| self.component(ui, screen, root, out));
                let top = ui.min_rect().bottom();
                footer(ui, out);
                let height = ui.min_rect().bottom() - top;
                ctx.data_mut(|d| d.insert_temp(kept, height));
            }
        });
        if let (true, Some(left)) = (anchored, at_left(place)) {
            ctx.data_mut(|d| d.insert_temp(anchor_key, Anchor { left, pass }));
        }
        if let Some(a) = titled.filter(|_| shown.help) {
            out.push(Intent::Run(a.id.clone()));
        }
        shown
    }
}
