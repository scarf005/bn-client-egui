use bn_client_core::protocol::{GridSpan, GridSquare, SpanShape};
use serde_json::Value;

/// Every case of the engine's `span_shapes.json`: the squares a span drag shows are those the release draws.
#[test]
fn a_span_covers_the_squares_the_engine_draws() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/span_shapes.json"
    ))
    .unwrap();
    let fixture: Value = serde_json::from_str(&text).unwrap();
    let (cols, rows) = (
        fixture["cols"].as_i64().unwrap() as i32,
        fixture["rows"].as_i64().unwrap() as i32,
    );
    let point = |v: &Value| (v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32);
    let points = |v: &Value| v.as_array().unwrap().iter().map(point).collect::<Vec<_>>();
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() > 100);
    for case in cases {
        let span = GridSpan {
            shape: serde_json::from_value::<SpanShape>(case["shape"].clone()).unwrap(),
            size: case["size"].as_u64().unwrap() as u32,
            mirror_x: case["mirror_x"].as_bool().unwrap(),
            mirror_y: case["mirror_y"].as_bool().unwrap(),
            from: GridSquare { x: 0, y: 0 },
            to: GridSquare {
                x: cols - 1,
                y: rows - 1,
            },
            looks: vec![],
            fill: vec![],
        };
        let got = span.squares(point(&case["from"]), point(&case["to"]));
        assert_eq!(got.drawn, points(&case["squares"]), "{case}");
        let fill = case.get("fill").map(points).unwrap_or_default();
        assert_eq!(got.fill, fill, "{case}");
    }
}
