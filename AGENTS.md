# Agent guidelines

## Client boundary

The engine owns game rules and game text; this client only renders engine data and forwards input.

- Never decide game outcomes in the client: no tile/item id string matching, no distance or range math, no row
  hotkey/mark/colour derivation, no pathing or walk loops, no inferring which screen accepts which key.
- Never key behaviour on display text (`label`, `title`, column labels, messages); use stable engine ids and fields.
- Never write game text in the client (status, errors, item lines, sidebar labels); show the engine's localized text.
- When the engine does not send what a view needs, add the field or event to the engine protocol
  (`docs/schema/engine-client/0.1.schema.json` in the engine) and consume it here; do not approximate it.
- The engine sends semantic facts (what exists, what the player sees, attitudes, states); presentation is client-owned
  like CSS: colour RGB/themes, sprite and variant choice, overlay sprites, layer priority, layout and widgets. Do not
  ask the engine for presentation, and do not derive facts in the client.
- Client-owned: layout, widgets, input-device mapping to engine action ids, tileset rendering, transport, and client
  chrome strings.
- Word every client chrome string with `localise::{tr, tr_format, tr_plural}` (printf placeholders, imported names
  so `xgettext` finds them), then run `tools/update_po.sh` and translate each `crates/core/po/<locale>.po`.
- Known violations to remove: `client-logic-audit.md` in the engine's `out/lanes/`.

## Tests

- One integration-test binary per crate: `crates/<crate>/tests/it/main.rs` declares `mod <file>;` for every
  `tests/it/<file>.rs` (shared helpers in `tests/it/common/`). A new test file is a new `mod` line there, never a new
  `tests/*.rs`. Run one with `cargo test -p <crate> --test it <file>::`.
- The tests of a crate share one process: no `set_var`, `set_current_dir` or fixed temp paths; derive scratch dirs from
  the process id plus a per-call counter or the test's own tag.

