# bn-client-egui

Prototype of a mouse-first, screen-reader-accessible desktop client for
Cataclysm: Bright Nights, built on egui/eframe (AccessKit). The UI talks to a
`GameModel` trait with two implementations: `FakeModel` (in-memory, used by most tests) and
`EngineModel` (unified engine protocol 0.2 over the engine's stdio, newline-delimited JSON-RPC).

License: AGPL-3.0-only (see LICENSE). Bundled fonts: [Noto Sans CJK KR and Noto Sans Mono CJK KR](https://github.com/notofonts/noto-cjk)
2.004, subset by `tools/subset_fonts.sh`, SIL Open Font License 1.1 (`crates/egui/assets/fonts/LICENSE`).

    cargo run --release -p bn-client-egui      # finds the engine itself (see below)
    cargo run -p bn-client-egui --features fake -- --fake  # in-memory fake model, no engine
    cargo install --path crates/egui           # then just `bn-client-egui` (same for crates/tui: `bn-client-tui`)
    cargo test    # egui_kittest UI tests, protocol fixtures, reconstruction, scripted engine

Engine lookup (first hit wins; `--help` lists the same): `--engine PATH`, `BN_ENGINE`, `cataclysm-bn-tiles` /
`cataclysm-bn` (+ `.exe`) next to the client executable or in its parent directory, the path saved in
`$XDG_CONFIG_HOME/bn-client/config.json` (`~/.config/bn-client/config.json`; written only when you pick the engine in the setup window),
then `PATH`. If nothing is found the client exits with the list of searched places. Game data comes from `BN_ROOT`,
else the nearest ancestor of the chosen engine containing `data/json`. So run once with
`--engine /path/to/cataclysm-bn-tiles` and later runs need no arguments.

The engine is started as `<engine> --client mcp` on its own default user and config directories, so your
saves, options and keybindings are used (Load tab, Save and Quit work as in Tiles). `--userdir PATH` /
`--configdir PATH` override them; `--fresh-profile` (or `BN_FRESH_PROFILE=1`) uses a private throwaway profile.
Live tests always use one. `--seed TEXT` (or `BN_SEED`) passes the engine's `--seed`, so the same seed starts the
same world and game; live tests run on `BN_SEED`, else `live`.

Sound: the egui client plays the sound effects, ambience and music the engine asks for from the sound pack its
`Choose soundpack` option picks (off with `Sound Enabled`; volumes from the engine's volume options).
`BN_SOUND_LOG=PATH` writes every file started and track stopped, with milliseconds since the audio device opened.

Terminal client over the same model (`Ctrl-C` quits; the terminal size is declared to the engine as the map viewport).

Keys: the client binds none. The engine sends, with every action, the keys of the active input context
(the game, a menu, a popup, look mode) including the player's own rebinds in the engine profile's
`keybindings.json`, and the client forwards what is pressed. So movement, menu navigation, paging,
confirm, cancel, tab switching, look and every other key are the engine's bindings; a rebound `CONFIRM`
confirms in menus. The only keys the clients keep for themselves are not game input:
egui: Tab and Shift+Tab move keyboard focus, Space and Enter act on a focused widget, and arrows move the
map cursor of the mouse-and-keyboard view; TUI: Ctrl-C quits, Tab opens the actions list, and its look
mode starts and leaves with the keys the engine binds to its look action, with the cursor moved by the
engine's direction keys.

    cargo run -p bn-client-tui                 # same engine lookup; --features fake -- --fake for the fake model

The map draws with the game's tilesets (`<game root>/gfx/<name>/tile_config.json`) like the native Tiles
client, or as ASCII glyphs. The Tiles / ASCII buttons above the map switch at runtime and the Tileset menu picks
one of the game's tilesets (default `UNDEAD_PEOPLE_BASE`); the choice is kept in
`$XDG_CONFIG_HOME/bn-client-egui/settings.json` (`~/.config/...` without it). The engine publishes the
sprite facts only it knows (tile selection per square, light, season, facing, `looks_like`), see
`docs/en/dev/reference/engine_client_protocol.md` in the game; ids without a sprite draw as their glyph.

Crates: `crates/core` (protocol, transport, replica, models, colours, keys), `crates/egui`, `crates/tui`.
Live integration tests (ignored by default, need an engine build; the TUI one drives its key handling):

    BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-core --test live_engine -- --ignored
    BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-tui --test live_tui -- --ignored
    BN_ENGINE=... cargo test --release -p bn-client-core --test live_latency -- --ignored --nocapture   # timings

`crates/core/schema/0.2.schema.json` and `tests/fixtures/0.2.examples/` are verbatim copies of the engine's
`docs/schema/engine-client/` files (`deno run --allow-read --allow-write tools/sync_contract.ts <engine checkout>`;
`--check` verifies it, read-only); the schema's SHA-256 prefix is what the hello names, and a mismatch fails the
connection with the engine's error. `tests/fixtures/design.json` is a hand-written session, not captured from an engine;
`tests/fixtures/menu-interaction.json` is the 0.2 menu examples with the `interaction` the bespoke menus still read.

Mouse: left-click walks, right-click opens Look / Move here, hover shows a tooltip,
items drag between Ground and Inventory. Keyboard: focus the map, arrows move the
cursor; per-item Pick up / Drop buttons replace dragging.

## TODO (manual, not done)

- Verify with NVDA on Windows and Orca on Linux that controls, the map status line
  and the live message log are read correctly.
- Map cells are painted, not individual accessibility nodes; only the status line
  describes the cursor cell.
- Verify `EngineModel` against a live engine: action ids (`UP`..`LEFTUP`, `pickup`, `drop`) and
  the avatar `inventory` field are assumptions; menu paging beyond `choices` is not implemented;
  the map shows a 40x25 window that re-centres near its edge (a walk target is view-relative).
- Engine: movement needs engine-owned travel with completion/blocked events (or a
  pending/blocked step result) and cancel; the fake model only returns `moved`.
- Engine: item commands use list indices; switch to stable item ids plus source
  location so reordering during a drag cannot move the wrong item.
- Context menu opens only by secondary click; add a keyboard opener (Shift+F10/Menu).
- Live announcement of identical consecutive messages is not distinguishable.
- Add a model readiness/coordinate-validity contract for loading and disconnects.
