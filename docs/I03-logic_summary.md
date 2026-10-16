# I03 — Logic plan summary (session 3 / worktree `Mindustry-Godot-session3`, 2026-10-15)

Pass 1 of round-I plan `I03-logic.md` (14 tasks). Branch `parity/session-3` @ `10121f70`.
Worktree clean at stop; all work committed; no stash.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T1 ubind/ucontrol/ulocate (PAR-0221) | done | commit `aa3c17e5`; 10 tests; in-engine poly LogicAI at (801,480) |
| T2 completionLogicCode (PAR-0224) | done (headless only, per plan) | commit `7069f63f`; `objective_completion_script_runs_live` |
| T3 sensor/fetch/printchar (PAR-0218) | **done headless-only slice** (family `logic_sense` overrides not reached) | commit `e7a4fefa`; 7 tests + in-engine processor |
| T4 turret control shoot/shootp (PAR-0223) | done | commit `7d9ad1df`; 2 tests + in-engine duo r=0/s=1/bullets |
| T5 radar/uradar (PAR-0222) | done | commit `10121f70`; 4 tests + in-engine near/far flip |
| T6 processor links / pencil panel (PAR-0220) | not reached | — |
| T12 display sim half (PAR-0105 p1) | not reached | — |
| T13 display render half (PAR-0105) | not reached | — |
| T14 canvas (PAR-0225) | not reached | — |
| T7 logic dialog shell (PAR-0103) | not reached | — |
| T8 field layouts (PAR-0103) | not reached | — |
| T9 drag/jump/add (PAR-0103) | not reached | — |
| T10 variables/globals (PAR-0103) | not reached | — |
| T11 overlays (PAR-0103 slice) | not reached | — |

Dependency watch: `I01 T5` (`BulletDef.range_change`, `fn turret_range`) WAS on main and was
merged (`6f0f619a`) before T3; `I01 T10` (`unit_by_id`) was **not** on main at stop — T3 did not
need it, so no dependent half was skipped; T11 (not reached) is the item that relates to it.

## Commits (`git log --oneline main..HEAD`)

```
10121f70 Fixes PAR-0222: radar sort direction, range, filters, permissions and 30-tick cache
7d9ad1df Fixes PAR-0223: logic-controlled turrets (control shoot/shootp)
e7a4fefa PAR-0218: sensor/fetch/printchar tables match Java (content, unit, cores, client locale)
6f0f619a Merge branch 'main' into parity/session-3
7069f63f Fixes PAR-0224: objective completionLogicCode runs against the live world
aa3c17e5 Fixes PAR-0221: ubind/ucontrol/ulocate bind and drive live units through LogicAI
```

`6f0f619a` resolved one adjacent-append conflict in `core/sim/logic_campaign_tests.rs`
(session 2's I01 T6 explosion test vs my T1/T2 tests); both sides kept, full suites re-run green
after the merge. The merge picked up I01 T1-T6 and I05 T1-T4/whatever was on main at the time.

## Test commands and results (final, at T5)

Run from `client/rust`, ONE cargo at a time (`flock /tmp/opencode/mg-build.lock`, `CARGO_BUILD_JOBS=6`):

```
cargo test -p mind-core --lib --features msav-import
  -> test result: ok. 2581 passed; 0 failed; 4 ignored; 0 measured
cargo test -p mind-headless --lib
  -> test result: ok. 133 passed; 0 failed
cargo test -p mind-gdext --lib
  -> test result: ok. 48 passed; 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings
  -> Finished; no warnings
cargo fmt -p mind-core -p mind-gdext -p mind-headless -- --check
  -> clean
cargo run -q -p mind-headless -- parity goldens
  -> 97 PASS / 0 non-PASS
```

Baseline (I00): core 2551 / headless 133 / gdext 47 / goldens 97. Note: `cargo clippy -p
mind-headless` has 2 pre-existing `expect_used` errors in `campaign_scenarios.rs` (not in the
I-environment clippy gate, not touched by this branch).

## In-engine evidence (session 3 bridge 6990, display :12, Ground Zero serpulo 170)

T1 — `runs/20261015-s3-I03-T1/` (`poly.png`, `poly-moved.png`, `notes.md`):
- spawn poly team 1 at (1080,480), world processor (118,50), code
  `ubind @poly / ucontrol move 100 60 0 0 0 / set u @unit / stop`:
  `unit_debug_json` -> poly `kind:"LogicAI"`, `pos:[801.0,480.0]` (target 800,480);
  `logic_get_state(118,50)` -> `u` = poly. After `logic_set_code(118,50,"stop")` + `step(601)`:
  poly `kind:"CommandAI"` (type default). Log panic count 0.

T2 — headless only (no shipped map uses `completionLogicCode`).

T3 — `runs/20261015-s3-I03-T3/notes.md` (no PNG): processor (118,50) program
`fetch core c @sharded 0 / sensor n c @type / fetch coreCount k @sharded 0 @air /
sensor h @dagger @health / set l @clientLocale` ->
`c = building core-shard`, `n = content core-shard`, `k = 1`, `h = 150`
(Java `UnitTypes.java:104` is 150; the plan text's "130" is wrong), `l = "en"`. Panic count 0.

T4 — `runs/20261015-s3-I03-T4/` (`duo-logic-shoot.png`, `notes.md`): drill (130,48) + duo
(132,48), processor (118,50) looping `getblock building d 132 48 / control shoot d 137 48 1 /
sensor r d @rotation / sensor s d @shooting` -> `r = 0`, `s = 1`, `get_group_count("bullet") = 2`;
after code `stop` + `step(300)` -> 0 bullets. Before the fix: `r = 90`, `s = 0`, 0 bullets.
Panic count 0.

T5 — `runs/20261015-s3-I03-T5/notes.md`: two crux daggers at x 1100/1180, processor (118,50)
`radar enemy any any distance d 1 near` / `... d 0 far` -> near @x 136.24, far @x 140.96
(the nearer/farther flip). Panic count 0.

## Deviations from Java (Java file:line)

- T1 `autoPathfind` (J/ai/types/LogicAI.java:70-90): the spawner drop-zone stop is omitted (the
  wave-spawner list is removed from the world while the unit loop runs); the enemy-core walk is
  exact. `payEnter` (`Call.unitBuildingControlSelect`) deferred — no building-unit possession seam
  (I01 T12). `payTake`/`payDrop` apply via a same-tick `LogicWorldEvent::Payload` handled by
  `Sim` (the payload helpers need the block runtime the VM cannot reach); the 90-tick transfer
  gate runs in the executor before emitting.
- T1 `ucontrol build`'s `block.unlockedNow()` term is satisfied unconditionally: the port's
  placement guard has no tech-unlock check (existing). `canBeBuilt` approximated by
  `buildVisibility != Hidden`.
- T3 building `@name`/`@id` are null now (J/entities/comp/BuildingComp.java:2099-2170 has no rows);
  unit `@id` is still the port's entity index (Java `UnitComp.sense` has no `id` case -> NaN);
  left as an existing deviation, not changed in this slice. Family `logic_sense` overrides
  (`@progress` for GenericCrafter/Drill/Separator/UnitFactory/Reconstructor/ConstructBlock/
  MassDriver/LaunchPad/MendProjector; heat families; ForceProjector `@shield`; Door `@enabled`;
  Conveyor `@progress`/`@firstItem`; Core `@maxUnits`/`sense(UnitType)`; LightBlock `@color`) are
  NOT implemented. Boss radar target matches nothing (no boss flag on `UnitTypeDef`).
- Existing (not introduced): the port's `mono` has no weapons while Java `UnitTypes.mono` has a
  `RepairBeamWeapon`, so `hasWeapons()` rows differ for mono-family units.
- T4: the `!unit.isPlayer()` gate on turret control has no port surface (no possessed turret);
  omitted.
- T5 changed the mind-headless capability checksum (`a740d5e42b2bb201` -> `88460edb203c67e0`,
  `farthest Some(4) -> Some(5)`): the old value encoded the inverted health sort
  (J/logic/RadarSort.java `sortDir = order ? 1 : -1`).

## Blockers

None. No task hit a hard blocker; the remaining tasks are budget, not blockage.

## Ledger changes to apply (coordinator applies to `chains.json`)

| Item | After | Note |
|---|---|---|
| PAR-0221 | verified | T1 tests + in-engine poly |
| PAR-0224 | verified (headless) | T2 |
| PAR-0218 | **keep `fixing`** | content/unit/base-building/fetch/printchar/client-locale slice landed (`e7a4fefa`); family `logic_sense` overrides remain |
| PAR-0223 | verified | T4 tests + in-engine duo |
| PAR-0222 | verified | T5 tests + in-engine near/far; golden re-baselined |
| PAR-0220, PAR-0105, PAR-0225, PAR-0103 | unchanged | not reached (T6, T12-T14, T7-T11) |

## Done criteria

Not met: 5 of 14 tasks are done (T3 as a partial slice); T6, T12, T13, T14, T7-T11 remain.
All landed work is committed, the suites/clippy/fmt/goldens are green on the branch, and the
summary is written here. A continuation pass should start at T6 (per the plan's order after T5)
and then T12 -> T14 -> T7-T11.

---

# Pass 2 (session 3, 2026-10-15)

Branch `parity/session-3` @ `2c55be4b`. Worktree clean at stop; all work committed; no stash.
Start-of-session merge landed `48bc7a23`; a second `git merge --no-edit main` landed the I07
fixes (the earlier main tip had a broken `MindCampaign`/`MindUi` call site, fixed upstream by
`4f95c4e0`). No conflicts.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T6 processor links / pencil panel (PAR-0220) | **done** | commit `b00cedf3`; 7 tests + real-click in-engine (pencil panel, tap add/remove link) |
| T12 display sim half (PAR-0105 p1) | **done** | commit `577dc3af`; 12 tests (6 raster + 6 sim) + in-engine bufferSize/displayWidth |
| T13 display render half (PAR-0105) | **done** | commit `3aa608db`; in-engine red/green display + merged 2x2 group + frame |
| T14 canvas (PAR-0225) | **partial**: sim + render + sense landed | commit `2c55be4b`; 5 tests + in-engine canvas with painted pixels |
| T3 remainder (family `logic_sense` overrides) | not reached | — |
| T7 logic dialog shell (PAR-0103) | not reached | — |
| T8 field layouts | not reached | — |
| T9 drag/jump/add | not reached | — |
| T10 variables/globals | not reached | — |
| T11 overlays | not reached | — |
| T14 pencil panel + `canvas_edit` dialog | not reached (inside T14) | — |

## Commits (`git log --oneline main..HEAD`)

```
2c55be4b PAR-0225: canvas blocks render and accept logic read/write (sim/render half)
3aa608db Fixes PAR-0105: logic displays render drawflush output (incl. tileable displays)
577dc3af PAR-0105: tileable display linking, root routing and a CPU display rasterizer
b00cedf3 Fixes PAR-0220: processor link config and lifecycle, pencil config panel, tap-to-link
```

## Test commands and results (final, at T14)

Run from `client/rust`, ONE cargo at a time (`flock /tmp/opencode/mg-build.lock`, `CARGO_BUILD_JOBS=6`):

```
cargo test -p mind-core --lib --features msav-import
  -> test result: ok. 2623 passed; 0 failed; 4 ignored; 0 measured
cargo test -p mind-headless --lib
  -> test result: ok. 133 passed; 0 failed
cargo test -p mind-gdext --lib
  -> test result: ok. 48 passed; 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings
  -> Finished; no warnings
cargo fmt -p mind-core -p mind-gdext -p mind-headless -- --check
  -> clean
cargo run -q -p mind-headless -- parity goldens
  -> 97 PASS / 0 non-PASS
bash tools/godot.sh --headless --editor --quit --path client | grep -E "SCRIPT ERROR|Parse Error"
  -> prints nothing
```

New tests: `link_toggle_via_number_config`, `unlink_reenables_disabled`,
`processor_removed_reenables`, `world_processor_not_accessible`, `tap_links_valid_target`,
`processor_cannot_be_picked_up`, `privileged_processor_immune` (T6);
`clear_fills_buffer`, `rect_covers_expected_pixels`, `line_with_stroke`,
`poly_hexagon_area_within_2pct`, `transform_translate_then_rect`,
`transform_rotate_then_rect`, `tileable_2x2_links_root_bottom_left`,
`flush_draw_forwards_to_root`, `image_packs_root_index`, `queue_cap_1024`,
`display_process_drains_into_raster`, `display_sense_dimensions` (T12);
`bitmask_table_matches_java_edges` (T13 `TileBitmask`);
`canvas_linear_pixels_roundtrip`, `canvas_write_out_of_range_ignored`,
`canvas_logic_read_write`, `canvas_save_roundtrip` (T14).

## In-engine evidence (bridge 6990, display :12, Ground Zero serpulo 170)

T6 — `runs/20261015-s3-I03-T6/` (`01-processor-panel.png`, `02-linked.png`, `03-link-final.png`):
micro-processor (135,60) + switch (133,60); real click opens the panel (`config_selected_json` =
`[135,60]`) with one 40x40 pencil button; a click on the switch toggles the link
(`logic_get_state(135,60).links` `[]` -> `[{"name":"switch1","x":133,"y":60,"valid":true}]` -> `[]`)
with the panel staying (`Consume`); the pencil does nothing until T7; panel opens no dialog.
Log panic count 0.

T12 — `runs/20261015-s3-I03-T12/` (`tileable-group.png`): world processor (118,50) programs:
`logic-display` (124,50) -> `@bufferSize` = 1, `@displayWidth` = 80; 2x2 `tile-logic-display`
member (141,61) -> `@bufferSize` = 1 (root queue), `@displayWidth` = 52 (`2*32-12`),
`@operations` = 1. Panic count 0.

T13 — `runs/20261015-s3-I03-T13/` (`display-red-green.png`, `tileable-2x2.png`,
`tileable-closeup.png`): `logic-display` (124,48) with `draw clear 255 0 0 / draw color 0 255 0
255 / draw rect 10 10 30 30 / drawflush` -> `logic_display_pixel(124,48,20,20)` = 0x00FF00FF
(green), `(5,5)` = 0xFF0000FF (red, cleared); screenshot shows the red square with the green
rectangle at the lower-left. The 2x2 group (140..141,60..61) with `draw clear 0 0 255 / draw
color 255 255 0 255 / draw rect 20 20 40 40` -> `logic_display_state` 52x52 `rectangular:true`,
`logic_display_pixel(140,60,25,25)` = 0xFFFF00FF, member (141,61) reads the ROOT raster; the
screenshot shows ONE merged blue/yellow picture with the tile frame sprites. Panic count 0.

T14 — `runs/20261015-s3-I03-T14/` (`canvas-pixels.png`, `canvas-closeup.png`): `canvas` (128,46)
+ processor program `write 1 c 0 / write 5 c 5 / write 9 c 3 / sensor w c @displayWidth` ->
`w` = 12 (canvasSize); the screenshot shows the 12x12 canvas with position 0 pink, position 5
teal and the palette-out-of-range write ignored. Panic count 0.

## Deviations from Java (Java file:line)

- T6 icon tag: `Character` config dropped from the input path (`ConfigValue` has no char
  variant; `LogicBlock.java:80-84` is editor-only anyway).
- T6 `on_proximity_removed` iterates the resolved link list instead of `executor.links` so a
  processor removed before its first `updateCode` still re-enables its targets.
- T12/T13 rendering is a CPU raster + `ImageTexture` upload instead of Java's GPU
  `FrameBuffer` (the plan's binding design decision). `process_display` is called by the view.
- T12 drops Java's off-screen image-source pre-pass (`LogicDisplay.java:163-178`): a display
  used as an `image` source is only current if it was visible/processed in an earlier frame.
- T12/T13: the display transform lives in the view raster (Java mutates and saves the build's
  `Mat` during rendering; the port keeps "view never feeds sim").
- T13 `commandPrint` glyphs are not drawn (`AtlasIcons::glyph` always misses; no logic font
  rasterizer); non-rectangular tileable groups draw the root buffer instead of per-tile slices.
- T14 `write`/`read` use the LINEAR position (`CanvasBlock.java:283-295`); the port's old x/y
  form is gone. Canvas edge sprites (`side1/side2/corner1/corner2`) are not drawn.
- T13 panic fix (deviation in implementation): the pass does all host work in `process()`;
  a draw-time `bind_mut` on `MindSimHost`/`MindAssets` panicked when the frame was triggered
  inside a caller holding a shared borrow (two crash reports at 22:39).

## Blockers

None hard. T7-T11 and the T3 remainder are budget, not blockage. `I01 T10` (`unit_by_id`) is
still not on `main`, so T11 (LogicAI unit squares) may want a re-check when it is reached.

## Ledger changes to apply (coordinator applies to `chains.json`)

| Item | After | Note |
|---|---|---|
| PAR-0220 | verified | T6 tests + real-click in-engine (pencil panel, tap-to-link) |
| PAR-0105 | verified | T12+T13 tests + in-engine red/green display, merged 2x2 group, frame |
| PAR-0225 | **keep `fixing`** | T14 sim/render/sense landed (`2c55be4b`); the pencil panel + `canvas_edit` dialog remain |
| PAR-0218 | keep `fixing` | unchanged this pass (family `logic_sense` overrides still outstanding) |
| PAR-0103 | unchanged | T7-T11 not reached |

## Skipped

No `skipped: waiting for <dep>` lines: T6/T12/T13/T14 needed no cross-plan seam.

## Done criteria

Not met: 3 tasks done (T6, T12, T13) plus the T14 sim/render half; T7-T11, T14's edit dialog
and the T3 remainder remain. All landed work is committed on `parity/session-3` (`b00cedf3`,
`577dc3af`, `3aa608db`, `2c55be4b`), suites/clippy/fmt/goldens/parse gate are green, and the
in-engine evidence is under `runs/20261015-s3-I03-T{6,12,13,14}/`. A continuation should start
at T14's dialog half or T7 (the logic dialog shell) per the plan's order, then T8-T11.

---

# Pass 3 (session 3, 2026-10-16)

Branch `parity/session-3` @ `2b35f8f1`. Worktree clean at stop; all work committed; no stash.
Start-of-pass merge landed `3d029285` (I06 pass 3) cleanly; no second merge was needed.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T14 tail (pencil panel + `canvas_edit` dialog) | **done** (T14 now complete) | commit `110a5ba6`; in-engine pencil -> paint -> save |
| T3 remainder (family `logic_sense` overrides) | **done** | commit `2ed5485d`; `family_sense_overrides` + in-engine door/core |
| T7 logic dialog shell (PAR-0103) | **done** | commit `bebe927a`; in-engine open/delete/save |
| T8 field layouts (PAR-0103) | **partial**: `sanitize` + the generic field builder landed | commit `2b35f8f1`; in-engine field edit saved |
| T8 tail (per-statement `build(Table)` layouts, enum popups, colour pencil, conditional rebuilds) | not reached | — |
| T9 drag/jump/add, T10 variables/globals, T11 overlays | not reached | — |

## Commits (`git log --oneline main..HEAD`)

```
2b35f8f1 PAR-0103: logic editor generic statement fields (sanitize + per-field editors)
bebe927a PAR-0103: logic editor dialog shell (statement list, edit menu, save through configure)
2ed5485d Fixes PAR-0218: family logic_sense overrides (crafter, drill, heat, doors, cores, turrets' neighbours)
110a5ba6 Fixes PAR-0225: canvas blocks render, accept logic read/write and edit dialog
```

## Test commands and results (final, at the T8 slice)

Run from `client/rust`, ONE cargo at a time (`flock /tmp/opencode/mg-build.lock`, `CARGO_BUILD_JOBS=6`):

```
cargo test -p mind-core --lib --features msav-import
  -> test result: ok. 2641 passed; 0 failed; 4 ignored; 0 measured
cargo test -p mind-headless --lib
  -> test result: ok. 133 passed; 0 failed
cargo test -p mind-gdext --lib
  -> test result: ok. 50 passed; 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings
  -> Finished; no warnings
cargo fmt -p mind-core -p mind-gdext -p mind-headless -- --check
  -> clean
cargo run -q -p mind-headless -- parity goldens
  -> 97 PASS / 0 non-PASS
bash tools/godot.sh --headless --editor --quit --path client | grep -E "SCRIPT ERROR|Parse Error"
  -> prints nothing
```

New tests: `family_sense_overrides`, `textio::tests::field_values_roundtrip`;
`repo_dialogs_manifest_complete` now also checks `EXPECTED_LOGIC_DIALOGS`
(`canvas_edit`, `logic`); the canvas tests use the palette-len constructor.

## In-engine evidence (bridge 6990, Ground Zero serpulo 170)

T14 tail — `runs/20261016-s3-I03-T14-tail/` (`01-canvas-panel.png`,
`02-canvas-edit.png`, `03-painted.png`, `04-world-painted.png`): the canvas
pencil opens `canvas_edit` (size 12, bpp 3, 8 palette colours seeded from the
live block); swatch 5 + a cell click paint (`_pixels[104]` = 5); Back saves
through `configure_building_bytes` (`logic_canvas_pixels(128,46)[104]` = 5) and
the world canvas shows the new pixel.

T3 remainder — `runs/20261016-s3-I03-T3-remainder/notes.md`: a live door reads
`@enabled` 0 -> 1 after `configure_building(bool)` and the core-shard reads
`@itemCapacity` = 4000 (building `@name` stays null).

T7 — `runs/20261016-s3-I03-T7/` (`01-logic-dialog.png`, `02-saved.png`): the
processor pencil opens `logic` with 4 statement rows (`lst.operation` for `op`
via the new `bundle_key`); deleting the `print` row and clicking Back leaves
`logic_get_code(135,60)` = `set x 1\nop add x x 2\nstop\n`.

T8 slice — `runs/20261016-s3-I03-T7/03-fields.png`: each element shows its
field rows (`set` -> `to [x] From [5]`, `op` -> `op [add] dest [x] a [x] b
[2]`); submitting `from = 5` and Back saves `set x 5\nop add x x 2\nstop\n`.
The synthetic key events do not reach the LineEdits, so the commit was driven
through the field's `text_submitted` signal.

Log panic count 0 after every in-engine step.

## Deviations from Java (Java file:line)

- T14 tail: `@import`/`@export` PNG buttons (FileChooser) and the 2-second
  auto-save timer are not ported; the canvas paint surface is 300px (Java 480)
  so the palette row fits the port's 648px panel; `Icon.fill` has no icon-font
  alias (text fallback). `large-canvas` gained its 16-colour woodspark palette
  + bpp 4 (`Blocks.java:6965-6983`).
- T3 remainder not implemented (ported-state gaps): core `@maxUnits` (no
  `Rules.unitCap` field yet), `UnitFactory/Reconstructor @itemCapacity`
  (`Mathf.round(itemCapacity * rules.unitCost(team))` needs the factory item
  capacity/rules unit cost in the hook), `@config` (`configSenseable`).
- T7: `@add`/`@variables` are disabled buttons until T9/T10; the `@edit` menu
  is an inline popup without Java's `@editor.worldprocessors.editname` row; the
  statement rows render at the panel's top-left (layout polish deferred).
- T8 slice: field labels use the raw field names (Java uses bundle `token`
  keys); enum fields are plain text fields (no `showSelect` popup until the T8
  tail); commits fire on submit/blur instead of Java's per-keystroke `changed`.
- `MindTable.clear()` was added (a dialog row rebuild otherwise left stale
  `_cells` entries and a `_grid` freed-instance SCRIPT ERROR).

## Blockers

None hard; T8-tail/T9/T10/T11 are budget, not blockage. `I01 T10`
(`unit_by_id`) is still not on `main`, so T11 (LogicAI unit squares) should be
re-checked when it is reached.

## Ledger changes to apply (coordinator applies to `chains.json`)

| Item | After | Note |
|---|---|---|
| PAR-0225 | verified | T14 complete (sim/render/sense + pencil panel + `canvas_edit`) |
| PAR-0218 | verified | T3 remainder landed: family `logic_sense` overrides (the three listed gaps stay) |
| PAR-0103 | **keep `fixing`** | T7 done; T8 is a partial slice (generic fields); T8-tail/T9/T10/T11 remain |

## Skipped

No `skipped: waiting for <dep>` lines: nothing in this pass needed a cross-plan
seam.

## Done criteria

Not met: T6, T12, T13, T14, T7 are done and T3 is complete apart from three
listed gaps; T8's tail and T9-T11 remain. All landed work is committed on
`parity/session-3` (`110a5ba6`, `2ed5485d`, `bebe927a`, `2b35f8f1`), the
suites/clippy/fmt/goldens/parse gate are green, and the in-engine evidence is
under `runs/20261016-s3-I03-T14-tail/`, `runs/20261016-s3-I03-T3-remainder/`
and `runs/20261016-s3-I03-T7/`. A continuation should finish T8's per-statement
layouts, then T9 (drag/jump/add), T10 (variables/globals) and T11 (overlays).

---

# Pass 4 (session 3, 2026-10-16)

Branch `parity/session-3` @ `30c9d329`. Worktree clean at stop; all work committed; no stash.
Start-of-pass merge landed `90d2b688` (I01 pass 3, I06 pass 4, I02 pass 1) cleanly.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T8 tail — enum selects, colour picker, conditional layouts | **partial**: the enum/colour/discriminator machinery + the `draw` layout landed; the remaining per-statement tables are open | commit `522d4e8e`; in-engine popup -> `draw poly` saved |
| T9 — add dialog | **done** | commit `c19c4fc3`; in-engine add + save |
| T9 tail — drag reorder + jump arrows | not reached | — |
| T10 — variables and globals dialogs | **done** | commit `ca9bf1d0`; in-engine `x | 3 | number` + the globals list |
| T11 — logic overlays | **done** | commit `30c9d329`; `overlay_probe` + screenshot |

## Commits (`git log --oneline main..HEAD`)

```
30c9d329 PAR-0103: processor link/range overlay, LogicAI unit squares, tags and message hover text
ca9bf1d0 PAR-0103: logic variables and globals dialogs
c19c4fc3 PAR-0103: logic editor add dialog
522d4e8e PAR-0103: logic editor enum selects, colour picker and the draw layout
```

## Test commands and results (final, at T11)

```
cargo test -p mind-core --lib --features msav-import
  -> test result: ok. 2650 passed; 0 failed; 4 ignored; 0 measured
cargo test -p mind-headless --lib      -> ok. 133 passed; 0 failed
cargo test -p mind-gdext --lib         -> ok. 51 passed; 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings
  -> Finished; no warnings
cargo fmt ... --check                  -> clean
cargo run -q -p mind-headless -- parity goldens -> 97 PASS / 0 non-PASS
bash tools/godot.sh --headless --editor --quit --path client | grep -E "SCRIPT ERROR|Parse Error"
  -> prints nothing
```

New tests: `draw_editor_fields_match_java`, `enum_fields_carry_options`,
`draw_type_change_applies_java_defaults`,
`overlay_links_report_half_sizes`.

## In-engine evidence (bridge 6990, Ground Zero serpulo 170)

T8 tail — `runs/20261016-s3-I03-T8/` (`draw-clear.png`, `select-popup.png`,
`draw-poly.png`): the `draw clear` element shows only `r/g/b`; the type popup
lists all 16 `GraphicsType` values; picking `poly` rebuilds the row to
`x/y/sides/radius/rotation` and Back saves `draw poly 255 0 0 0 0 0`.

T9 — `runs/20261016-s3-I03-T9/` (`add-dialog.png`, `add-saved.png`): the `@add`
picker groups the localised statements (`Unknown`/`Input & Output`/`Block
Control`); clicking `Read` appends it and Back saves
`set x 1\nstop\nread result cell1 0\n`.

T10 — `runs/20261016-s3-I03-T10/` (`variables.png`, `globals.png`): the `Vars`
panel shows `x | 3 | number`; the `Built-in Variables` button lists the globals
with their `lglobal.<name>` descriptions (`false` -> 0, `@pi` -> "The
mathematical constant pi (3.141...)", ...).

T11 — `runs/20261016-s3-I03-T11/` (`config-overlay.png`): with the
micro-processor selected the range circle (80 px), the `Pal.place` square
around the linked switch and the `switch1` name render in the world;
`overlay_probe(135,60)` = `{"config":{"kind":"logic","range":80,
"links":[{"x":1064,"y":480,"size":1,"name":"switch1"}]},
"select":{"kind":"logic-tag","units":[{x,y,hit:8,rotation} x2]}}`.

Log panic count 0 after every in-engine step.

## Deviations from Java (Java file:line)

- T8 tail: only the `draw` layout is transcribed so far (the remaining
  io/block, operation/flow, unit and world per-statement layouts, the
  `fieldAlignSelect` picker and Java's exact `showSelect` sizes are open);
  enum popup labels use the raw option names (Java's `<enum>.label.<name>`
  bundle fallback).
- T9: drag reorder and the jump arrows
  (`DragLayout`/`JumpButton`/`JumpCurve`) are not ported; the add picker is a
  popup inside the logic dialog (Java uses a sub-dialog), without the
  `@players.search` focus post and the `nonPrivileged` filter.
- T10: the panels do not un-pause the game (Java's variables dialog does) and
  the button label does not flip to `@logic.globals` for var-less processors.
- T11: the world-processor icon tag is deferred (`Fonts.unicodeToName` is not
  ported), the driven-unit squares are axis-aligned (`canvas::select`; the
  rotation rides in the overlay data) and the tag text uses the default font.

## Blockers

None hard; the T8 layout tail and the T9 drag/jump tail are budget, not
blockage.

## Ledger changes to apply (coordinator applies to `chains.json`)

| Item | After | Note |
|---|---|---|
| PAR-0103 | **keep `fixing`** | T7/T9/T10/T11 landed; T8 is a slice (enum selects + `draw`) and T9's drag/jump remain |

## Skipped

No `skipped: waiting for <dep>` lines; `I01 T10` (`unit_by_id`) was on main at
start and T11 did not need it (the LogicTag scan walks `LogicAi.controller`).

## Done criteria

Not met: T1-T7, T9, T10, T11, T12, T13, T14 are done (T3 with three listed
gaps); T8's remaining per-statement layouts and T9's drag reorder + jump arrows
are open. All landed work is committed on `parity/session-3` (`522d4e8e`,
`c19c4fc3`, `ca9bf1d0`, `30c9d329`), suites/clippy/fmt/goldens/parse gate are
green, and the evidence is under `runs/20261016-s3-I03-T8/`, `-T9/`, `-T10/`
and `-T11/`. A continuation should finish T8's category batches (the four plan
commits) and T9's drag/jump arrows.

---

# Pass 5 (session 5, 2026-10-16)

Branch `parity/session-5` (the I03 continuation moved here because session 3 is
running I08; the worktree had only I06 work, merged through `872a7430`). Worktree
clean at stop; all work committed; no stash.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T8 tail — io/block layouts | **done** | commit `37778398`; 4 new Rust tests + in-engine rows |
| T8 tail — operation/flow layouts | **done** | commit `28c5db05`; op symbols/select/jump rows in-engine |
| T8 tail — unit + world layouts | **done** (one commit; see deviations) | commit `3f616942`; 21 statements + options + `editor_normalize` |
| T8 tail — world facade (`editor_normalize`) | **done** | commit `8fe892b9` |
| T9 tail — drag reorder + jump arrows | **done** | commit `70d77ff3`; real drags + saved code |
| T1-T7, T10-T14 | done in passes 1-4 | see the earlier sections |

## Commits (`git log --oneline main..HEAD`)

```
70d77ff3 PAR-0103: logic editor drag reorder and jump arrows
8fe892b9 PAR-0103: logic editor world-batch facade (editor_normalize)
3f616942 PAR-0103: logic editor field layouts (unit and world)
28c5db05 PAR-0103: logic editor field layouts (operation/flow)
37778398 PAR-0103: logic editor field layouts (io/block)
```

## Test commands and results (final, at T9)

Run from `client/rust`, ONE cargo at a time (`flock /tmp/opencode/mg-build.lock`,
`CARGO_BUILD_JOBS=6`):

```
cargo test -p mind-core --lib --features msav-import
  -> test result: ok. 2671 passed; 0 failed; 4 ignored; 0 measured
cargo test -p mind-headless --lib      -> ok. 133 passed; 0 failed
cargo test -p mind-gdext --lib         -> ok. 51 passed; 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings
  -> Finished; no warnings
cargo fmt -p mind-core -p mind-gdext -- --check -> clean
cargo run -q -p mind-headless -- parity goldens -> 97 PASS / 0 non-PASS
bash tools/godot.sh --headless --editor --quit --path client | grep -E "SCRIPT ERROR|Parse Error"
  -> prints nothing
```

New tests: `io_block_layouts_match_java`, `operation_flow_layouts_match_java`,
`unit_layouts_match_java`, `world_layouts_match_java`, the updated
`editor_select_options_match_java`; `LUnitControl::params` / `LMarkerControl::params`.

## In-engine evidence (bridge 7010, display :14, Ground Zero serpulo 170)

T8 io/block — `runs/20261016-s5-I03-T8-io-block/` (`notes.md`):
- `editor_layout("read",[a,b,c])` -> `[text read, field 0, text =, field 1, text at, field 2]`.
- `editor_layout("control",[shoot,b,0,0,0,0])` -> `[text set, select 0, text of, field 1, field 2 x, field 3 y, field 4 shoot]`.
- `editor_options("control",0)` -> only `laccess.label.enabled/shoot/shootp/config/color`.
- Screenshots `01-io-block-fields.png` (read/write/print rows),
  `03-draw-control-radar-sensor.png` (`col` + red swatch, `rect`, `print` + align,
  `drawflush to display1`).

T8 operation/flow — `runs/20261016-s5-I03-T8-op-flow/`:
- `editor_layout("op",[add,...])` vs `[not,...]` (binary vs unary) and
  `editor_layout("jump",[3,notEqual,x,false])` ending in the `jump` item.
- `editor_options("op",0)` -> `add=+`, `notEqual=not` (`name.token.not`).
- Screenshots `01-op-flow.png` (`r = flip a`, `r = max a b`, `r = logn a b`),
  `02-select-jump.png` (`s = if x not false then a else b`, `if x not false [99]`,
  `0.5 seconds`).

T8 unit — `runs/20261016-s5-I03-T8-unit/`:
- Screenshot `01-unit-rows.png`: `type @poly`; `ucontrol target x 1 y 2 shoot 1`;
  `uradar r = enemy any any order 1 sort distance`; `ulocate find building group
  core enemy? true out X ox out Y oy found? f building b`.

T8 world — `runs/20261016-s5-I03-T8-world/`:
- `editor_normalize("setrule",[unitHealth,10,0,0,100,100])` ->
  `["unitHealth","10","@sharded","0","100","100"]`.
- Screenshots `01-world-rows.png` (getblock/setblock/spawn/status),
  `02-world-rows-2.png` (message announce, cutscene pan, effect trail + swatch),
  `03-world-rows-3.png` (fetch `#`/unit rows, setmarker colour + align pencils).

T9 — `runs/20261016-s5-I03-T9/`:
- Header drag (synthetic mouse events) moved row 3 above row 0 ->
  `0 set result 0 / 1 set a 1 / 2 set b 2 / 3 set c 3` (`01-drag-reordered.png`).
- Jump-button drag onto statement 1 -> `_statements[3].fields[0] == "1"`, header
  `-> 1`, purple `Pal.place` trapezoid drawn (`02-jump-target.png`).
- Close saved `set a 1\nset b 2\nset c 3\njump 1 notEqual x false\n` on the micro
  processor (135,60).

Log panic count 0 after every in-engine step.

## Deviations from Java (Java file:line)

- T8: content/effect/sound/layer pickers remain text fields or plain selects —
  Java draws atlas-icon grids there (`LStatements.java` sensor pencil,
  `UnitBindStatement` unit grid, `EffectStatement` `ui.effects.show`,
  `PlaySoundStatement.showSoundSelect`, `SetMarkerStatement`'s `Layer.class` list,
  `SetPropStatement`/`ApplyStatusStatement`/`Weather*` content pickers).
- T8: enum popups are always 4 columns (Java passes 2/3/4 per call site) and the
  enum tooltips (`LCanvas.tooltip(Enum)`) are not ported.
- T8 `status`: `seconds` is shown for every non-clear status (Java hides it for
  permanent statuses via `content.statusEffect(...).permanent`,
  `LStatements.java:1483-1485`; the port has no status-content lookup in the editor).
- T8 world save: writing a WORLD processor through the dialog is rejected by the
  campaign `accessible()` gate when `rules.allowEditWorldProcessors` is off
  (`LogicBlock.java:120-122`, landed in T6). The normalizer mutation is proven by
  the Rust tests and the in-engine `editor_normalize` call; the micro-processor
  save path is proven end-to-end by the T9 close (above). Not a regression.
- T9: the dragged element does not translate with the pointer (Java moves it;
  the port draws the insertion line only); `setJumpHeights` is a greedy lane
  allocator instead of Java's two-pass recursion; a press-release without motion
  opens the port's target picker (Java clears the target).
- The plan's four T8 commits became five: the unit and world statement tables were
  written in one editing pass and landed together in `3f616942` (the message says
  so); the facade half followed in `8fe892b9`.
- `MindTable.sort_now()` + a deferred repeat were added to `_rebuild_list`: a
  runtime rebuild (add/drag) otherwise left every row stacked at one position
  (the other dialogs already call it).
- `LogicOp::is_func` gained `Logn` (`LogicOp.java:44` sets `func = true`; the port
  was missing it, which only the editor layout consumed).

## Blockers

None. Every I03 task is committed; the remaining gaps are the documented deviations.

## Ledger changes to apply (coordinator applies to `chains.json`)

| Item | After | Note |
|---|---|---|
| PAR-0103 | **verified** | T7-T11 complete: T8 field layouts (`37778398`, `28c5db05`, `3f616942`, `8fe892b9`) + T9 drag/jump (`70d77ff3`); passes 1-4 landed T7/T9/T10/T11. Suites 2671/133/51, clippy + fmt clean, goldens 97 PASS, parse gate clean, summary written |

## Skipped

No `skipped: waiting for <dep>` lines; no cross-plan seam was needed this pass.

## Done criteria

**Met.** Every I03 task (T1-T14) is committed with its tests and evidence (T3's
three content rows and the documented deviations remain noted, not blocking);
`mind-core` 2671 / `mind-headless` 133 / `mind-gdext` 51 all pass; clippy and fmt
clean; `parity goldens` 97 PASS / 0 non-PASS; the GDScript parse gate prints
nothing; this summary is written. The coordinator can mark PAR-0103 verified.

