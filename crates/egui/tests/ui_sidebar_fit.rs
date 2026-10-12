use bn_client_core::Rgb;
use bn_client_core::sidebar::{Line, Span};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

fn span(text: &str) -> Span {
    Span {
        text: text.into(),
        color: Rgb(255, 255, 255),
    }
}

fn leg() -> Line {
    ["R LEG", "84/84", "Spd 100", "0 walk", "Stm 10000/10000"]
        .map(span)
        .to_vec()
}

fn shown(width: f32) -> (String, f32) {
    let lines = vec![leg()];
    let h = Harness::builder()
        .with_size(egui::vec2(width, 200.0))
        .build_ui(move |ui| bn_client_egui::sidebar::show(ui, &lines));
    let label = h.get_by_label_contains("R LEG");
    let text = label
        .accesskit_node()
        .value()
        .unwrap_or_default()
        .to_string();
    (text, label.rect().width())
}

#[test]
fn a_wide_panel_keeps_the_native_spacing_on_one_line() {
    let (text, _) = shown(600.0);
    assert_eq!(text, "R LEG  84/84  Spd 100  0 walk  Stm 10000/10000");
}

#[test]
fn a_narrower_panel_drops_the_padding_before_it_breaks_anything() {
    let (text, width) = shown(420.0);
    assert_eq!(text, "R LEG 84/84 Spd 100 0 walk Stm 10000/10000");
    assert!(width <= 420.0, "{width}");
}

#[test]
fn a_narrow_panel_breaks_between_spans_and_never_inside_a_label_and_value_pair() {
    let (text, width) = shown(260.0);
    assert!(text.contains('\n'), "{text:?}");
    for pair in ["R LEG", "84/84", "Spd 100", "0 walk", "Stm 10000/10000"] {
        assert!(text.contains(pair), "{pair} kept whole in {text:?}");
    }
    assert!(width <= 260.0, "{width}");
}

/// Where the first letter of `word` is painted on the row that holds `row`, in points.
fn start_of(h: &Harness<'_, Vec<Line>>, row: &str, word: &str) -> f32 {
    h.output()
        .shapes
        .iter()
        .find_map(|clipped| {
            let egui::Shape::Text(text) = &clipped.shape else {
                return None;
            };
            if !text.galley.text().contains(row) {
                return None;
            }
            let glyphs = text
                .galley
                .rows
                .iter()
                .flat_map(|r| &r.glyphs)
                .collect::<Vec<_>>();
            let chars = word.chars().collect::<Vec<_>>();
            let at = glyphs
                .windows(chars.len())
                .position(|w| w.iter().map(|g| g.chr).eq(chars.iter().copied()))?;
            Some(text.pos.x + glyphs[at].pos.x)
        })
        .unwrap_or_else(|| panic!("{word} is not painted on the {row} row"))
}

fn status(spans: &[&str]) -> Vec<Line> {
    vec![spans.iter().map(|t| span(t)).collect()]
}

fn stepper(width: f32, first: &[&str]) -> Harness<'static, Vec<Line>> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(width, 200.0))
        .build_ui_state(
            |ui, lines: &mut Vec<Line>| bn_client_egui::sidebar::show(ui, lines),
            status(first),
        );
    h.run_steps(3);
    h
}

#[test]
fn values_changing_while_moving_never_move_the_other_spans_of_the_row() {
    let before = ["R LEG", "84/84", "Spd 100", "100 walk", "Stm 9055/10000"];
    let after = ["R LEG", "84/84", "Spd 100", "0 walk", "Stm 10000/10000"];
    for width in [600.0, 420.0, 300.0] {
        let mut h = stepper(width, &before);
        let places = |h: &Harness<'_, Vec<Line>>| {
            ["84/84", "Spd", "walk", "Stm"].map(|word| start_of(h, "R LEG", word))
        };
        let (a, b) = (places(&h), {
            *h.state_mut() = status(&after);
            h.run_steps(3);
            places(&h)
        });
        // The counter's own digits move its word; every span that begins a column stays where it was.
        assert_eq!(
            (a[0], a[1], a[3]),
            (b[0], b[1], b[3]),
            "at {width}: {a:?} {b:?}"
        );
        *h.state_mut() = status(&before);
        h.run_steps(3);
        assert_eq!(places(&h), a, "back again at {width}");
    }
}

#[test]
fn a_span_that_prints_nothing_keeps_the_others_in_their_columns() {
    let mut h = stepper(600.0, &["R LEG", "84/84", "Pain 3", "Str 8"]);
    let with = start_of(&h, "R LEG", "Str");
    *h.state_mut() = status(&["R LEG", "84/84", "", "Str 8"]);
    h.run_steps(3);
    assert_eq!(start_of(&h, "R LEG", "Str"), with);
}

#[test]
fn a_wide_row_below_the_table_never_widens_the_table() {
    let width = |extra: bool| {
        let mut lines = vec![leg(); 6];
        if extra {
            lines.push(["Weather:", "a long forecast of rain"].map(span).to_vec());
        }
        let h = Harness::builder()
            .with_size(egui::vec2(600.0, 400.0))
            .build_ui(move |ui| bn_client_egui::sidebar::show(ui, &lines));
        h.get_all_by_label_contains("R LEG")
            .next()
            .unwrap()
            .rect()
            .width()
    };
    assert_eq!(width(true), width(false));
}
