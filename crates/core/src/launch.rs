//! Finds the engine binary so the clients start without arguments, like a game launcher.
//!
//! Order: `--engine` flag, `BN_ENGINE` (both via clap), a binary next to the client executable or in its
//! parent directory, the path saved in the client's config file, then `PATH`.

use crate::colors::ColorMap;
use crate::engine_model::EngineModel;
use crate::transport::{Profile, StdioTransport, Transport, game_root};
use clap::Args;
use serde_json::{Value, json};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub const ENGINE_NAMES: [&str; 4] = [
    "cataclysm-bn-tiles",
    "cataclysm-bn",
    "cataclysm-bn-tiles.exe",
    "cataclysm-bn.exe",
];

/// Command-line options shared by the client binaries: engine selection and profile.
#[derive(Args, Debug, Clone, Default)]
#[command(after_help = "\
Engine lookup order (first hit wins):
  1. --engine PATH
  2. BN_ENGINE environment variable
  3. cataclysm-bn-tiles / cataclysm-bn (+ .exe) next to this executable or in its parent directory
  4. the path saved in the config file (written only when you pick the engine in the setup window)
  5. cataclysm-bn-tiles / cataclysm-bn on PATH
Default profile: the engine's own user and config dirs (your saves, options, keybindings).
Game data is taken from BN_ROOT, else the nearest ancestor of the engine containing data/json.")]
pub struct LaunchArgs {
    /// Engine executable (default: search, see below)
    #[arg(long, value_name = "PATH", env = "BN_ENGINE")]
    pub engine: Option<PathBuf>,
    /// Run the in-memory fake model instead of an engine
    #[arg(long, conflicts_with = "engine")]
    pub fake: bool,
    #[command(flatten)]
    pub profile: Profile,
}

/// Where the automatic lookup searches.
#[derive(Debug, Clone, Default)]
pub struct Search {
    pub exe: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
    pub path_var: Option<OsString>,
}

impl Search {
    /// The running executable, the client's XDG config file and `PATH`.
    pub fn from_env() -> Self {
        Self {
            exe: std::env::current_exe().ok(),
            config_file: config_file(),
            path_var: std::env::var_os("PATH"),
        }
    }
}

/// `<XDG config>/bn-client/config.json`.
pub fn config_file() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "bn-client")?;
    Some(dirs.config_dir().join("config.json"))
}

/// A regular file that may be executed (any execute bit on Unix).
pub fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    let mode_ok = path
        .metadata()
        .is_ok_and(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0);
    #[cfg(not(unix))]
    let mode_ok = true;
    path.is_file() && mode_ok
}

fn is_file(path: &Path) -> bool {
    path.is_file()
}

fn saved_engine(config_file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(config_file).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value.get("engine")?.as_str().map(PathBuf::from)
}

/// Remembers a working engine path in the config file.
pub fn save_engine(config_file: &Path, engine: &Path) -> std::io::Result<()> {
    let engine = std::fs::canonicalize(engine).unwrap_or_else(|_| engine.to_path_buf());
    if saved_engine(config_file).as_deref() == Some(engine.as_path()) {
        return Ok(());
    }
    if let Some(dir) = config_file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(config_file, json!({ "engine": engine }).to_string())
}

/// Every place that was searched, for the error message.
#[derive(Debug)]
pub struct NotFound(pub Vec<String>);

impl std::fmt::Display for NotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "no Cataclysm: Bright Nights engine found. Searched:")?;
        for line in &self.0 {
            writeln!(f, "  - {line}")?;
        }
        write!(
            f,
            "Set it once with `--engine /path/to/cataclysm-bn-tiles` (remembered afterwards), or export BN_ENGINE, \
             or put the client next to the game."
        )
    }
}

impl std::error::Error for NotFound {}

/// The engine to start: an explicit `flag` (flag or `BN_ENGINE`) is used as given, otherwise the lookup order
/// of this module's docs applies.
pub fn resolve(flag: Option<&Path>, search: &Search) -> Result<PathBuf, NotFound> {
    if let Some(flag) = flag {
        return Ok(flag.to_path_buf());
    }
    let mut searched = vec!["--engine / BN_ENGINE: not set".to_string()];
    let exe_dir = search.exe.as_deref().and_then(Path::parent);
    for dir in exe_dir.into_iter().flat_map(|d| d.ancestors().take(2)) {
        let hit = ENGINE_NAMES
            .iter()
            .map(|n| dir.join(n))
            .find(|p| is_file(p));
        if let Some(hit) = hit {
            return Ok(hit);
        }
        searched.push(format!("{}/{{{}}}", dir.display(), ENGINE_NAMES.join(",")));
    }
    if let Some(file) = &search.config_file {
        match saved_engine(file) {
            Some(saved) if is_file(&saved) => return Ok(saved),
            Some(saved) => searched.push(format!(
                "{}: saved {} is missing",
                file.display(),
                saved.display()
            )),
            None => searched.push(format!("{}: no saved engine", file.display())),
        }
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    for name in ENGINE_NAMES {
        if let Ok(hit) = which::which_in(name, search.path_var.as_deref(), &cwd) {
            return Ok(hit);
        }
    }
    let path = search.path_var.as_deref().unwrap_or(OsStr::new(""));
    searched.push(format!(
        "PATH for {}: {}",
        ENGINE_NAMES.join(", "),
        path.to_string_lossy()
    ));
    Err(NotFound(searched))
}

/// A started engine with the data loaded from its game root.
pub struct Opened {
    pub model: EngineModel,
    pub root: Option<PathBuf>,
}

/// Starts the engine and loads colours and keys from its game root. Writes no file: remembering the engine
/// path is the user's act (`save_engine`).
///
/// `tiles` says the client draws tiles: the engine then sends tile ids where it has them.
pub fn open_engine(engine: &Path, profile: &Profile, tiles: bool) -> Result<Opened, String> {
    let transport = StdioTransport::spawn_with(engine, profile)
        .map_err(|e| format!("cannot start engine {}: {e}", engine.display()))?;
    let root = game_root(engine, std::env::var_os("BN_ROOT").map(PathBuf::from));
    let colors = root
        .as_deref()
        .map_or_else(ColorMap::default, ColorMap::load);
    let (engine, profile) = (engine.to_path_buf(), profile.clone());
    let respawn = Box::new(move || {
        StdioTransport::spawn_with(&engine, &profile).map(|t| Box::new(t) as Box<dyn Transport>)
    });
    let model = EngineModel::connect(Box::new(transport), tiles)
        .with_colors(colors)
        .with_respawn(respawn);
    Ok(Opened { model, root })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct Dirs(PathBuf);
    impl Dirs {
        fn new(tag: &str) -> Self {
            let base = std::env::temp_dir().join(format!("bn-launch-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            std::fs::create_dir_all(&base).unwrap();
            Self(base)
        }
        fn engine(&self, rel: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }
    }
    impl Drop for Dirs {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn search(d: &Dirs) -> Search {
        Search {
            exe: Some(d.0.join("game/client/bn-client")),
            config_file: Some(d.0.join("cfg/config.json")),
            path_var: Some(d.0.join("bin").into()),
        }
    }

    #[test]
    fn explicit_flag_beats_everything() {
        let d = Dirs::new("flag");
        d.engine("game/client/cataclysm-bn-tiles");
        let flag = PathBuf::from("/explicit/engine");
        assert_eq!(resolve(Some(&flag), &search(&d)).unwrap(), flag);
    }

    #[test]
    fn next_to_exe_beats_config_and_path() {
        let d = Dirs::new("exe");
        let next = d.engine("game/client/cataclysm-bn-tiles");
        let saved = d.engine("saved/engine");
        save_engine(&d.0.join("cfg/config.json"), &saved).unwrap();
        d.engine("bin/cataclysm-bn-tiles");
        assert_eq!(resolve(None, &search(&d)).unwrap(), next);
    }

    #[test]
    fn parent_game_directory_is_searched() {
        let d = Dirs::new("parent");
        let parent = d.engine("game/cataclysm-bn");
        assert_eq!(resolve(None, &search(&d)).unwrap(), parent);
    }

    #[test]
    fn config_beats_path_and_is_roundtripped() {
        let d = Dirs::new("cfg");
        let saved = d.engine("saved/engine");
        d.engine("bin/cataclysm-bn-tiles");
        save_engine(&d.0.join("cfg/config.json"), &saved).unwrap();
        assert_eq!(
            resolve(None, &search(&d)).unwrap(),
            std::fs::canonicalize(saved).unwrap()
        );
    }

    #[test]
    fn stale_config_falls_through_to_path() {
        let d = Dirs::new("stale");
        let on_path = d.engine("bin/cataclysm-bn-tiles");
        std::fs::create_dir_all(d.0.join("cfg")).unwrap();
        std::fs::write(d.0.join("cfg/config.json"), r#"{"engine":"/gone/engine"}"#).unwrap();
        assert_eq!(resolve(None, &search(&d)).unwrap(), on_path);
    }

    #[test]
    fn not_found_lists_every_searched_place() {
        let d = Dirs::new("none");
        std::fs::create_dir_all(d.0.join("cfg")).unwrap();
        std::fs::write(d.0.join("cfg/config.json"), "not json").unwrap();
        let msg = resolve(None, &search(&d)).unwrap_err().to_string();
        for needle in [
            "BN_ENGINE",
            "game/client/{",
            "no saved engine",
            "PATH for",
            "--engine",
        ] {
            assert!(msg.contains(needle), "{needle} missing in {msg}");
        }
    }
}
