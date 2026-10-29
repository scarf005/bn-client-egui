//! A split component: its banner and prompt over its parts, laid side by side in panes as wide as their content
//! needs, on further lines when they do not fit.

use super::*;

/// The margin around each pane of a split, on every side.
pub(super) const PANE_MARGIN: i8 = 2;
/// The most rows a list in a peer pane shows before it scrolls.
pub(super) const PEER_ROWS: usize = 12;

/// Whether `c` is running text: a text block, or a panel holding only one.
fn prose(c: &Component) -> bool {
    match c {
        Component::Text(t) => t.role.is_none(),
        Component::Panel(p) => {
            p.stats.as_ref().is_none_or(Vec::is_empty) && p.child.as_deref().is_some_and(prose)
        }
        _ => false,
    }
}

/// Whether `c` heads its split above the parts: the screen's banner, or the question the parts answer.
pub(super) fn heading(c: &Component) -> bool {
    matches!(c, Component::Text(t) if matches!(t.role, Some(TextRole::Banner | TextRole::Prompt)))
}

impl<M: GameModel> App<M> {
    /// The panes side by side, each as wide as its content needs (`split::split_widths`), the only prose of
    /// peers as wide as the rest (`split::prose_widths`) and a list beside its detail at least two fifths of the
    /// room (`split::detail_widths`). Panes whose minimums do not fit go on to further lines
    /// (`split::wrap_lines`), a detail under its list; a line whose minimums still do not fit scrolls sideways
    /// instead of breaking words.
    /// Peers on more than one line are as tall as what they hold, a list up to `PEER_ROWS` rows, and scroll
    /// down together; one line of panes, peers around a map or a canvas, and a list beside its detail share the
    /// height they have.
    pub(super) fn split(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        split: &Split,
        out: &mut Vec<Intent>,
    ) {
        // A banner and a prompt head the parts, whatever their number.
        let (headings, parts) = split.children.iter().partition::<Vec<_>, _>(|c| heading(c));
        for heading in headings {
            self.component(ui, screen, heading, out);
        }
        let parts = parts.as_slice();
        // A list filter's box is above the results it filters.
        if form::filter_submit(screen).is_some() {
            return parts
                .iter()
                .for_each(|child| self.component(ui, screen, child, out));
        }
        let gap = ui.spacing().item_spacing.x;
        let room = ui.available_width();
        let texts = (0..parts.len())
            .filter(|&i| prose(parts[i]))
            .collect::<Vec<_>>();
        let prose_at = match (split.relation, texts.as_slice()) {
            (Relation::Peers, [at]) => Some(*at),
            _ => None,
        };
        let fills = parts.iter().flat_map(|c| c.nested()).any(takes_room);
        // Beside a map or a canvas, which take the room left, the other panes keep their natural width.
        let measure = |ui: &egui::Ui, panes: &[&Component], room: f32| {
            panes
                .iter()
                .map(|c| {
                    let extent = with_margin(self.extent(ui, screen, c, room));
                    Extent {
                        grows: extent.grows && (!fills || c.nested().into_iter().any(takes_room)),
                        ..extent
                    }
                })
                .collect::<Vec<_>>()
        };
        let laid = |ui: &egui::Ui, room: f32| {
            let extents = measure(ui, parts, room);
            let lines = split::wrap_lines(&extents, room, gap);
            (extents, lines)
        };
        let (mut extents, mut lines) = laid(ui, room);
        let natural = split.relation == Relation::Peers && !fills && lines.len() > 1;
        // Peers that scroll down keep room beside them for the scroll bar.
        let room = match natural {
            true => room - ui.spacing().scroll.allocated_width(),
            false => room,
        };
        if natural {
            (extents, lines) = laid(ui, room);
        }
        let count = lines.len();
        let mut draw = |ui: &mut egui::Ui| {
            for (n, line) in lines.iter().enumerate() {
                let panes = &parts[line.clone()];
                let fit = &extents[line.clone()];
                let widths = match (split.relation, prose_at.filter(|at| line.contains(at))) {
                    (Relation::Detail, _) if !matches!(panes.first(), Some(Component::Text(_))) => {
                        split::detail_widths(fit, room, gap)
                    }
                    // Beside the prose, which takes at least an equal share, the panes are offered the rest.
                    (Relation::Peers, Some(at)) => {
                        let offered = room - room / panes.len() as f32;
                        split::prose_widths(
                            &measure(ui, panes, offered),
                            at - line.start,
                            room,
                            gap,
                        )
                    }
                    _ => split::split_widths(fit, room, gap),
                };
                // Peers as tall as they hold have no bottom; sharing the height, each line gets its share of
                // what is left, past a few rows.
                let spacing = ui.spacing();
                let floor = 4.0 * (spacing.interact_size.y + spacing.item_spacing.y);
                // A line holding a map or a canvas takes all the room; the lines under it are as tall as what they
                // hold, a list up to `PEER_ROWS` rows, and scroll below.
                let takes = |line: &std::ops::Range<usize>| {
                    parts[line.clone()]
                        .iter()
                        .flat_map(|c| c.nested())
                        .any(takes_room)
                };
                let below = lines[..n].iter().any(takes);
                let height = match (natural || below, takes(line)) {
                    (_, true) => ui.available_height().max(floor),
                    (true, false) => f32::INFINITY,
                    (false, false) => (ui.available_height() / (count - n) as f32).max(floor),
                };
                // Inside a peer, a pane that shares its height still has none to share.
                let rows = match natural || below {
                    true => Some(PEER_ROWS),
                    false => self.screens.list_rows.get(),
                };
                let mut line_ui = |ui: &mut egui::Ui| {
                    let mut active = None;
                    let drawn = ui.horizontal_top(|ui| {
                        for (child, width) in panes.iter().zip(&widths) {
                            let size = egui::vec2(*width, height);
                            let rect = egui::Rect::from_min_size(ui.cursor().min, size);
                            let down = egui::Layout::top_down(egui::Align::Min);
                            let mut pane =
                                ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(down));
                            pane.shrink_clip_rect(egui::Rect::from_x_y_ranges(
                                rect.x_range(),
                                ui.clip_rect().y_range(),
                            ));
                            // The margin keeps room inside the pane's width for the active outline.
                            let frame = egui::Frame::NONE.inner_margin(PANE_MARGIN);
                            frame.show(&mut pane, |ui| {
                                ui.set_width(width - frame.total_margin().sum().x);
                                self.with_list_rows(rows, || self.component(ui, screen, child, out))
                            });
                            // The pane takes its width and no more, whatever its content draws: wider, it
                            // would widen the window that the next frame's room comes from, every frame.
                            let drawn = egui::vec2(*width, pane.min_rect().height());
                            let (slot, _) = ui.allocate_exact_size(drawn, egui::Sense::hover());
                            if split.active.as_deref() == Some(child.id()) && !self.empty(child) {
                                active = Some(slot.x_range());
                            }
                        }
                    });
                    // The active pane is outlined over its whole slot, as tall as its line.
                    if let Some(x) = active {
                        let rect = egui::Rect::from_x_y_ranges(x, drawn.response.rect.y_range());
                        let stroke = ui.visuals().selection.stroke;
                        ui.painter()
                            .rect_stroke(rect, 0, stroke, egui::StrokeKind::Inside);
                    }
                };
                match split::overflows(&widths, room, gap) {
                    true => {
                        egui::ScrollArea::horizontal()
                            .id_salt(("split", &split.head.id, n))
                            .auto_shrink([false, true])
                            .show(ui, line_ui);
                    }
                    false => line_ui(ui),
                }
            }
        };
        match natural {
            true => {
                let scroll = egui::ScrollArea::vertical()
                    .id_salt(("split-lines", &split.head.id))
                    .auto_shrink([false, true]);
                let room = ui.available_height();
                body_scroll(ui, scroll, room, draw);
            }
            false => draw(ui),
        }
    }

    /// Draws with lists showing at most `rows` rows before they scroll, or as many as their room holds.
    pub(super) fn with_list_rows<R>(&self, rows: Option<usize>, draw: impl FnOnce() -> R) -> R {
        let was = self.screens.list_rows.replace(rows);
        let drawn = draw();
        self.screens.list_rows.set(was);
        drawn
    }

    /// Whether `c` shows nothing: a list without rows.
    fn empty(&self, c: &Component) -> bool {
        matches!(c, Component::List(list) if self.known_rows(list).is_empty())
    }
}
