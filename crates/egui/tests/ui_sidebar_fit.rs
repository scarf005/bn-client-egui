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
    let text = label.accesskit_node().value().unwrap_or_default().to_string();
    (text, label.rect().width())
}

#[test]
fn a_wide_panel_keeps_the_native_spacing_on_one_line() {
    let (text, _) = shown(600.0);
    assert_eq!(text, "R LEG  84/84  Spd 100  0 walk  Stm 10000/10000");
}

#[test]
fn a_narrower_panel_drops_the_padding_before_it_breaks_anything() {
    let (text, width) = shown(400.0);
    assert_eq!(text, "R LEG 84/84 Spd 100 0 walk Stm 10000/10000");
    assert!(width <= 400.0, "{width}");
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
