//! The widths components need, measured without laying them out.

use super::*;

/// The widths a list's label and its columns need.
pub(super) struct ListWidths {
    pub(super) label: f32,
    pub(super) columns: Vec<f32>,
}

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
    pub(super) fn extent(&self, ui: &egui::Ui, c: &Component, room: f32) -> Extent {
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
            child.as_ref().map(|child| self.extent(ui, child, room))
        };
        match c {
            Component::List(list) => {
                let rows = self.model.rows(list);
                let widths = self.list_widths(ui, list, &rows);
                let natural = widths.label + widths.columns.iter().map(|w| w + gap).sum::<f32>();
                Extent {
                    min: natural.min(room / 2.0),
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
                let parts = split
                    .children
                    .iter()
                    .map(|c| with_margin(self.extent(ui, c, room)))
                    .collect::<Vec<_>>();
                Extent {
                    min: parts.iter().map(|p| p.min).sum::<f32>() + layout::gaps(parts.len(), gap),
                    natural: parts.iter().map(|p| p.natural).sum::<f32>()
                        + layout::gaps(parts.len(), gap),
                    grows: parts.iter().any(|p| p.grows),
                }
            }
            Component::Panel(panel) => {
                let frame = egui::Frame::group(ui.style()).total_margin().sum().x;
                let stats = panel.stats.iter().flatten().map(|stat| {
                    let label = markup::plain(&stat.label.shown());
                    fixed(
                        text_width(ui, &label, &font)
                            + gap
                            + text_width(ui, &stat_text(stat), &font),
                    )
                });
                let lines = [Some(prose(&markup::plain(&panel.title.shown())))]
                    .into_iter()
                    .flatten()
                    .chain(
                        panel
                            .subtitle
                            .iter()
                            .map(|t| prose(&markup::plain(&t.shown()))),
                    )
                    .chain(stats)
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
                let w = label + ui.spacing().text_edit_width;
                Extent {
                    min: w,
                    natural: w,
                    grows: true,
                }
            }
            Component::Text(block) => {
                let text = markup::plain(&block.markup.shown());
                match block.role {
                    Some(TextRole::TerminalOutput | TextRole::Banner) => {
                        let mono = egui::TextStyle::Monospace.resolve(ui.style());
                        fixed(line_width(ui, &text, &mono))
                    }
                    None => Extent {
                        grows: true,
                        ..prose(&text)
                    },
                }
            }
            Component::Grid(grid) => fixed(drawn_span(grid).0.len() as f32 * (GRID_CELL + gap)),
            Component::Map(_) => Extent {
                min: 160.0,
                natural: room.min(480.0),
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
        }
    }

    /// The widths a list's label and each of its columns need: the widest caption or value in each.
    pub(super) fn list_widths(&self, ui: &egui::Ui, list: &List, rows: &[&Row]) -> ListWidths {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let spacing = ui.spacing();
        let pad = 2.0 * spacing.button_padding.x;
        let widest = |texts: &mut dyn Iterator<Item = f32>| texts.fold(0.0, f32::max);
        let squares = self.squares(rows);
        let icon = if squares {
            list::ICON + spacing.item_spacing.x
        } else {
            0.0
        };
        let label = widest(
            &mut rows.iter().filter(|r| r.header != Some(true)).map(|row| {
                let square = squares && row.look.is_some();
                let marked = Self::row_markup(list, row, false, square);
                icon + text_width(ui, &markup::plain(&marked), &font) + pad
            }),
        );
        let columns = list
            .columns
            .iter()
            .map(|column| {
                let caption = text_width(ui, &markup::plain(&column.label.shown()), &font);
                let values = rows.iter().map(|row| {
                    let value = row
                        .columns
                        .iter()
                        .find(|c| c.key.as_deref() == Some(column.key.as_str()));
                    value.map_or(0.0, |c| {
                        text_width(ui, &markup::plain(&c.value.shown()), &font)
                    })
                });
                caption.max(widest(&mut { values }))
            })
            .collect();
        ListWidths { label, columns }
    }
}
