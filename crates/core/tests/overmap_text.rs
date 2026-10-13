//! What a screen reader is told about an overmap square's notes.
use bn_client_core::Overmap;

fn overmap(notes: &[((i32, i32, i32), &str)]) -> Overmap {
    Overmap {
        cols: 0,
        rows: 0,
        cells: vec![],
        origin: (0, 0, 0),
        cursor: (51, 50, 0),
        player: (50, 50, 0),
        legend: vec![],
        cursor_description: String::new(),
        notes: notes.iter().map(|(p, t)| (*p, t.to_string())).collect(),
        path: vec![],
        tiles: vec![],
    }
}

#[test]
fn notes_are_listed_with_their_squares() {
    let om = overmap(&[((49, 50, 0), "cache"), ((60, 61, -1), "base")]);
    assert_eq!(
        om.notes_text().as_deref(),
        Some("Notes: (49, 50, 0) cache; (60, 61, -1) base.")
    );
    assert_eq!(overmap(&[]).notes_text(), None);
}
