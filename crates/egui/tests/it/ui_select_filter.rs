//! A select field with many options filters them by what is typed in its popup, and the popup is tall.
use bn_client_egui::editor::{ValueField, value_editor};
use bn_client_egui::protocol::{Editor, EditorValue};
use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn editor(count: usize) -> Editor {
    Editor {
        kind: "select".into(),
        value: "f0".into(),
        values: (0..count)
            .map(|n| EditorValue {
                id: format!("f{n}"),
                label: format!("fuel {n} {}", if n == 17 { "gasoline" } else { "other" }),
            })
            .collect(),
        minimum: None,
        maximum: None,
        max_length: -1,
        multiline: false,
    }
}

fn harness<'a>(count: usize, sent: std::rc::Rc<std::cell::RefCell<Vec<String>>>) -> Harness<'a> {
    let editor = editor(count);
    Harness::builder()
        .with_size(egui::vec2(600.0, 700.0))
        .build_ui(move |ui| {
            let edit = value_editor(
                ui,
                &ValueField {
                    id: "fuel",
                    label: "Fuel",
                    editor: &editor,
                    enabled: true,
                    refusal: None,
                    required: false,
                },
            );
            sent.borrow_mut().extend(edit.committed);
        })
}

#[test]
fn a_long_select_filters_its_options_by_what_is_typed() {
    let sent = Default::default();
    let mut h = harness(40, std::rc::Rc::clone(&sent));
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Fuel")
        .click();
    h.run();
    h.get_by_label("fuel 17 gasoline");
    h.get_by_label("fuel 3 other");
    h.event(egui::Event::Text("gaso".into()));
    h.run();
    assert!(h.query_by_label("fuel 3 other").is_none());
    h.get_by_label("fuel 17 gasoline").click();
    h.run();
    assert_eq!(*sent.borrow(), ["f17"]);
}

#[test]
fn a_short_select_has_no_filter_box() {
    let mut h = harness(5, Default::default());
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Fuel")
        .click();
    h.run();
    h.get_by_label("fuel 3 other");
    assert!(
        h.query_by_role(egui::accesskit::Role::TextInput).is_none(),
        "five options need no filter"
    );
}

#[test]
fn enter_in_the_filter_picks_the_first_match_and_the_next_opening_starts_unfiltered() {
    let sent = Default::default();
    let mut h = harness(40, std::rc::Rc::clone(&sent));
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Fuel")
        .click();
    h.run();
    h.event(egui::Event::Text("gaso".into()));
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(*sent.borrow(), ["f17"]);
    assert!(
        h.query_by_label("fuel 3 other").is_none(),
        "the popup closed"
    );
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Fuel")
        .click();
    h.run();
    h.get_by_label("fuel 3 other");
}
