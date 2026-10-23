//! The squares a screen marks on the map besides its cursor: the map editor's selection, mapgen border and stamp.
use crate::common::session;
use bn_client_egui::app::CELL;
use bn_client_egui::{App, Clock, EngineModel, GameModel};
use eframe::egui;
use egui_kittest::Harness;
use serde_json::{Value, json};

type H<'a> = Harness<'a, App<EngineModel>>;

/// The client over the fixture session, whose avatar stands at world (3, 4).
fn harness<'a>() -> (H<'a>, bn_client_egui::transport::ScriptHandle) {
    let (t, script) = session();
    let clock = Clock::manual();
    let model = EngineModel::new(Box::new(t)).with_clock(clock.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(
            |ui, app: &mut App<EngineModel>| app.show(ui),
            App::new(model).with_clock(clock),
        );
    h.run_steps(3);
    (h, script)
}

fn world(x: i32, y: i32) -> Value {
    json!({"dim":"","x":x,"y":y,"z":0})
}

/// The engine's next stack: the map editor with its cursor at world (3, 4) and `marks`, under `over`.
fn editor(sequence: u64, marks: Value, over: &[Value]) -> Value {
    let editor = json!({"screen_id":"screen:7","role":"dialog","kind":"editmap","title":"Map editor",
        "root":{"id":"target","kind":"panel","title":"Map editor","child":{"id":"target:map",
            "kind":"map","mode":"world",
            "target":{"source":{"dim":"","x":2,"y":4,"z":0},"current":{"dim":"","x":3,"y":4,"z":0},
                "range":20,"distance_metric":"square","status":"editmap","candidates":[],
                "marks":marks}}},
        "affordances":[]});
    let map = json!({"screen_id":"screen:map","role":"map","root":null,"affordances":[]});
    let stack = [map, editor]
        .into_iter()
        .chain(over.iter().cloned())
        .collect::<Vec<_>>();
    json!({"jsonrpc":"2.0","method":"bn.events","params":{"epoch":"e7f3","events":[{
        "sequence": sequence.to_string(), "revision": (sequence - 9).to_string(),
        "type": "interaction.changed", "changes": {"interaction": {
        "boundary_id": format!("b:{sequence}"),
        "readiness": {"phase":"playing","game_ready":true,"accepts_interaction_commands":true,
            "accepts_registered_actions":true},
        "actions": [], "interaction": null, "stack": stack}}}]}})
}

/// The screen corner of the square `dx`, `dy` from the avatar, plus (`fx`, `fy`) of a square.
fn corner(h: &H, (dx, dy): (i32, i32), (fx, fy): (f32, f32)) -> egui::Pos2 {
    let me = h.state().model.player();
    h.state().map_squares().unwrap().min
        + egui::vec2((me.x + dx) as f32 + fx, (me.y + dy) as f32 + fy) * CELL
}

/// Every shape painted this frame, groups flattened.
fn shapes(h: &H) -> Vec<egui::Shape> {
    fn flat(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(all) => all.iter().for_each(|s| flat(s, out)),
            shape => out.push(shape.clone()),
        }
    }
    let mut out = vec![];
    h.output()
        .shapes
        .iter()
        .for_each(|c| flat(&c.shape, &mut out));
    out
}

fn near(a: egui::Pos2, b: egui::Pos2) -> bool {
    a.distance(b) < 0.5
}

/// The filled squares exactly over the square `d` from the avatar.
fn fills_on(h: &H, shapes: &[egui::Shape], d: (i32, i32)) -> Vec<egui::Color32> {
    let square = egui::Rect::from_min_max(corner(h, d, (0.0, 0.0)), corner(h, d, (1.0, 1.0)));
    shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.rect == square && r.fill.a() > 0 => Some(r.fill),
            _ => None,
        })
        .collect()
}

#[test]
fn the_map_editors_selection_fills_each_selected_square_and_no_other() {
    let (mut h, script) = harness();
    script.push(editor(41, json!([]), &[]));
    h.run_steps(4);
    let plain = shapes(&h);
    script.push(editor(
        42,
        json!([{"role":"selection","squares":[world(5, 4), world(5, 5), world(3, 6)]}]),
        &[],
    ));
    h.run_steps(4);
    let marked = shapes(&h);
    let added = |d| {
        let before = fills_on(&h, &plain, d);
        fills_on(&h, &marked, d)
            .into_iter()
            .filter(|c| !before.contains(c))
            .count()
    };
    assert_eq!(added((2, 0)), 1, "a selected square is filled once");
    assert_eq!(added((2, 1)), 1);
    assert_eq!(added((0, 2)), 1);
    assert_eq!(added((1, 0)), 0, "a square between is not");
    assert_eq!(added((2, 2)), 0);
}

#[test]
fn the_mapgen_border_is_outlined_and_the_stamp_cornered_under_the_mapgen_list() {
    let (mut h, script) = harness();
    let marks = json!([
        {"role":"omt_border","rect":{"min":world(1, 2),"max":world(6, 6)}},
        {"role":"stamp","rect":{"min":world(0, 1),"max":world(7, 7)}}]);
    let mapgen = json!({"screen_id":"screen:11","role":"dialog","title":"Mapgen",
        "root":{"id":"menu","kind":"list","columns":[],"rows":[],"total":0,"selection":"single"},
        "affordances":[]});
    script.push(editor(41, marks, &[mapgen]));
    h.run_steps(4);
    let shapes = shapes(&h);
    // From the avatar at world (3, 4): the border's outer corners, then the stamp's.
    let border = [
        corner(&h, (-2, -2), (0.0, 0.0)),
        corner(&h, (3, -2), (1.0, 0.0)),
        corner(&h, (3, 2), (1.0, 1.0)),
        corner(&h, (-2, 2), (0.0, 1.0)),
    ];
    let outlined = shapes.iter().any(|s| match s {
        egui::Shape::Path(p) => {
            p.closed
                && p.points.len() == 4
                && p.points.iter().zip(&border).all(|(a, b)| near(*a, *b))
        }
        _ => false,
    });
    assert!(outlined, "the border is the rectangle around its squares");
    let stamp = [
        corner(&h, (-3, -3), (0.0, 0.0)),
        corner(&h, (4, -3), (1.0, 0.0)),
        corner(&h, (4, 3), (1.0, 1.0)),
        corner(&h, (-3, 3), (0.0, 1.0)),
    ];
    for at in stamp {
        let ticks = shapes
            .iter()
            .filter(|s| match s {
                egui::Shape::LineSegment { points, .. } => near(points[0], at),
                _ => false,
            })
            .count();
        assert_eq!(
            ticks, 2,
            "two ticks along the edges from the stamp's corner {at:?}"
        );
    }
}
