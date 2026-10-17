<!-- SPDX-License-Identifier: GPL-3.0-only -->
# J08 — Map editor and logic residuals (session 3, wave D)

Branch `parity/session-3`, base after the wave-C merge `7178b401`, session 3
(display :12, bridge 6990).

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 — Editor building icons (`fullIcon` + rotation) | done (verified in-engine) | `061ddaba` |
| T2 — Editor palette content/order + highlight | done (verified in-engine) | `d2176386` |
| T3 — Editor input residuals | partial (T3.3 landed; T3.1/T3.2 recorded) | `ddeae35d` |
| T4 — Sub-dialog residue | recorded (`runs/20261017-s3-J08/recorded-items.md`) | — |
| T5 — Map assets dialog | **gated on J09 T4** (session 4 in flight) | — |
| T6 — Logic font + printchar + icon tags | recorded (full anchors; needs the bitmap-font rasteriser) | — |
| T7 — Display/canvas residuals | recorded (4 items with anchors) | — |
| T8 — Logic editor pickers/polish | recorded (5 items with anchors) | — |

## T1 — editor building icons

Java `EditorRenderer.recacheChunk` (`editor/EditorRenderer.java:223-239`) draws
`block.fullIcon` centred at `x*tilesize + block.offset`, rotated by
`tile.build.rotdeg()` when `block.rotate && block.rotateDrawEditor`, then the
team region. `world/Block.java:124-130` defines `rotate` (false),
`rotateDraw`/`rotateDrawEditor` (both true) and `visualRotationOffset` (0).

- `BlockSpec`/`BlockDef` gained `rotate`, `rotate_draw`, `rotate_draw_editor`,
  `visual_rotation_offset`; `rotate` derives from the block kind through the
  canonical `kind_rotates` list (moved beside the block content, aliased from
  `world::block` so the sim and the editor cannot drift).
- The five `rotateDraw = false` sites (`Blocks.java:1318-1320,1440,1458,1476,6795`):
  coal-centrifuge, oxidation-chamber, electric-heater, slag-heater, heat-source.
- `editor_renderer.rs` resolves the packed `block-<name>-full` region (base
  `def.region` fallback) and draws it through the rotated unit mesh builder.
- Tests: `rotate_fields_match_java_defaults` + the existing
  `kind_rotates_matches_java`.

Evidence `runs/20261017-s3-J08-T1/`: the sandbox core now draws its sprite in the
editor where the twin frame had a grey placeholder (`diff/core-zoom.png`).

## T2 — editor palette

Checked field by field against `MapEditorDialog.addBlockSelection`
(`editor/MapEditorDialog.java:844-895`): the sort (core → synthetic → overlay →
id), the filters (`uiIcon`/`inEditor`/`debugOnly`/localized-name search), the
`clearNoneTogglei` 50 px cells with 32 px icons and 6 desktop columns — the port
already matches (`mind-gdext/src/editor/mod.rs:646-677`).
`spawn` renders its real `block-spawn-ui` region (the source sprite's centre is
(179,48,48) — the red S seen in the frames is the real icon).

- `theme_builder.gd`: `clearNonei`/`clearNoneTogglei` now use the accent
  `flat-down-base` frame for `down`/`checked` (`ui/Styles.java:310-327`), so the
  selected palette entry shows Java's accent frame instead of a flat slate box.

Evidence `runs/20261017-s3-J08-T2/` (`godot/01-palette-selected-frame.png`,
`diff/palette-side.png`, `notes.md`). Recorded residual: Java's *block filter
presets* restrict by `BlockGroup` (no `spawn` under the Stone preset); the
port's own search matches the localized name like Java.

## T3 — editor input residuals

Landed: `ecs::TeamId::SHARDED` now aliases `game::team::SHARDED` (`TeamId(1)`,
`game/Team.java`); it was `TeamId(0)` (derelict). `cargo test -p mind-core` stays
green (2740 passed) so no caller relied on the stale id.

Recorded (`runs/20261017-s3-J08-T3/notes.md`): WASD panning still polls physical
keys (`client/scenes/editor/map_view.gd:44-65`) instead of
`Core.input.axis(Binding.moveX/Y)` — needs a `MindInput` binding-query seam; and
`resetSaved` still fires for non-editing tools (`mind-gdext/src/editor/mod.rs:225`
area) — needs a tool-kind guard.

## T4/T6/T7/T8 — recorded

`runs/20261017-s3-J08/recorded-items.md` holds each item with its Java anchors
and the port's current behaviour: objective-editor markers/drag curves/popups;
map-generate structure passes + `ui.loadAnd`; resize OK-disable; banned-content
raw-name search; processor `Icon.eye`/`Icon.none`; the logic bitmap-font
rasteriser for `drawflush`/`printchar` (`LogicDisplay.java`, the deferral at
`mind-gdext/src/display_context.rs:130`) and `Fonts.unicodeToName`
(`ui/Fonts.java:121-123`) for the processor icon tag
(`sim_host.rs:8508`); the display/canvas residuals (off-screen image pre-pass,
edge sprites, import/export + 2 s auto-save, 480 px surface); and the logic
editor picker/enum/drag/sensor polish list.

## T5 — gated

The map-assets dialog needs the plan-20 mod-data client half from J09 T4
(session 4 landing it in parallel); the port's `map_assets_dialog.gd` remains
the stub and `MindEditor.assets()` the stub seam.

## Gates

- `cargo fmt --check` clean; clippy `-D warnings` clean on `mind-core` +
  `mind-gdext`; `cargo test -p mind-core` **2740 passed** (the only failure is
  the pre-existing `blocks_golden::family_counts_match_golden`); Godot headless
  import/parse gate clean.
- `editor_renderer.rs` changes are additive (the floors/walls path untouched).

## Ledger changes to apply

- New item: "Editor draws buildings as unrotated base regions (grey squares)"
  → closed by `061ddaba` (T1).
- New item: "Editor block palette selected entry has no accent frame" → closed
  by `d2176386` (T2); the sort/filter/spawn-region checks are recorded in the
  task notes.
- New item: "`ecs::TeamId::SHARDED` is the derelict id" → closed by `ddeae35d`
  (T3).
- Fragments to queue for wave E: WASD bindings + `resetSaved` guard (T3),
  the logic bitmap-font rasteriser + `unicodeToName` (T6), the four display/
  canvas items (T7), the five logic-picker items (T8), the five sub-dialog
  items (T4); T5 stays J09-gated.
- No chain/ledger files were edited.
