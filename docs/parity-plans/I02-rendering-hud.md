<!-- SPDX-License-Identifier: GPL-3.0-only -->
# I02 — World rendering and HUD: H03 leftovers, visuals for round-H sim features, status markers

Round I, written 2026-10-15. Read `docs/I-environment.md` fully, then
`docs/I-INDEX.md` §3, then this plan. Start only after I00 has merged session 4's H03 work
(T9, T10, T11/PAR-0129, T15, T16, T17) into `main` and parked the H03 T13 WIP on the branch
`wip/h03-t13-pods` (I00 T2). Check with `git log main --oneline | grep -m1 "H03 T17"`.

H03 landed T1-T8, T12, T18 on `main`, and T9, T10, T11, T15, T16, T17 on `parity/session-4`
(I00 merges them). Left over: T13 (pods; a WIP exists) and T14 (mesh-null noise). Round H's
sim plans also added features whose draw half was assigned to "H03" and never done: wall hit flash
(H02 T12), beam-node lasers (H02 T15), the F6 status markers' Java rules (found 2026-10-15), and a
few checks.

---

## 1. Tasks

Order: T1 -> T2 -> T3 -> T4 -> T5 -> T6 -> T7 -> T8 -> T9 -> T10 -> T11.

### T1 — Launch pods and landing-pad arrivals (finish H03 T13 from the WIP)

**Start from the WIP.** I00 parks session 4's uncommitted T13 work on the branch
`wip/h03-t13-pods` (one commit on top of the session-4 H03 commits). It contains, per its diff:
`core/render/drawf.rs` (+84: `pow5_out`, `pow3_in`, `pow4_in`, `slope`, `curve`,
`launch_pod_path(x, y, fin, seed_cx, seed_cy, seed_rot)`, `landing_pod_path(...)` and the two tests
below), `core/world/behavior/campaign.rs` (+43: `launch_pad_rows(world) -> Vec<(index, x, y, block, progress)>`),
`gdext/render.rs` (+31), `gdext/render/canvas.rs` (+26) and `gdext/sim_host.rs` (+108). In your
worktree: `git merge --no-edit wip/h03-t13-pods`, run the tests, read the diff, and finish the task.
If the branch does not exist, start from scratch with the Java below.

**Java.**
* `J/world/blocks/campaign/LaunchPad.java:227-280` `LaunchPayloadComp.draw` (lifetime 120, `:153`):
  `alpha = fout(pow5Out)`; `scale = (1 - alpha)*1.3 + 1`;
  `cx = x + fin(pow2In) * (12 + randomSeedRange(id+3, 4))`;
  `cy = y + fin(pow5In) * (100 + randomSeedRange(id+2, 30))`;
  `rotation = fin * (130 + randomSeedRange(id, 50))`; at `Layer.effect + 0.001`:
  `Fill.light(cx, cy, 10, 25*(rad + scale - 1), engine@alpha, engine@0)` with `rad = 0.2 + fslope`
  (`fslope = (0.5 - |fin - 0.5|)*2`), then `alpha(alpha)` and four
  `Drawf.tri(cx, cy, 6, 40*(rad + scale - 1), i*90 + rotation)` in `Pal.engine` (`ffbb64`); at
  `Layer.weather - 1`: the pod region (`<pad>-pod`, fallback `launchpod`) sized `region*0.25*scale`,
  rotated, with alpha; the shadow at `Layer.flyingUnit + 1`: the same region tinted
  `(0,0,0,0.22*alpha)` offset `trns(225, fin(pow3In)*250)`.
* `LaunchPadBuild.draw` (`LaunchPad.java` ~110-135): base; if `lightRegion` is found:
  `progress = min(items.total/itemCapacity, launchCounter/launchTime)`; for `i in 0..4`,
  `j in 0..lightSteps`: `alpha = curve(progress, j/lightSteps, (j+1)/lightSteps)`,
  `offset = -(j - 1)*lightStep`, colour `lerp(Pal.metalGrayDark, lightColor, alpha)`, `lightRegion`
  at `(x + d8edge(i).x*offset, y + d8edge(i).y*offset)` rotation `i*90` (read the `lightSteps`,
  `lightStep`, `lightColor` defaults); then `Drawf.shadow(x, y, size*8)` and the pod at the centre.
* `J/world/blocks/campaign/LandingPad.java` `draw` (~205-265): base; if `arriving != null`:
  `fin = clamp(arrivingTimer)`, `alpha = pow5Out(fin)`, `scale = (1 - alpha)*1.3 + 1`,
  `cy = y + pow4In(1-fin) * (100 + randomSeedRange(id+2, 30))`,
  `rotation = (1-fin) * (90 + randomSeedRange(id, 50))`; engine light + 4 tris with
  `rad = 0.15 + pow5Out(slope(fin))`; the pod at `Layer.weather - 1` with `Drawf.shadow(cx, cy, size*8, fin)`;
  a dark copy at `Layer.flyingUnit + 1` offset `trns(225, pow3In(1-fin)*250)`; else if
  `cooldown > 0`: `Drawf.shadow(x, y, size*8, cooldown)`, `alpha(cooldown)`, `mixcol(accent, 1 - cooldown)`,
  the pod at the centre. Pod region `<pad>-pod`, fallback `advanced-launch-pad-pod`.
* `Fill.light(x, y, sides, radius, center, edge)` (`ARC/arc-core/src/arc/graphics/g2d/Fill.java:171-191`):
  triangle fan, `sides` rounded up to even. `Drawf.shadow(x, y, rad, alpha)` (`J/graphics/Drawf.java:350-364`):
  `circle-shadow` region `rad x rad`, colour `(0,0,0,0.4*alpha)`.

**Seams (on `main`).** `#[func] SimHost.launch_payloads_json()` (`[{id, x, y, time, lifetime, block, team}]`)
and `landing_pads_json()`; `LandingPadState` (H04 T6/T7).

**Changes.** A `gdext/render/pods.rs` pass: engine glow in the `Layer::Effect` band, pods first in
the `Layer::Weather` band, shadow copies in the `Layer::FlyingUnit` band, static pad parts (lights,
idle pod, cooldown pod) in the `Layer::Block` band after the block meshes; `canvas::fill_light`.
Seeds: use the payload `id` from `launch_payloads_json` for `randomSeedRange` (Arc
`Mathf.randomSeedRange(seed, range)`, already ported for fires as `seed_range`; reuse it).
All motion is derived from `time/lifetime` on the view side; add no sim state.

**Tests.** `launch_pod_path_math` (`fin = 0.5`, zero seed ranges -> `cx = x + 3`, `cy = y + 3.125`,
`rotation = 65`), `landing_pod_path_at_touchdown`.

**Verify in-engine.** Launch (H04 T6's recipe, evidence `runs/20261014-s5-H04-T6/pad-launch.png`):
Ground Zero, `place_block` an `advanced-launch-pad` on free ground (tiles 136..147, 57..72), set a
destination with `$MC.set_launch_destination("serpulo", <another sector id>)`, fill it
(`debug_add_building_items(x, y, "copper", 1000)` or a drill + conveyors), and step until
`launch_payloads_json()` lists a payload (H04 saw it at ~1805 ticks: `{"id":2,"time":4,"lifetime":120,...}`).
Then `set_paused(true)` and six rounds of `step(20)` + screenshot (`pod-0..5.png`). Landing (H04 T7's
fake-pad recipe, `runs/20261014-s5-H04-T7/fake-pad-landed.png`): a team-2 `landing-pad` at (136,66)
with `config` copper; `landing_pads_json()` lists it; capture the arrival frames the same way.

**Commit.** `Gameplay I02 T1: launch pods and landing-pad arrivals (H03 T13)`

---

### T2 — `Parameter "mesh" is null` errors (H03 T14)

**Evidence:** every session log has bursts of
`ERROR: Parameter "mesh" is null. at: mesh_get_surface_count (mesh_storage.cpp:691)`, including the
plan writer's 2026-10-15 probe (`~/.local/share/godot/app_userdata/Mindustry-Godot/logs/`), each with an
`_eval` frame on the stack (the stack shows whatever script was running, not the cause).

**Port today.** World meshes go through `MeshInstance2D.set_mesh` with `ArrayMesh`es from
`SurfaceTool::commit()` (`gdext/render/atlas_bind.rs build_mesh`, `floor.rs build_mesh`, blocks,
units, `building_cache`). A mesh with zero surfaces, or a freed mesh still assigned, triggers it.

**Procedure.** (1) Count the errors in a fresh run with the Ground Zero entry recipe plus 600 ticks.
(2) Bisect by render layer: before the sector load, hide one layer at a time (find the renderer's
layer-visibility func: `grep -n "fn set_layer_visible\|fn set_band_visible" gdext/render.rs`) and
re-count; the layer whose hiding removes the errors owns the bad mesh. (3) In that pass, find
`set_mesh` calls that can receive an empty mesh (a quad list of length 0 still committed) or a mesh
whose `Gd` is dropped while assigned. Fix it: when there are no quads, `set_mesh(Gd::null())` or hide
the node instead of assigning an empty mesh; make `build_mesh` return `None` for an empty slice.
Also check evals: if the errors appear only when an eval runs, look for passes that rebuild meshes
inside a `#[func]` called from GDScript mid-frame.

**Tests.** A unit test for the helper that now returns `None` for empty input.
**Verify.** The count is 0 on a Ground Zero load + 600 ticks + 10 evals.
**Commit.** `Gameplay I02 T2: no empty or freed meshes in the render passes`

---

### T3 — F6 building-status markers follow `BuildingComp.status()`

**Evidence (plan writer, 2026-10-15):** with F6 markers on, a full `mechanical-drill` that cannot
offload shows a RED marker; Java shows ORANGE (`noOutput`).

**Java.** `BuildingComp.status()` (`J/entities/comp/BuildingComp.java:638-652`):
```java
if(!enabled) return BlockStatus.logicDisable;
if(!shouldConsume()) return BlockStatus.noOutput;
if(efficiency <= 0 || !productionValid()) return BlockStatus.noInput;
return ((state.tick / 30f) % 1f) < efficiency ? BlockStatus.active : BlockStatus.noInput;
```
`state.tick` is a double, so `(tick / 30) % 1` is the FRACTIONAL part (0..1): a building at 40%
efficiency blinks active/noInput. Colours (`J/world/meta/BlockStatus.java:7-12`): active `5ce677`,
noOutput `Color.orange`, noInput `Pal.remove`, logicDisable, inactive `lightGray`.
Per-family `shouldConsume()` overrides decide `noOutput` (Drill: `items.total() < itemCapacity && enabled && dominantItem != null`,
`J/world/blocks/production/Drill.java:247-249`; GenericCrafter: output full; etc.). H01 T8 already
transcribed them for targeting: `core/world/status.rs java_block_status`.

**Port today.** `gdext/sim_host.rs:5100-5140` `building_statuses()` computes its own status:
`!enabled -> LogicDisable`, `efficiency <= 0 -> NoInput`, else `Active`, with a comment that wrongly
says the pulse is "integer modulo 1, always true".

**Changes.** Use `mind_core::world::status::java_block_status(world, e)` (check its exact signature)
plus the pulse with the sim tick (`(tick as f64 / 30.0).fract() < efficiency as f64`). Fix the comment.

**Tests.** A core test for the pulse helper (`status_pulses_with_efficiency`).
**Verify.** Ground Zero with F6 on: an unconnected full drill shows orange (screenshot); a
half-powered crafter blinks.

**Commit.** `Gameplay I02 T3: building status markers follow BuildingComp.status (noOutput, pulse)`

---

### T4 — Wall hit flash (visual half of H02 T12)

**Java.** `Wall.WallBuild.draw()` (`J/world/blocks/defense/Wall.java:146-161`): if `flashHit && hit >= 0.001`:
colour `flashColor`, alpha `hit * 0.5`, additive blend, `Fill.rect(x, y, size*8, size*8)`; when not
paused `hit = clamp(hit - delta / 10)`. `collision(bullet)` sets `hit = 1` (`:164-...`). Blocks with
`flashHit = true`: `grep -n "flashHit = true" J/content/Blocks.java` (plastanium, phase, surge walls...).
**Port today.** H02 T12 left `hit`/`flashHit` to the renderer (its deviation note).
**Changes.** A view-side `hit` per wall (set from a sim event on bullet collision with a `flashHit`
wall: emit through the existing FX/event seam the renderer already drains; never add sim state for the
decay), drawn additive in the block band.
**Verify.** A plastanium wall hit by a dagger flashes (screenshot sequence).
**Commit.** `Gameplay I02 T4: walls flash when hit (Wall.flashHit)`

---

### T5 — Beam node and beam tower lasers (visual half of H02 T15)

**Java.** `BeamNode.BeamNodeBuild.draw()` (`J/world/blocks/power/BeamNode.java:207-233`): at
`Layer.power`, colour `lerp(laserColor1, laserColor2, (1 - satisfaction) * 0.86 + absin(3, 0.1))`,
alpha `Renderer.laserOpacity`, width `laserWidth + absin(pulseScl, pulseMag)`; for each of the 4
directions with a destination and the dedupe condition shown at `:218-220`, skip adjacent blocks
(`dst > 1 + size/2`), and draw `Drawf.laser(laser, laserEnd, x + 4*size*dx, y + 4*size*dy, dest.worldx() - 4*dx, dest.worldy() - 4*dy, w)`.
Derelict team never draws.
**Port today.** H02 T15 mirrors links into `PowerModule.links` (`refresh_beam_nodes`); the power-laser
pass (`gdext/render/links.rs`) draws power-node lasers only.
**Changes.** Add the beam-node case to `links.rs` with Java's colour/width/dedupe rules (read the
`laserColor1/2`, `laserWidth`, `pulseScl`, `pulseMag` values from the block defs).
**Verify.** Erekir: a beam node next to a battery and a consumer 5 tiles away draws a pulsing laser.
**Commit.** `Gameplay I02 T5: beam node and beam tower lasers`

---

### T6 — Power-link overlay draws pale rings around power buildings

**Evidence (H03 pass 2, 2026-10-15):** `runs/20261015-s5-H03-T8/power-band.png`: large pale rings and
lines around power buildings, independent of the newer passes (they persist with them hidden). The
`Band_power_0` layer holds the link overlay and the F6 status pass.
**Changes.** Reproduce (place a `combustion-generator`, `power-node`, `battery` at Ground Zero; screenshot
with the power band only). Compare with Java's `PowerNode.drawLaser`/`drawSelect`/`drawConfigure`
(`J/world/blocks/power/PowerNode.java`) and the overlay renderer rules (`J/graphics/OverlayRenderer.java`):
range rings are drawn only for the SELECTED/hovered/placing node. Remove the always-on rings.
**Commit.** `Gameplay I02 T6: power overlays draw range rings only for selected nodes`

---

### T7 — DrawBlock `-bottom`/`LiquidTile` layers draw under the base sprite

**Evidence:** H03 T8 deviation 4: in the live DrawBlock chains, the extras nodes carry `z_index = 1`
inside the `Layer.block` band, so descriptors emitted before `Default` (`-bottom`, `LiquidTile`) draw
ABOVE the base sprite instead of below.
**Java.** `DrawMulti` draws its parts in order (`J/world/draw/DrawMulti.java`); `DrawRegion("-bottom")`
and `DrawLiquidTile` come first in the crafter chains (`J/content/Blocks.java`, e.g. `separator`,
`cryofluid-mixer`, `melter`).
**Changes.** Split extras into "before base" and "after base" nodes (z below/above the block mesh) in
`gdext/render/block_extras.rs`, keeping the chain order.
**Verify.** A cryofluid mixer: the liquid tile shows UNDER the frame (screenshot vs a Java capture if
one exists: `ls REPO/runs/*ref*/java | grep -i mixer`).
**Commit.** `Gameplay I02 T7: DrawBlock parts before the base draw under it`

---

### T8 — Toasts and announcements are visible

**Evidence (H05 pass 1):** the `ui_root.gd _toast_label` (overlay layer, position (24,24)) was created
but not visible in in-game screenshots. H03 T17 (merged by I00) reworked toast markup.
**Changes.** Check after the merge: `$UI.toast("test", "info")` in Ground Zero -> screenshot. If it is
still invisible, fix the layer/z-order/anchor so it matches Java `HudFragment.showToast`
(`J/ui/fragments/HudFragment.java`, grep `showToast`: a top-centre table that slides in, waits, fades).
Also check `MindUi.announce` (Java `UI.announce`: centre, `Styles.black3`, fades).
**Commit.** `Gameplay I02 T8: HUD toasts and announcements are visible and positioned like Java` (only if changed)

---

### T9 — Unit sprite scale check (PAR-0055)

**Ledger (blocked since 2026-10-10):** "the port's atlas is the 4x sprite set (`scale_marker` 4x4) and
Java draws regions with `Draw.scl = 1/scale_marker.width` (`J/core/Control.java:330`); the unit pass drew
the 46-px alpha region at 46 world px". Later rounds rewrote the unit pass (G03/H03), so this may be fixed.
**Procedure.** Ground Zero, zoom 4: measure the alpha's on-screen width at the core (Java ~40-50 px;
`runs/20261009-065416-ref-ground-zero/java/` has frames) and a dagger's. If they match Java within 10%,
report PAR-0055 as verified with the screenshot. Otherwise apply `Draw.scl` (divide region pixel size by
the `scale_marker` width) in the unit pass and re-measure.
**Commit.** only if changed: `Fixes PAR-0055: unit sprites draw at Draw.scl`

---

### T10 — Floor walk effects and fog-gated step sounds

**Java.** `UnitComp.update` (`J/entities/comp/UnitComp.java:739`):
`floor.walkEffect.at(x, y, hitSize()/8f, tile.getFloorColor())` for grounded units on a floor with a
walk effect, gated by `Mathf.chanceDelta(0.12 * ...)` (read the lines); `LegsComp` (`:183`) at each
leg base with `type.rippleScale`. Step sounds are skipped when `!inFogTo(player.team())`
(H07 T3 left this gate out because fog had not landed; H09 landed fog).
**Changes.** Check what H03 T10 (merged by I00: "walk ripples and landing dust") covered: if it emits
`floor.walkEffect` for every floor that has one (`grep -n "walkEffect" J/content/Blocks.java`), only add
the fog gate to step sounds (`core/audio/steps.rs`, H07's `steps::*`, using H09's `fog_visible_tile`).
Otherwise add the missing floors.
**Commit.** `Gameplay I02 T10: floor walk effects for every floor; step sounds respect fog`

---

### T11 — In-engine re-verification of earlier render items

Run and record (screenshot + one-line verdict each). Fix only small mismatches; file the rest.
1. PAR-0191 place/break/rotate block effects (G03 T22 landed the sim emission headless): place, break and
   rotate a conveyor at Ground Zero; the Java `Fx.place`/`Fx.breakBlock`/rotate effects appear.
2. PAR-0244 bridge links: two `bridge-conveyor`s linked by a real tap (H02 T6) draw the bridge link.
3. PAR-0129 shields (merged from session 4): a powered force projector dome animates; a unit's shield
   bubble shows only while `shieldAlpha > 0`.
4. PAR-0188 door/sorter state sprites and cracks (H03 T18): toggle a door; damage a wall.
5. PAR-0157 command reticle (H03 T3): select own daggers, right-click an enemy -> the attack reticle.
**Commit.** none unless a fix is needed.

---

## 2. Hot files

`gdext/render.rs` (one child per new pass), `gdext/render/links.rs` (T5, T6), `gdext/render/block_extras.rs`
(T7), `gdext/sim_host.rs` (T3).

## 3. Ledger changes to report

| Item | After |
|---|---|
| PAR-0198 | verified visually (T1; the sim half was H04) |
| PAR-0055 | verified or fixed (T9) |
| PAR-0191, PAR-0244, PAR-0129, PAR-0188, PAR-0157 | verified per T11 results |
| new | "Status markers ignore noOutput and the efficiency pulse" -> verified (T3) |

## 4. Done criteria

All tasks done or recorded; screenshots under `runs/<date>-s<N>-I02-*`; summary in
`REPO/docs/I02-rendering-hud_summary.md`.
