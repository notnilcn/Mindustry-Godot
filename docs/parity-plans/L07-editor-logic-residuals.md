<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L07 — Map editor and logic residuals

**Goal.** Close the J08 "recorded" list (PAR-0407) and the editor items the round-L walk still
sees.

**Build.** Mixed: T2's `resetSaved` guard and T4's logic font/icon tag are Rust
(`mind-gdext/src/editor/mod.rs`, `display_context.rs`, `sim_host.rs`). The editor UI tasks
(T1, T3, T5, T6) are GDScript-side (T1 still ships its `mind-core` pinning test).

**Evidence.** Round-L report G7 and §2 item 11; `runs/20261017-s3-J08/recorded-items.md`;
`docs/J08-editor-logic-residuals_summary.md`; I03/I06 deviation lists (round I).

## T1 — Editor block palette order and the `spawn` entry — HIGH

- Java `MapEditorDialog.addBlockSelection` (`editor/MapEditorDialog.java:844-870`):
  - Sort: cores first, then `!synthetic()` before synthetic, then non-`OverlayFloor` before
    `OverlayFloor`, then `id`. `SpawnBlock` is an `OverlayFloor`, so it lands with the ores.
  - Skip `!uiIcon.found`, `!inEditor`, `BuildVisibility.debugOnly` and search misses.
  - `clearNoneTogglei` buttons with `resizeImage(32)`.
- Port: `spawn` appears in row 2, drawn as a red `S` glyph, which shifts every later entry
  (`runs/20261017-L-twin/diff/31-editor-side.png`). J08 T2 recorded this but fixed only the
  highlight.
- Change: port the comparator and filter exactly (a `mind-core` test pins the first 40 names
  against a list dumped from Java), and give `spawn` its real `uiIcon` region.

## T2 — Editor input residuals — MEDIUM

1. WASD panning polls physical keys (`client/scenes/editor/map_view.gd:44-65`). Java uses
   `Core.input.axis(Binding.moveX/moveY)` (`editor/MapView.java`). Add a `MindInput` binding
   query seam and use it.
2. `resetSaved` fires for non-editing tools (`mind-gdext/src/editor/mod.rs:225` area). Add a
   tool-kind guard per `MapEditor` (`editor/MapEditor.java`).

## T3 — Editor sub-dialog residue — LOW

1. Objectives:
   - `Vertices` (QuadMarker) row editing;
   - connector/parent drag curves;
   - per-tile pencil/trash popups
   (`editor/MapObjectivesDialog.java`, `MapObjectivesCanvas.java`).
2. Map generate: run the generator structure passes (enemy base, ruins, launch loadout) in the
   editor, and show the `ui.loadAnd` loading screen (`editor/MapGenerateDialog.java`,
   `SectorGenerateDialog.java`).
3. Resize: disable OK while a size is out of range (`editor/MapResizeDialog.java`).
4. Banned content: search `localizedName` only; port the portrait branch
   (`editor/BannedContentDialog.java`).
5. Processors: the `Icon.eye` config-open, plus the `Icon.none` entry in `IconSelectDialog`
   (`editor/MapProcessorsDialog.java`, `ui/dialogs/IconSelectDialog.java`).

## T4 — Logic display font + `printchar` + world-processor icon tag — MEDIUM

- Java:
  - `LogicDisplay` draws `drawflush` text with `Fonts.logic`; `printchar` draws glyphs
    (`world/blocks/logic/LogicDisplay.java`).
  - World processors show an icon tag through `Fonts.unicodeToName`
    (`ui/Fonts.java:121-123`).
- Port: "Deferred: the logic font rasterizer is not ported yet"
  (`mind-gdext/src/display_context.rs:130`); the icon tag is deferred (`sim_host.rs:8508`).
- Change: rasterise the logic bitmap font (`assets/fonts/`) into the CPU display raster, and
  port the `unicodeToName` table.
- Accept: a processor printing text and `printchar` glyphs to a display matches Java.

## T5 — Display and canvas residuals — LOW

1. Off-screen image-source pre-pass (`LogicDisplay.java:163-178`).
2. Canvas edge sprites (`side1/side2/corner1/corner2`, `CanvasBlock.java`).
3. Canvas `@import`/`@export` PNG + the 2 s auto-save timer.
4. Canvas paint surface at 480 px (the port uses 300).

## T6 — Logic editor pickers and polish — LOW

1. Icon-grid pickers in place of text fields:
   - `ui.effects.show` → port `ui/dialogs/EffectsDialog.java` (named nowhere in the port);
   - `PlaySoundStatement.showSoundSelect`;
   - the `UnitBindStatement` unit grid;
   - the `SetMarkerStatement` layer list;
   - the `SetProp`/`ApplyStatus`/`Weather*` content pickers (`logic/LStatements.java`).
2. Enum popup columns per call site (2/3/4) + `LCanvas.tooltip(Enum)`.
3. Hide `seconds` for permanent statuses (`LStatements.java:1483-1485`).
4. The dragged statement follows the pointer; `setJumpHeights` uses Java's two-pass
   recursion; a press-release without motion clears the jump target.
5. Sensors:
   - building `@name`/`@id` (null in Java, check);
   - unit `@id` must be NaN (Java `UnitComp.sense` has no `id` case; the port returns the
     entity index);
   - boss radar target (needs a boss flag on `UnitTypeDef`).

## Done criteria

T1, T2 and T4 landed and verified in-engine. T3/T5/T6 landed or recorded. PAR-0110 closes when
the editor walk is MATCH. Summary in `docs/L07-editor-logic-residuals_summary.md`.
