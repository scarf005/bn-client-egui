//! Against a real engine: the windows of a room, curtains drawn, curtains open and the window itself open, as the
//! 3D map, the first-person view and the isometric flat map draw them.
//!
//! A mod in the profile builds a room around the avatar with the three windows in its north and east walls.
//! `BN_ENGINE=<engine> SHOT_DIR=<dir> cargo test --release -p bn-client-egui --test it live_curtain:: -- --ignored
//! --nocapture`.
use crate::common::live::{dismiss_popups, new_game, root, run_until, save, settle};
use bn_client_egui::transport::{Profile, StdioTransport};
use bn_client_egui::{App, EngineModel, GameModel, Pos};
use eframe::egui;
use egui_kittest::Harness;

type H<'a> = Harness<'a, App<EngineModel>>;

const MODINFO: &str = r#"[{"type":"MOD_INFO","id":"curtain_room","name":"Curtain room","authors":["test"],
"description":"A room with curtained windows around the avatar.","category":"content","lua_api_version":2,"dependencies":["bn"]}]"#;

/// Builds the room at the first turn: floor inside, walls three squares out, grass beyond, and in the north and east
/// walls the window with its curtains drawn, with them open, and opened, between the door curtain closed and open.
const PRELOAD: &str = r#"
local built = false
local function at(x, y, z) return TripointBubMs.new(x, y, z) end
gapi.add_on_every_x_hook(TimeDuration.from_turns(1), function()
  if built then return end
  built = true
  local map = gapi.get_map()
  local p = gapi.get_avatar():get_pos_ms()
  local windows = {
    [-2] = "t_door_curtain_c", [-1] = "t_curtains", [0] = "t_window_domestic", [1] = "t_window_open",
    [2] = "t_door_curtain_o"
  }
  for dx = -8, 8 do
    for dy = -8, 8 do
      local x, y = p.x + dx, p.y + dy
      local d = math.max(math.abs(dx), math.abs(dy))
      map:set_furn_at(at(x, y, p.z), FurnId.new("f_null"):int_id())
      local id = d < 3 and "t_floor" or d == 3 and "t_brick_wall" or "t_grass"
      if dy == -3 and windows[dx] then id = windows[dx] end
      if dx == 3 and windows[dy] then id = windows[dy] end
      map:set_ter_at(at(x, y, p.z), TerId.new(id):int_id())
    end
  end
  gdebug.log_info("curtain_room built")
end)
"#;

fn spawn() -> StdioTransport {
    let dir = std::env::temp_dir().join(format!("bn-curtain-{}", std::process::id()));
    let config = dir.join("config");
    let modpath = dir.join("mods").join("curtain_room");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&modpath).unwrap();
    std::fs::write(modpath.join("modinfo.json"), MODINFO).unwrap();
    std::fs::write(modpath.join("preload.lua"), PRELOAD).unwrap();
    std::fs::write(
        config.join("options.json"),
        r#"[{"name":"USE_LANG","value":"en_US"},{"name":"SAFEMODE","value":"false"}]"#,
    )
    .unwrap();
    std::fs::write(config.join("default_mods.json"), r#"["bn","curtain_room"]"#).unwrap();
    let profile = Profile {
        fresh: false,
        userdir: Some(dir),
        configdir: Some(config),
        seed: Some("2".into()),
    };
    StdioTransport::spawn_with(&crate::common::live::engine(), &profile).expect("engine starts")
}

fn start(settings_json: &str) -> H<'static> {
    let root = root();
    let dir = std::env::temp_dir().join(format!("bn-curtain-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, settings_json).unwrap();
    let app = App::new(EngineModel::new(Box::new(spawn())))
        .with_settings(settings)
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    let h = Harness::builder()
        .with_size(egui::vec2(1920.0, 1080.0))
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::fonts::install(&h.ctx, &root);
    h
}

fn room(name: &str, settings: &str) {
    let mut h = start(settings);
    new_game(&mut h, "new_game:play_now");
    dismiss_popups(&mut h);
    for _ in 0..2 {
        assert!(h.state_mut().model.invoke_action("pause"));
        run_until(&mut h, "a turn", 120, |h| !h.state().model.busy());
    }
    let me = h.state().model.player();
    let north = h.state().model.cell(Pos {
        x: me.x,
        y: me.y - 3,
    });
    for dx in -2..=2 {
        let c = h.state().model.cell(Pos {
            x: me.x + dx,
            y: me.y - 3,
        });
        let l = c
            .layers
            .iter()
            .map(|l| (&l.id, l.wall, l.window, l.coverage))
            .collect::<Vec<_>>();
        println!("LIVE {name} north {dx} {l:?}");
    }
    assert!(
        north.layers.iter().any(|l| l.id == "t_window_domestic"),
        "the room is built"
    );
    dismiss_popups(&mut h);
    settle(&mut h);
    for (w, ht) in [(1290.0, 830.0), (1920.0, 1080.0)] {
        h.set_size(egui::vec2(w, ht));
        settle(&mut h);
        save(
            &mut h,
            &format!("curtain-{name}-{}x{}.png", w as u32, ht as u32),
        );
    }
}

const ISO: &str = r#""tiles":true,"tileset":"HitButton_iso""#;
const ABOVE: &str = r#""tiles":true,"tileset":"UltimateCataclysm""#;

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_curtain_views() {
    room("3d", &format!(r#"{{{ISO},"three_d":true}}"#));
    room(
        "first",
        &format!(r#"{{{ISO},"three_d":true,"first_person":true}}"#),
    );
    room("iso", &format!(r#"{{{ISO},"three_d":false}}"#));
}

#[test]
#[ignore = "requires BN_ENGINE pointing at an engine binary"]
fn live_curtain_views_from_above() {
    room("above-3d", &format!(r#"{{{ABOVE},"three_d":true}}"#));
    room(
        "above-first",
        &format!(r#"{{{ABOVE},"three_d":true,"first_person":true}}"#),
    );
}
