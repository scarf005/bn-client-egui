//! The width rule of split panes: content-sized, never below the minimum, the rest to the panes that grow.
use bn_client_egui::layout::{Extent, gaps, overflows, split_widths, wrap_lines};

const GAP: f32 = 8.0;

fn pane(min: f32, natural: f32, grows: bool) -> Extent {
    Extent {
        min,
        natural,
        grows,
    }
}

#[test]
fn panes_take_their_natural_widths_when_the_room_holds_them() {
    let parts = [pane(60.0, 120.0, false), pane(100.0, 180.0, false)];
    assert_eq!(split_widths(&parts, 800.0, GAP).len(), 2);
    // With no pane growing, the room left is shared; each is at least natural.
    let widths = split_widths(&parts, 308.0, GAP);
    assert_eq!(widths, vec![120.0, 180.0]);
}

#[test]
fn a_pane_never_gets_less_than_its_minimum_even_when_the_room_is_short() {
    let parts = [pane(150.0, 300.0, true), pane(180.0, 240.0, true)];
    let widths = split_widths(&parts, 200.0, GAP);
    assert_eq!(widths, vec![150.0, 180.0]);
    assert!(overflows(&widths, 200.0, GAP), "the split has to scroll");
}

#[test]
fn between_minimum_and_natural_the_spare_room_goes_by_need() {
    let parts = [pane(100.0, 200.0, false), pane(100.0, 400.0, false)];
    // 100 spare of 400 wanted: a quarter of each pane's want.
    let widths = split_widths(&parts, 100.0 + 100.0 + 100.0 + GAP, GAP);
    assert_eq!(widths, vec![125.0, 175.0]);
    assert!(!overflows(&widths, 308.0, GAP));
}

#[test]
fn the_room_left_over_goes_to_the_panes_that_grow() {
    let parts = [pane(80.0, 80.0, false), pane(100.0, 150.0, true)];
    let widths = split_widths(&parts, 600.0, GAP);
    assert_eq!(widths[0], 80.0);
    assert_eq!(widths[1], 600.0 - GAP - 80.0);
}

#[test]
fn with_no_growing_pane_the_leftover_is_shared_equally() {
    let parts = [pane(50.0, 50.0, false), pane(70.0, 70.0, false)];
    let widths = split_widths(&parts, 120.0 + GAP + 40.0, GAP);
    assert_eq!(widths, vec![70.0, 90.0]);
}

#[test]
fn panes_never_come_out_equal_unless_their_content_is() {
    let parts = [
        pane(60.0, 60.0, false),
        pane(200.0, 200.0, false),
        pane(90.0, 90.0, false),
    ];
    let widths = split_widths(&parts, 350.0 + 2.0 * GAP, GAP);
    assert_eq!(widths, vec![60.0, 200.0, 90.0]);
}

#[test]
fn gaps_count_between_panes_only() {
    assert_eq!(gaps(0, GAP), 0.0);
    assert_eq!(gaps(1, GAP), 0.0);
    assert_eq!(gaps(3, GAP), 2.0 * GAP);
}

#[test]
fn panes_whose_minimums_fit_stay_on_one_line_and_the_rest_wrap_by_natural_width() {
    let parts = [
        pane(100.0, 300.0, false),
        pane(100.0, 300.0, false),
        pane(100.0, 300.0, false),
    ];
    assert_eq!(wrap_lines(&parts, 320.0, GAP), vec![0..3]);
    assert_eq!(wrap_lines(&parts, 300.0, GAP), vec![0..1, 1..2, 2..3]);
    assert_eq!(wrap_lines(&parts, 300.0 * 2.0 + GAP, GAP), vec![0..3]);
    let wide = [
        pane(250.0, 600.0, false),
        pane(80.0, 100.0, false),
        pane(80.0, 100.0, false),
    ];
    assert_eq!(wrap_lines(&wide, 300.0, GAP), vec![0..1, 1..3]);
}
