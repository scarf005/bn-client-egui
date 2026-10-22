//! The widths components need, measured without laying them out.

use super::*;
use crate::kit::table;

/// The smallest font a canvas is drawn in.
const CANVAS_MIN_FONT: f32 = 10.0;

/// The width of `text` on one line.
pub(in crate::app) fn text_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.into(), font.clone(), egui::Color32::WHITE)
        .size()
        .x
}

/// The width of the longest line of `text`, unwrapped.
fn line_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    text.lines()
        .map(|line| text_width(ui, line, font))
        .fold(0.0, f32::max)
}

/// The width of the longest word of `text`: the least a wrapped label can be without breaking inside one.
fn word_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    text.split_whitespace()
        .map(|word| text_width(ui, word, font))
        .fold(0.0, f32::max)
}

/// What `extent` needs, with the margin a split leaves around its pane.
pub(super) fn with_margin(extent: Extent) -> Extent {
    let margin = 2.0 * f32::from(PANE_MARGIN);
    Extent {
        min: extent.min + margin,
        natural: extent.natural + margin,
        ..extent
    }
}

/// A stat's value with its maximum and unit, as the panel shows it.
pub(super) fn stat_text(stat: &PanelStat) -> String {
    let max = stat
        .max
        .as_ref()
        .map_or(String::new(), |m| format!("/{}", m.shown()));
    let unit = stat
        .unit
        .as_ref()
        .map_or(String::new(), |u| format!(" {}", u.shown()));
    format!("{}{max}{unit}", markup::plain(&stat.value.shown()))
}

impl<M: GameModel> App<M> {
    /// What `c` needs in width, measured without laying it out: `room` is what its surroundings offer.
    pub(super) fn extent(
        &self,
        ui: &egui::Ui,
        screen: &Screen,
        c: &Component,
        room: f32,
    ) -> Extent {
        let gap = ui.spacing().item_spacing.x;
        let font = egui::TextStyle::Body.resolve(ui.style());
        let stacked = |a: Extent, b: Extent| Extent {
            min: a.min.max(b.min),
            natural: a.natural.max(b.natural),
            grows: a.grows || b.grows,
        };
        let fixed = |w: f32| Extent {
            min: w,
            natural: w,
            grows: false,
        };
        let prose = |text: &str| Extent {
            min: word_width(ui, text, &font),
            natural: line_width(ui, text, &font),
            grows: false,
        };
        let inside = |child: &Option<Box<Component>>| {
            child
                .as_ref()
                .map(|child| self.extent(ui, screen, child, room))
        };
        let own = match c {
            Component::List(list) => {
                let rows = self.known_rows(list);
                let widths = self.list_widths(ui, list, &rows);
                // The rows sit beside their scroll bar.
                let bar = ui.spacing().scroll.allocated_width();
                let natural = widths.natural(gap) + bar;
                // Every column keeps its values in sight; only the label is cut, down to its longest word.
                let least = natural - widths.label + widths.label_min;
                Extent {
                    min: least.min(room),
                    natural,
                    grows: true,
                }
            }
            Component::Tabs(tabs) => {
                let pad = 2.0 * ui.spacing().button_padding.x;
                let labels = tabs
                    .tabs
                    .iter()
                    .map(|t| text_width(ui, &markup::plain(&t.label.shown()), &font) + pad)
                    .collect::<Vec<_>>();
                let row = Extent {
                    min: labels.iter().copied().fold(0.0, f32::max),
                    natural: labels.iter().map(|w| w + gap).sum(),
                    grows: false,
                };
                inside(&tabs.child).map_or(row, |child| stacked(row, child))
            }
            Component::Split(split) => {
                let (headings, parts) =
                    split.children.iter().partition::<Vec<_>, _>(|c| heading(c));
                let parts = parts
                    .into_iter()
                    .map(|c| with_margin(self.extent(ui, screen, c, room)))
                    .collect::<Vec<_>>();
                // Peers wrap onto further lines, so they need only the room of their widest.
                let min = match split.relation {
                    Relation::Peers => parts.iter().map(|p| p.min).fold(0.0, f32::max),
                    Relation::Detail => {
                        parts.iter().map(|p| p.min).sum::<f32>() + split::gaps(parts.len(), gap)
                    }
                };
                let beside = Extent {
                    min,
                    natural: parts.iter().map(|p| p.natural).sum::<f32>()
                        + split::gaps(parts.len(), gap),
                    grows: parts.iter().any(|p| p.grows),
                };
                headings
                    .into_iter()
                    .map(|c| self.extent(ui, screen, c, room))
                    .fold(beside, stacked)
            }
            Component::Panel(panel) => {
                let frame = section::margin_width();
                let stats = panel
                    .stats
                    .iter()
                    .flatten()
                    .map(|stat| (markup::plain(&stat.label.shown()), stat_text(stat)))
                    .collect::<Vec<_>>();
                let width = |(label, value): &(String, String)| {
                    (text_width(ui, label, &font), text_width(ui, value, &font))
                };
                // A fact without a value is a line of prose, wrapped at its longest word; one with a value
                // keeps its label beside it.
                let (valued, plain) = stats.iter().partition::<Vec<_>, _>(|(_, v)| !v.is_empty());
                let valued = section::facts_width(ui, valued.into_iter().map(width));
                let word = plain
                    .iter()
                    .map(|(label, _)| word_width(ui, label, &font))
                    .fold(0.0, f32::max);
                let stats = Extent {
                    min: valued.max(word),
                    natural: section::facts_width(ui, stats.iter().map(width)),
                    grows: false,
                };
                let lines = [Some(prose(&markup::plain(&panel.title.shown())))]
                    .into_iter()
                    .flatten()
                    .chain(
                        panel
                            .subtitle
                            .iter()
                            .map(|t| prose(&markup::plain(&t.shown()))),
                    )
                    .chain([stats])
                    .chain(inside(&panel.child))
                    .fold(fixed(0.0), stacked);
                Extent {
                    min: lines.min + frame,
                    natural: lines.natural + frame,
                    ..lines
                }
            }
            Component::Form(form) => {
                let label = form
                    .fields
                    .iter()
                    .map(|f| text_width(ui, &markup::plain(&f.label.shown()), &font))
                    .fold(0.0, f32::max);
                // A one-line field is the theme's width, or wider to show its whole value.
                let typed = |f: &&FormField| {
                    matches!(
                        f.kind,
                        FieldKind::Text | FieldKind::Integer | FieldKind::Float
                    ) && f.multiline != Some(true)
                };
                let field = form
                    .fields
                    .iter()
                    .filter(typed)
                    .map(|f| editor::field_width(ui, &f.value))
                    .fold(ui.spacing().text_edit_width, f32::max);
                Extent {
                    grows: true,
                    ..fixed(label + gap + field)
                }
            }
            Component::Text(block) => {
                let text = markup::plain(&block.markup.shown());
                match block.role {
                    Some(TextRole::TerminalOutput | TextRole::Banner) => {
                        let mono = egui::TextStyle::Monospace.resolve(ui.style());
                        fixed(line_width(ui, &text, &mono))
                    }
                    Some(TextRole::Prompt) | None => Extent {
                        grows: true,
                        ..prose(&text)
                    },
                }
            }
            Component::Grid(grid) => {
                let cols = drawn_span(grid).0.len() as f32;
                Extent {
                    min: cols * (GRID_CELL + gap),
                    natural: cols * (GRID_MAX_CELL + gap),
                    grows: false,
                }
            }
            // The overmap's squares take all the room they get; the game's map is drawn under the window, and
            // its slot here holds a few lines.
            Component::Map(map) => Extent {
                min: 160.0,
                natural: match map.mode {
                    MapMode::Overmap => room,
                    MapMode::World => room.min(480.0),
                },
                grows: true,
            },
            Component::Canvas(canvas) => {
                let mono = egui::FontId::monospace(CANVAS_MIN_FONT);
                let cell = text_width(ui, "0", &mono);
                Extent {
                    grows: true,
                    ..fixed(canvas.cols as f32 * cell)
                }
            }
        };
        // Its buttons sit side by side, none broken over lines, and wrap to further rows when they do not fit.
        let buttons = own_buttons(screen, c)
            .into_iter()
            .map(|a| button_width(ui, a))
            .collect::<Vec<_>>();
        let own = stacked(
            own,
            Extent {
                min: buttons.iter().copied().fold(0.0, f32::max),
                natural: buttons.iter().sum::<f32>() + split::gaps(buttons.len(), gap),
                grows: false,
            },
        );
        // Every part but a panel draws its heading over itself, on one line.
        match (&c.head().label, c) {
            (_, Component::Panel(_)) | (None, _) => own,
            (Some(label), _) => stacked(
                own,
                fixed(text_width(ui, &markup::plain(&label.shown()), &font)),
            ),
        }
    }

    /// What each column of a list needs: the widest of its rows' parts, and of a value column's caption.
    pub(super) fn list_widths(&self, ui: &egui::Ui, list: &List, rows: &[&Row]) -> table::Widths {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let spacing = ui.spacing();
        let widest = |widths: &mut dyn Iterator<Item = f32>| widths.fold(0.0, f32::max);
        let plain = |markup: &str| text_width(ui, &markup::plain(markup), &font);
        let items = rows.iter().filter(|r| r.header != Some(true));
        let pick = widest(&mut items.clone().map(|row| {
            let check = Self::row_check(list, row, false).map(|_| table::CHECK);
            let marks = Self::row_marks(row).map(|m| plain(&m));
            let parts = [check, marks].into_iter().flatten().collect::<Vec<_>>();
            parts.iter().sum::<f32>() + split::gaps(parts.len(), spacing.item_spacing.x)
        }));
        let item = match self.squares(rows) {
            true => tokens::ITEM,
            false => widest(
                &mut items
                    .clone()
                    .filter_map(|r| r.look.as_ref())
                    .map(|l| plain(&l.glyph)),
            ),
        };
        let keys = items.clone().filter_map(|r| r.hotkey.as_deref());
        let key = widest(&mut keys.clone().map(|k| text_width(ui, k, &table::key_font())));
        let key = match keys.count() {
            0 => 0.0,
            _ => key.max(table::key_min(ui)),
        };
        // A label sits in its button, a nested one past its indent.
        let pad = 2.0 * spacing.button_padding.x;
        let indent = |row: &Row| f32::from(u8::from(row.parent_id.is_some())) * tokens::INDENT;
        let label = widest(
            &mut rows
                .iter()
                .map(|row| indent(row) + plain(&Self::row_label(row)) + pad),
        );
        let label_min = widest(&mut rows.iter().map(|row| {
            indent(row) + word_width(ui, &markup::plain(&Self::row_label(row)), &font) + pad
        }));
        let caption = egui::FontId::proportional(tokens::CAPTION);
        let values = list
            .columns
            .iter()
            .map(|column| {
                let caption = text_width(ui, &markup::plain(&column.label.shown()), &caption);
                let values = rows.iter().map(|row| {
                    let value = row
                        .columns
                        .iter()
                        .find(|c| c.key.as_deref() == Some(column.key.as_str()));
                    value.map_or(0.0, |c| plain(&c.value.shown()))
                });
                caption.max(widest(&mut { values }))
            })
            .collect();
        let count = match list.selection {
            Selection::Count => spacing.interact_size.x,
            _ => 0.0,
        };
        table::Widths {
            pick,
            item,
            key,
            label_min,
            label,
            values,
            count,
        }
    }
}
