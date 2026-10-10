//! What a screen reader is told about an overmap square: the native sidebar's description, no frame.
use bn_client_core::Overmap;

fn overmap(legend: &[&str], notes: &[((i32, i32, i32), &str)]) -> Overmap {
    Overmap {
        cols: 0,
        rows: 0,
        cells: vec![],
        origin: (0, 0, 0),
        cursor: (51, 50, 0),
        player: (50, 50, 0),
        legend: legend.iter().map(|s| s.to_string()).collect(),
        notes: notes.iter().map(|(p, t)| (*p, t.to_string())).collect(),
    }
}

#[test]
fn the_square_is_described_by_the_sidebars_first_paragraph_without_frame_or_glyph() {
    let om = overmap(
        &[
            "│",
            "│# lake on the northwest",
            "│  outskirts of Bristol",
            "│",
            "│Use movement keys to pan.",
        ],
        &[],
    );
    assert_eq!(
        om.here().as_deref(),
        Some("lake on the northwest outskirts of Bristol")
    );
}

#[test]
fn an_empty_sidebar_describes_nothing() {
    assert_eq!(overmap(&["│", "│"], &[]).here(), None);
    assert_eq!(overmap(&[], &[]).here(), None);
}

#[test]
fn a_glyph_that_is_a_word_is_kept() {
    assert_eq!(overmap(&["│forest"], &[]).here().as_deref(), Some("forest"));
}

#[test]
fn notes_are_listed_with_their_squares() {
    let om = overmap(&[], &[((49, 50, 0), "cache"), ((60, 61, -1), "base")]);
    assert_eq!(
        om.notes_text().as_deref(),
        Some("Notes: (49, 50, 0) cache; (60, 61, -1) base.")
    );
    assert_eq!(overmap(&[], &[]).notes_text(), None);
}
