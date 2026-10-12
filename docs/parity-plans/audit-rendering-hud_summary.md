# Execution summary — audit-rendering-hud (session 3)

- Plan: `/home/clintonnguyen/g/code_examples/Mindustry-Godot/docs/parity-plans/audit-rendering-hud.md`
- Session 3: worktree `../Mindustry-Godot-session3`, branch `parity/session-3`, bridge 6990, display :12
- Dates: 2026-10-12 → 2026-10-13. Bootstrap: `tools/parity-session.sh --session 3`, then `git merge --ff-only main` (clean fast-forward to `06340c4`; the §3 merge/conflict note was obsolete as stated).
- Local gates (final): `cargo test -p mind-core --lib` → **1942 passed, 0 failed, 4 ignored**; `cargo test -p mind-gdext --lib` → 26 passed; `cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` → clean; `mind-headless run spine_place_break` → checksum `a1a7b96167c9718d`, `pass: true`.

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 UI CanvasLayer above objective markers | done (in-engine) | `f01b327` |
| T2 shadow/darkness half-tile offset | done (in-engine) | `1a1ac6b` + `37ba080` (clippy follow-up) |
| T3 top-table key-combo line | done (in-engine) | `52b4d42` |
| T4 palette checked accent frame | done (in-engine) | `8bf1b7f` |
| T11 belt item ordering + weak test | done (in-engine) | `5aeb51f` |
| T8 belt autotiling + animation | done (in-engine) | `e031438` |
| T6 flying ground shadow | done (in-engine) | `100ac50` |
| T7a soft shadow over legs | done (code; visual delta subtle) | `100ac50` |
| T7b unit/weapon outlines | done (in-engine; generation deviation, see below) | `5dcd208` |
| T12 FX producer + regions + polyline + shake | done (in-engine for producer/regions; shake structural) | `11ce662` |
| T10 per-part drill/turret drawing | done (drill verified in-engine; turret structural) | `0bd9e5d` |
| T9 construct sites → blockbuild shader | done (in-engine) | `5075b87` |
| T14 F6 block status markers | done (in-engine) | `d5adb63` |
| T15 minimap attack indicators + unit icons | done (in-engine); feed stubbed | `b03aed9` |
| T19 map-area darkness | **partial — plan's stop rule** | `f59fcab` |
| T18 menu label centring | done (in-engine) | `17b9598` |
| T20 skip-wave button + hint | done (skip placed; hint = no change, Java-verified) | `eae6e94` |
| T13 shields | done (code); **in-engine blocked** | `4b160e3` |
| T16 command-mode selection overlay | done (in-engine) | `6be52fd` |
| T5 cursor-based unit hover | done (in-engine) | `aade84d` |
| T17 Shift command panel | **not reached** | — |

Commits (`git log --oneline main..HEAD`, newest first) with one-line rationale:

```
aade84d Fixes PAR-0094 residual (T5): hover uses the cursor world position
6be52fd Fixes PAR-0157 (T16): command-mode selection overlay
4b160e3 Fixes PAR-0129 (T13): force-projector domes and unit shield bubbles
eae6e94 Audit fix (rendering audit T20): skip-wave button inside the status row
b03aed9 Fixes PAR-0096 (T15): minimap attack indicators and unit icons
f59fcab Audit fix (rendering audit T19, partial): border-darkness helper + probe
17b9598 Audit fix (rendering audit T18): centre the menu button labels
d5adb63 Fixes PAR-0097 (T14): F6 draws block status markers
5075b87 Fixes PAR-0152 residual (T9): construct sites use the blockbuild shader
0bd9e5d Fixes PAR-0152 residual (T10): per-part drill and turret drawing
11ce662 Fixes PAR-0070/PAR-0128 (T12): combat FX reach the view; regions, polylines, shake
5dcd208 Fixes PAR-0126 (T7b): draw unit/weapon outline sprites
100ac50 Fixes PAR-0126 (T6+T7a): flying ground shadow, soft shadow over the legs
e031438 Fixes PAR-0130 (T8): belt autotiling and frame animation
37ba080 Audit fix (rendering audit T2 follow-up): shadow test module after the helpers
5aeb51f Fixes PAR-0130: belt items draw above every belt; fix the weak axis test
8bf1b7f Audit fix (rendering audit T4): palette cell shows the checked accent frame
52b4d42 Audit fix (rendering audit T3): top table key-combo line from the bundle
1a1ac6b Audit fix (rendering audit T2): shadow/darkness maps shift half a tile up-left
f01b327 Audit fix (rendering audit T1): UI CanvasLayer draws above objective markers
```

## In-engine verification (screenshots vs Java frames)

All evidence under `runs/<dir>/` (shared runs tree):

- T1 `runs/20261012-s3-t1/`: before (audit capture, label over HUD) vs `after-label-behind-palette.png` (label dimmed behind the UI; Ui layer=2) + planet dialog still above the planet. Luminance in the label row over the panel dropped 63.5 → 38.0.
- T2 `runs/20261012-s3-t2/`: core/boulder luminance bands (up/down/left/right) now match `java-groundzero-ingame.png` within ~10 units (before: 40–60 dark up-right).
- T3 `runs/20261012-s3-t3/after-key-combo.png` vs `runs/20261009-065416-ref-ground-zero/java/r05-19-conveyors.png`: “Conveyor / Key: [3,1]” + 40×40 “?” at the right edge.
- T4 `runs/20261012-s3-t4/`: accent frame on conveyor, moves to junction, cleared on air; matches `r05-19-conveyors.png`. (Found and fixed a root cause: a double `_rebuild` in one frame makes Godot auto-rename cells, so the highlight matches via a block→button map.)
- T11 `runs/20261012-s3-t11/belt-items-clean.png`: copper items spanning the drill belt ring across tile boundaries.
- T8 `runs/20261012-s3-t8/t8-belts-a2.png` (L/T belt cluster, corner+join sprites, `missing_regions: 0`; chevron phase moves between shots). Also fixed a real sim bug: `ConveyorBuild.blend_bits` stored the 4-bit neighbour mask instead of the autotile case (`Conveyor.java:216`, duct.rs already used the case), which also mis-gated the clog threshold.
- T6/T7a `runs/20261012-s3-t6/after-alpha-crop.png` vs the Java crop: the alpha’s grey silhouette shadow now sits down-left of the core as in `java-groundzero-ingame.png`.
- T7b `runs/20261012-s3-t7/after-alpha-unit.png` vs the same Java crop: body + non-top `small-basic-weapon` outline edges match.
- T12 `runs/20261012-s3-t12/`: effect arcs/lines/light discs render over the world (`final-hit-small-crop.png`, `after-regions-paused-crop.png`); before the Z fix (`probe-z1000-crop.png`) nothing was visible. Also fixed invisibility: `MindFx` sat at z_index 0 under all world bands; it is now parked at the `Layer.effect` band.
- T10 `runs/20261012-s3-t10/drill-cd.png`: rotator interior pattern moves between shots; base/top fixed.
- T9 `runs/20261012-s3-t9/construct-shader-p5.png` / `-p30.png`: dashed accent outline scan + diamond reveal at progress ≈0.08/0.5 (staged with a player build plan while paused + `step()`).
- T14 `runs/20261012-s3-t14/f6-on2-duo.png` vs `f6-off2-duo.png`: green diamond at the duo’s bottom-right with F6 on; none off.
- T15 `runs/20261012-s3-t15/minimap-a.png`: pulsing orange square + alpha team icon at the player tile (`indicators_json`/`minimap_units_json` corroborate).
- T18 `runs/20261012-s3-t18/after-menu.png` vs `java-menu.png`: labels centred.
- T20 `runs/20261012-s3-t20/skip-forced-crop.png`: play glyph in `StatusRow/Row`, node at (277,0) 40×80 (forced visible for the shot; hidden in-game because `waveSending` is false in the tutorial).
- T16 `runs/20261012-s3-t16/`: spawn a team-1 dagger, box-drag it through real `godot_input` (`selection_json` → `[1000000001]`), accent hexagon fan+ring renders; `command-move-target.png` shows the moved unit still ringed.
- T5 `runs/20261012-s3-t5/hover-unit.png`: top table shows “Dagger” for a cursor on the unit between tile centres; `hover_info(132,53)` (old path) returns `{}`.

Structured-only checks (stated, not visual): T12 shake wiring (no `hit_shake>0` weapon stageable in Ground Zero; `FxEvent::Shake` → `Camera2D::apply_shake_event` code path + mind-core sink test), T10 turret aim (same `TurretState.rotation` the combat tests assert), T19 darkness bands (unit tests only — see blockers).

## Deviations from the plan (each documented in its commit body)

1. **T7b generation** — the plan asks for a new unit pass writing `<name>-outline` and a re-pack. The Java source it cites does the opposite for vanilla units: `UnitType.createIcons` outlines the body region *in place* (`makeOutline(makeNew = alwaysCreateOutline && …)`, UnlockableContent.java:174-190), and the packed `alpha` region already carries the #565666 border; `generate/units.rs` already mirrors this. Added only the missing **draw** path (unit `<name>-outline` if present + `<weapon>-outline` for non-top weapons); no re-pack, `probe("alpha-outline")` stays false on purpose.
2. **T8 blending logic** — the plan asked to port `Autotiler.buildBlending` into `render/autotile.rs`; the port already had it (`mind-core/src/world/blocks/autotiler.rs`) and the sim cached it on `ConveyorBuild`, so `render/autotile.rs` only holds the Java frame math; `ConveyorBuild.blend_bits` was corrected from the mask to the case.
3. **T12 shake scaling** — the plan says to multiply by `settings screenshake / 4`; that scaling already lives in `ShakeState::offset`, so raw intensity is forwarded (no double scaling).
4. **T13 shader path** — the plan’s CanvasGroup + `shield.gdshader` cannot work: the ported shader is a screenspace effect sampling a full-screen `u_texture` (Java’s effectBuffer blit); a CanvasGroup buffer is only reachable as `TEXTURE`. Implemented Java’s `animateShields == false` branch instead; the animated path belongs to the plan-16 effectBuffer composite.
5. **T15 age-out** — `AttackIndicators.update` adds `Time.delta` (seconds) with `duration = 15*60` → 900 s, not 900 ticks as the plan says.
6. **T19** — stopped after the helper + tests per the plan’s own stop rule (see blockers).
7. **T20 hint** — no scene change: Java anchors the hint mid-left (`group.fill(t -> t.left())`), confirmed against `r02-01-move.png`; the port already matched. Only the skip button moved.
8. **T14 gate** — Java’s F6 gate is `block.hasConsumers`, so drills never get a marker (the plan’s drill example does not apply).

## Blockers (for the coordinator)

1. **T19 partial (campaign owner)** — Ground Zero loads with `limit_area = null` (`tile_debug`), i.e. the loaded sector `Rules` drop `limitMapArea`, so the minimap/world keep the unrestricted look. Plumb `Rules.limitMapArea`/`borderDarkness` into the loaded sector world, then apply `max(get_static_darkness, border_darkness)` in `minimap.rs::tile_color` (helper + tests already landed).
2. **T13 in-engine (sim/power owner)** — a placed `force-projector` stays `efficiency 0.0` (radscl 0) even with adjacent sandbox `power-source`s and after a sector reload, so no dome could be staged; also the screen-space shield shader needs the plan-16 effectBuffer composite.
3. **T15 feed (combat owner)** — `BuildDamageEvent` exists but is never emitted by the sim; emit it on player-team building damage and drain it into `MindMinimap::add_indicator` (provider + widget drawing are done; `add_indicator` is verifiable from the console).
4. **T16 attack reticle (units owner)** — `AttackTarget` stores an entity id, not a position; resolving it needs an entity-id lookup before `Drawf.target` can be drawn.
5. **T7b follow-up (render owner)** — the old merged defect “unit outlines missing” was a draw-path gap, now fixed; if the render owner ever adds an `effectBuffer`-style composite, revisit animated shields.

## Follow-ups

- **T17 (Shift command panel) was not reached.** Everything it needs now exists (`command_mode()`, `selection_json()`, `set_unit_stance`; T16 proved selection works), but `set_unit_command` still does not exist in `input/mod.rs`, so per the plan the command buttons would render disabled; the panel itself (palette swap + unit-type buttons + counts) is untouched.
- The editor/game for session 3 is still running (bridge 6990). Run `tools/parity-session.sh --session 3 --stop` to tear it down.
- `runs/` contains the T1–T20 evidence dirs (`runs/20261012-s3-t*/`), all under the shared runs tree.
