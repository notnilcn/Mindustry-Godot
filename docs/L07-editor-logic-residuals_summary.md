<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L07 — Map editor and logic residuals (wave 4: T1–T3; T4 session B; T5–T6 wave 5)

Session A (session 2: worktree `Mindustry-Godot-session2`, branch `parity/session-2`, display `:11`, bridge 6980).
Owner of this summary; session B reports L07 T4 (logic font / printchar / icon tag) for merging in.

## Per-task status

| Task | Status | Notes |
|---|---|---|
| T1 palette order + `spawn` (PAR-0110) | **done, MATCH** | Comparator moved to mind-core with Java's `instanceof OverlayFloor`; five `update` flags fixed; the editor now paints overlay floors onto the overlay layer. |
| T2 WASD binding seam + `resetSaved` guard | **done, MATCH** | `MindInput.binding_axis`; the pan speed was 60x slow; `saved` resets only for editing tools. |
| T3 sub-dialog residue (PAR-0431, PAR-0432, PAR-0434) | **done** (plan items 1 and 5-eye recorded) | Banned Blocks, World Processors, Waves (line graph), Objectives, Locales and Map Load all match Java; generate runs through `ui.loadAnd`. |
| T4 logic font / printchar / icon tag | **done (code)** | glyph rasteriser + `unicodeToName`/`getLargeIcon`; no live display pair yet (see continuation). |
| T5 display/canvas residuals | **partial** | canvas size/PNG/autosave/edges/pre-pass landed; off-screen image pre-pass landed. |
| T6 logic pickers | **done (code + widget checks)** | pickers, enum columns, permanent-status seconds, drag-follow, sensors; residuals below (2026-10-19 merge). |
| PAR-0438 (Save Game -> New Save) | **done, MATCH** | Saves while paused; Java's keys and prompt layout. |
| Stretch | done | `MindRichLabel` `font_color` bridge; Arc gray constants; `fullTogglet`/`flatTogglet`/`squareTogglet` theme styles; disabled icon-button text. |

## Commits

```
d1d3bff4 L07 stretch: Arc gray/lightGray instead of Godot's X11 named grays
2b060e9b L07 T1: golden manifest hash for the patched golden_content.json
7f5c1d16 L07 T2: java_xkeys --hold-ms for held Java keys (editor WASD pairing)
fccf1386 PAR-0438: New Save writes its file while paused; prompt keys and layout like Java
5bb046cd L07 T3: editor generate runs through ui.loadAnd; processor row glyphs
b2b3c7e5 L07 T3: Objectives, Locales and Map Load layouts like Java (PAR-0432)
a23c894e L07 T3: Waves dialog line graph, group rows and button order (PAR-0432, part)
013d8d40 L07 T3: World Processors empty state centred; banned search/portrait (PAR-0434)
109a6b2f L07 T3: Banned Blocks pane titles, centred <empty>, uniform panes (PAR-0431)
d25ff196 L07 T2: editor WASD through the move_x/move_y bindings; resetSaved tool guard
cbee76a4 L07 T1: editor palette order puts spawn with the ores (PAR-0110)
```

## T1 — palette order and `spawn` (`runs/20261018-s2-L07-T1/notes.md`)

- **Root cause.** `palette_blocks` sorted with `maps::filters::block_info::is_overlay`. That helper omits `SpawnBlock` and
  `RemoveOre`, which both extend `OverlayFloor` (`SpawnBlock.java:6`, `RemoveOre.java:11`). So `spawn` sorted as a
  natural floor into row 2.
- **Comparator.** `mind_core::editor::palette::palette_order` ports `MapEditorDialog.java:846-864`:
  - order: core, then `!synthetic`, then non-`OverlayFloor`, then id;
  - filters: `inEditor` and `debugOnly`.
  - It uses the registry's `is_overlay_floor_kind`. The gdext `palette_blocks` calls it. The icon and `localizedName`
    filters stay in the GDScript shell.
- **The pinned list** is derived from Java source with the exact comparator over the `Blocks.java` declaration order
  (content ids follow it). It was then checked against a fresh Java palette capture. `air` is in the list but dropped
  by the UI icon filter (no `air` sprite).
- **Second bug found by the test.** `target-dummy`, `unit-cargo-unload-point` and the three Erekir assemblers lacked
  `update = true` (`TargetDummy.java:38`, `UnitCargoUnloadPoint.java:26`, `UnitAssembler.java:62`). `synthetic()`
  therefore sorted them with the terrain. The port-derived `parity/golden_content.json` pinned the old flags; it is
  patched for those five blocks only, and the manifest sha follows (`2b060e9b`).
- **Editor drawing** (found while classifying `spawn`). The port had no `EditorTile.setFloor` overlay redirect
  (`EditorTile.java:27-32`): pencilling an ore replaced the floor, and `spawn` was placed as a block. Now:
  - `EditorBlockInfo` classifies `spawn`/`remove-ore` as `Floor` + `OverlayFloor`;
  - `draw_one` sends an `OverlayFloor` to the overlay layer, honouring `needsSurface` (ores skip liquids, `spawn` does
    not).
- **Icon.** The red "S" is Java's real `spawn` sprite (`assets-raw/sprites/blocks/environment/spawn.png`). No icon change
  was needed.
- **Tests:**
  - `editor::palette::tests::palette_first_40_match_java`;
  - `spawn_sorts_with_the_ores_and_buildings_after`;
  - `editor::tests::draw_overlay_floor_goes_to_overlay_layer`.
- **In-engine (real input):** Editor → sandbox → Open In Editor, palette scrolled with a real wheel. Fresh Java batch:
  `runs/20261018-s2-L07-java/`.
  - `diff/editor-side-side.png` MATCH: six cores, `deep-water`.., `stone` on row 4;
  - `diff/palette-ores-side.png` MATCH: the `spawn` row (white/grey squares, blue, S, A, A) before the ores.

## T2 — editor input (`runs/20261018-s2-L07-T1/notes.md`)

- **Binding seam.** `MindInput.binding_axis(name)` is a raw `Core.input.axis(Binding)` over the bridge's pressed keys,
  without `pan_axis`'s dialog gate. `map_view.gd` pans by `move_x`/`move_y` (`MapView.java:180-185`): rebinds apply,
  and arrows no longer pan (Java's defaults are A/D, S/W).
- **Pan speed.** The step is `15 * Time.delta`, where `Time.delta` counts 60 Hz ticks. The port used `15 * delta`
  seconds, 60x slower than Java.
- **Real-input check:**
  - a 1 s D hold moves `offset_x` −922 (Java: 900/s);
  - 200 ms D: Godot −184 px vs Java −173 px (`java_xkeys.py --hold-ms 200 d`). MATCH within a frame of hold jitter.
- **`resetSaved` guard.** `map_view_touch` marks the map unsaved only for `tool.edit` (`MapView.java:96-98`).
  `map_view_drag` does so only for a moved draggable drag (`:135-137`); `map_view_flush` never does. The Line release
  still resets via `dev_draw_line` (`:114-116`). Pinned by `EditorTool::press_resets_saved`/`drag_resets_saved`
  (`editor::tool` tests).
  - Java never reads `saved` (`MapEditorDialog.java:50`, "currently never read") and clears it on every open (`:322`),
    so this is state parity with no visible frame.
- **Harness.** `tools/java_xkeys.py --hold-ms` holds a key by XSendEvent: XTEST keys do not reach Java on the session
  Xvfb.

## T3 — sub-dialogs (`runs/20261018-s2-L07-T3/notes.md`; Java frames = the fresh `:11` batch in `runs/20261018-s2-L01-T3/java/`)

| Dialog | Change (Java) | Result |
|---|---|---|
| Banned Blocks (PAR-0431) | Title colours `f25555`/`Pal.accent` (the MindRichLabel `font_color` bridge), titles centred, pane content centred like Arc's pane table (`<empty>` mid-pane), `uniform()` pane widths, centred search row; search on `localizedName` only; portrait branch (`BannedContentDialog.java:55-160`) | MATCH; panes 452 vs 422 px (Godot scroll bar) |
| World Processors (PAR-0434) | List content-sized and centred (`MapProcessorsDialog.java:67-69`); `Icon.pencil`/`Icon.trash` | MATCH (x 250 vs 247) |
| Waves (PAR-0432) | `WaveGraph.java` ported: the line plot, marks, ticks, hover column, zoom/pan, legend and mode row. Rust `used` keeps only units that spawn in the window and exposes `max_y`/`unit_count`. `SpawnGroup::from_json` reads doubles as ints (Arc `getInt`): GDScript's JSON round trip had dropped every `end`/`max`/`spacing`. Rows: icon + accent name + light-gray wave; Back/Edit/Random; begin-sort ties on type id | MATCH (same curves, max 9); row pitch ~6 px taller |
| Objectives (PAR-0432) | No side panel without a selection; `Pal.darkestGray` grid at stroke 3; buttons at their pref widths | MATCH |
| Locales (PAR-0432) | `MapLocalesDialog.java:60-200` three-column layout, rows in a pane, centred `<empty>`, `?` at the right end, 64 px buttons | MATCH |
| Map Load (PAR-0432) | Rewritten to `MapLoadDialog.java:35-71`: 3-column 250x90 `squareTogglet` cards with 64 px previews; Cancel/Load | MATCH (custom-map order is profile file order) |
| Generate / Sector Generate | `UiRoot.load_and` = `UI.loadAnd` (show text, 7 ticks, call, hide); the HUD IO driver no longer hides foreign overlays | overlay frame is STAGED (`loading-overlay-staged.png`); the real Apply path was checked by state |
| Resize (plan item 3) | none: Java never disables OK. Arc `Table.field` calls its listener only when `valid()` passes, so out-of-range text keeps the last valid size (`MapResizeDialog.java:31-34,56-61`); the port does the same | recorded |

Map generate structure passes:
- The editor's Sector Generate is `world.loadSector` (`SectorGenerateDialog.java:88-107`), so it runs whatever the
  planet generator runs. Enemy bases, ruins and the launch loadout live in session B's generator (the Simplex fix
  `341d9f74` and the Erekir rules work).
- No editor-side pass was added; the port's `sector_generate` already calls that generator. In-engine Erekir sector 0
  generated in the editor.

## PAR-0438 — New Save (`runs/20261018-s2-PAR-0438/notes.md`)

- **Root cause.** `save_slot` queued the write for the next IO boundary, but the pause menu that offers Save Game halts
  the pump: `io_pending` stayed 1 until the game resumed. `save_slot` now `flush_io()`s when paused, like the exit save
  (Java writes inside `ui.loadAnd`, `SaveDialog.java:26-31`).
- **Prompt.**
  - Keys: `showTextInput("@save", "@save.newslot", 30, …)`; `@save.name` is not a bundle key.
  - Behaviour: the name goes through `load_and("@saving")`, and the dialog stays open and relists the slots
    (`Core.app.post(this::setup)`). The prompt answer is a one-shot connection; it was a lasting one that would have
    turned later prompts into saves.
  - Layout: `showTextInput` is Java's plain `Dialog` (`UI.java:324-340`): left title with no rule, one 400x4 accent line,
    underline field.
- **Real input:** typed name `s2fixed` → `saves/s2fixed.msav` written while paused, listed in the dialog.
  - Java: dialog opened only, then cancelled.
  - `diff/newsave-prompt-side.png`: MATCH.

## Stretch fixes

- `MindRichLabel` mirrors a `font_color` override into `default_color`. `MindLabel` is a RichTextLabel, so ports of
  Arc's `.color(...)` on MindLabels were ignored.
- Arc grays:
  - Godot `Color.GRAY` is X11 #BEBEBE and `LIGHT_GRAY` #D3D3D3; Arc's are 0x7f7f7f / 0xbfbfbf.
  - 25 uses now take `MindStyles.ARC_GRAY`/`ARC_LIGHT_GRAY` (map-list "Custom", disabled texts, placeholders, …).
- Theme: `fullTogglet`, `flatTogglet` and `squareTogglet` get their Java styles (they used the generic fallback).
  `icon_button` greys its text label when disabled.

## Tests and gates

```
cargo test -p mind-core --lib editor::            # 65+ pass (palette, overlay draw, tool resetSaved)
cargo test -p mind-core --lib wave_graph          # 5 pass (units_without_spawns_in_window_are_dropped)
cargo test -p mind-core --lib spawn_group         # 9 pass (spawn_group_json_reads_float_ints)
cargo test -p mind-core --lib content::parity     # 5 pass (golden patched)
cargo test --no-fail-fast -p mind-core -p mind-gdext -p mind-headless
    # mind-core 2781 lib pass; only the 3 planet generation checksums fail (expected until session B's batch);
    # mind-headless parity:: 59 pass after the manifest hash update
cargo fmt --all -- --check                        # clean
cargo clippy --workspace --all-targets -- -D warnings   # clean
cargo run -p mind-headless -- parity goldens      # 97 PASS
bash tools/godot.sh --headless --editor --quit --path client     # import gate OK
godot --script res://tools/ui_layout_check.gd     # LAYOUTCHECK failed=0
godot --script res://tools/ui_widgets_check.gd    # UICHECK failed=0
```

## Deviations and residuals (recorded)

- T3 plan item 1, Objectives editing, is not ported: `Vertices` (QuadMarker) rows, connector/parent drag curves and
  the per-tile pencil/trash popups (`MapObjectivesDialog.java`, `MapObjectivesCanvas.java`).
- T3 plan item 5: the `Icon.eye` config-open button exists only in the in-game editor (`state.isGame() &&
  state.isEditor()`, `MapProcessorsDialog.java:108-114`) and is not ported. `Icon.none` was already in
  `IconSelectDialog`.
- Waves:
  - the search filter does not show the selected unit's icon (`imageUp = filterType.uiIcon`);
  - group rows ~6 px taller.
- Banned Blocks panes are 452 px against 422 (Godot scroll bar plus panel margin).
- Map Load: Java's `makeButtonOverlay` lets the pane run under the buttons; the port stops above them.
- The loading overlay frame is staged (7 ticks < screenshot round trip).
- The editor shell palette header (bigger magnifier, "Stone" lower in Java) is unchanged: L03 territory.

## Ledger changes to apply

| Item | Change | Evidence |
|---|---|---|
| PAR-0110 | verified / close | `runs/20261018-s2-L07-T1/diff/{editor-side,palette-ores}-side.png` |
| PAR-0431 | verified / close | `runs/20261018-s2-L07-T3/diff/ed-banned-side.png` |
| PAR-0432 | verified / close (residuals above) | `runs/20261018-s2-L07-T3/diff/ed-{waves,objectives,locales,mapload}-side.png` |
| PAR-0434 | verified / close | `runs/20261018-s2-L07-T3/diff/ed-processors-side.png` |
| PAR-0438 | verified / close | `runs/20261018-s2-PAR-0438/diff/newsave-prompt-side.png`, notes |
| PAR-0407 | partial: T1–T3 landed; T4 (B), T5–T6 pending | this summary |
| new (fixed) | Editor pencil with an ore replaced the floor, and `spawn` was placed as a block (no `EditorTile.setFloor` redirect) | `cbee76a4`, mind-core test |
| new (fixed) | Editor wave groups lost `end`/`max`/`spacing` after the GDScript JSON round trip (graph to 45 vs 9) | `a23c894e` |
| new (fixed) | MindLabel colour overrides ignored; Godot named grays used for Arc grays | `109a6b2f`, `d1d3bff4` |
| new (fixed) | The HUD IO driver hid every loading overlay one frame after it was shown | `5bb046cd` |
| new (open) | Save Game dialog is a text list; Java's is the LoadDialog layout (search, mode filters, 2-column slot cards, New Save in the button row) | `runs/20261018-s2-L07-java/java/savedialog.png` |
| new (open) | `unit-cargo-unload-point` and the Erekir assemblers lack `solid = true` (`UnitCargoUnloadPoint.java:26`, `UnitAssembler.java:62`); not changed (sim/pathing effect) | — |

## For session B / coordinator

- `maps/filters/mod.rs` (B-owned): `block_info::is_overlay` and `is_floor` omit `SpawnBlock` and `RemoveOre`. Java's
  `instanceof OverlayFloor`/`Floor` includes both. The editor and palette no longer use them. Check whether the filters
  should (registry `is_overlay_floor_kind`/`is_floor_kind`).
- `golden_content.json` + manifest changed for five `update` flags; the sim goldens all pass.
- The `ui_widgets_check` run prints a Rust panic (`godot-core object.rs:420`) that also occurs on the clean tree.

## Done criteria (wave 4 scope)

T1 and T2 landed and verified in-engine with real input against a fresh Java capture. T3 landed (items 1 and the
in-game eye recorded). PAR-0110 closes: the editor walk is MATCH. T4 is session B's; T5–T6 are wave 5.


---

## Continuation (later 2026-10-18 session)

### T4 — logic display font + printchar + world-processor icon tag — **DONE (code)**

- `MindAssets.logic_font()` exposes `Fonts.logic` (`logic.ttf`).
- `display_context.rs` now rasterises `commandPrint`/`printchar` glyphs through
  the Godot `FontFile` at 16 px: glyph index, `render_glyph`, texture crop
  (`get_glyph_uv_rect` with a normalized-UV fallback), `glyph.xoffset/yoffset`,
  `get_ascent()` and a cap height from the `H` glyph. `Glyph` caching is per
  character. `LogicDisplay.commandPrint`'s existing placement math is unchanged.
- `Fonts.unicodeToName`/`getLargeIcon` are ported through
  `MindAssets.icon_name_for_code` (`Iconc::name`) and
  `icon_region_for_name` (`Iconc::texture`). `select_overlay` now returns the
  icon `(name, region)` pair and `overlay_top.rs` draws the region above the
  processor at `tilesize` width, matching `LogicBlock.java:715-724`.
- Recorded deviation: the per-call `Draw.alpha(0.5)` has no Godot
  `draw_texture_rect` modulate equivalent, so the icon draws opaque; the atlas
  region is used instead of a 48 px `iconLarge` bitmap font.
- No live text/`printchar` frame pair was captured this session (a display +
  processor walk needs a Java reference capture).

### T5 — display/canvas residuals — **PARTIAL**

- 480 px canvas: `canvas_edit_dialog.gd` now uses `480x480`
  (`CanvasEditDialog.java:185`).
- Canvas PNG import/export + 2 s auto-save: implemented with `FileDialog`,
  `Image.load_from_file`/`save_png`, nearest-palette mapping and an alpha blend
  over `palette[0]` (`CanvasEditDialog.java:60-71,216-274`).
- Canvas edge sprites (**landed**): `logic_displays.rs` now draws `side1`/
  `side2`/`corner1`/`corner2` for every mask side with no same-block
  neighbour, including the y-flipped second corner (`CanvasBlock.java:274-288`).
- Off-screen image-source pre-pass (**landed**): `process_display` now walks
  `draw image` source slots first and recursively processes their buffers with
  a visiting set (`LogicDisplay.java:163-178`).

### T6 — logic pickers/polish — **PARTIAL**

- Permanent statuses hide the `seconds` field: `MindSimHost.status_permanent`
  + a GDScript filter in `logic_dialog.gd` (`LStatements.java:1465-1474`).
- Sensors: unit `@id` now returns `NaN` (Java `UnitComp.sense` has no `id` case)
  and `RadarTarget.boss` tests the `boss` status (`u.isBoss()`), with
  `RadarCandidate.boss` populated from `StatusComp`.
- The icon-grid pickers, enum popup columns/tooltips, drag-follow and jump
  release capture remain open.

### Other

- `Input.config_spec_json(x,y)` was added as a probe/test seam for the config
  panels (the same `DesktopBridge::config_spec` the tap handler uses).

### PAR-0445 — Save Game layout — **DONE (code + UI gate)**

- `save_dialog.gd` now extends `load_dialog.gd` (`SaveDialog extends
  LoadDialog`): shared search row, gamemode toggles, two-column save cards and
  the per-card autosave/delete/rename/export buttons.
- `LoadDialog` exposes `title_key`, `_add_setup_buttons()` and
  `_card_pressed()`; Save overrides the title (`@savegame`), replaces the
  `@save.import` button with `@save.new` (`new_save`) in the button row and
  routes a card click through `@save.overwrite` before `save_slot`.
- `ui_widgets_check.gd` EV-0066 updated to the Java layout (one card + the
  bottom New Save button); the prompt still queues the entered name.

---

## Continuation (2026-10-19): T6 pickers merged

The T6 branch (`agent/l07t6`, merged as `d694829b`) lands the logic-editor
picker work on top of the earlier T4/T5/T6 code:

```
0ab54d20 L07 T6: port ui.effects and expose logic picker metadata
09987959 L07 T6: expose content, sound and preview seams for logic pickers
f70b44f2 L07 T6: logic editor content pickers, enum columns and drag-follow
96f9e31c L07 T6: widget checks for picker columns and permanent statuses
73550716 L07 T6: match the remaining enum button and cell sizes
4e8874f3 L07 T6: cap the sensor tab's unit/block panes
f0241834 L07 T6: pin the Layer reflection order for SetMarker
```

### What landed

- `ui.effects.show` is a manifest dialog (`effects_dialog.gd/.tscn`) ported from
  `EffectsDialog.java`; `MindLogic` exposes `fx_entries`, the reflected `Layer`
  list and `LAccess.settable`/`senseable` for `setprop`/sensor tabs.
- `MindSimHost.logic_picker_json` returns unit/item/liquid/status/weather/block
  rows with Java's unlocked/hidden/logicControllable filters; `MindAssets`
  exposes sound entries and a UI-bus preview with stop-previous.
- `logic_dialog.gd` uses icon-grid pickers for `ubind`, `playsound`, status,
  weather, `setmarker` layers, `setprop`/sensor tabs, ore and `ui.effects`.
- Enum popup columns/cell sizes follow the call site (2/3/4 columns), with
  `LCanvas.tooltip(Enum)`; the 35-entry `Layer.java` order is pinned.
- Permanent statuses hide `seconds` (the earlier filter read field 0 instead of
  field 1); drag-follow, jump-release clearing and the two-pass
  `setJumpHeights` are in place; building `@name`/`@id` are null and unit `@id`
  is NaN (as recorded), boss radar uses the `boss` status.

### Tests and gates

```
cargo test -p mind-core --lib logic::statement      # 13 pass
cargo test -p mind-core --lib ui::manifest          # 6 pass
cargo test -p mind-core --lib                       # 2823 pass / 4 ignored (merged)
cargo test -p mind-gdext --lib                      # 81 pass
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings  # clean
cargo fmt --all -- --check                          # clean
bash tools/godot.sh --headless --editor --quit --path client          # exit 0
ui_widgets_check.gd                                 # UICHECK failed=0 (merged tree)
```

### Residuals (recorded)

- Effects preview cells draw an animated HSV/checker wash rather than Java's
  live `EffectCell.draw` particle render; `calculateSize` is unused.
- Sound picker: no play/pause icon state and no `countPlaying` polling; preview
  stops when the popup closes.
- Drag draws the dragged row and a drop line but not Java's placeholder gap.
- `setJumpHeights`, jump release, building/unit `@id` and boss radar were
  already correct from the earlier T6 code (no change needed).

### Ledger

PAR-0407: T4/T5/T6 code landed; keep open only for the listed residuals and the
live Java display/picker frames.
