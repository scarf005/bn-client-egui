#![allow(dead_code)]
use bn_client_egui::transport::{ScriptHandle, ScriptedTransport};
use bn_client_egui::{EngineModel, GameModel};
use serde_json::Value;

pub fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/fixtures/design-1.0.json"
    ))
    .unwrap();
    serde_json::from_str::<Value>(&text).unwrap()[name].clone()
}

pub fn subscribe_reply() -> Vec<Value> {
    ["subscribe_result", "snapshot_part_0", "snapshot_part_1"]
        .map(fixture)
        .to_vec()
}

/// Answers hello and subscribe with the fixture session; extra replies can be chained.
pub fn session() -> (ScriptedTransport, ScriptHandle) {
    let (t, h) = ScriptedTransport::new();
    (
        t.on("bn.hello", vec![fixture("hello_result")])
            .on("bn.subscribe", subscribe_reply()),
        h,
    )
}

/// A model that finished hello, subscribe and the snapshot.
pub fn connected(script: ScriptedTransport) -> EngineModel {
    let mut m = EngineModel::new(Box::new(script));
    m.poll();
    assert!(m.live());
    m
}

/// A game root holding one 4x4 tileset `test` whose sheets are solid colours: `a.png` has sprites 0 to 7
/// (red, green, blue, ... one per cell) and `b.png` sprites 8 and 9. The root is new for every call.
pub fn game_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("bn-client-tiles-{}-{tag}", std::process::id()));
    let dir = root.join("gfx/test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tileset.txt"),
        "# A test tileset\nNAME: test\nVIEW: Test Set\nJSON: tile_config.json\nTILESET: a.png\n",
    )
    .unwrap();
    let sheet = |name: &str, cells: u32| {
        let mut image = image::RgbaImage::new(4 * cells.min(4), 4 * cells.div_ceil(4));
        for (x, y, px) in image.enumerate_pixels_mut() {
            let n = (y / 4) * 4 + x / 4;
            *px = image::Rgba([40 + 25 * n as u8, 255 - 25 * n as u8, 128, 255]);
        }
        image.save(dir.join(name)).unwrap();
    };
    sheet("a.png", 8);
    sheet("b.png", 2);
    let config = serde_json::json!({
        "tile_info": [{"width": 4, "height": 4}],
        "tiles-new": [
            {"file": "a.png", "tiles": [
                {"id": ["t_floor", "t_dirt"], "fg": 0},
                {"id": "t_wall", "fg": 1, "multitile": true, "additional_tiles": [
                    {"id": "edge", "fg": [2, 3]},
                    {"id": "corner", "fg": 4}
                ]},
                {"id": "t_wall_season_winter", "fg": 5},
                {"id": "grass", "fg": [{"weight": 1, "sprite": 5}, {"weight": 3, "sprite": 6}]},
                {"id": ["mon_a", "mon_b", "mon_zombie"], "fg": 7}
            ]},
            {"file": "b.png", "sprite_offset_x": 1, "sprite_offset_y": -2, "tiles": [
                {"id": "two_sheets", "fg": 8, "bg": 9}
            ]},
            {"file": "missing_fallback.png", "tiles": [], "ascii": []}
        ]
    });
    std::fs::write(dir.join("tile_config.json"), config.to_string()).unwrap();
    root
}
