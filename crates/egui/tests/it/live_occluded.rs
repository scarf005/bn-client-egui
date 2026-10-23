//! Against a real engine: open air in sight over ground the avatar neither sees nor remembers is a solid dark block
//! in 3D and stays black on the flat map of an isometric tileset.
//!
//! A mod in the profile builds a house, a roof over it and open air around it, and puts the avatar on the roof.
//! `BN_ENGINE=<engine> BN_ROOT=<game root with gfx/Ultica_iso> SHOT_DIR=<dir> SHOT_TAG=<prefix> cargo test
//! --release -p bn-client-egui --test it live_occluded:: -- --ignored --nocapture`.
use crate::common::live::{dismiss_popups, new_game, root, run_until, save, settle};
use bn_client_egui::transport::{Profile, StdioTransport};
use bn_client_egui::{App, EngineModel, GameModel, Pos};
use eframe::egui;
use egui_kittest::Harness;

type H<'a> = Harness<'a, App<EngineModel>>;

const MODINFO: &str = r#"[{"type":"MOD_INFO","id":"occluded_scene","name":"Occluded scene","authors":["test"],
"description":"A house with a roof the avatar stands on.","category":"content","lua_api_version":2,"dependencies":["bn"]}]"#;

/// Builds the scene at the first turn: grass, a walled house, its flat roof one level up, open air around it, the
/// avatar on the roof over the square it stood on.
const PRELOAD: &str = r#"
local built = false
local function at(x, y, z) return TripointBubMs.new(x, y, z) end
gapi.add_on_every_x_hook(TimeDuration.from_turns(1), function()
  if built then return end
  built = true
  local map = gapi.get_map()
  local you = gapi.get_avatar()
  local p = you:get_pos_ms()
  local function ter(x, y, z, id) map:set_ter_at(at(x, y, z), TerId.new(id):int_id()) end
  for dx = -20, 20 do
    for dy = -20, 20 do
      local x, y = p.x + dx, p.y + dy
      local d = math.max(math.abs(dx), math.abs(dy))
      map:set_furn_at(at(x, y, p.z), FurnId.new("f_null"):int_id())
      ter(x, y, p.z + 1, d <= 5 and "t_flat_roof" or "t_open_air")
      ter(x, y, p.z, d == 5 and "t_wall" or d < 5 and "t_floor" or "t_grass")
    end
  end
  you:set_pos_ms(at(p.x, p.y, p.z + 1))
  gdebug.log_info("occluded_scene built")
end)
"#;

fn spawn() -> StdioTransport {
    let dir = std::env::temp_dir().join(format!("bn-occluded-{}", std::process::id()));
    let config = dir.join("config");
    let modpath = dir.join("mods").join("occluded_scene");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&modpath).unwrap();
    std::fs::write(modpath.join("modinfo.json"), MODINFO).unwrap();
    std::fs::write(modpath.join("preload.lua"), PRELOAD).unwrap();
    std::fs::write(
        config.join("options.json"),
        r#"[{"name":"USE_LANG","value":"en_US"}]"#,
    )
    .unwrap();
    std::fs::write(
        config.join("default_mods.json"),
        r#"["bn","occluded_scene"]"#,
    )
    .unwrap();
    let profile = Profile {
        fresh: false,
        userdir: Some(dir),
        configdir: Some(config),
        seed: Some("2".into()),
    };
    StdioTransport::spawn_with(&crate::common::live::engine(), &profile).expect("engine starts")
}

fn start(three_d: bool, size: egui::Vec2) -> H<'static> {
    let root = root();
    let dir = std::env::temp_dir().join(format!("bn-occluded-ui-{}-{three_d}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(
        &settings,
        format!(r#"{{"tiles":true,"tileset":"Ultica_iso","three_d":{three_d}}}"#),
    )
    .unwrap();
    let app = App::new(EngineModel::new(Box::new(spawn())))
        .with_settings(settings)
        .with_root(root.clone())
        .with_target_format(eframe::wgpu::TextureFormat::Rgba8Unorm);
    let h = Harness::builder()
        .with_size(size)
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut App<EngineModel>| app.show(ui), app);
    bn_client_egui::fonts::install(&h.ctx, &root);
    h
}

/// The squares of the view as a picture of text: `.` nothing known, `X` open air over unknown ground, `R` roof, `o`
/// ground drawn lower and plain, `m` ground drawn lower and remembered.
fn grid(h: &H) -> (String, usize, usize) {
    let m = &h.state().model;
    let size = m.size();
    let (mut text, mut plain, mut dim) = (String::new(), 0, 0);
    for y in 0..size.y {
        for x in 0..size.x {
            let c = m.cell(Pos { x, y });
            text.push(match (c.layers.first(), c.depth, c.remembered) {
                (None, depth, _) if depth > 0 => 'X',
                (None, ..) => '.',
                (Some(l), 0, _) if l.id.contains("roof") => 'R',
                (Some(_), 0, _) => '#',
                (Some(_), _, true) => {
                    dim += 1;
                    'm'
                }
                (Some(_), ..) => {
                    plain += 1;
                    'o'
                }
            });
        }
        text.push('\n');
    }
    (text, plain, dim)
}

fn roof_scene(three_d: bool, name: &str) {
    let tag = std::env::var("SHOT_TAG").unwrap_or_default();
    let mut h = start(three_d, egui::vec2(1280.0, 800.0));
    new_game(&mut h, "new_game:play_now");
    dismiss_popups(&mut h);
    // The mod builds the scene at the first turn.
    assert!(h.state_mut().model.invoke_action("pause"));
    run_until(&mut h, "the avatar on the roof", 120, |h| {
        !h.state().model.busy() && grid(h).0.contains('R')
    });
    settle(&mut h);
    dismiss_popups(&mut h);
    settle(&mut h);
    let (text, plain, dim) = grid(&h);
    println!("LIVE {name} grid plain {plain} dim {dim}\n{text}");
    assert!(
        text.contains('X'),
        "open air over unknown ground is known as such"
    );
    for (w, ht) in [(1280.0, 800.0), (1920.0, 1080.0)] {
        h.set_size(egui::vec2(w, ht));
        settle(&mut h);
        save(
            &mut h,
            &format!("{tag}{name}-{}x{}.png", w as u32, ht as u32),
        );
    }
}

#[test]
#[ignore = "requires BN_ENGINE and a game root with the Ultica_iso tileset"]
fn open_air_over_unknown_ground_is_a_dark_block_in_3d() {
    roof_scene(true, "pseudo3d");
}

#[test]
#[ignore = "requires BN_ENGINE and a game root with the Ultica_iso tileset"]
fn open_air_over_unknown_ground_stays_black_on_the_isometric_flat_map() {
    roof_scene(false, "iso2d");
}
