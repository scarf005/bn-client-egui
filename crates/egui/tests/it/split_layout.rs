//! The width rule of split panes: content-sized, never below the minimum, the rest to the panes that grow.
use bn_client_egui::kit::split::{
    Extent, detail_widths, gaps, overflows, prose_widths, split_widths, whole_rows, wrap_lines,
};

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

#[test]
fn a_list_beside_its_detail_takes_two_fifths_of_the_room_or_its_natural_width() {
    let detail = pane(50.0, 300.0, true);
    let narrow = detail_widths(&[pane(80.0, 120.0, true), detail], 1000.0, GAP);
    assert_eq!(narrow, vec![400.0, 1000.0 - 400.0 - GAP]);
    let wide = detail_widths(&[pane(80.0, 520.0, true), detail], 1000.0, GAP);
    assert_eq!(wide, vec![520.0, 1000.0 - 520.0 - GAP]);
}

#[test]
fn a_list_beside_its_detail_leaves_the_detail_its_minimum() {
    let widths = detail_widths(
        &[pane(80.0, 900.0, true), pane(150.0, 300.0, true)],
        1000.0,
        GAP,
    );
    assert_eq!(widths, vec![1000.0 - 150.0 - GAP, 150.0]);
    assert_eq!(
        detail_widths(&[pane(80.0, 120.0, true)], 600.0, GAP),
        vec![600.0]
    );
}

#[test]
fn the_only_prose_takes_what_the_other_panes_leave_of_their_natural_widths() {
    let parts = [pane(100.0, 900.0, true), pane(100.0, 200.0, true)];
    let widths = prose_widths(&parts, 0, 1000.0, GAP);
    assert_eq!(widths, vec![1000.0 - GAP - 200.0, 200.0]);
}

#[test]
fn the_only_prose_longer_than_an_equal_share_gets_one() {
    let parts = [pane(100.0, 900.0, true), pane(100.0, 500.0, true)];
    let widths = prose_widths(&parts, 0, 600.0 + GAP, GAP);
    assert_eq!(widths, vec![300.0, 300.0]);
    // A short text is no wider than its line, whatever share it could have.
    let short = [pane(100.0, 900.0, true), pane(50.0, 120.0, true)];
    let widths = prose_widths(&short, 1, 600.0 + GAP, GAP);
    assert_eq!(widths, vec![600.0 - 120.0, 120.0]);
}

#[test]
fn a_short_prose_wraps_to_leave_the_other_panes_their_natural_widths() {
    let parts = [pane(80.0, 290.0, true), pane(100.0, 560.0, true)];
    let widths = prose_widths(&parts, 0, 800.0 + GAP, GAP);
    assert_eq!(widths, vec![240.0, 560.0]);
}

#[test]
fn the_only_prose_never_takes_the_other_panes_below_their_minimums() {
    let parts = [pane(400.0, 500.0, true), pane(100.0, 900.0, true)];
    let widths = prose_widths(&parts, 1, 600.0 + GAP, GAP);
    assert_eq!(widths, vec![400.0, 200.0]);
}

#[test]
fn a_viewport_ends_above_the_line_it_would_cut_and_at_the_room_between_lines() {
    let lines = [(0.0, 20.0), (24.0, 44.0), (48.0, 68.0)];
    assert_eq!(whole_rows(&lines, 60.0), 48.0);
    assert_eq!(whole_rows(&lines, 40.0), 24.0);
    assert_eq!(whole_rows(&lines, 46.0), 46.0);
    assert_eq!(whole_rows(&lines, 100.0), 100.0);
    assert_eq!(whole_rows(&[], 60.0), 60.0);
}

#[test]
fn lines_side_by_side_that_overlap_move_the_end_above_both() {
    let panes = [(0.0, 20.0), (4.0, 24.0), (30.0, 50.0), (34.0, 54.0)];
    assert_eq!(whole_rows(&panes, 40.0), 30.0);
}

#[test]
fn a_chain_of_stale_lines_moves_the_end_up_by_a_few_lines_not_to_the_top() {
    let stale = (0..40)
        .map(|n| (n as f32 * 15.0, n as f32 * 15.0 + 18.0))
        .collect::<Vec<_>>();
    let end = whole_rows(&stale, 600.0);
    assert!((540.0..600.0).contains(&end), "{end}");
}
