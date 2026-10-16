# I02 — Rendering/HUD summary (session 4, 2026-10-15/16)

Plan: `docs/parity-plans/I02-rendering-hud.md`; environment: `docs/parity-plans/I-environment.md`.
Worktree `Mindustry-Godot-session4` (branch `parity/session-4`), bridge 7000, display :13.
The H03 T13 WIP was merged from `wip/h03-t13-pods` as instructed (`31f6a4ef`).

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T1 launch pods / landing arrivals | **done** (committed `8d83966c`) | `runs/20261015-s4-I02-T1/` (pod-0..5, flight-0..5, landing-0..5, pad-idle + crops, notes.md) |
| T2 mesh-null errors | **done** (committed `34e8c59b`) | `runs/20261015-s4-I02-T2/` (hitboxes-on, world-normal, notes.md) |
| T3 status markers | **done** (committed `e74aa16d`) | `runs/20261015-s4-I02-T3/` (f6-drill-full + marker crops, notes.md) |
| T4 wall hit flash | **done** (committed `622df962`) | `runs/20261015-s4-I02-T4/` (wall-before/flash/decay300 + t4-flash-sequence, notes.md) |
| T5 beam node/tower lasers | **done** (committed `4ae65c09`) | `runs/20261015-s4-I02-T5/` (beam-on/off + crop, notes.md) |
| T6 power range rings | **done, no change needed** (no commit) | `runs/20261015-s4-I02-T6/` (notes.md: the rings are Java's `laser-end` caps; range rings only while selected/configured/placing) |
| T7 pre-`DrawDefault` layers | **done** (committed `d152f6ab`) | `runs/20261015-s4-I02-T7/` (mixer-fixed vs mixer-buggy-z + mixer-ab, notes.md) |
| T8 toasts visible/like Java | **done** (committed `abb1ae06`) | `runs/20261015-s4-I02-T8/` (toast-topcentre, toast-warning2, announce, notes.md) |
| T9 PAR-0055 unit sprite scale | **partial / measured, uncommitted** | `runs/20261015-s4-I02-T9/` (core-alpha2.png + crops). Port alpha body measured 37 px at camerascale 4 vs the Java reference frame's ~46-47 px (same threshold family); the *code* matches Java exactly (`units.rs:92-93` divides by `scale_marker` width, `Control.java:330`), and both clients' cores measure ~96 vs ~102 px, so the world scale matches. The 20 % residual is inside my crude threshold measurement (transparent region padding), not proven to be a sprite-scale bug -> NOT claimed fixed, flagged for a re-check with a matched unit/frame pair. |
| T10 floor walk effects + fog-gated step sounds | **not reached** | — |
| T11 in-engine re-verification of PAR-0191/0244/0129/0188/0157 | **not reached** | — |

## `git log --oneline main..HEAD`

```
abb1ae06 Gameplay I02 T8: HUD toasts and announcements are visible and positioned like Java
d152f6ab Gameplay I02 T7: DrawBlock parts before the base draw under it
4ae65c09 Gameplay I02 T5: beam node and beam tower lasers
25586803 Merge branch 'main' into parity/session-4
622df962 Gameplay I02 T4: walls flash when hit (Wall.flashHit)
e74aa16d Gameplay I02 T3: building status markers follow BuildingComp.status (noOutput, pulse)
34e8c59b Gameplay I02 T2: no empty or freed meshes in the render passes
8d83966c Gameplay I02 T1: launch pods and landing-pad arrivals (H03 T13)
31f6a4ef Merge branch 'wip/h03-t13-pods' into parity/session-4
```

Working tree clean at stop; no stash.

## Test commands and exact results (final state of the branch)

```
cargo test -p mind-core --lib --features msav-import   -> 2644 passed; 0 failed; 4 ignored (after T7; T5/T8 added no core tests)
cargo test -p mind-core --lib --features msav-import   -> 2643 passed; 0 failed; 4 ignored (after T5)
cargo test -p mind-gdext --lib                         -> 51 passed; 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings -> clean (exit 0, no warnings)
cargo fmt -p mind-core -p mind-gdext -- --check        -> exit 0
cargo test -p mind-core --lib -- launch_pod_path_math landing_pod_path_at_touchdown      -> 2 passed
cargo test -p mind-core --lib -- status_pulses_with_efficiency                            -> 1 passed
cargo test -p mind-core --lib -- flash_hit_wall_logs_collision                            -> 1 passed
cargo test -p mind-core --lib -- chain_splits_at_draw_default                             -> 1 passed
cargo test -p mind-gdext --lib -- empty_quad_slice_has_no_mesh_surface                    -> 1 passed (T2)
bash tools/godot.sh --headless --editor --quit --path client | grep -E "SCRIPT ERROR|Parse Error" -> nothing (T8)
```

`parity goldens` was not re-run after T5-T8 (no golden-touching change); T1's run was **97 PASS / 0 non-PASS**.

## In-engine evidence per task (quoted)

* **T1**: `launch_payloads_json()` = `[{"id":3,"x":1124.0,"y":484.0,"time":5.0,"lifetime":120.0,"block":"advanced-launch-pad","team":1}]`;
  engine glow/flares/pod/dark copy in `pod-0..5.png` + `flight-0..5.png`; landing
  pad arrival `arriving:"copper", arrivingTimer:0.02` in `landing-0..5.png`.
* **T2**: Ground Zero load + 600 ticks + 10 evals -> 0 new `mesh_get_surface_count`
  lines (12 historic, all inside MCP `_eval` frames of an earlier broken-eval run);
  scene walk: 23 `MeshInstance2D`, 0 visible with a null mesh.
* **T3**: full drill marker sampled `(255,165,0)` = `Color.orange` = `noOutput`;
  ammo-less duo marker `(229,84,84)` = `Pal.remove` = `noInput`.
* **T4**: same wall box: no shooter `(240,196,153)` -> hit `(254,243,228)` ->
  +300 ticks `(243,223,192)`; `t4-flash-sequence.png`.
* **T5**: `power_debug` puts beam-node+battery on graph 6; the beam draws
  (`beam-on.png`) and vanishes when the battery breaks (`beam-off.png`).
* **T8**: `toast-topcentre.png`, `toast-warning2.png` (top-centre, icon, wrapped,
  Java panel), `announce.png`.

## Deviations / notes (with Java file:line)

1. **T1**: pod seeds use the port's payload counter / entity index instead of
   Java's entity id (`randomSeedRange(id, ..)`, `LaunchPad.java:252-262`) — same
   distribution (existing `drawf::random_seed_range` approximation).
2. **T5**: the pre-existing power-node branch of `links.rs` feeds `absin` with
   *ticks* where Java uses `Time.time` seconds (`Mathf.java:74-76`) — the node
   shimmer runs ~60x too fast. Not changed (out of T5's scope); **follow-up item**.
3. **T6**: the H03 pass-2 "pale rings" are Java's `laser-end` end caps
   (`Drawf.java:528-531`; 72x72 px region = 18 world px) plus the selected/
   configuring node's range ring — no always-on ring exists. Ledger PAR-0285
   should be recorded as *verified, not a bug*.
4. **T4**: the flash overlay node needs a texture in this project (a texture-less
   `MeshInstance2D` is skipped by the canvas renderer — found with an in-engine
   probe); it carries a 1x1 white `ImageTexture`.
5. **T3 (not reproduced)**: a partially-powered crafter could not be staged: a
   `combustion-generator` next to a `phase-weaver` stayed on a different power
   graph (graph 4 vs 5, satisfaction 0). The pulse itself is covered by the core
   test. **Follow-up item** (generator/consumer adjacency or generator output).
6. **T9**: see the table — the sprite-scale formula matches Java; my pixel
   measurement is 20 % off the Java reference and is not conclusive.
7. Sessions notes: several evals with mixed tab/space indentation hung the frame
   loop; recovery is `tools/session-play.sh 4 --play` (never start a second
   editor). `Camera2D.set_zoom` changes the rendered scale (it invalidated one
   T9 measurement round) — use `camera_state_json`/`center_on_tile` instead.

## Ledger changes to apply (chains.json is main-only; not edited here)

| Item | New state / note |
|---|---|
| PAR-0198 | verified visually in-engine (T1; the sim half was H04) |
| PAR-0285 | verified: not a bug — the "rings" are the Java `laser-end` caps; range rings only while selected/configured/placing (T6) |
| PAR-0055 | *not* confirmed fixed by measurement; code path matches `Control.java:330` — re-check with a matched unit/frame pair (T9) |
| new | "`Wall.flashHit` (phase walls) never flashed; the wall kinds drew cached instead of dynamic" -> fixed (T4 `622df962`) |
| new | "`DrawBlock` parts before `DrawDefault` (`-bottom`, `DrawLiquidTile`) drew above the base sprite" -> fixed (T7 `d152f6ab`) |
| new | "Beam nodes/towers drew no lasers" -> fixed (T5 `4ae65c09`) |
| new | "MCP evals with mixed indentation hang the frame loop" (session tooling note) |
| new | "power-node laser shimmer uses ticks where Java uses `Time.time` seconds" (T5 note) |
| note on PAR-0309..: none | — |
| PAR-0191, PAR-0244, PAR-0129, PAR-0188, PAR-0157 | unchanged (T11 not reached) |

## Blockers / remaining work

* T10 (walk effects + fog-gated step sounds) and T11 (the five in-engine
  re-verifications) were not reached; T9 needs a conclusive measurement.
* No merge into main was performed (per the work rules); main was merged *into*
  this branch once (`25586803`).

## Done criteria

Partially met: 8 of 11 tasks done (one of them, T6, as a recorded no-change),
evidence directories under `runs/20261015-s4-I02-*/`, this summary written; T9 is
partial, T10/T11 are not reached.

# Pass 2 (session 4, 2026-10-16)

Continued after the coordinator's request: `git merge --no-edit main` in the
worktree fast-forwarded to `90d2b688` (main's merge of this branch's pass 1,
plus I01 pass 3 / I03 pass 3 / I06 pass 4); rebuild + editor restart, no Java
client, bridge 7000.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T9 tail (PAR-0055 scale) | **done, verified, no change needed** (no commit) | `runs/20261015-s4-I02-T9/notes-pass2.md`, `port-frame.png`, `core-alpha2.png` |
| T10 (walk effects + fog-gated steps) | **done** (commit `1ed2048e`) | `runs/20261015-s4-I02-T10/notes.md` |
| T11 (re-verification) | **partial**: items 2, 3 verified; item 5 observed indirectly; item 1 inconclusive (no fix); item 4 not completed | `runs/20261015-s4-I02-T11/` |

## `git log --oneline main..HEAD` (pass 2)

```
1ed2048e Gameplay I02 T10: floor walk effects for every floor; step sounds respect fog
```

## T9 tail — matched re-measurement (PAR-0055)

Both clients draw a unit region at `region.width * 1/scale_marker.width`
(`Control.java:330`; `render/units.rs:92-93`; the marker is 4x4 px), i.e. the
alpha's 46-px region at 11.5 world px. Matched frames at the port's default
`camerascale 4.0` (no `set_zoom` this time - that call invalidated the pass-1
measurement):

* port alpha sprite bbox **41 x 40 px** on clean snow -> 41 / 4.0 =
  **10.25 world px** (`core-alpha2.png`);
* Java reference `r07-00-world.png`: alpha ~46 px with the 3x3 core (24 world
  px) measuring 103 px -> 4.29 px/world -> 46 / 4.29 = **10.7 world px**.

Difference **4.2 %** -> within the plan's 10 % bar. **PAR-0055 verified** (no
change; commit only if changed). The dagger measurement was scene-cluttered and
is not quoted.

## T10 — fog gate on step sounds (commit `1ed2048e`)

* Walk-effect coverage verified complete: `grep walkEffect` over the Java tree
  shows only `Floor.java:36` (`Fx.none`), `Floor.java:205-206` (liquid ->
  `Fx.ripple`), `UnitComp.java:739` and `LegsComp.java:183` - no vanilla floor
  defines a custom effect, and the port already emits the liquid ripple
  (`unit_draw.rs:269-287`). The `LegsComp` leg-base emitter remains a follow-up
  (the port's legs are still approximate).
* Change: `audio/steps.rs` gates the `MechComp` step with
  `!in_fog_to_player(world, x, y)` (`MechComp.java:40`), a port of
  `UnitComp.inFogTo` (`UnitComp.java:232-240`) over
  `FogControl::is_visible_tile` with the viewer team from `rules.default_team`.
  The `UnitComp` walk sound has no gate in Java and none was added.

```
cargo test -p mind-core --lib -- step_sounds_respect_fog  -> 1 passed
cargo test -p mind-core --lib --features msav-import      -> 2650 passed; 0 failed; 4 ignored
cargo test -p mind-gdext --lib                            -> 51 passed; 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings -> clean
```

Enemy `dagger` inside undiscovered fog: 100 walk steps -> 0 `MECH_STEP_SMALL`
audio events; with `rules.fog = false` the same unit steps again. Headless-only:
no `#[func]` exposes the audio log, so no in-engine check.

## T11 — re-verification results

1. **PAR-0191 place/break effects — inconclusive, no fix.** Placed/broke a
   conveyor through the player queue at (138,70) with the camera fixed and the
   node zoom at 2x (`zb-*.png`, `place-*.png`): the 2-tick frame diffs over the
   effect window decay (975 -> 12 px) consistent with a short-lived puff, but no
   frame isolates the effect sprite cleanly in this cluttered scene. The sim
   emission was already pinned headless (G03 T22).
2. **PAR-0244 bridge links — verified.** Two `bridge-conveyor`s at (134,62) and
   (137,62): a real tap on A opens its link panel, and a tap on B through
   `MindInput.push_event` (`{"t":"MouseButton","button":"left","down":true,...}`)
   sets `building_config_json(134,62)` = `{"type":"point","x":3,"y":0}` - the
   same relative link H02 T6 recorded. `02-bridge-link3.png` shows the linked
   source (orange) and target (blue) highlight; the link line itself was covered
   by the open config panel in the captures (`02-bridge-crop2.png`) - the panel
   did not close via `$I.close_block_config()` from the bridge's own panel.
   Note: the `godot_input` synthetic click path did **not** link (the panel
   swallowed it); the port's `push_event` path did - same fallback the
   environment documents.
3. **PAR-0129 shields — verified (dome).** `force-projector` (136,66) +
   `power-source` (138,66): `power_debug` satisfaction 1 and the animated dome
   with its accent outlines (`03-shield-dome.png`). The unit-bubble half was not
   staged (no shielded-unit fixture); unshielded daggers in the same frame show
   no bubble.
4. **PAR-0188 door/sprite + cracks — not completed.** The door was placed at
   (132,64) but `$I.configure_building(132,64,"bool",true)` timed out and
   stalled the frame loop (see below); the crack half needs the T4-style shooter
   setup and was not run.
5. **PAR-0157 command reticle — observed indirectly.** The T9 frame
   (`runs/20261015-s4-I02-T9/dagger-frame-crop.png`) shows the orange command
   reticle around the selected mechs. The "right-click an enemy" half needs an
   enemy on a captured sector, which the sim removes on spawn (the T4 finding),
   so it was not staged here.

## Deviations / blockers (pass 2)

* Two bridge taps: the `godot_input` click was consumed by the open link panel;
  `MindInput.push_event` registers the world tap (documented env fallback).
* `$I.configure_building(132,64,"bool",true)` timed out at the eval budget and
  left the frame loop stalled (frames frozen while time advances); recovery is
  `tools/session-play.sh 4 --play` (done, game live again). The same happened
  once earlier in pass 2. Root cause not diagnosed (out of this plan's scope;
  worth a bridge/tooling note).
* `Camera2D.set_zoom` changes the rendered scale; pass 1's T9 measurement was
  invalidated by it (pass 2 measured at the untouched default).

## Ledger changes to apply (pass 2)

| Item | New state / note |
|---|---|
| PAR-0055 | verified: the unit pass draws at `Draw.scl` (4.2 % of the Java reference in world units); no fix needed |
| PAR-0191 | re-check inconclusive in-engine (effect too transient to isolate); sim emission already headless-verified (G03 T22) |
| PAR-0244 | verified: real-tap bridge link (`{"type":"point","x":3,"y":0}`) + linked highlights |
| PAR-0129 | verified: powered force-projector dome animates |
| PAR-0188 | not re-run (blocked by the eval timeout that stalled the frame loop) |
| PAR-0157 | command reticle observed (T9 frame); the enemy-target half blocked by the captured-sector unit rule |
| new | "`$I.configure_building` on a door can time out and stall the frame loop" (tooling note) |

## Done criteria (pass 2)

T9 and T10 complete (T9 verified, T10 committed); T11 run with 2 of 5 items
verified, 1 observed indirectly, 1 inconclusive and 1 not completed. Summary
extended, worktree clean, no stash.

# Pass 3 (session 4, 2026-10-16)

T11 residuals only; branch fast-forwarded to `9d32f9d0` first. Two fixes
committed after in-engine verification; evidence in
`runs/20261016-s4-I02-T11/` (`notes.md` has the full detail). No Java
client; bridge 7000, display :13; editor restarted after each build.

## Per-residual outcome

| Residual | Outcome |
|---|---|
| T11-1 PAR-0191 place/break/rotate FX | **VERIFIED** (paused world, single player-queue ops at (146,68)) + one fix below |
| T11-4/T11-2 PAR-0188 door sprites + cracks | **VERIFIED** + two fixes below |
| T11-5 PAR-0157 command reticle | **VERIFIED fully** (hexagon rings on selected daggers + red attack reticle on a spawned enemy; pass 2's spawn blocker was an invalid-unit-id artifact) |
| T11-2 PAR-0244 bridge links | verified in pass 2; unchanged (no code between touches it) |
| T11-3 PAR-0129 shield dome | verified in pass 2; unchanged |

## Commits (this pass)

```
a33235d1 Fixes PAR-0188: doors emit open/close FX; raw configs not tap-throttled
4275a5de Fixes PAR-0191: block FX used a 32 px tile (4x too big)
```

* `4275a5de`: `wave_f56.rs` `TILESIZE` 32 -> `config::TILESIZE` (8;
  `Vars.java:135`). The rotate ring measured 126 px (15.75 world px)
  before, 36 px (one 8-px tile) after, matching
  `Fill.square(x, y, e.rotation * tilesize / 2f)` (`Fx.java:2797-2801`).
  Same bug made dooropen/doorclose/dooropenlarge/doorcloselarge,
  healBlock, lightBlock and overdriveBlockFull 4x too big.
* `a33235d1`: (a) `DoorBehavior::configured` no longer applies
  `DoorBuild.tapped`'s 60-tick `timer(timerToggle, 60f)` gate
  (`Door.java:149`) to raw configs (`Door.java:38-58` has none); the
  timer moved to `defense::tap_allowed` + `SimHost::door_tap_allowed`,
  consulted by the gdext tap path, so taps still throttle; (b) dooropen/
  doorclose ring FX (`Door.java:40,110-112`; large pair per
  `Blocks.java:1794-1796`) are now emitted.

## In-engine evidence (quoted)

* place: `[{"id":22,"name":"placeBlock","x":1168,"y":544,"rotation":1,"time":1,"lifetime":16,"layer":110}]`;
  same-tick A/B = 882 px, bbox (590,306)-(623,341), 732 px exactly
  `Pal.accent` (255,211,127) (`fx-ab-diff.png`).
* break: `[{"id":25,"name":"breakBlock",...}]`; tight remove-red pixel
  count 2664 -> 1244 -> 835 -> 0 over t01/t04/t08/t14.
* rotate: `[{"id":254,"name":"rotateBlock",...}]`; conveyor rotation
  0 -> 1; 36 px span after the fix.
* door: configs `true`/`false` one tick apart with live
  `[{"id":229,"name":"dooropen",...}]` / `[{"id":230,"name":"doorclose",...}]`;
  tap 1 toggles, tap 2 within 60 ticks dropped, tap after `step(61)`
  toggles (`door-p3-strip.png` shows solid / hollow+ring / closed+ring).
* cracks: `copper-wall` healthf 0.60625 (crack index 3) vs fresh wall:
  148 diff px, bbox (16,16)-(39,38), max channel delta 206
  (`crack-strip3.png`, `crack-diff.png`).
* reticle: `selection_json` `[1000000015,1000000016]`;
  `command_targets_json` `[{...15...,"attack":true},{...16...,"attack":true}]`;
  `ret-01-crop.png` shows the hexagon rings + red enemy reticle.

## Gates

```
cargo test -p mind-core --lib --features msav-import -> 2652 passed; 0 failed; 4 ignored
cargo test -p mind-headless --lib                   -> 133 passed
cargo test -p mind-headless --test fx_golden        -> 2 passed
cargo test -p mind-gdext --lib                      -> 51 passed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings -> clean
cargo fmt -p mind-core -p mind-gdext -- --check     -> clean
```

Panic gate `grep -c panic` on the newest session log = 0. Worktree clean
at stop; no stash.

## Ledger changes to apply (pass 3)

| Item | New state / note |
|---|---|
| PAR-0191 | fixed (`4275a5de`): block FX used a 32 px tile; place/break/rotate all verified in-engine |
| PAR-0188 | re-verified (sprites + cracks); fixed (`a33235d1`): door raw configs were tap-throttled, dooropen/doorclose FX missing |
| PAR-0157 | verified fully: hexagon rings + enemy attack reticle; enemy spawn persists via `apply_sim_command_json` with a valid unit id |
| PAR-0244, PAR-0129, PAR-0055 | unchanged from pass 2 (verified) |
| new | "`FxEmit::square`/`square_line` draw `Poly(4)` where Arc's `Fill.square`/`Lines.square` need a 45-degree-shifted vertex set" -> follow-up (audit ~30 call sites; Arc `Fill.java:357-363`, `Lines.java:503-509`) |
| new | "A raw door config can close a door under a unit; the `Units.anyEntities(tile) && !open` guard exists only on the tap path" (`Door.java:49`) |

## Done criteria

Met: T1-T10 complete from passes 1-2; T11's five items all re-verified
(two with small fixes committed), evidence in `runs/20261016-s4-I02-T11/`,
this summary extended. Remaining items are filed findings (square-convention
audit, door close-guard, the pass-1/2 follow-ups), not T11 blockers.
