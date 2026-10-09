# bn-client-egui

Prototype of a mouse-first, screen-reader-accessible desktop client for
Cataclysm: Bright Nights, built on egui/eframe (AccessKit). There is no engine
connection yet: the UI talks to a `GameModel` trait backed by an in-memory `FakeModel`.

License: undecided (no LICENSE file yet).

    cargo run     # open the window
    cargo test    # egui_kittest UI tests (clicks, context menu, drag/drop, labels)

Mouse: left-click walks, right-click opens Look / Move here, hover shows a tooltip,
items drag between Ground and Inventory. Keyboard: focus the map, arrows move the
cursor, Enter walks there, L looks; per-item Pick up / Drop buttons replace dragging.

## TODO (manual, not done)

- Verify with NVDA on Windows and Orca on Linux that controls, the map status line
  and the live message log are read correctly.
- Map cells are painted, not individual accessibility nodes; only the status line
  describes the cursor cell.
- Replace `FakeModel` with the engine protocol.
