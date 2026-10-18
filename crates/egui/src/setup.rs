//! First-run window shown when no engine was found (or the found one cannot start): a desktop-launched
//! client has no visible stderr, so the searched locations and a path field are shown instead.

use bn_client_core::launch::{is_executable, save_engine};
use bn_client_core::localise::{Value as Arg, tr, tr_format};
use eframe::egui;
use std::path::PathBuf;

/// Builds the running app for an engine path, or says why it cannot start.
pub type Starter = Box<dyn Fn(&egui::Context, PathBuf) -> Result<Box<dyn eframe::App>, String>>;

pub struct Launcher {
    searched: String,
    input: String,
    error: Option<String>,
    start: Starter,
    running: Option<Box<dyn eframe::App>>,
    /// Where a path the user typed here is remembered; nothing is remembered without it.
    config: Option<PathBuf>,
}

impl Launcher {
    pub fn new(searched: String, start: Starter) -> Self {
        Self {
            searched,
            input: String::new(),
            error: None,
            start,
            running: None,
            config: None,
        }
    }

    pub fn running(app: Box<dyn eframe::App>, start: Starter) -> Self {
        Self {
            running: Some(app),
            ..Self::new(String::new(), start)
        }
    }

    /// Remembers the engine path the user picks in this window in `config`.
    pub fn with_config(mut self, config: PathBuf) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// The setup window contents.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr("No engine found"));
        ui.label(&self.searched);
        ui.label(tr("Path to cataclysm-bn-tiles:"));
        let field = ui.text_edit_singleline(&mut self.input);
        let submitted = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button(tr("Use")).clicked() || submitted {
            self.use_input(ui.ctx());
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
    }

    fn use_input(&mut self, ctx: &egui::Context) {
        let path = PathBuf::from(self.input.trim());
        if !is_executable(&path) {
            self.error = Some(tr_format(
                "%s is not an executable file",
                &[Arg::Text(path.display().to_string())],
            ));
            return;
        }
        match (self.start)(ctx, path.clone()) {
            Ok(app) => {
                if let Some(config) = &self.config {
                    let _ = save_engine(config, &path);
                }
                self.running = Some(app);
            }
            Err(e) => self.error = Some(e),
        }
    }
}

impl eframe::App for Launcher {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        match &mut self.running {
            Some(app) => app.ui(ui, frame),
            None => self.show(ui),
        }
    }
}
