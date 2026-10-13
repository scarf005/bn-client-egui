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
- Known violations to remove: `client-logic-audit.md` in the engine's `out/lanes/`.
