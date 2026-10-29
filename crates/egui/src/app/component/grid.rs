//! A grid component: its squares drawn as cells with their outlines, and the drags over them; a canvas grid
//! zooms and pans (`canvas`).

pub(super) mod canvas;
mod drag;
mod region;

use super::*;
use drag::Square;

/// Side of a grid cell in points.
pub(super) const GRID_CELL: f32 = 26.0;
/// The largest a grid's cell grows to when the room allows.
pub(super) const GRID_MAX_CELL: f32 = 2.0 * GRID_CELL;
/// The largest a canvas grid's square grows to when it fits its room, so a small grid keeps readable sprites; the
/// zoom gesture goes past it.
const CANVAS_MAX_CELL: f32 = 40.0;
/// The opacity of the squares a grid marks faded.
const GRID_FADED: f32 = 0.45;
/// The most the zoom gesture scales a canvas grid's cells from the size that fills its room.
const GRID_ZOOM_MAX: f32 = 4.0;

/// What a square says on hover: what it holds and, when it is off, why.
fn cell_tip(cell: &GridCell, refusals: &[String]) -> String {
    let label = cell.label.as_ref().map(|l| markup::plain(&l.shown()));
    let why = cell.denial.as_ref().filter(|_| !cell.enabled);
    label
        .into_iter()
        .chain(why.map(|w| markup::plain(&w.shown())))
        .chain(cell.refused.and_then(|i| refusals.get(i).cloned()))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Where a grid's squares were drawn.
struct GridLayout {
    /// The top left of the first square drawn, which is the column and the row `first` names.
    origin: egui::Pos2,
    first: Square,
    size: egui::Vec2,
    /// Clipped to what the grid shows.
    painter: egui::Painter,
}

impl<M: GameModel> App<M> {
    /// `grid`, with room left under it for a row of its `buttons` when it has any.
    pub(super) fn grid(
        &self,
        ui: &mut egui::Ui,
        screen: &Screen,
        (grid, buttons): (&Grid, bool),
        out: &mut Vec<Intent>,
    ) {
        let (xs, ys) = drawn_span(grid);
        let below = match buttons {
            true => ui.spacing().interact_size.y + ui.spacing().item_spacing.y,
            false => 0.0,
        };
        let available = (ui.available_size() - egui::vec2(0.0, below)).max(egui::Vec2::ZERO);
        // The cells grow with the room the grid has, up to twice their size, their glyphs with them; a canvas's
        // the same, and the zoom gesture over it scales them from there.
        let fit = (available.x / xs.len().max(1) as f32).min(available.y / ys.len().max(1) as f32);
        let canvas = grid.role == Some(GridRole::Canvas);
        // A canvas first fits its whole grid to its room, its squares no larger than `CANVAS_MAX_CELL`; it zooms out
        // until the whole grid fits, and in until a cell is as large as the room.
        let base = match canvas {
            true => fit.clamp(GRID_CELL, CANVAS_MAX_CELL),
            false => fit.clamp(GRID_CELL, GRID_MAX_CELL),
        };
        let limits = ((fit / base).min(1.0), GRID_ZOOM_MAX.max(fit / base));
        let view_key = egui::Id::new(("grid view", &screen.screen_id, &grid.head.id));
        // The workspace's stop for the keyboard over the canvas (`canvas_stop`).
        let keyboard = ui.memory(|m| m.has_focus(view_key.with("keyboard")));
        let view = ui
            .data(|d| d.get_temp::<canvas::View>(view_key))
            .filter(|v| v.span == (xs.len(), ys.len()));
        // A canvas with room to draw around what it holds first shows what it holds, at a readable size.
        let anchor = match view {
            Some(view) => canvas::zoomed(ui, view, limits),
            None if canvas => content_span(grid, (&xs, &ys))
                .and_then(|content| canvas::framed(content, available, base, limits)),
            None => None,
        };
        // Until the player zooms, the whole grid is in view at any size of the room: a cell shrinks below its usual
        // size to fit.
        let kept = view.filter(|v| v.zoomed).map(|v| v.zoom);
        let zoom = anchor
            .as_ref()
            .map_or(kept.unwrap_or(limits.0), |a| a.zoom)
            .clamp(limits.0, limits.1);
        let cell = match canvas {
            true => base * zoom,
            false => fit.clamp(GRID_CELL, GRID_MAX_CELL),
        };
        let size = egui::vec2(cell, cell);
        let glyph_size = egui::TextStyle::Monospace.resolve(ui.style()).size * cell / GRID_CELL;
        let mut squares = HashMap::new();
        let layout;
        if canvas {
            // The wheel zooms it; a middle-button drag, or Space and the primary one, pans it.
            let panning = canvas::panning(ui);
            let pan = ui.input(|i| {
                (i.pointer.middle_down() || (panning && i.pointer.primary_down()))
                    .then(|| i.pointer.delta())
            });
            // The viewport is a whole number of cells tall, so none is cut across.
            let whole = available;
            let rows = (whole.y / cell).floor().max(1.0);
            let room = egui::vec2(whole.x, rows * cell);
            let grid_size = egui::vec2(xs.len() as f32, ys.len() as f32) * cell;
            // The area clamps an asked offset to last frame's content, so the margins of a new zoom take it only
            // the frame after: it is asked for again then.
            let zoomed = zoom > limits.0 + 1e-3;
            let placed = canvas::place(
                (view, anchor.as_ref()),
                zoomed,
                (whole, room, grid_size),
                cell,
            );
            let offset = placed.offset;
            let asked = anchor.is_some().then_some(offset).flatten();

            let mut area = egui::ScrollArea::both()
                .id_salt(("grid", &screen.screen_id, &grid.head.id))
                .max_height(rows * cell)
                .auto_shrink([true, true])
                // The whole grid is in view until it is zoomed past that.
                .scroll_bar_visibility(match zoomed {
                    true => egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    false => egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                })
                .scroll_source(egui::scroll_area::ScrollSource {
                    drag: egui::scroll_area::DragScroll::Never,
                    mouse_wheel: false,
                    scroll_bar: true,
                });
            if let Some(offset) = offset {
                area = area.scroll_offset(offset);
            }
            let shown = area.show(ui, |ui| {
                canvas::note_clip(ui.ctx(), ui.clip_rect());
                canvas::note_hover(ui);
                if let Some(pan) = pan.filter(|_| ui.rect_contains_pointer(ui.clip_rect())) {
                    ui.scroll_with_delta(pan);
                }
                let cells = GridCells {
                    size,
                    glyph_size,
                    keyboard,
                };
                // The cells sit `lead` in from the corner, and the area runs on `tail` past them.
                let corner = ui.cursor().min;
                let laid = ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_size(corner + placed.lead, grid_size)),
                    |ui| self.grid_cells(ui, grid, cells, &mut squares, out),
                );
                let whole = placed.lead + grid_size + placed.tail;
                ui.allocate_rect(
                    egui::Rect::from_min_size(corner, whole),
                    egui::Sense::hover(),
                );
                laid.inner
            });
            layout = Some(shown.inner);
            // The canvas takes its room's whole width, which the grid is centred in.
            ui.allocate_space(egui::vec2(ui.available_width(), 0.0));
            let view = canvas::View {
                span: (xs.len(), ys.len()),
                zoom,
                zoomed,
                cell,
                rect: shown.inner_rect,
                // While zooming, the offset asked for: one a smaller square clamped must not drift the anchor.
                offset: offset.unwrap_or(shown.state.offset),
                pending: asked,
                lead: placed.lead,
                room: whole,
            };
            ui.data_mut(|d| d.insert_temp(view_key, view));
            self.hover_outline(ui, shown.inner_rect, &squares);
        } else {
            let cells = GridCells {
                size,
                glyph_size,
                keyboard,
            };
            layout = Some(self.grid_cells(ui, grid, cells, &mut squares, out));
        }
        if let Some(mode) = grid.drag {
            self.drag_squares(ui, grid, mode, &squares, out);
        }
        if let (Some(_), Some(secondary)) = (grid.secondary_drag, &grid.secondary) {
            self.drag_squares_by(ui, grid, DragBy::Secondary(secondary), &squares, out);
        }
        if let (Some(affordance), Some(layout)) = (&grid.region_drag, layout) {
            self.drag_region(ui, grid, affordance, (&squares, &layout), out);
        }
        self.stamp_outline(ui, grid, &squares);
    }

    /// Where the tool in hand would put its squares down from the square under the pointer, outlined.
    fn stamp_outline(
        &self,
        ui: &egui::Ui,
        grid: &Grid,
        squares: &HashMap<drag::Square, (egui::Rect, String, String)>,
    ) {
        let pointer = ui.ctx().pointer_hover_pos();
        let Some((x, y)) = pointer.and_then(|at| {
            squares
                .iter()
                .find(|(_, s)| s.0.contains(at))
                .map(|(square, _)| *square)
        }) else {
            return;
        };
        let shapes = grid
            .stamp
            .iter()
            .filter_map(|d| {
                let to = (x.checked_add_signed(d.x)?, y.checked_add_signed(d.y)?);
                squares.get(&to).map(|s| s.0)
            })
            .flat_map(|rect| {
                std::iter::once(egui::Shape::rect_filled(
                    rect,
                    0.0,
                    tokens::REGION_PICKED.gamma_multiply(0.35),
                ))
                .chain(outline_shapes(rect, tokens::REGION_PICKED))
            })
            .collect::<Vec<_>>();
        if !shapes.is_empty() {
            let painter = ui.painter().with_clip_rect(ui.clip_rect());
            self.screens.rules.borrow_mut().push((painter, shapes));
        }
    }

    /// The square under the pointer, outlined over the sprites.
    fn hover_outline(
        &self,
        ui: &egui::Ui,
        viewport: egui::Rect,
        squares: &HashMap<drag::Square, (egui::Rect, String, String)>,
    ) {
        let at = canvas::pointer_over(ui, viewport);
        let Some((rect, ..)) = at.and_then(|at| squares.values().find(|s| s.0.contains(at))) else {
            return;
        };
        let shapes = outline_shapes(*rect, tokens::TEXT);
        let painter = ui
            .painter()
            .with_clip_rect(viewport.intersect(ui.clip_rect()));
        self.screens.rules.borrow_mut().push((painter, shapes));
    }

    /// The cells of `grid`, each `size`, their glyphs `glyph_size` large; a cell names what it holds on hover.
    fn grid_cells(
        &self,
        ui: &mut egui::Ui,
        grid: &Grid,
        GridCells {
            size,
            glyph_size,
            keyboard,
        }: GridCells,
        squares: &mut HashMap<drag::Square, (egui::Rect, String, String)>,
        out: &mut Vec<Intent>,
    ) -> GridLayout {
        let (xs, ys) = drawn_span(grid);
        let opacity = ui.opacity();
        let focus = self.screens.interaction.focus();
        let focus = focus
            .row_id
            .as_deref()
            .filter(|_| keyboard && focus.component_id.as_deref() == Some(&grid.head.id));
        let drawn = ui.scope(|ui| {
            // A square is its cell's size, however small: no row is as tall as a control, no button pads its glyph.
            let spacing = ui.spacing_mut();
            spacing.item_spacing = egui::Vec2::ZERO;
            spacing.interact_size = egui::Vec2::ZERO;
            spacing.button_padding = egui::Vec2::ZERO;
            let cells = grid
                .cells
                .iter()
                .map(|c| ((c.x, c.y), c))
                .collect::<HashMap<_, _>>();
            for y in ys.clone() {
                ui.horizontal(|ui| {
                    for x in xs.clone() {
                        let Some(&cell) = cells.get(&(x, y)) else {
                            ui.allocate_space(size);
                            continue;
                        };
                        // Another level seen through open air is drawn faded, as its sprites and its glyph.
                        // A square that is off or refused is drawn faded too, unless it holds a part, which keeps
                        // its look to be read; a disabled button does that itself.
                        let off = (!(cell.enabled
                            || cell.id.is_some() && cell.affordance.is_some())
                            || cell.refused.is_some())
                            && cell.looks.is_empty();
                        // The square the cursor or a selection marks keeps its full look, to be seen.
                        let at_cursor = grid
                            .cursor
                            .is_some_and(|c| (c.x, c.y) == (x as i32, y as i32));
                        let marked = cell.selected == Some(true) || at_cursor;
                        ui.set_opacity(match (cell.faded == Some(true) || off) && !marked {
                            true => opacity * GRID_FADED,
                            false => opacity,
                        });
                        let glyph = cell.glyph.clone().unwrap_or_else(|| " ".into());
                        let name = cell.label.as_ref().map_or(glyph.clone(), Text::shown);
                        // With a tileset the cell is its sprites, drawn over it once its window is drawn.
                        let looks = cell
                            .looks
                            .iter()
                            .filter_map(|&i| grid.looks.get(i).cloned())
                            .collect::<Vec<_>>();
                        let sprite = (self.display.tiles() && !looks.is_empty())
                            .then(|| self.model.looks_cell(&looks, x | y << 16))
                            .flatten();
                        let glyph = if sprite.is_some() { " ".into() } else { glyph };
                        let color = cell.color.as_deref().map(|c| self.tint(c));
                        let actionable = cell.id.is_some()
                            && (cell.affordance.is_some() || grid.secondary.is_some());
                        let mut shown = egui::RichText::new(&glyph).monospace().size(glyph_size);
                        if let Some(color) = color {
                            shown = shown.color(color);
                        }
                        // The square the keyboard reached is outlined over its sprites.
                        let reached = focus.is_some() && cell.id.as_deref() == focus;
                        // A marked cell keeps a rim of its highlight around the sprite.
                        let queue = |ui: &egui::Ui, rect: egui::Rect| {
                            if let Some(cell) = sprite.clone() {
                                let rect = rect.shrink(if marked { 3.0 } else { 0.0 });
                                self.screens.icons.borrow_mut().push(RowIcon {
                                    painter: ui.painter_at(rect),
                                    rect,
                                    cell,
                                });
                            }
                        };
                        if !actionable {
                            let shown = match marked {
                                true => shown.background_color(tokens::SELECTION),
                                false => shown,
                            };
                            let label = ui.add_sized(size, egui::Label::new(shown));
                            queue(ui, label.rect);
                            let tip = cell_tip(cell, &grid.refusals);
                            if !tip.is_empty() {
                                label.on_hover_text(tip);
                            }
                            continue;
                        }
                        // A canvas is one stop of the keyboard's walk, whose squares the arrows move among.
                        let sense = match grid.role == Some(GridRole::Canvas) {
                            true => egui::Sense::CLICK,
                            false => egui::Sense::click(),
                        };
                        let button = ui.add_enabled(
                            cell.enabled,
                            egui::Button::new(shown)
                                .min_size(size)
                                .selected(marked)
                                .sense(sense),
                        );
                        queue(ui, button.rect);
                        if reached {
                            let ring = outline_shapes(button.rect, tokens::TEXT);
                            let mut painter = ui.painter().clone();
                            painter.set_opacity(opacity);
                            self.screens.rules.borrow_mut().push((painter, ring));
                        }
                        // An empty square to draw on shows as an outlined one, not a gap in the grid;
                        // a blank glyph that shows something, such as open air, is no empty square.
                        if glyph.trim().is_empty() && looks.is_empty() {
                            ui.painter().rect_stroke(
                                button.rect,
                                0.0,
                                egui::Stroke::new(1.0, tokens::HOVER),
                                egui::StrokeKind::Inside,
                            );
                        }
                        if let (true, Some(id)) = (cell.enabled, &cell.id)
                            && (cell.affordance.is_some() || grid.secondary_drag.is_some())
                        {
                            // Only the part of the square in view takes a drag; a square with no primary
                            // affordance is the secondary drag's alone, and names none.
                            let seen = button.rect.intersect(ui.clip_rect());
                            let a = cell.affordance.clone().unwrap_or_default();
                            squares.insert((x, y), (seen, id.clone(), a));
                        }
                        button.widget_info(|| {
                            WidgetInfo::labeled(
                                WidgetType::Button,
                                button.enabled(),
                                markup::plain(&name),
                            )
                        });
                        let tip = cell_tip(cell, &grid.refusals);
                        let button = match (cell.enabled, tip.is_empty()) {
                            (_, true) => button,
                            (true, false) => button.on_hover_text(tip),
                            (false, false) => button.on_disabled_hover_text(tip),
                        };
                        // A secondary drag, which a click is the shortest of, runs the secondary itself.
                        if let (true, Some(id), Some(a)) = (
                            button.secondary_clicked()
                                && grid.secondary_drag.is_none()
                                && !canvas::panning(ui),
                            &cell.id,
                            &grid.secondary,
                        ) {
                            out.push(Intent::Focus {
                                component: self.row_component(a, &grid.head.id),
                                row: id.clone(),
                            });
                            out.push(Intent::Run(a.clone()));
                        }
                        // A pointer click on a draggable canvas is the drag's press and release.
                        let pointer_click =
                            grid.drag.is_some() && ui.input(|i| i.pointer.any_released());
                        if let (true, Some(id), Some(a)) = (
                            button.clicked() && !pointer_click && !canvas::panning(ui),
                            &cell.id,
                            &cell.affordance,
                        ) {
                            out.push(Intent::Focus {
                                component: self.row_component(a, &grid.head.id),
                                row: id.clone(),
                            });
                            out.push(Intent::Run(a.clone()));
                        }
                    }
                });
            }
        });
        let origin = drawn.response.rect.min;
        let layout = GridLayout {
            origin,
            first: (xs.start, ys.start),
            size,
            painter: ui.painter().clone(),
        };
        let outlines = grid
            .regions
            .iter()
            .flat_map(|region| self.outline(ui, &layout, region))
            .collect();
        self.screens
            .rules
            .borrow_mut()
            .push((layout.painter.clone(), outlines));
        if let Some(block) = grid.block.filter(|b| b.x > 0 && b.y > 0) {
            // A light line over a dark one, so it reads on pale and dark squares alike.
            let strokes = [
                egui::Stroke::new(3.0, egui::Color32::from_black_alpha(160)),
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            ];
            let along = |range: &std::ops::Range<u32>, step: u32| {
                (range.start..=range.end)
                    .filter(move |n| n % step == 0)
                    .map(|n| (n - range.start) as f32)
                    .collect::<Vec<_>>()
            };
            let (width, height) = (xs.len() as f32 * size.x, ys.len() as f32 * size.y);
            let lines = along(&xs, block.x)
                .into_iter()
                .map(|n| origin.x + n * size.x)
                .map(|x| [egui::pos2(x, origin.y), egui::pos2(x, origin.y + height)])
                .chain(
                    along(&ys, block.y)
                        .into_iter()
                        .map(|n| origin.y + n * size.y)
                        .map(|y| [egui::pos2(origin.x, y), egui::pos2(origin.x + width, y)]),
                )
                .flat_map(|points| strokes.map(|stroke| egui::Shape::line_segment(points, stroke)))
                .collect();
            self.screens
                .rules
                .borrow_mut()
                .push((ui.painter().clone(), lines));
        }
        layout
    }

    /// The outline of `region` over the squares of `layout`, and its label in the top left when it fits.
    fn outline(&self, ui: &egui::Ui, layout: &GridLayout, region: &GridRegion) -> Vec<egui::Shape> {
        let rect = layout.rect([region.from.x, region.from.y, region.to.x, region.to.y]);
        let picked = region.selected == Some(true);
        let mut shapes = outline_shapes(
            rect,
            if picked {
                tokens::REGION_PICKED
            } else {
                tokens::REGION
            },
        );
        let label = region.label.as_deref().filter(|l| !l.is_empty());
        if let Some(label) = label {
            let font = egui::TextStyle::Small.resolve(ui.style());
            let galley = layout
                .painter
                .layout_no_wrap(label.to_owned(), font, tokens::TEXT);
            if galley.size().x + 4.0 <= rect.width() && galley.size().y + 4.0 <= rect.height() {
                let at = rect.min + egui::vec2(3.0, 2.0);
                let back = egui::Rect::from_min_size(at, galley.size()).expand(1.0);
                shapes.push(egui::Shape::rect_filled(
                    back,
                    0.0,
                    egui::Color32::from_black_alpha(170),
                ));
                shapes.push(egui::Shape::galley(at, galley, tokens::TEXT));
            }
        }
        shapes
    }

    /// The component whose row `affordance` acts on: the one its row argument names, else `own`.
    fn row_component(&self, affordance: &str, own: &str) -> String {
        interaction::described(self.model.screens())
            .and_then(|top| top.affordances.iter().find(|a| a.id == affordance))
            .and_then(|a| a.args.as_ref()?.row.as_ref())
            .map_or(own, |of| of.component_id.as_str())
            .to_owned()
    }

    /// Runs `affordance` for a pointer drag of one of the grid's regions, and outlines where it would land.
    fn drag_region(
        &self,
        ui: &egui::Ui,
        grid: &Grid,
        affordance: &str,
        (squares, layout): (
            &HashMap<drag::Square, (egui::Rect, String, String)>,
            &GridLayout,
        ),
        out: &mut Vec<Intent>,
    ) {
        let key = egui::Id::new(("region-drag", &grid.head.id));
        let mut held = recent::<Option<region::Grab>>(ui.ctx(), key).flatten();
        let pointer = grid_pointer(ui, squares, egui::PointerButton::Primary);
        if let Some(moved) = region::advance(&grid.regions, &mut held, pointer) {
            out.push(Intent::Paint(Paint {
                component: self.row_component(affordance, &grid.head.id),
                row: Some(moved.id),
                affordance: affordance.to_owned(),
                text: Some(moved.text),
            }));
        }
        if let Some(grab) = held.as_ref().filter(|g| g.deltas() != [0; 4]) {
            let color = tokens::REGION_PICKED;
            self.screens.rules.borrow_mut().push((
                layout.painter.clone(),
                outline_shapes(layout.rect(grab.preview()), color),
            ));
        }
        keep_recent(ui.ctx(), key, held);
    }

    /// Runs the affordance of the squares a primary drag over `squares` crosses.
    fn drag_squares(
        &self,
        ui: &egui::Ui,
        grid: &Grid,
        mode: GridDrag,
        squares: &HashMap<drag::Square, (egui::Rect, String, String)>,
        out: &mut Vec<Intent>,
    ) {
        self.drag_squares_by(ui, grid, DragBy::Primary(mode), squares, out);
    }

    /// Runs the affordance of the squares a drag of `by` over `squares` crosses.
    fn drag_squares_by(
        &self,
        ui: &egui::Ui,
        grid: &Grid,
        by: DragBy,
        squares: &HashMap<drag::Square, (egui::Rect, String, String)>,
        out: &mut Vec<Intent>,
    ) {
        let (mode, button, name, other) = match by {
            DragBy::Primary(mode) => (
                mode,
                egui::PointerButton::Primary,
                "grid-drag",
                "grid-drag-secondary",
            ),
            DragBy::Secondary(_) => (
                GridDrag::Trail,
                egui::PointerButton::Secondary,
                "grid-drag-secondary",
                "grid-drag",
            ),
        };
        let key = egui::Id::new((name, &grid.head.id));
        // A drag not kept in the last pass belongs to an earlier showing of the grid: it ends unsent.
        let (mut held, mut began) = recent::<Stroke>(ui.ctx(), key).unwrap_or_default();
        let was_held = held.is_some();
        // One button drags at a time: the other's stroke is one step of the engine's.
        let busy = recent::<Stroke>(ui.ctx(), egui::Id::new((other, &grid.head.id)))
            .is_some_and(|(held, _)| held.is_some());
        let mut pointer = grid_pointer(ui, squares, button);
        pointer.pressed &= !busy;
        let run = drag::advance(mode, &mut held, pointer);
        let marker = |affordance: &Option<String>| {
            affordance.clone().map(|affordance| {
                Intent::Paint(Paint {
                    component: grid.head.id.clone(),
                    row: None,
                    affordance,
                    text: None,
                })
            })
        };
        // A drag begins on the press and ends on the frame that lets go of it.
        out.extend(marker(&grid.drag_begin).filter(|_| !was_held && held.is_some()));
        // Only the squares of the affordance the drag was pressed on: a stroke does not run a neighbour's, nor
        // what a grid shown anew under the pointer puts there.
        if !was_held && held.is_some() {
            began = run
                .first()
                .and_then(|at| squares.get(at))
                .map(|s| s.2.clone());
        }
        for at in run {
            let Some((_, row, own)) = squares.get(&at) else {
                continue;
            };
            let affordance = match by {
                DragBy::Primary(_) if began.as_ref() == Some(own) && !own.is_empty() => own,
                DragBy::Secondary(secondary) => secondary,
                DragBy::Primary(_) => continue,
            };
            out.push(Intent::Paint(Paint {
                component: self.row_component(affordance, &grid.head.id),
                row: Some(row.clone()),
                affordance: affordance.clone(),
                text: None,
            }));
        }
        out.extend(marker(&grid.drag_end).filter(|_| was_held && held.is_none()));
        let began = began.filter(|_| held.is_some());
        keep_recent(ui.ctx(), key, (held, began));
    }
}

impl GridLayout {
    /// The rectangle of the squares from the west and north ones to the east and south ones, both included.
    fn rect(&self, [west, north, east, south]: [i32; 4]) -> egui::Rect {
        let at = |x: i32, y: i32| {
            self.origin
                + egui::vec2(
                    (x - self.first.0 as i32) as f32 * self.size.x,
                    (y - self.first.1 as i32) as f32 * self.size.y,
                )
        };
        let (a, b) = (
            at(west.min(east), north.min(south)),
            at(west.max(east) + 1, north.max(south) + 1),
        );
        egui::Rect::from_min_max(a, b)
    }
}

/// A button's drag over a grid's squares, and the affordance of the square it was pressed on.
type Stroke = (Option<drag::Drag>, Option<String>);

/// Which button drags over a grid's squares, and what it runs there.
#[derive(Clone, Copy)]
enum DragBy<'a> {
    /// The primary button, as the grid's drag mode says, each square running its own affordance.
    Primary(GridDrag),
    /// The secondary button, a trail running this affordance on every square it crosses.
    Secondary(&'a String),
}

/// The pointer over the drawn squares of a grid this frame; a press counts only on the grid's own layer.
fn grid_pointer(
    ui: &egui::Ui,
    squares: &HashMap<drag::Square, (egui::Rect, String, String)>,
    button: egui::PointerButton,
) -> drag::Pointer {
    let at = ui.input(|i| i.pointer.interact_pos());
    let on = at.and_then(|at| {
        squares
            .iter()
            .find(|(_, s)| {
                s.0.contains(at)
                    && ui.clip_rect().contains(at)
                    && (button == egui::PointerButton::Secondary || !s.2.is_empty())
            })
            .map(|(&square, s)| (square, (at - s.0.min) / s.0.size()))
    });
    let on_top = ui.ctx().layer_id_at(at.unwrap_or_default()) == Some(ui.layer_id());
    let panning = canvas::panning(ui);
    ui.input(|i| drag::Pointer {
        over: on.map(|(square, _)| square),
        spot: on.map(|(_, spot)| (spot.x, spot.y)),
        pressed: i.pointer.button_pressed(button) && on_top && !panning,
        down: i.pointer.button_down(button) && !panning,
        released: i.pointer.button_released(button) && !panning,
    })
}

/// A dark line under a light one, so the outline reads on pale and dark squares alike.
fn outline_shapes(rect: egui::Rect, color: egui::Color32) -> Vec<egui::Shape> {
    let rect = rect.shrink(1.0);
    let strokes = [
        egui::Stroke::new(4.0, egui::Color32::from_black_alpha(160)),
        egui::Stroke::new(2.0, color),
    ];
    strokes
        .map(|stroke| egui::Shape::rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Middle))
        .into()
}

/// The sizes a grid's cells are drawn at.
#[derive(Clone, Copy)]
struct GridCells {
    size: egui::Vec2,
    /// Side of the glyphs.
    glyph_size: f32,
    /// Whether the keyboard is on the grid, which then outlines the square its focus reached.
    keyboard: bool,
}

/// The squares of a grid sent with `blank` that show something else, and its cursor, from the first of `drawn`
/// columns and rows; none when every square is blank.
fn content_span(
    grid: &Grid,
    (xs, ys): (&std::ops::Range<u32>, &std::ops::Range<u32>),
) -> Option<egui::Rect> {
    let blank = grid.blank.as_ref()?;
    let shown = grid
        .cells
        .iter()
        .filter(|c| {
            let at_origin = GridCell {
                x: blank.x,
                y: blank.y,
                id: blank.id.clone(),
                ..(*c).clone()
            };
            at_origin != *blank
        })
        .map(|c| (c.x, c.y));
    let cursor = grid
        .cursor
        .and_then(|c| Some((u32::try_from(c.x).ok()?, u32::try_from(c.y).ok()?)));
    shown
        .chain(cursor)
        .map(|(x, y)| {
            let at = egui::pos2(
                x.saturating_sub(xs.start) as f32,
                y.saturating_sub(ys.start) as f32,
            );
            egui::Rect::from_min_size(at, egui::Vec2::splat(1.0))
        })
        .reduce(|a, b| a.union(b))
}

/// The columns and rows of `grid` from its first to its last cell or cursor: the empty edges around them draw
/// nothing, so they take no room.
pub(super) fn drawn_span(grid: &Grid) -> (std::ops::Range<u32>, std::ops::Range<u32>) {
    let cursor = grid
        .cursor
        .and_then(|c| Some((u32::try_from(c.x).ok()?, u32::try_from(c.y).ok()?)));
    let at = grid.cells.iter().map(|c| (c.x, c.y)).chain(cursor);
    let span = |pick: fn((u32, u32)) -> u32, end: u32| {
        let (lo, hi) = at
            .clone()
            .map(pick)
            .filter(|&v| v < end)
            .fold((end, 0), |(lo, hi), v| (lo.min(v), hi.max(v + 1)));
        lo.min(hi)..hi
    };
    (span(|p| p.0, grid.cols), span(|p| p.1, grid.rows))
}
