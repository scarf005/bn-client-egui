//! Drives the real window (winit, wgpu, the compositor) through a fresh-profile tutorial and 20 steps, then
//! prints the frame scopes eframe recorded, present included.
//!
//! `BN_ENGINE=<engine> cargo run --release --features profile -p bn-client-egui --example present_probe`
use bn_client_egui::transport::{Profile, StdioTransport, game_root};
use bn_client_egui::{App, EngineModel, GameModel, MenuModel};
use eframe::egui;
use std::time::{Duration, Instant};

const STEPS: usize = 20;
/// Frames drawn back to back after the walk, to see what the display path does at full rate.
const FLOOD: Duration = Duration::from_secs(3);

enum Phase {
    NewGame,
    Tutorial,
    Settle(Instant),
    Walk {
        sent: Option<Instant>,
        from: (i32, i32),
    },
    Flood(Instant),
    Done,
}

struct Probe {
    app: App<EngineModel>,
    phase: Phase,
    step: usize,
    inject: Option<&'static str>,
    latencies: Vec<f64>,
    view: puffin::GlobalFrameView,
    last_frame: Option<Instant>,
    gaps: Vec<f64>,
    shown: Option<&'static str>,
    started: Instant,
}

impl Probe {
    fn square(&self) -> (i32, i32) {
        let m = &self.app.model;
        (m.player().x + m.origin().x, m.player().y + m.origin().y)
    }

    fn choose(&mut self, label: &str) -> bool {
        let id = self.app.model.menu().and_then(|m| {
            m.choices
                .iter()
                .find(|c| c.label.contains(label))
                .map(|c| c.id.clone())
        });
        id.is_some_and(|id| {
            !self.app.model.busy() && {
                self.app.model.choose(&id);
                true
            }
        })
    }

    fn script(&mut self, ctx: &egui::Context) {
        let busy = self.app.model.busy();
        let square = self.square();
        match &mut self.phase {
            Phase::NewGame => {
                if self.choose("New Game") {
                    self.phase = Phase::Tutorial;
                }
            }
            Phase::Tutorial => {
                if self.choose("Tutorial") {
                    self.phase = Phase::Settle(Instant::now());
                }
            }
            Phase::Settle(since) => {
                let popup = self.app.model.menu();
                if let Some(menu) = popup.filter(|_| !busy) {
                    let id = menu.choices.first().map(|c| c.id.clone());
                    match id {
                        Some(id) if !menu.can_cancel => self.app.model.choose(&id),
                        _ => self.app.model.cancel_menu(),
                    }
                    *since = Instant::now();
                } else if self.app.model.size().x > 0
                    && !busy
                    && since.elapsed() > Duration::from_secs(3)
                {
                    self.phase = Phase::Walk {
                        sent: None,
                        from: square,
                    };
                }
            }
            Phase::Walk { sent, from } => {
                let (now, origin) = (square, *from);
                match *sent {
                    None => {
                        self.inject = Some(if self.step.is_multiple_of(2) { "l" } else { "h" });
                        *sent = Some(Instant::now());
                    }
                    Some(at) if now != origin => {
                        self.latencies.push(at.elapsed().as_secs_f64() * 1e3);
                        self.step += 1;
                        self.phase = if self.step == STEPS {
                            Phase::Flood(Instant::now())
                        } else {
                            Phase::Settle(Instant::now() - Duration::from_millis(2500))
                        };
                    }
                    Some(at) if at.elapsed() > Duration::from_millis(700) && !busy => {
                        // Blocked or rejected: try again.
                        *sent = None;
                    }
                    Some(_) => {}
                }
            }
            Phase::Flood(since) => {
                if since.elapsed() > FLOOD {
                    self.phase = Phase::Done;
                } else {
                    ctx.request_repaint();
                    let now = Instant::now();
                    if let Some(last) = self.last_frame {
                        self.gaps.push((now - last).as_secs_f64() * 1e3);
                    }
                    self.last_frame = Some(now);
                }
            }
            Phase::Done => {
                self.report(ctx);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        let name = match self.phase {
            Phase::NewGame => "new game",
            Phase::Tutorial => "tutorial",
            Phase::Settle(_) => "settle",
            Phase::Walk { .. } => "walk",
            Phase::Flood(_) => "flood",
            Phase::Done => "done",
        };
        if self.shown != Some(name) {
            self.shown = Some(name);
            println!("PRESENT phase {name} at {:?}", self.started.elapsed());
        }
        if !matches!(self.phase, Phase::Done | Phase::Flood(_)) {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn report(&mut self, ctx: &egui::Context) {
        let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
        let mut lat = self.latencies.clone();
        lat.sort_by(f64::total_cmp);
        println!("PRESENT load {}", load.trim());
        println!(
            "PRESENT {} steps, step latency ms p50={:.0} max={:.0}",
            lat.len(),
            lat[lat.len() / 2],
            lat[lat.len() - 1]
        );
        let mut gaps = self.gaps.clone();
        gaps.sort_by(f64::total_cmp);
        if !gaps.is_empty() {
            let median = gaps[gaps.len() / 2];
            let over = gaps.iter().filter(|g| **g > median * 1.5).count();
            println!(
                "PRESENT flood {} frames in {FLOOD:?}: interval ms p50={median:.2} p95={:.2} max={:.2}, {over} longer than 1.5x the median",
                gaps.len(),
                gaps[(gaps.len() - 1) * 95 / 100],
                gaps[gaps.len() - 1]
            );
        }
        let _ = ctx;
        bn_client_egui::profile::scopes("walk and flood", &self.view);
    }
}

impl eframe::App for Probe {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.show(ui);
        self.script(&ui.ctx().clone());
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(key) = self.inject.take() {
            raw_input.events.push(egui::Event::Text(key.into()));
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result {
    let engine = std::env::var_os("BN_ENGINE").expect("set BN_ENGINE");
    let transport =
        StdioTransport::spawn_with(engine.as_ref(), &Profile::fresh()).expect("engine starts");
    let root = game_root(engine.as_ref(), None).expect("game root");
    let dir = std::env::temp_dir().join(format!("bn-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    std::fs::write(&settings, r#"{"tiles":true,"tileset":null}"#).unwrap();
    puffin::set_scopes_on(true);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    println!(
        "PRESENT present mode {:?}",
        options.wgpu_options.surface.present_mode
    );
    eframe::run_native(
        "Cataclysm: Bright Nights client (present probe)",
        options,
        Box::new(move |cc| {
            bn_client_egui::fonts::install(&cc.egui_ctx, &root);
            if let Some(state) = &cc.wgpu_render_state {
                let info = state.adapter.get_info();
                println!(
                    "PRESENT adapter {} ({:?}, {:?})",
                    info.name, info.backend, info.device_type
                );
            }
            let app = App::new(EngineModel::new(Box::new(transport)))
                .with_settings(settings)
                .with_root(root);
            Ok(Box::new(Probe {
                app,
                phase: Phase::NewGame,
                step: 0,
                inject: None,
                latencies: vec![],
                view: puffin::GlobalFrameView::default(),
                last_frame: None,
                gaps: vec![],
                shown: None,
                started: Instant::now(),
            }))
        }),
    )
}
