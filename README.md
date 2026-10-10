# bn-client-egui

Prototype of a mouse-first, screen-reader-accessible desktop client for
Cataclysm: Bright Nights, built on egui/eframe (AccessKit). The UI talks to a
`GameModel` trait with two implementations: `FakeModel` (in-memory, used by most tests) and
`EngineModel` (unified engine protocol 1.0 over the engine's stdio, newline-delimited JSON-RPC).

License: undecided (no LICENSE file yet).

    cargo run -p bn-client-egui                # fake model
    cargo run -p bn-client-egui -- --engine /path/to/cataclysm-bn-tiles   # or BN_ENGINE=...; real engine
    cargo test    # egui_kittest UI tests, protocol fixtures, reconstruction, scripted engine

The engine is started as `<engine> --client mcp --userdir <tmp> --configdir <tmp>` with private
temporary directories. Terminal client over the same model (`Ctrl-C` quits, arrows/vi-keys/game keys act, Up/Down/Enter/Esc in menus):

    cargo run -p bn-client-tui -- --engine /path/to/cataclysm-bn-tiles

Crates: `crates/core` (protocol, transport, replica, models, colours, keys), `crates/egui`, `crates/tui`.
Live integration tests (ignored by default, need an engine build; the TUI one drives its key handling):

    BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-core --test live_engine -- --ignored
    BN_ENGINE=/path/to/cataclysm-bn-tiles cargo test -p bn-client-tui --test live_tui -- --ignored
    BN_ENGINE=... cargo test --release -p bn-client-core --test live_latency -- --ignored --nocapture   # timings

`tests/fixtures/1.0.examples.json` is the contract lane's file (round-trip tested exactly);
`tests/fixtures/design-1.0.json` is a hand-written session from the design doc, not captured from an engine;
replace it with a recorded session once an engine can produce one.

Mouse: left-click walks, right-click opens Look / Move here, hover shows a tooltip,
items drag between Ground and Inventory. Keyboard: focus the map, arrows move the
cursor, Enter walks there, L looks; per-item Pick up / Drop buttons replace dragging.

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
