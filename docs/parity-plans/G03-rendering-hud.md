<!-- SPDX-License-Identifier: GPL-3.0-only -->
# G03 — In-game rendering + HUD: audit and execution plan (2026-10-13)

This plan is self-contained. Read `docs/parity-plans/00-environment.md` first
(paths `core/`, `gdext/`, `ui/`, `scenes/`, `J/`, `ARC/`, the session/build/test
recipes, the gmcp shorthands `$G $H $I $C $MC` and §4.1 "Entering a sector";
they are not repeated here). Work the tasks in the order of §3. Every task is
one commit. If what you see in the code differs from what a task says (a symbol
is missing, a line moved by more than ~50 lines, a seam already exists), stop
that task, write down what you saw in your summary, and continue with the next
task that does not depend on it. Do not improvise designs: where a choice
exists, this plan made it.

Start state: `main` contains the G00 integration merge
(`git log main --oneline | grep -m1 'G00: integrate'` prints a line).

Java line numbers were checked on 2026-10-13 against
`/home/clintonnguyen/g/code_examples/Mindustry` (read-only). Port line numbers
were checked against the integration tree; if one drifted, `grep -n` the quoted
function name.

## 0. Conventions used by every task

* **World coordinates.** The port's world is Java's world numerically
  (tile `(x,y)` centre = world `(x*8, y*8)`, +y up). The camera flips y
  (`gdext/camera.rs` ~198: `set_zoom(Vector2::new(zoom, -zoom))`). Therefore:
  * angles in degrees are Java angles (CCW, 0 = +x) and can be used as-is in
    `draw_set_transform` / polygon math;
  * a texture drawn with `draw_texture_rect*` in a world-space `Node2D` appears
    **upside down** unless its rect has a negative height. Use the helper of
    T1 (`canvas::draw_region`) for every textured custom draw; never call
    `draw_texture_rect*` directly in new code.
* **Atlas scale.** Region pixels are 4x world units (`scale_marker` region is
  4x4 px, checked in `REPO/assets/sprites/sprites.atlas.json`). A region drawn
  "at natural size" in Java (`Draw.rect(region, x, y)`, `region.width *
  region.scl()`) is `pixels * 0.25` world units. Java also packs with
  `stripWhitespaceCenter: true` (`core/assets-raw/sprites/pack.json`), so the
  port's trimmed region sizes are what Java draws; do not compensate offsets.
* **Layers.** `J/graphics/Layer.java`: floor 0, blockUnder 29.5, block 30,
  blockCracks 30.1, blockOver 35, blockBuilding 40, turret 50, groundUnit 60,
  power 70, legUnit 75, darkness 80, plans 85, flyingUnitLow 90, bullet 100,
  effect 110, flyingUnit 115, overlayUI 120, buildBeam 122, shields 125,
  weather 130, light 140, playerName 150, fogOfWar 155, space 160. The port has
  one `Node2D` band per layer (`gdext/render.rs::build_bands`, nodes are
  `z_as_relative = false`); reach a band with
  `self.band_node_at(BandKey::base(Layer::X))` inside `MindWorldRenderer`.
  New passes are created in `MindWorldRenderer::build_floor` (render.rs
  ~436-553) exactly like the existing `status::StatusRenderer` block
  (render.rs ~515-522): `new_alloc()`, `# code-instantiated` comment,
  `band.add_child(&node)`, store the `Gd` in a new `Option<Gd<..>>` field of
  `MindWorldRenderer` (declare it next to `status`/`shields`, initialise `None`
  in `init`).
* **Custom-draw pass template.** Copy `gdext/render/status.rs` (a
  `#[derive(GodotClass)] #[class(base=Node2D)]` with `init/ready/
  on_notification(EXTENSION_RELOADED)/process(queue_redraw)/draw`,
  `bootstrap()` resolving `/root/Spine/SimHost`, `visible_rect()` camera
  culling). Register the module in `gdext/render.rs` (`mod xxx;` list at
  ~38-58). New GodotClasses need no other registration (gdext auto-registers).
* **Reading settings from Rust**: copy `StatusRenderer::enabled`
  (`/root/MindUi` `settings_get_bool`) and use `settings_get_i32` for sliders
  (`lasersopacity`, `unitlaseropacity`, `bridgeopacity` are 0..100 sliders,
  default 100, `core/ui/settings.rs:206-208`). Read settings once per
  `process()` call, cache in a field; never in an inner loop.
* **Time.** Java `Time.time` is a frame clock in ticks (60/s). Use the sim
  tick for sim-tied animations and `Time::singleton().get_ticks_msec() as f32 *
  0.06` (= ticks at 60/s) for pure view pulses (`Mathf.absin(Time.time, ...)`).
  Arc helpers you will need (transcribe into `core/render/drawf.rs`, T1):
  * `absin(x, scl, mag) = ((sin(x / scl) + 1) / 2) * mag`
    (`ARC/arc-core/src/arc/math/Mathf.java` `absin`);
  * `sin(x, scl, mag) = sin(x / scl) * mag`;
  * `Interp.pow2Out(a) = 1 - (1-a)^2`, `pow3Out`, `pow2In(a) = a^2`.
* **Colours** (`J/graphics/Pal.java`): accent `ffd37f`, accentBack `d4816b`,
  remove `e55454`, removeBack `a73e3e`, breakInvalid `d44b3d`, place
  `6335f8`, placing = accent, gray `454545`, health `ff341c`, ammo `ff8947`,
  items `2ea756`, powerBar `ec7b4c`, powerLight `fbd367`, lancerLaser
  `a9d8ff`, bulletYellow `fff8e8`, bulletYellowBack `f9c27a`, lightTrail
  `ffe2a9`, sapBullet `bf92f9`, sapBulletBack `6d56bf`, darkMetal `6e7080`.
  `core/render/drawf.rs::pal` already holds most of them as `[f32;4]`;
  add missing ones there (append, keep alphabetic order irrelevant).
* **Every new Rust seam** on `MindSimHost` is a Rust-only `pub fn` (no
  `#[func]`) unless the task says `#[func]`. Put new seams at the end of the
  `impl MindSimHost` block that holds `unit_draws_full` (sim_host.rs ~2254+),
  reuse `self.tile_at(x, y)` for bounds-checked tile reads.

---

## 1. Inventory

Status legend: **W** ported & wired in the live client; **NW** ported but not
wired; **P** partial; **M** missing; **OOS** out of scope / other area.

### 1.1 Renderer / graphics (`J/core/Renderer.java`, `J/graphics/`)

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `Renderer.draw` layer order (Renderer.java:~300-440) | `gdext/render.rs::frame` + band plan | P | bands exist; stages `plans`, `darkness`, `bloom`, `blockbuild`, `space` are trace strings only (render.rs ~604-745) | T2,T3,T12 |
| `Renderer.shake` | `gdext/camera.rs::apply_shake_event` fed by `MindFx::tick_view` | W | fx.rs ~545-560 | — |
| `Renderer` landing (`landTime`, `CoreBlock.drawLanding`) | `scenes/landing_core.gd`, `camera.rs::landing_draw_json` | P | no thrusters, real core not hidden (landing_core.gd header) | T18 |
| Rules → renderer (lighting, ambientLight, fog, env) | `MindWorldRenderer::set_rules_*` #[func]s | NW | nobody calls `set_rules_lighting/set_fog_enabled/set_rules_env` (grep) | T12 |
| `FloorRenderer` | `gdext/render/floor.rs` | W | — | — |
| `BlockRenderer.drawBlocks` building sprites | `gdext/render/blocks.rs`, `building_cache.rs` | P | composed `-full` icon; drills/turrets/belts/construct special-cased | T8,T15 |
| `BlockRenderer` cracks (`drawCracks`) | — | M | no `cracks-` lookup in gdext | T8 |
| `BlockRenderer` enemy `drawTeam` (`block-border`) | — | M | grep `block-border` = 0 in gdext | T8 |
| `BlockRenderer.drawDestroyed` (rebuild ghosts) | — | M | — | T3 |
| `BlockRenderer.drawShadows/drawDarkness` | `gdext/render/shadow.rs` | P | darkness = static wall darkness only; no `borderDarkness`, no `limitMapArea` (shadow.rs ~135) | T11 |
| `BlockRenderer` status (F6) | `gdext/render/status.rs` | W | T14 of previous plan | — |
| `BlockRenderer` lights (`drawLight`) | `gdext/render/light.rs` | P | placeholder: monochrome, radius `size*16`, hard-coded ambient, statics only (light.rs header "Deviation S16-5") | T12 |
| `OverlayRenderer.drawBottom` (build plans) | — | M | `placement_preview_json`/`player_build_plans_json` have no render consumer | T2,T3 |
| `OverlayRenderer.drawTop` core edges / enemy core build radius | `gdext/render/overlays.rs` | NW | `set_core_edges` never called; no radius circles | T14 |
| `OverlayRenderer.drawTop` spawn drop-zone dash circle | — | M | — | T14 |
| `OverlayRenderer.drawTop` offscreen indicators (`indicators`) | `core/render/overlay.rs` geometry only | NW | — | T14 |
| `OverlayRenderer.drawTop` `build.drawSelect()` hover | — | M | — | T14 |
| `OverlayRenderer.drawTop` `drawConfigure` square | — | M | — | T14 |
| `OverlayRenderer` possession select arrows | — | M | possession (ctrl-click) is G08; draw deferred | deferred |
| `InputHandler.drawCommanded` | `gdext/render/command_overlay.rs` | P | no attack reticle (`drawCommandedTargets`) | T10 |
| `DesktopInput.drawTop/drawBottom`, `InputHandler.drawPlan/drawBreaking/drawSelection/drawArrow` | — | M | no ghost/arrow/selection drawing anywhere | T2 |
| `MinimapRenderer` | `gdext/render/minimap.rs`, `ui/widgets/mind_minimap.gd` | P | units + attack indicators done; darkness ignores border/limit area | T11 |
| `LightRenderer` | `gdext/render/light.rs` | P | see above | T12 |
| `FogRenderer` | `gdext/render/fog.rs` | P (placeholder quad) | `FogControl` (`core/game/fog.rs`) has no live consumer | deferred (Erekir) |
| `Pixelator` | `gdext/render/pixelate.rs` | W | — | — |
| `Bloom` | `core/render/bloom.rs` | M (stage stub) | "bloom" trace only | deferred (cosmetic) |
| `Drawf` helpers (`dashCircle`, `selected`, `square`, `target`, `laser`, `buildBeam`, `arrow`) | `core/render/drawf.rs` (colours only) | M | — | T1 |
| `EnvRenderers` | `gdext/render/env.rs` | W (driven by rules env once T12 lands) | — | T12 |
| Shields effectBuffer + `Shaders.shield` | `gdext/render/shields.rs` (non-animated branch) | P | — | T17 |
| `Shaders.buildBeam` composite | — | M | — | T3 (plain), T17 (animated) |
| `MenuRenderer`, `LoadRenderer` | — | OOS | menus | — |

### 1.2 Entities drawing (`J/entities/comp`, `J/type/UnitType.java`, `J/entities/bullet`)

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `UnitType.draw` body/legs(mech)/cell/weapons/outlines/shadows | `gdext/render/units.rs` | W | previous plan T6/T7 | — |
| `UnitType.drawEngines` / `UnitEngine.draw` | — | M | no engine code in units.rs | T5 |
| `UnitType.drawTrail` (flying trails) | — | M | — | T5 |
| `UnitType.drawMining` / mining beam | — | M | `minelaser` region exists, never drawn | T5 |
| `UnitType.drawItems` (item stack + ring) | — | M | — | T5 |
| `UnitType.applyColor` hit flash (`hitTime`) | — | M | no `hit_time` anywhere in core | T5 |
| `UnitType.drawLight` | — | M | — | T12 |
| `UnitType.drawShield` | `render/shields.rs` | W | — | — |
| `UnitType.drawLegs` (legged units) | — | M | `LegsComp` exists | deferred (mid-game atrax+) |
| `UnitType.drawTank` (treads) | — | M | Erekir only | deferred |
| `UnitType.drawPayload` | — | M | payload pickup is G01 (PAR-0138) | deferred |
| `UnitType.parts`, abilities `draw` | — | M | — | deferred |
| `BuilderComp.drawBuilding` / `drawBuildingBeam` | — | M | — | T3 |
| `BulletComp.draw` → `BulletType.draw` (all kinds) | — | **M** | no bullet drawing in gdext at all (grep `bullet` in gdext/render = 0) | T4 |
| bullet trails (`BulletType.drawTrail`) | — | M | `core/render/trail.rs::Trail` exists | T4 |
| `Effect` named effects | `gdext/fx.rs` | W | — | — |
| `Effect` inline (`EffectRef::Inline`, 53 uses) | `fx.rs::tick_view` drops them | M | fx.rs ~527-536 | deferred (needs per-lambda ports) |
| `FxEvent::Trail/Light` | dropped | M | fx.rs ~575 | deferred (T12 covers lights from blocks/units) |
| Fx `PrimKind::Region` orientation | `fx.rs::draw_prim` passes `tile=true` instead of flipping | **bug** | fx.rs ~310 `draw_texture_rect_ex(.., true)` (3rd arg is `tile`) | T1 |
| `PuddleComp.draw`, `FireComp.draw` | — | M | no puddle/fire drawing | deferred (fires emit Fx through the sink) |
| `WeatherComp` drawing | `fx.rs` weather prims | OOS (G07) | — | — |

### 1.3 Block draw paths (`J/world/...`)

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `Block.drawPlan/drawPlanRegion` | — | M | — | T2 |
| `Block.drawPlace` overlays (turret range, power-node range, drill text, …) | — | M | 31 Java blocks override `drawPlace` | T2 (turret/BaseTurret range, PowerNode range, MendProjector/Overdrive range) ; rest deferred |
| `Building.drawSelect` (hover) | — | M | 35 Java overrides | T14 (BaseTurret/Turret, MendProjector, OverdriveProjector, PowerNode range) |
| `Building.drawConfigure` | — | M | — | T14 (square only) |
| `Building.drawLight` | light placeholder | P | — | T12 |
| `DrawBlock` descriptors (`J/world/draw/*`) | `core/render/draw_desc.rs` (`DrawDesc`, 32 variants, `vanilla_chain` for 13 blocks) | **NW** | only `mind-headless/src/render_scenarios.rs` executes it | T15 |
| `DrawTurret` | `gdext/fx/draw_turret.rs` (dev probe) + blocks.rs turret parts | P | blocks.rs draws base+barrel+top+heat | — |
| `PowerNode.draw` lasers | — | M | — | T7 |
| `ItemBridge.draw` bridge + arrows | — | M | — | T7 |
| `Door.draw` open sprite, `SwitchBlock` on sprite, `Sorter`/`Unloader`/`ItemSource` item icon, `LiquidSource` colour | — | M | all draw the static `-full` icon | T8 |
| `Battery.draw` fill | — | M | — | deferred (low) |
| `Conduit`/`LiquidBlock`/`LiquidRouter`/`Pump` liquid drawing | — | M | — | deferred (see §2 deferred list) |
| `UnitFactory`/`Reconstructor` unit build preview | — | M | — | deferred |
| `Mend/Overdrive/RegenProjector` pulse | — | M | — | deferred |
| `CoreBlock.draw` thrusters | — | M | — | T18 |
| `LaunchPayload.draw`, `LaunchPad.draw` lights/pod, `LandingPad.draw` arrivals | — | M | data from G04 T10/T11 | T20 |
| `ConstructBlock` place/break effects, `Fx.rotateBlock` | effects ported, never emitted | NW | no `fx.effect` outside combat | T22 |
| `StatusEffect.effect` particles, `Floor.walkEffect`, `Fx.unitLand` | — | M | view-side | T21 |
| `UI.announce` / `showInfoToast` visuals | `ui_root.gd::_on_announce` plain label | P | — | T9 |

### 1.4 HUD / UI (`J/ui/fragments/`, `J/ui/dialogs/`)

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `HudFragment` status table (wave text, objectives, skip) | `ui/fragments/hud_fragment.gd` + `gdext/ui/hud.rs` + `core/ui/hud_text.rs` | W | — | — |
| `HudFragment` player status (health/shield/ammo SideBars) | `ui/widgets/player_status.gd` | P | health only; no shield/payload bar | T9 |
| `HudFragment.addInfoTable` status-effect icons | — | M | — | T9 |
| `HudFragment` boss bar (`guardian`) | — | M | grep `boss` in ui = 0 | T9 |
| `HudFragment` `@coreattack` notice | — | M | — | T9 |
| `HudFragment` `@nearpoint` warning | — | M | — | T9 |
| `HudFragment` guardian-wave toasts (`wave.guardianwarn`) | — | M | — | T9 |
| `HudFragment` capture → `@sector.capture.current` announce | **wrong** | P | port toasts + opens the game-over dialog on capture (hud_fragment.gd `_check_game_over`, hud.rs ~380-395) | T9 |
| `HudFragment` `@saving` indicator | **wrong** | P | port shows the full loading overlay while any IO is pending (hud_fragment.gd `_refresh_loading`) | T9 |
| `HudFragment` paused banner `@sector.curlost` | P | always `@paused` | T9 |
| `HudFragment` toggle menus (`toggleMenus`, `Binding.toggleMenus`) | — | M | — | T9 |
| `CoreItemsDisplay` | `ui/widgets/core_items_display.gd` | W | — | — |
| `PlacementFragment` block grid, top table, key combo | `ui/fragments/placement_fragment.gd` | W | previous plan T3/T4/T5 | — |
| `PlacementFragment` hovered `Building.display` (bars) | icon+name only | P | `_build_hover_row`; `core/world/stats.rs::evaluate_bar` unused | T6 |
| `PlacementFragment` command table (PAR-0095) | — | M | no command branch; no `set_unit_command` #[func] | T10 |
| `BlockInventoryFragment` | `ui/fragments/block_inventory_fragment.gd` | P | text buttons `"copper x10"`, no live refresh/positioning/hold-take | T13 |
| `MinimapFragment` (M full map) | `ui/fragments/minimap_fragment.gd` | W | — | — |
| `PausedDialog` | `ui/dialogs/paused_dialog.gd` | W | button set matches `PausedDialog.rebuild` desktop branch | — |
| `GameOverDialog` presentation | `ui/dialogs/game_over_dialog.gd` | P | opened immediately (Java: 3 s delay, `Logic.java:438`), opened on capture, no stats rows passed | T16 |
| `ChatFragment` | `ui/fragments/chat_fragment.gd` | OOS (multiplayer) | — | — |
| `ConsoleFragment` | `ui/fragments/console_fragment.gd` | low priority, not audited | — | — |
| `HintsFragment` | — | OOS (G05) | — | — |
| `LoadingFragment` | `ui/fragments/loading_fragment.gd` | W | — | — |
| `PlayerListFragment`, `MenuFragment` | — | OOS | — | — |

### 1.5 Already done and verified (do not redo)

* Previous plan `audit-rendering-hud.md` T1–T16, T18, T20, T5 (commits listed in
  `audit-rendering-hud_summary.md`): UI layer order, shadow offset, top-table
  key combo + highlight + cursor hover, belt autotile/animation/items,
  flying shadow, outlines, FX producer/regions/shake, drill rotator/turret
  parts, blockbuild shader, F6 status, minimap indicators+units, command
  overlay rings, menu label centring, skip button placement, shield domes
  (non-animated branch).
* PAR-0055 (sprite scale): **resolved in code** —
  `gdext/render/units.rs:73-74` divides region pixels by the `scale_marker`
  width (`sprite_scale = 1/4`); T6/T7 screenshots of the previous plan matched
  the Java alpha size. Only a ledger re-check is needed (T19).
* `core/world/darkness.rs::border_darkness` + tests (previous T19 partial,
  `f59fcab`).
* `MindSimHost` seams that already exist and are reused here:
  `unit_draws_full`, `turret_draw`, `drill_draw`, `belt_draw`,
  `conveyor_items`, `construct_draw`, `construct_at`, `building_team`,
  `building_rotation`, `force_shields`, `unit_shields`, `building_statuses`,
  `unit_command_view`, `player_build_plans_json` (#[func]),
  `player_unit_status` (#[func]), `hover_info_world` (#[func]),
  `light_color` (#[func]), `tile_at` (private bounds helper).

---

## 2. Tasks

Tasks are numbered by player impact; §3 gives the execution order.

### T1 — Shared world-canvas helpers (`Drawf` math + y-flip-correct region draw) and the FX region flip fix

**Symptom / acceptance.** No new feature by itself; every later task draws
through these helpers. Today `MindFx` draws effect sprites through
`draw_texture_rect_ex(texture, rect, true)` (`gdext/fx.rs` Region branch,
~line 310): the third argument of Godot's `draw_texture_rect` is `tile`, not a
flip, so every effect region is drawn vertically mirrored under the y-flip
camera (invisible for round sprites, wrong for `casing`/arrow-like ones).
Acceptance: `cargo test -p mind-core --lib render::drawf` passes the new tests;
after T4 lands, a duo bullet sprite's pointed tip faces its flight direction
(this is the in-engine orientation check for the helper).

**Java.**
* `ARC/arc-core/src/arc/graphics/g2d/Lines.java:354-370` `dashCircle`:
  `sides = 10 + (int)(radius * 0.6); if(sides % 2 == 1) sides++;` then for
  `i = 0; i < sides; i += 2`: segment from angle `360/sides*i + 90` to
  `360/sides*(i+1) + 90` at `radius`.
* `J/graphics/Drawf.java:398-404` `dashCircle(x,y,rad,color)`: stroke 3
  `Pal.gray` dash circle, then stroke 1 `color` dash circle.
* `Drawf.java:410-415` `circles`: same with full circles.
* `Drawf.java:338-348` `selected(x, y, block, color)`: for `i in 0..4`,
  `p = Geometry.d8edge[i]` (= `(1,1),(-1,1),(-1,-1),(1,-1)`),
  `offset = -max(block.size - 1, 0) / 2 * 8`, draw region `block-select` at
  `(x*8 + block.offset + offset*p.x, y*8 + block.offset + offset*p.y)`,
  rotation `i*90`, natural size (32 px → 8 world), tinted `color`.
* `Drawf.java:426-440` `square(x, y, radius, rotation, color)` =
  `square(.., color, bg = Pal.gray with alpha color.a, scaling 1)`:
  `Lines.stroke(3, bg); Lines.square(x, y, radius + 1, rotation);
  Lines.stroke(1, color); Lines.square(x, y, radius + 1, rotation)`.
  `square(x, y, radius, color)` = rotation 45 (Drawf.java:476). Arc
  `Lines.square(x, y, rad, rot)` = `Lines.poly(x, y, 4, rad * sqrt2, rot + 45)`
  (check `ARC/arc-core/src/arc/graphics/g2d/Lines.java` `square`).
* `Drawf.java:406-415` `circles(x, y, rad[, color = Pal.accent])`: stroke 3
  `Pal.gray` `Lines.circle`, then stroke 1 `color` `Lines.circle`.
* `Drawf.java:275-287` `target(x, y, rad, alpha, color)`: stroke 3 `Pal.gray`
  `Lines.poly(x, y, 4, rad, Time.time*1.5)` + `Lines.spikes(x, y, 3/7*rad,
  6/7*rad, 4, Time.time*1.5)`, then the same at stroke 1 in `color`.
  `Lines.spikes(x,y,radius,length,spikes,rot)` (Lines.java:372-385): for
  each `i`, line from `trns(i*360/spikes + rot, radius)` to
  `trns(same, radius + length)`.
* `Drawf.java:521-540` `laser(line, start, end, x, y, x2, y2, scale)`:
  `scl = 8 * scale * Draw.scl` (`Draw.scl` = 0.25 → `scl = 2*scale`),
  `rot = angle(x2-x, y2-y)`, `vx,vy = cos/sin(rot) * scl`; draw `start` at
  `(x,y)` size `start.w*scale*0.25 × start.h*scale*0.25` rotation `rot+180`,
  `end` at `(x2,y2)` rotation `rot`; then `Lines.stroke(12*scale)` and
  `Lines.line(line, x+vx, y+vy, x2-vx, y2-vy, cap=false)` = a textured quad
  from p1 to p2, width `12*scale`, the `line` region stretched along it.
  (LOD alpha: multiply the end-cap alpha by `Lod.alpha1` when
  `start.w*scale*0.25 < 10` else `Lod.alpha2`; the port's `RenderStats` has
  `lod_l1/lod_l2` booleans only — use 1.0 when the flag is true, 0 when false.)
* `Drawf.java:131-157` `buildBeam(x, y, tx, ty, radius)` (transcribed in T3).
* `Drawf.java:544-555` `tri(x, y, width, length, rotation)`: triangle
  `(x + trns(rotation-90, width/2)), (x + trns(rotation, length)),
  (x - trns(rotation-90, width/2))`.

**Port today.** `core/render/drawf.rs` has only `pal` colours, `to_bits`,
`lerp`, `with_alpha`, `mul`. Custom-draw passes (`status.rs`, `shields.rs`,
`command_overlay.rs`) each hand-roll polygons. `fx.rs` has
`polygon_points`/`triangle_points` (private).

**Changes.**
1. `core/render/drawf.rs`: append pure geometry helpers (no Godot), each with
   a doc comment citing the Java line above:
   ```rust
   pub fn absin(x: f32, scl: f32, mag: f32) -> f32 { ((x / scl).sin() + 1.0) / 2.0 * mag }
   pub fn trns(angle_deg: f32, len: f32) -> (f32, f32) { let r = angle_deg.to_radians(); (r.cos() * len, r.sin() * len) }
   /// `Lines.dashCircle`: segment endpoints `[(x1,y1,x2,y2)]`.
   pub fn dash_circle_segments(x: f32, y: f32, radius: f32) -> Vec<[f32; 4]>
   /// `Lines.poly(x, y, sides, radius, angle)`: closed vertex ring (first == last).
   pub fn poly_ring(x: f32, y: f32, sides: u32, radius: f32, angle_deg: f32) -> Vec<(f32, f32)>
   /// `Lines.spikes`.
   pub fn spikes(x: f32, y: f32, radius: f32, length: f32, count: u32, rot_deg: f32) -> Vec<[f32; 4]>
   /// `Drawf.tri` vertices.
   pub fn tri(x: f32, y: f32, width: f32, length: f32, rotation_deg: f32) -> [(f32, f32); 3]
   /// `Geometry.d8edge` order used by `Drawf.selected`.
   pub const D8EDGE: [(i32, i32); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];
   /// `Drawf.selected` corner centres + rotations for a block of `size` at tile `(x, y)` with `offset`.
   pub fn selected_corners(x: i32, y: i32, size: i32, offset: f32) -> [(f32, f32, f32); 4]
   ```
2. New file `gdext/render/canvas.rs` (header lines per env §3) with
   `pub(crate)` free functions taking `canvas: &mut Node2D`. Inside a pass's
   `draw()`, get the canvas with `let mut base = self.base_mut();` and pass
   `&mut base` (`BaseMut<Node2D>` derefs to `Node2D`); Godot `draw_*` calls
   are only valid on the node being drawn, inside its `draw()`. Functions:
   ```rust
   /// Draws `texture` centred at (x, y), size (w, h) world units, rotated `rot_deg`,
   /// modulated by `color`, compensating the camera y-flip.
   pub(crate) fn draw_region(canvas: &mut Node2D, texture: &Gd<Texture2D>, x: f32, y: f32, w: f32, h: f32, rot_deg: f32, color: Color) {
       canvas.draw_set_transform_ex(Vector2::new(x, y)).rotation(rot_deg.to_radians()).done();
       // Negative height = vertical flip: the camera renders +y up, textures are top-row-first.
       canvas.draw_texture_rect_ex(texture, Rect2::new(Vector2::new(-w / 2.0, h / 2.0), Vector2::new(w, -h)), false)
           .modulate(color).done();
       canvas.draw_set_transform(Vector2::ZERO);
   }
   /// Textured line quad (`Lines.line(region, x, y, x2, y2, false)` with `stroke`).
   pub(crate) fn draw_region_line(canvas: &mut Node2D, texture: &Gd<Texture2D>, x: f32, y: f32, x2: f32, y2: f32, stroke: f32, color: Color)
   //   = draw_region at the midpoint, w = len, h = stroke, rot = atan2(y2-y, x2-x) in degrees.
   pub(crate) fn dash_circle(canvas: &mut Node2D, x: f32, y: f32, r: f32, color: Color)   // Drawf.dashCircle (gray w3 + color w1)
   pub(crate) fn circles(canvas: &mut Node2D, x: f32, y: f32, r: f32, color: Color)       // Drawf.circles
   pub(crate) fn poly_line(canvas: &mut Node2D, pts: &[(f32, f32)], stroke: f32, color: Color)
   pub(crate) fn rect_line(canvas: &mut Node2D, x: f32, y: f32, w: f32, h: f32, stroke: f32, color: Color) // Lines.rect
   pub(crate) fn fill_tri(canvas: &mut Node2D, pts: [(f32, f32); 3], color: Color)
   pub(crate) fn color_u32(rgba: u32) -> Color   // RGBA8888
   pub(crate) fn color_f4(c: [f32; 4]) -> Color
   ```
   plus a region cache type for passes:
   ```rust
   /// Name → `AtlasTexture` cache over `/root/MindAssets` (`find_region`).
   pub(crate) struct RegionCache { cache: HashMap<String, Option<Gd<Texture2D>>> }
   impl RegionCache { pub fn new() -> Self; pub fn get(&mut self, node: &Node2D, name: &str) -> Option<Gd<Texture2D>>;
                      /// natural world size (`pixels * 0.25`).
                      pub fn natural_size(tex: &Gd<Texture2D>) -> (f32, f32) }
   ```
   `get` resolves exactly like `MindFx::region_texture` (fx.rs ~356-372). Call
   `get` **before** `self.base_mut()` in `draw()` (borrow rule: the cache lives
   on `self`; collect textures into a local `Vec` first, then take
   `base_mut()`), the same pattern fx.rs uses.
   Register `mod canvas;` in `gdext/render.rs`.
3. `gdext/fx.rs` Region branch: replace the transform + `draw_texture_rect_ex
   (&texture, Rect2::new(Vector2::new(-w * origin.0, -h * origin.1),
   Vector2::new(*w, *h)), true)` with the flip-correct form keeping the
   origin: rect position `(-w*origin.0, h*(1.0-origin.1))`, size `(w, -h)`,
   `tile = false`. Fix the comment.

**Tests** (`core/render/drawf.rs` `#[cfg(test)] mod tests`, at file end):
* `dash_circle_segments_matches_arc`: radius 80 → `sides = 10 + 48 = 58`
  (even) → 29 segments; first segment starts at angle 90° → `(x, y+80)`
  (assert `|x1 - x| < 1e-4`, `|y1 - (y+80)| < 1e-4`), ends at angle
  `90 + 360/58 = 96.2069°`.
  radius 5 → `10 + 3 = 13` → odd → 14 → 7 segments.
* `selected_corners_size_two`: `selected_corners(10, 10, 2, 4.0)`: offset
  `-max(1,0)/2*8 = -4`; corner 0 = `(10*8 + 4 - 4*1, 80 + 4 - 4) = (80, 80)`,
  rot 0; corner 2 = `(88, 88)`, rot 180.
* `tri_points`: `tri(0,0,2,10,0)` = `[(0,-1),(10,0),(0,1)]` (±1e-4).
* `absin_range`: `absin(0, 1, 2) == 1.0`.

**Verify in-engine.** Build, `tools/session-play.sh N`, enter Ground Zero
(env §4.1), `$G --eval 'return get_node("/root/Spine/MindFx").spawn_effect("hitBulletSmall", 1032, 440, 0)'`,
screenshot within 1 s: sparks still render (no regression). Orientation is
re-checked by T4.

**Pitfalls.** Do not change `build_unit_mesh` (its UV mapping already
compensates the flip). `draw_texture_rect_ex` needs the node's texture filter
NEAREST — passes are children of `MindWorldRenderer` which sets NEAREST
(render.rs ~320) and children inherit it.

**Commit.** `Gameplay G03 T1: shared world-canvas helpers; FX regions no longer mirrored`

---

### T2 — Placement preview: cursor ghost, line plans, rotation arrow, break rectangle, range circles

**Symptom / acceptance.** Today selecting a block shows nothing under the
cursor: no ghost, no red/white validity tint, no rotation arrow, no line
preview while dragging, no turret range circle, and a right-drag break shows
no rectangle. After: with `conveyor` selected the cursor tile shows the
conveyor icon pulsing white (valid) or red-tinted (invalid, e.g. over the
core), an arrow in the rotation direction, a dragged line shows one ghost per
plan, a selected `duo` shows a dashed accent circle of radius 110 world px,
and a right-drag shows a red rectangle with red corner brackets on each block
inside. Compare with `runs/20261009-065416-ref-ground-zero/java/r05-19-conveyors.png`
(Java placement ghost) — the ghost sits on the cursor tile with an arrow.

**Java.**
* `J/input/DesktopInput.java:151-230` `drawBottom` (placing branch):
  ```java
  if(mode == placing && block != null){ for each linePlans i: if(i == last && plan.block.rotate && plan.block.drawArrow) drawArrow(block, x, y, rot); drawPlan(plan); }
  else if(isPlacing()){ rot = block.planRotation(rotation); if(block.rotate && block.drawArrow) drawArrow(block, cx, cy, rot);
      valid = validPlace(cx, cy, block, rot); drawPlan(cx, cy, block, rot); block.drawPlace(cx, cy, rot, valid); drawOverlapCheck(...); }
  ```
  `drawBreakSelection` (`InputHandler.java:1675-1724`) when `mode == breaking`:
  red corners (`drawBreaking` → `Drawf.selected(x,y,tile.block(),Pal.remove)`)
  on every breakable building tile in the normalized area (max length 100),
  red corners on queued placement plans overlapping it, then
  `Lines.stroke(2); color removeBack; Lines.rect(x, y-1, x2-x, y2-y);
  color remove; Lines.rect(x, y, x2-x, y2-y)` where the area is
  `x = minx*8 - 4`, `x2 = maxx*8 + 4` (same for y) (`Placement.normalizeDrawArea`
  with air: size 1, offset 0, `Placement.java:416-439`).
* `J/world/Block.java:843-850` `drawPlan(plan, list, valid, alpha)`:
  `Draw.mixcol(!valid ? Pal.breakInvalid : Color.white, (!valid ? 0.4 : 0.24) + absin(Time.globalTime, 6, 0.28)); Draw.alpha(alpha);`
  then `drawDefaultPlanRegion`: region = `fullIcon` at `(plan.drawx(), plan.drawy())`
  (= `x*8 + offset`), rotation `rotate && rotateDraw ? rotation*90 : 0`.
  `Draw.mixcol(c, a)` lerps the sprite RGB toward `c` by `a` (keeps alpha).
* `J/input/InputHandler.java:2405-2425` `drawArrow(block, x, y, rot, valid)`:
  `trns = (block.size / 2) * 8` (integer division), `d4(rot)`,
  `offx = x*8 + offset + dx*trns`, `offy = y*8 + offset + dy*trns`;
  `color(!valid ? Pal.removeBack : Pal.accentBack)`; region `place-arrow`
  natural size (46x90 px → 11.5x22.5 world) at `(offx, offy - 1)` rotation
  `rot*90 - 90`; then `color(!valid ? Pal.remove : Pal.accent)` and the same
  region at `(offx, offy)` (InputHandler.java:2405-2427, verified).
* `J/world/blocks/defense/turrets/BaseTurret.java:84-90` `drawPlace`:
  `Drawf.dashCircle(x*8 + offset, y*8 + offset, range, Pal.placing)`.
  Turret ranges (Blocks.java): `duo` 160 (line 3337), `scatter` 220 (3416),
  `hail` 235, `lancer` 165, `arc` 90, `wave` 110 — the port has them in
  `TurretConfig.range`; do not hard-code turrets.
* `J/world/blocks/power/PowerNode.java:159-173` `drawPlace`: if `autolink`:
  `Lines.stroke(1); Draw.color(Pal.placing); Drawf.circles(x*8+offset,
  y*8+offset, laserRange*8)` (gray w3 + accent w1 full circle), then potential
  link lasers (deferred to T7's follow-up; skip here). `laserRange`:
  power-node 6 (Blocks.java:2483+), power-node-large 15, surge-tower 40.
* `J/world/blocks/defense/MendProjector.java:76-82` and
  `OverdriveProjector.java:54-60` `drawPlace`: `Drawf.dashCircle(x*8+offset,
  y*8+offset, range, baseColor)` (+ pulsing `Drawf.selected` on blocks in
  range — skip, needs the building index; note it in the commit).
  `MendProjector.baseColor = 84f491`, default `range = 60`; `mender` 40
  (Blocks.java:1926), `mend-projector` 85 (1939). `OverdriveProjector.baseColor
  = feb380`, default `range = 80` (`overdrive-projector` keeps 80),
  `overdrive-dome` 200 (1958).

**Port today.**
* Input state: `MindInput.bridge: DesktopBridge` (private field,
  `gdext/input/mod.rs` ~70); `bridge.controller.state: InputState`
  (`core/input/client_input.rs:30`) with `block: Option<BlockId>`,
  `rotation: u8`, `place_mode: PlaceMode` (`None/Breaking/Placing/
  SchematicSelect/RebuildSelect`), `line_plans: Vec<ClientPlan>`
  (`core/input/plan.rs:22`: `x, y, rotation, block, config, breaking,
  anim_scale`). `DesktopBridge::cursor_tile()` is `pub`
  (`gdext/input/desktop.rs` ~994). The drag (`drag: Option<Drag>` with
  `kind: DragKind::{Place, Break, SelectRect}`, `start`, `last`) is private.
* Validity: `SimRuntime::can_place` is `pub(crate)` (`core/sim/runtime.rs`
  ~346); `Sim::queue_player_place` uses it (`core/sim/builder.rs` ~777).
* No render consumer of any of this.

**Changes.**
1. `core/sim/builder.rs` (impl `Sim`, next to `queue_player_place`): add
   ```rust
   /// `InputHandler.validPlace(x, y, block, rot)` for the local team (preview only, no side effects).
   pub fn preview_can_place(&self, x: i16, y: i16, block: BlockId, rotation: u8) -> bool {
       let team = self.live_default_team();
       self.block_runtime.as_ref().is_some_and(|rt| rt.can_place(&self.ecs.0, &self.grid, x, y, block, rotation, team))
   }
   ```
   (`live_default_team` is the helper `queue_player_place` already calls;
   confirm with `grep -n "fn live_default_team" core/sim/`.)
2. `gdext/sim_host.rs` Rust-only seams:
   ```rust
   pub fn preview_can_place(&self, x: i32, y: i32, block: BlockId, rot: u8) -> bool  // bounds-checked i16 conversion, forwards to Sim
   /// `BaseTurret.range` / `PowerNode.laserRange*8` / projector `range` for drawPlace; `None` = no circle.
   pub fn block_place_range(&self, block: BlockId) -> Option<(f32, PlaceRangeKind)>
   ```
   `PlaceRangeKind::{Turret, PowerNode, Projector}` (new enum in sim_host.rs).
   Turret: `mind_core::world::blocks::defense::turrets::config_for(content,
   &def.name, content.bullet_names()).map(|c| c.range)`. Power node: match
   `def.name` against `"power-node" => 6.0, "power-node-large" => 15.0,
   "surge-tower" => 40.0` (Blocks.java `laserRange`; verify each with
   `grep -n -A8 '"power-node"' J/content/Blocks.java` before typing) and
   multiply by 8. Projector: `const PROJECTOR_RANGES: [(&str, f32, u32); 4] =
   [("mender", 40.0, 0x84f491ff), ("mend-projector", 85.0, 0x84f491ff),
   ("overdrive-projector", 80.0, 0xfeb380ff), ("overdrive-dome", 200.0,
   0xfeb380ff)]` (return the colour too: make the seam return
   `Option<(f32, PlaceRangeKind, u32)>`; turret colour `pal::ACCENT`, power
   node `pal::ACCENT` drawn with `canvas::circles`, projector its own colour
   with `canvas::dash_circle`).
   Cache the turret lookup in a `RefCell<HashMap<BlockId, Option<f32>>>` field
   (`config_for` rebuilds a config each call).
3. `gdext/input/desktop.rs`: add
   ```rust
   /// Read-only view for the placement overlay (`DesktopInput.drawBottom/drawTop`).
   pub struct PlacementView { pub cursor: Option<(i32, i32)>, pub block: Option<BlockId>, pub rotation: u8,
       pub mode: PlaceMode, pub line_plans: Vec<ClientPlan>, pub break_rect: Option<((i32, i32), (i32, i32))> }
   impl DesktopBridge { pub fn placement_view(&self) -> PlacementView }
   ```
   `break_rect` = `Some((drag.start, drag.last))` when `drag.kind ==
   DragKind::Break`. `line_plans` cloned only when `mode == Placing`.
   In `gdext/input/mod.rs` add a Rust-only `pub fn placement_view(&self) ->
   PlacementView { self.bridge.placement_view() }` on `MindInput`.
4. New pass `gdext/render/plans.rs`: `PlanOverlay` (`Node2D`, template of
   §0) under the `Layer::Plans` band; created in `build_floor` after the
   command overlay. `process()`: `queue_redraw()` every frame while
   `/root/Spine/Input` exists. `draw()`:
   * Resolve `MindInput` (`try_get_node_as::<MindInput>("/root/Spine/Input")`)
     and take the view: `let view = input.bind().placement_view();` — drop
     the bind before touching the host.
   * Host bind for validity + content (`content_registry()`); resolve all
     textures into a local `Vec<DrawCmd>` (an enum local to plans.rs:
     `Region{tex,x,y,w,h,rot,color}`, `Lines{segs,stroke,color}`), drop the
     host bind, then `base_mut()` and execute.
   * Pulse: `let t = ticks_now(); let pulse = drawf::absin(t, 6.0, 0.28);`
     where `ticks_now()` = `Time::singleton().get_ticks_msec() as f32 * 0.06`.
   * Ghost colour for `valid`: `mixcol(WHITE, 0.24 + pulse)` → RGB
     `1.0` (white lerp of white is white) alpha 1; for invalid:
     `rgb = lerp(1.0, breakInvalid, 0.4 + pulse)` per channel, alpha 1.
     Godot modulate multiplies, it cannot "mix toward" a colour; approximate
     `mixcol(c, a)` on a sprite by `modulate = lerp(WHITE, c, a)` (multiply
     tint). Record this as a documented deviation in the commit body.
   * Ghost region: `block-<name>-full` → `<def.region>` (use
     `mind_core::render::block_region_candidates(&def.name, &def.region)`,
     first found in the cache), size `def.size * 8` square, centre
     `x*8 + def.offset`, rotation `rotation * 90` when
     `blocks::rotates_sprite(def)` (make that fn `pub(crate)` in
     `gdext/render/blocks.rs`), else 0.
   * Arrow: `fn draws_arrow(def: &BlockDef) -> bool { rotates_sprite(def)
     || matches!(def.kind, BlockKind::DirectionalUnloader | BlockKind::Duct
     | BlockKind::DuctBridge | BlockKind::OverflowDuct | BlockKind::DuctRouter
     | BlockKind::StackRouter | BlockKind::UnitFactory | BlockKind::Reconstructor
     | BlockKind::PayloadConveyor | BlockKind::PayloadRouter) }` with the comment
     "Java `Block.rotate && drawArrow`; BlockDef has no `rotate` field". If a
     listed `BlockKind` variant does not exist, drop it (do not invent).
   * Cursor branch (`mode != Placing`, `block` is Some, not AIR, cursor Some):
     arrow, ghost (valid = `host.preview_can_place`), then the range circle
     per `PlaceRangeKind` (see step 2).
   * Line branch (`mode == Placing`): for each plan: ghost with its own
     validity; arrow only on the last plan.
   * Break branch (`break_rect` Some): normalize `x1=min, x2=max` (clamp the
     span to 100 tiles like `maxLength`), for each tile in the rect with a
     building that passes `mind_core::world::build::valid_break` — call it
     through a new host seam `preview_can_break(x, y) -> bool` that builds
     `BuildRules` from the World resource like `can_place` does — collect its
     centre tile once per building (`tile.build` entity dedupe) and draw
     `Drawf.selected` corners in `pal::REMOVE` with the `block-select` region
     (natural size 8x8 world). Then the two rects: stroke 2,
     `REMOVE_BACK` at `y-1`, then `REMOVE`.
5. Do **not** draw schematic select/rebuild selection (G08 owns those
   modes; add a `// TODO(G08)` comment).

**Tests.**
* `core/sim/builder.rs` tests (module at the end of the file):
  `preview_can_place_matches_queue`: `let (mut sim, _unit, _core, wall) =
  sim_with_player();` (core-shard at (10,10), team 1; tick once like the
  neighbouring tests do so the place command applies) →
  `sim.preview_can_place(20, 20, wall, 0) == true`,
  `sim.preview_can_place(10, 10, wall, 0) == false` (core footprint), and
  `sim.queue_player_place(20, 20, wall, 0) == true`,
  `sim.queue_player_place(10, 10, wall, 0) == false`.
* `gdext/input/desktop.rs` unit test `placement_view_reports_break_drag`:
  construct `DesktopBridge::new()`, set `drag = Some(Drag{kind: Break,
  start: (1,1), last: (3,4), moved: true})` (test module is inside the file so
  private fields are reachable), assert `placement_view().break_rect ==
  Some(((1,1),(3,4)))`.

**Verify in-engine.** Ground Zero:
```bash
$G --eval "return $I.select_block_by_name(\"conveyor\")" \
   --eval "$C.center_on_tile(126, 52); return true"; sleep 1
$G --eval "return $C.tile_to_screen(126, 52)"          # -> Vector2 (sx, sy)
$G --eval 'get_tree().root.warp_mouse(Vector2(SX, SY)); return true'; sleep 1
$G --screenshot runs/<date>-sN-G03-T2/ghost-valid.png
```
Expected: conveyor ghost + arrow at the tile. Repeat with the cursor on the
core tile (129,55): red-tinted ghost (`ghost-invalid.png`). Select `duo`:
dashed circle radius 160 world px (= 640 screen px at zoom 4; zoom out with
`$C.zoom_by(-2)` first so it fits).
Break rect: send `mouse_button` RIGHT pressed at tile (124,50), `mouse_motion`
is not routed — instead move with `warp_mouse` + a second
`godot_input mouse_button` RIGHT pressed=false only after the screenshot;
screenshot while held. If the held drag cannot be staged through the bridge,
state "break rectangle verified headless only (test)".

**Pitfalls.**
* `MindInput` and `MindSimHost` must never be bound at the same time inside
  `draw()` (the input bridge binds the host in some calls); take the view,
  drop it, then bind the host.
* `preview_can_place` runs per ghost per frame; a 100-tile line = 100 calls
  of `valid_place` — fine, but do not call it for tiles off-screen.
* The `selected_block()` seam returns `"air"` for no selection; `state.block`
  can be `Some(BlockId::AIR)` (input mod.rs ~935) — treat AIR as None.

**Commit.** `Gameplay G03 T2: placement preview ghosts, arrow, line plans, break rect, range circles`

### T3 — Player build plans, build beam and rebuild ghosts

**Symptom / acceptance.** Queued plans are invisible until the construct site
appears; the builder draws no beam; destroyed-block ghosts never show. After:
(1) shift-queued conveyors that are not started yet show as pulsing ghosts
(same look as T2); queued deconstructions show red `block-select` corners;
(2) while the player alpha builds, an accent (`#ffd37f`, red `#e55454` when
breaking) beam fan goes from 3.8 world px in front of the unit to the
construct site, with a pulsing diamond at the emitter; (3) after a player
building is destroyed (with `rules.ghostBlocks`), selecting any block shows a
33%-alpha white-flashing ghost of the lost block.

**Java.**
* `J/input/InputHandler.java:1438-1473` `drawBuildPlans` (called from
  `OverlayRenderer.drawBottom` at `Layer.plans`, OverlayRenderer.java drawBottom):
  ```java
  for(plan in plans in camera){
      if(plan.progress > 0.01f || (current == plan && plan.initialized && (u.within(plan.x*8, plan.y*8, u.type.buildRange)))) continue;
      if(plan.breaking) drawBreaking(plan);      // Drawf.selected(x, y, tile.block(), Pal.remove)
      else plan.block.drawPlan(plan, allPlans, Build.validPlace(...) || planMatches(plan), 1f);
  }
  ```
  `planMatches(plan)` = the tile is a ConstructBuild whose `current ==
  plan.block` (InputHandler.java ~1517).
* `J/entities/comp/BuilderComp.java:315-386` `drawBuilding` +
  `drawBuildingBeam(px, py)`:
  ```java
  active = activelyBuilding(); if(!active && lastActive == null) return;
  plan = active ? buildPlan() : lastActive; if(tile == null || !within(plan, buildRange)) return;
  // drawBuilding: if(type.drawBuildBeam){ focusLen = type.buildBeamOffset + absin(Time.time, 3, 0.6);
  //   px = x + trnsx(rotation, focusLen); py = y + trnsy(rotation, focusLen); drawBuildingBeam(px, py); }
  size = plan.breaking ? (active ? tile.block().size : lastSize) : plan.block.size;
  tx = plan.drawx(); ty = plan.drawy();
  Lines.stroke(1, plan.breaking ? Pal.remove : Pal.accent); Draw.z(Layer.buildBeam); Draw.alpha(buildAlpha);
  if(!active && !(tile.build instanceof ConstructBuild)) Fill.square(tx, ty, size*8/2);
  Drawf.buildBeam(px, py, tx, ty, 8*size/2);
  Fill.square(px, py, 1.8 + absin(Time.time, 2.2, 1.1), rotation + 45);
  ```
  `buildAlpha` (BuilderComp.java:85): `buildAlpha = lerpDelta(buildAlpha,
  activelyBuilding() ? 1 : 0, 0.15)`; `lastActive` cleared when
  `buildAlpha <= 0.01` (line 81). `activelyBuilding()` (300-309):
  `plans non-empty && updateBuilding && within(head, buildRange)`.
  `buildBeamOffset` default 3.8 (UnitType.java:127).
* `J/graphics/Drawf.java:131-157` `buildBeam(x, y, tx, ty, radius)`:
  ```java
  ang = angle(x, y, tx, ty);
  v = [(tx-r,ty-r),(tx+r,ty-r),(tx-r,ty+r),(tx+r,ty+r)];
  sort v by -angleDist(angle(x,y,v), ang)   // descending distance from the beam axis
  close = closest of v to (x,y);
  x1,y1 = v[0]; x2,y2 = close; x3,y3 = v[1];
  if(animateShields){ if(close != v[0] && close != v[1]){ Fill.tri(x,y,x1,y1,x2,y2); Fill.tri(x,y,x3,y3,x2,y2);} else Fill.tri(x,y,x1,y1,x3,y3); }
  else { Lines.line(x,y,x1,y1); Lines.line(x,y,x3,y3); }
  ```
  The port has no `animatedshields` setting row, Java desktop default is
  `true` (`SettingsMenuDialog`: `checkPref("animatedshields", !mobile)`), so
  draw the **filled** triangles with the stroke colour at alpha
  `buildAlpha * 0.37` (the `buildbeam` shader's base alpha
  `0.37 + 0.05*|sin|`, `client/shaders/buildbeam.gdshader`; T17 replaces this
  with the real composite).
* `J/graphics/BlockRenderer.java:454-475` `drawDestroyed`: if setting
  `destroyedblocks` (default true) and the player is placing/breaking:
  `brokenFade = lerpDelta(brokenFade, 1, 0.1)` else toward 0; when
  `> 0.001`, for each `player.team().data().plans` (BlockPlan) in camera:
  `Draw.alpha(0.33 * brokenFade); Draw.mixcol(white, 0.2 + absin(Time.globalTime, 6, 0.2));
  Draw.rect(b.fullIcon, x*8 + offset, y*8 + offset, rotate ? rot*90 : 0)`.

**Port today.**
* Queue: `Sim::player_build_plans() -> Vec<UnitBuildPlan>` (`core/sim/
  builder.rs` ~893; fields `x, y, rotation, block, breaking, initialized,
  stuck, progress`), `BuilderPlans.update_building` (`builder.rs:97-107`).
  Unit build range: `UnitTypeDef.build_range` (used at builder.rs ~426-454).
  `UnitTypeDef.build_beam_offset`, `draw_build_beam` exist
  (`core/content/registries/units/mod.rs` ~1081, ~1215).
* Team ghosts: pushed by `Sim::resolve_deaths` into
  `CampaignRuntime.session.teams` (`core/sim/unit_runtime.rs` ~588-604);
  read with `Teams::data(TeamId) -> Option<&TeamData>` (`core/game/teams.rs:328`),
  `TeamData.plans: VecDeque<BlockPlan>` (`x, y, rotation: i8, block`).

**Changes.**
1. `gdext/sim_host.rs` Rust-only seam:
   ```rust
   pub struct BuilderView { pub x: f32, pub y: f32, pub rotation: f32, pub beam_offset: f32, pub build_range: f32,
       pub draw_beam: bool, pub update_building: bool, pub plans: Vec<mind_core::sim::builder::UnitBuildPlan> }
   pub fn player_builder_view(&self) -> Option<BuilderView>
   /// `(block, progress_block_matches)` helper: whether (x,y) holds a construct site building `block`.
   pub fn plan_matches(&self, x: i32, y: i32, block: BlockId) -> bool   // construct_at(x,y).is_some_and(|(b,_,_)| b == block)
   pub fn team_block_plans(&self) -> Vec<(i16, i16, i8, BlockId)>        // player team; empty without CampaignRuntime
   ```
   `UnitBuildPlan` must be importable: check `pub use` of `sim::builder`
   (`grep -n "pub mod builder\|pub use builder" core/sim/mod.rs`); if the
   module is private, add `pub use builder::UnitBuildPlan;` to `core/sim/mod.rs`.
2. Extend `gdext/render/plans.rs` (T2) `draw()`:
   * after the cursor/line branches, draw queued plans (Java order: plans are
     drawn first in `drawBottom` → place this block **before** the T2
     branches so the cursor ghost is on top): skip rule above (use
     `hypot(plan.x*8 - u.x, plan.y*8 - u.y) <= build_range` for `within`);
     breaking → `selected` corners `REMOVE` using the **current tile block**
     size (`content.block(grid.tile(x,y).block)`); placing → ghost with
     `valid = preview_can_place(..) || plan_matches(..)`.
   * rebuild ghosts: keep `broken_fade: f32` on the node; placing/breaking =
     `view.block.is_some_and(non-air) || view.mode == Breaking`;
     `broken_fade += (target - broken_fade) * (1 - (1-0.1)^delta_ticks)`
     (Arc `lerpDelta(a, b, t) = lerp(a, b, 1 - (1-t)^Time.delta)`; with
     `delta_ticks = delta_seconds * 60`). Gate on
     `settings_get_bool("destroyedblocks")`. Ghost colour: `modulate =
     lerp(WHITE, WHITE, ..)` = white with alpha `0.33 * broken_fade` (the
     mixcol toward white is invisible under a multiply tint; document).
3. New pass `gdext/render/build_beam.rs` `BuildBeamOverlay` (§0 template)
   under `Layer::BuildBeam` (if `band_node_at(BandKey::base(Layer::BuildBeam))`
   is `None`, stop and report: the band plan lacks it). State:
   `build_alpha: f32`, `last_active: Option<(UnitBuildPlan, i32 /*size*/)>`.
   `process(delta)`: compute `active = view.update_building &&
   !plans.is_empty() && within(head)`; `build_alpha = lerp_delta(build_alpha,
   active as f32, 0.15)`; when active store `last_active = Some((head, size))`;
   when `build_alpha <= 0.01` and not active, `last_active = None`. `draw()`:
   if `!draw_beam` return; pick plan; compute `focus = beam_offset +
   absin(t, 3, 0.6)`, `(px,py) = (x,y) + trns(rotation, focus)`; colour
   accent/remove; port `Drawf.buildBeam` geometry into a pure fn
   `core/render/drawf.rs::build_beam_tris(x, y, tx, ty, radius) ->
   Vec<[(f32,f32);3]>` (1 or 2 triangles) and fill them at
   `alpha = build_alpha * 0.37`; the emitter diamond: `Fill.square(px, py,
   s, rotation + 45)` = 4-gon with half-width `s = 1.8 + absin(t, 2.2, 1.1)`
   (vertex radius `s * sqrt2`, angles `rotation + 45 + 45 + k*90`) at alpha
   `build_alpha`; the inactive square fill (`!active && tile not construct`)
   at alpha `build_alpha`.
   Register in `build_floor` like T2.

**Tests.** `core/render/drawf.rs`:
`build_beam_tris_axis_aligned`: emitter `(0,0)`, target `(20,0)`, radius 4:
corners `(16,-4),(24,-4),(16,4),(24,4)`; angleDist from the axis: the two at
x=16 have |angle| = atan(4/16) = 14.04°, x=24 have 9.46°; sorted by
**descending** distance → v0,v1 = the two x=16 corners; closest corner to the
emitter is also an x=16 corner → `close == v0 or v1` → one triangle
`[(0,0),(16,±4),(16,∓4)]`. Assert one triangle whose non-origin vertices have
x = 16.

**Verify in-engine.** Ground Zero, `debug_add_core_items("copper", 300)`,
queue three conveyors through the player queue:
```bash
$G --eval "return $H.content_list(\"block\").find(\"conveyor\")"     # -> B (numeric id)
$G --eval "return $H.apply_sim_command_json('{\"op\":\"place\",\"x\":124,\"y\":58,\"block\":B,\"rotation\":0,\"player\":0}')"
```
(`block` is the numeric id, `sim_host.rs` "place" arm ~543-562; repeat for
x=125,126), then
`$H.set_paused(true)`, `$H.step(20)`, screenshot: the alpha shows the beam to
(124,58), the two other plans are ghosts. `$H.step(600)` → all built, no
ghosts. Ghosts: `$H.place_block(126,60,"copper-wall")`, spawn a team-2
dagger next to it (`spawn_unit` op, env §4.2) and step until
`tile_debug(126,60)` shows no build, select `copper-wall`
(`$I.select_block_by_name("copper-wall")`), screenshot: faint ghost at
(126,60). If `ghostBlocks` is false in the sector rules no ghost appears —
check `get_rules_json` contains `"ghostBlocks":true` first.

**Pitfalls.** `player_build_plans` clones the queue each frame: fine (small).
Breaking plans have `block == AIR`; never resolve a region for AIR. Do not
read `BuilderPlans` while the sim steps (all reads happen in `draw`, on the
main thread, between ticks).

**Commit.** `Gameplay G03 T3: queued build plans, build beam and rebuild ghosts`

---

### T4 — Bullets are invisible: bullet draw pass (sprites, lasers, shrapnel, liquid orbs, trails)

**Symptom / acceptance.** Every turret/unit shot is invisible; only the hit
sparks (T12 FX of the previous plan) show. After: duo/dagger shots show the
yellow `bullet` sprite with its darker back, pointing along the flight
direction and shrinking with age; hail shells show `shell`; flare/horizon
missiles `missile` with trails; lancer shows the blue laser (once G01 keeps
laser bullets alive, see Pitfalls); wave orbs show liquid circles. Compare a
wave fight with `runs/20261010-234250-ref-PAR-0070-combat-fx/` Java frames.

**Java.**
* `J/entities/bullet/BulletType.java:73` `layer = Layer.bullet`;
  `draw(b)`: `drawTrail(b); drawParts(b);` (parts deferred).
* `BulletType.java:687-695` `drawTrail`: if `trailLength > 0 && b.trail !=
  null`: `b.trail.draw(trailColor, trailWidth)` at `z - 0.0001`.
  `updateTrail` (821-829): `b.trail.update(b.x, b.y, trailInterp(b.fin()) *
  (1 + (trailSinMag > 0 ? absin(Time.time, trailSinScl, trailSinMag) : 0)))`
  each update. Trails are view state (headless skips them).
* `BasicBulletType.java:48-67` `draw(b)`:
  ```java
  super.draw(b);
  shrink = shrinkInterp.apply(b.fout());               // linear default
  height = this.height * ((1 - shrinkY) + shrinkY * shrink);
  width  = this.width  * ((1 - shrinkX) + shrinkX * shrink);
  offset = -90 + (spin != 0 ? randomSeed(b.id, 360) + b.time * spin : 0) + rotationOffset;
  mix = mixColorFrom.lerp(mixColorTo, b.fin()); Draw.mixcol(mix, mix.a);
  if(backRegion.found()){ Draw.color(backColor); Draw.rect(backRegion, b.x, b.y, width, height, b.rotation() + offset); }
  Draw.color(frontColor); Draw.rect(frontRegion, b.x, b.y, width, height, b.rotation() + offset);
  ```
  regions: `frontRegion = sprite`, `backRegion = backSprite ?? sprite + "-back"`
  (BasicBulletType.java:40-44). Defaults width 5, height 7, shrinkX 0,
  shrinkY 0.5, backColor `bulletYellowBack`, frontColor `bulletYellow`.
  Subclasses with this draw: Missile (`missile`, shrinkY 0), Artillery
  (`shell`, shrinkY 0.5), Flak (`shell`), Bomb (sprite arg, shrinkY 0.7),
  Emp, Interceptor, LaserBolt (+ lines below).
* `LaserBoltBulletType.java:26-34`: after the Basic draw:
  `color(backColor); stroke(width=2); Lines.lineAngleCenter(x, y, rot, height=7);
  color(frontColor); Lines.lineAngleCenter(x, y, rot, height/2)`.
  (Java's sprite part uses BasicBulletType's own `width/height` fields — 5/7
  unless set; the port merged the fields: use `def.width/def.height` for both
  and note the deviation.)
* `LaserBulletType.java:76-104` `draw(b)` (lancer): `realLength = b.fdata;
  f = curve(fin, 0, 0.2) = clamp(fin/0.2); baseLen = realLength * f;
  cwidth = width (15); compound = 1;` for each `color in colors`
  (default `[lancerLaser@0.4, lancerLaser, white]`):
  `stroke((cwidth *= lengthFalloff(0.5)) * fout); Lines.lineAngle(x, y, rot, baseLen, cap=false);
  tri at (x,y)+trns(rot, baseLen) with width = stroke, length = cwidth*2 + width/2, rotation rot;
  Fill.circle(x, y, cwidth * fout); for i in ±1: tri(x, y, sideWidth(0.7)*fout*cwidth, sideLength(29)*compound, rot + sideAngle(90)*i);
  compound *= lengthFalloff`.
* `ShrapnelBulletType.java` `draw` (transcribed in the audit):
  `color = lerp(fromColor, toColor, fin)`; for `i in 0 until
  (int)(serrations * realLength / length)`: `v = trns(rot, i*serrationSpacing)`;
  `sl = clamp(fout - serrationFadeOffset) * (serrationSpaceOffset - i*serrationLenScl)`;
  `tri(x+v, serrationWidth, sl, rot ± 90)`; then `tri(x, y, width*fout,
  realLength + 4, rot)` and `tri(x, y, width*fout, 10, rot + 180)`.
* `LiquidBulletType.java:~95-106` `draw`: `super.draw` (trail only) then if
  `liquid.willBoil()` (skip: gas liquids) else `Draw.color(liquid.color,
  white, fout/100); Fill.circle(x, y, orbSize)`; `orbSize` default 3.

**Port today.** No bullet seam, no pass. Bullet entities:
`UnitRuntime.bullets: Vec<Entity>` (`core/sim/unit_runtime.rs:92`), each with
`EntitySeq`, `Pos`, `Vel`, `TeamComp`, `Bullet` (`core/combat/bullet/mod.rs:66`:
`def, rotation, time, lifetime, fdata, aim, data, flags`). Defs:
`ContentRegistry::bullet(id) -> Option<&BulletDef>` (`core/content/load.rs:1092`)
with `kind: BulletKind` and draw fields `sprite: Option<String>, back_sprite,
width, height, back_color, front_color, mix_color_from, mix_color_to,
shrink_x, shrink_y, shrink_interp, spin, rotation_offset, colors, length,
side_length, side_width, side_angle, serrations, serration_*, from_color,
to_color, trail_length, trail_width, trail_color, trail_interp, layer,
puddle_liquid` (`bullets.rs` ~144-560). Kind constructors set the sprite
(`bullets.rs:825` "bullet", 852 "shell", 868 "missile", 907 "laser").
`core/render/trail.rs::Trail` (`new(len)`, `update(x, y, delta)`,
`update_width(x, y, width, delta)`, `draw(width, &mut Vec<[f32;8]>)`,
`draw_cap(width)`) exists; MindFx already renders trails from it
(fx.rs ~410-445 is a working template).

**Changes.**
1. `gdext/sim_host.rs`:
   ```rust
   pub struct BulletDraw { pub id: u64 /*EntitySeq*/, pub x: f32, pub y: f32, pub rotation: f32, pub time: f32,
       pub lifetime: f32, pub fdata: f32, pub def: mind_core::content::BulletId, pub team: u8 }
   /// Live bullets in spawn order (`Groups.bullet`); Rust-only seam for the bullet pass.
   pub fn bullet_draws(&self) -> Vec<BulletDraw>
   ```
   Iterate `runtime.bullets`, skip entities missing `Pos`/`Bullet`, skip
   `bullet.finished()` (removed this tick).
2. New `gdext/render/bullets.rs` `BulletRenderer` (§0 template) under
   `Layer::Bullet`. Fields: `regions: canvas::RegionCache`,
   `trails: HashMap<u64, (Trail, u64 /*last seen frame*/)>`, `frame: u64`.
   `process()`: `frame += 1`; for each bullet with `def.trail_length > 0`,
   get-or-insert `Trail::new(trail_length)` and
   `trail.update(x, y, interp(def.trail_interp, fin))` (use the existing
   interp helper — `grep -n "fn apply" core/` for `InterpKind`); drop trails
   not seen this frame (Java keeps a dying trail fading only for some
   types — ignore). `queue_redraw()`.
   `draw()`: per bullet by kind:
   * `Basic | Missile | Artillery | Flak | Bomb | Emp | Interceptor |
     LaserBolt`: sprite = `def.sprite.as_deref().unwrap_or("bullet")`, back =
     `def.back_sprite` or `format!("{sprite}-back")` (skip when not found);
     `fin = time/lifetime` (clamp 0..1), `fout = 1-fin`; shrink with the
     interp; sizes and `offset` as Java (`spin != 0`: seed angle =
     `((id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) % 360) as f32` —
     document "Mathf.randomSeed replaced by a hash"); mix: when
     `mix_color_from.a > 0 || mix_color_to.a > 0` multiply-approximate as in
     T2. Draw back (`back_color`) then front (`front_color`) with
     `canvas::draw_region(w, h, rot = rotation + offset)`.
     LaserBolt adds the two `lineAngleCenter` strokes.
   * `Laser`: the LaserBulletType loop above with `length_falloff = 0.5`
     (`LaserBulletType.java:9`; BulletDef has no field), colours
     `def.colors` (fallback to the Java default triple when empty).
   * `Shrapnel`: as transcribed, `realLength = fdata`.
   * `Liquid`: colour = `content.liquid(def.puddle_liquid).color` lerped to
     white by `fout/100`, circle radius 3.
   * all others (`Plain, Fire, SpaceLiquid, Sap, Lightning, Explosion, Rail,
     Continuous*, Multi, Point, PointLaser, MassDriver, Empty`): trail only.
   Trails: draw before the bullet body with `trail.draw(def.trail_width,
   &mut quads)` → `draw_colored_polygon` per quad in `def.trail_color`
   (same code as fx.rs ~417-445) plus the cap circle.
3. Register in `build_floor`.

**Tests.** Pure geometry goes to `core/render/bullet_draw.rs` (new file,
`pub mod bullet_draw;` in `core/render/mod.rs`):
`pub fn basic_size(width, height, shrink_x, shrink_y, shrink) -> (f32, f32)`
and `pub fn laser_layers(width, falloff, fout, colors_len) -> Vec<(f32 /*stroke*/, f32 /*tri len*/, f32 /*circle r*/, f32 /*side len*/)>`.
* `basic_size_halves_height_at_end`: `(5,7,0,0.5, shrink=0)` → `(5, 3.5)`;
  shrink 1 → `(5, 7)`.
* `laser_layers_lancer_start`: width 15, falloff 0.5, fout 1, 3 colours →
  cwidth sequence 7.5, 3.75, 1.875 → strokes `[7.5, 3.75, 1.875]`, tri
  lengths `cwidth*2 + 7.5` = `[22.5, 15.0, 11.25]`, circle radii = cwidth,
  side lengths `29 * compound` with compound before the multiply `[29, 14.5, 7.25]`.

**Verify in-engine.** Ground Zero: place a duo + feed copper:
`$H.place_block(132,60,"duo")`, `$H.debug_add_building_items(132,60,"copper",20)`,
spawn a team-2 dagger 8 tiles east (`spawn_unit` op with the dagger index
`content_list("unit").find("dagger")`, px=(140*8), py=(60*8)), step 30
ticks paused, `$G --eval "return $H.get_group_count(\"bullet\")"` > 0, then
screenshot `runs/<date>-sN-G03-T4/duo-bullets.png`: yellow bullets with the
tip toward the dagger. This also verifies T1's flip.

**Pitfalls.**
* **Laser bullets are removed at spawn in the port** (`core/combat/bullet/
  kinds/laser.rs:58` sets `HIT` right after `collide_line`); Java keeps them
  for `lifetime = 16` ticks with speed 0 (`LaserBulletType.java:15-33`, no
  removal in `init`). Until G01 fixes it, lancer lasers stay invisible — this
  is a cross-area finding (§6), do not change the sim here.
* Bullet counts can be in the hundreds; skip bullets outside
  `visible_rect()` grown by 64 px.
* `id` for trails must be the `EntitySeq`, not the bevy `Entity` (reused).

**Commit.** `Gameplay G03 T4: draw bullets (basic/missile/shell sprites, lasers, shrapnel, liquid orbs, trails)`

---

### T5 — Unit pass: mining beam, carried item stack, engines + trails, hit flash

**Symptom / acceptance.** The alpha mining copper shows no laser; carried ore
is invisible; flares/horizons/the alpha have no engine glow; units never
flash white when hit. After: mining shows the `minelaser` beam from the
alpha's nose to the ore tile with a rotating accent square on the tile (own
unit only); a carried stack shows the item icon behind the unit with an
accent `ring-item` circle and the amount text; flying units show team-coloured
engine circles with a white core and trails; a damaged unit flashes white for
9 ticks. Compare the mining with the Java tutorial frame
`runs/20261009-065416-ref-ground-zero/java/` (mining step) when present.

**Java.**
* `UnitType.java:1471-1480` draw order: `drawBuilding` (T3), then
  `if(unit.mining()) drawMining(unit)` before the body; engines inside
  `draw` after outlines: `if(engineLayer > 0) Draw.z(engineLayer); if(trailLength
  > 0 && !naval && (flying || !useEngineElevation)) drawTrail; if(engines.size
  > 0) drawEngines; Draw.z(z); drawBody; drawCell; drawWeapons; if(drawItems)
  drawItems`.
* `UnitType.java:1599-1631` `drawMining`/`drawMiningBeam`:
  `focusLen = mineBeamOffset + absin(Time.time, 1.1, 0.5)`; emitter
  `(x,y) + trns(rotation, focusLen)`; `swingScl = 12, swingMag = 8/8 = 1,
  flashScl = 0.3`; target `ex = tile.worldx() + sin(Time.time + 48, 12, 1)`,
  `ey = tile.worldy() + sin(Time.time + 48, 14, 1)`; `Draw.z(Layer.flyingUnit
  + 0.1)`; colour `lerp(lightGray (0.75,0.75,0.75), white, 1 - 0.3 +
  absin(Time.time, 0.5, 0.3))`, alpha `unitLaserOpacity` (setting
  `unitlaseropacity`/100); `Drawf.laser(minelaser, minelaser-end, px, py, ex,
  ey, 0.75)`; if local: `Lines.stroke(1, accent); Lines.poly(tile.worldx(),
  tile.worldy(), 4, 8/2*sqrt2, Time.time)`. `mineBeamOffset` defaults to
  `hitSize/2` (UnitType.java:1031).
* `UnitType.java:1682-1714` `drawItems`: if item and `itemTime > 0.01`:
  `sin = absin(Time.time, 5, 1); size = (itemSize(5) + sin) * itemTime`;
  item `fullIcon` at `(x,y) + trns(rotation + 180, itemOffsetY(3))`, size
  `size×size`, rotation `unit.rotation`, `mixcol(accent, sin*0.1)`; then
  `ring-item` tinted accent, size `((3 + sin) * itemTime + 0.5) * 2`; if
  local: amount text (`Fonts.outline`, accent, scale `0.25*itemTime`) at
  `y - 3`. `itemTime = lerpDelta(itemTime, hasItem ? 1 : 0, 0.05)`
  (UnitComp update).
* `UnitType.java:1724-1733` + `UnitEngine.draw` (2018-2050): `scale =
  useEngineElevation ? elevation : 1`; skip if `<= 0.0001`; `rot =
  rotation - 90`; colour `engineColor ?? team.color`;
  `(ex,ey) = rotate((engine.x, engine.y), rot)`; `rad = (radius +
  absin(Time.time, 2, radius/4)) * scale`; `Fill.circle(x+ex, y+ey, rad)`;
  colour `engineColorInner` (white); `Fill.circle(x + ex - trnsx(rot +
  engine.rotation, rad/4), y + ey - trnsy(..), rad/2)`.
* `UnitType.java:1716-1722` `drawTrail`: `trail.draw(trailColor ?? team.color,
  (engineSize + absin(Time.time, 2, engineSize/4) * (useEngineElevation ?
  elevation : 1)) * trailScl)`; the trail is updated in `UnitComp.update` with
  (`J/entities/comp/UnitComp.java:780-788`): `scale = useEngineElevation ?
  elevation : 1; offset = engineOffset/2 + engineOffset/2*scale;
  trail.update(x + trnsx(rotation+180, offset), y + trnsy(rotation+180, offset))`
  (width argument defaults to 1).
* `UnitType.java:1986-1999` `applyColor`: `Draw.mixcol(white, hitTime)`
  (healFlash ignored); `HealthComp.java:9,26,75`: `hitDuration = 9`,
  `hitTime -= Time.delta / 9` each update, `hitTime = 1` in `damage()`.
  `ItemsComp.java:18`: `itemTime = lerpDelta(itemTime, hasItem ? 1 : 0, 0.05)`.

**Port today.** `UnitDraw` (sim_host.rs:30-55) has position/rotation/team/
health/elevation/mech/weapons. Components available: `MinerComp {mine_tile:
Option<TilePos>, mining}` (`core/entities/comp/unit/comp.rs:573`),
`ItemsComp {item: Option<(ItemId, i32)>}` (:392). `UnitTypeDef` fields:
`engines: Vec<EngineSpec{x,y,radius,rotation}>`, `engine_color:
Option<Rgba>`, `engine_color_inner`, `use_engine_elevation`, `trail_length`,
`trail_color`, `trail_scl`, `engine_size`, `engine_offset`, `mine_beam_offset`,
`item_offset_y`, `draw_items`, `draw_mine_beam` (units/mod.rs ~1080-1300).
No `hit_time` anywhere.

**Changes.**
1. Extend `UnitDraw` with `pub id: u64` (EntitySeq), `pub mine_tile:
   Option<(i16, i16)>` (only when `MinerComp.mining`), `pub item: Option<(String
   /*item name*/, i32)>`, `pub is_player: bool` (`Some(entity) ==
   self.sim.player_unit()`), `pub health: f32`. Fill them in
   `unit_draws_full`. Update the two existing constructors/users
   (`hover_info_world`, `unit_debug_positions`) only if they construct
   `UnitDraw` literally (they consume it — no change expected).
2. `core/render/unit_draw.rs`: pure helpers + tests:
   ```rust
   pub struct UnitViewState { pub hit_time: f32, pub item_time: f32, pub last_health: f32 }
   /// Per-frame advance: hit flash on health drop (HealthComp: hitTime = 1), decay delta/9; itemTime lerpDelta 0.05.
   pub fn advance_view(state: &mut UnitViewState, health: f32, has_item: bool, delta_ticks: f32)
   pub fn engine_circles(x, y, rotation, elevation, use_elev: bool, engine: (f32,f32,f32,f32), time: f32) -> Option<((f32,f32,f32),(f32,f32,f32))>
   pub fn mine_beam_points(x, y, rotation, offset, tile: (i16,i16), time: f32) -> ((f32,f32),(f32,f32))
   ```
3. `gdext/render/units.rs`: keep `view: HashMap<u64, UnitViewState>` (prune
   ids not seen this frame). In `update()`:
   * hit flash: `Draw.mixcol(white, hitTime)` cannot be expressed as a
     multiply tint. Draw the flash as a duplicate body quad (same region,
     transform) with colour `(1,1,1,hit_time)` into a separate pooled
     `MeshInstance2D` per `(bucket, page)` keyed `(bucket + 10, page)` whose
     material is one shared `CanvasItemMaterial` with
     `blend_mode = BLEND_MODE_ADD` (create it once in `UnitRenderer::new`;
     `# code-instantiated` comment). Parent the flash node to the same band
     after the normal node so it draws on top.
   * engines: Godot meshes need UVs; use the atlas `white` region (1x1 px,
     page 0) for solid fills: emit a 12-segment triangle fan into the bucket
     group after the weapon outlines and before the body (Java order). Add a
     helper `push_circle(groups, key, white_region, x, y, r, rgba)`.
   * mining beam + item stack + ring: separate custom-draw node is simpler
     (text needs a font): new `gdext/render/unit_overlays.rs` `UnitOverlay`
     under `Layer::FlyingUnit` band (Java mining z = flyingUnit + 0.1; item
     stacks are at the unit's z, drawing them in the flying band slightly
     over-draws ground units — accepted, note it). Draw mining with
     `canvas` helpers (`Drawf.laser` via `draw_region` + `draw_region_line`,
     stroke `12*0.75 = 9`, caps `minelaser-end` size `72*0.75*0.25 = 13.5`),
     the accent 4-gon for the player's unit, then items: item icon
     `item-<name>` (fallback `item-<name>-full`; check which exists with
     `MindWidgets.assets().probe`), ring, and for the player unit the amount
     via `draw_string(ThemeDB.fallback_font, pos, text, ALIGN_CENTER, -1,
     font_size = 4)` in accent — Godot font sizes are in world units here;
     4 world px ≈ Java `0.25` scale of the outline font. Mark the size
     UNVERIFIED: compare with the Java frame and adjust only the size.
   * trails: keep `trails: HashMap<u64, Trail>` in `UnitOverlay`, updated in
     `process()` per the transcribed `UnitComp` call; draw in the overlay
     before the engines would be ideal but the engines live in the unit mesh;
     draw trails in the `Layer::FlyingUnitLow` band instead (a third tiny
     node) — accepted deviation, documented.
4. Settings: `unitlaseropacity` multiplies the beam alpha.

**Tests** (`core/render/unit_draw.rs`):
* `hit_flash_decays_in_nine_ticks`: start health 10, then 8 → `hit_time = 1`;
  after 4.5 ticks → 0.5; after 9 → 0.
* `engine_circles_alpha_default`: alpha engine def (read from
  `content.unit_by_name("alpha")` in a test using
  `crate::content::test_support::test_registry()` if it loads vanilla units;
  otherwise use a literal `(0, -5, 2.5, -90)` = default `engineOffset 5`,
  `engineSize 2.5`): rotation 90, elevation 1, time 0: `rot = 0`,
  engine offset rotated `(0,-5)` → `(0,-5)`, `rad = 2.5 + absin(0,2,0.625) =
  2.5 + 0.3125 = 2.8125`; outer centre `(x, y-5)`.
* `mine_beam_target_swing`: time 0: `ex = tx*8 + sin(48/12)*1 = tx*8 - 0.7568`.

**Verify in-engine.** Ground Zero, player alpha mines: `$G --eval "$H.set_player_mine(...)"`
is Rust-only — use the input path: `$I.select_block_by_name("air")` then a
left-click `godot_input` on the copper ore tile (130,48) screen position
(`$C.tile_to_screen(130,48)`); step 120 ticks; screenshot: beam + ring +
item. `$G --eval "return $H.player_mine_tile()"` → `[130, 48]` confirms the
mine order. Engines: alpha is flying — the engine glow is visible behind it.
Hit flash: spawn a team-2 dagger next to the core and screenshot repeatedly
while the duo of T4 fires (white flash on the dagger).

**Pitfalls.** `unit_draws_full` allocates and is now called by the unit
pass and the overlay each frame; accept it (tens of units) and note it in the
commit. The hit flash is view-derived from health drops (no sim `hitTime`);
healing never flashes (Java `healFlash` is off by default for most units).

**Commit.** `Gameplay G03 T5: mining beam, carried items, engines, trails and hit flash`

### T6 — Hovered building shows its bars (health, power, items, liquids)

**Symptom / acceptance.** Hovering one of your buildings shows only its icon
and name in the top table; Java also shows the bar stack. After: hovering the
core shows `Health` (red `#ff341c`) and, for a powered consumer, `Power`
(`#ec7b4c`); a container shows `Items: N` (`#2ea756`); a conduit shows the
liquid bar named after the liquid. Enemy buildings show only icon+name (Java
`team == player.team()` gate). Bars refresh while hovering.

**Java.**
* `J/entities/comp/BuildingComp.java` `display(table)` (grep it, ~1600):
  icon (32) + `labelWrap(getDisplayName).width(190)`; then **only for the
  player team**: `table.table(bars -> { bars.defaults().growX().height(18).pad(4); displayBars(bars); })`
  (`displayBars`, :1667-1674: one row per `block.listBars()` result).
  Consumption icons (`displayConsumption`) and flow rates are deferred.
* `J/world/Block.java:709-760` `setBars()`:
  * `health`: `Bar("stat.health", Pal.health, healthf).blink(white)`;
  * `power` if `consPower != null`: label `buffered ? format("bar.poweramount",
    (int)(status * capacity)) : "bar.power"`, colour `Pal.powerBar`, value
    `status` (and 1 when the consumer requests 0 power and the graph has
    produced+stored > 0 — approximate with `status`);
  * `items` if `hasItems && configurable`: `format("bar.items", total)`,
    `Pal.items`, `total / itemCapacity`;
  * liquids (last) if `hasLiquids`: one bar per `ConsumeLiquid`/`ConsumeLiquids`
    liquid (`addLiquidBar(liq)`: label `liq.localizedName`, colour
    `liq.barColor()`, value `liquids.get(liq) / liquidCapacity`); if none was
    added and `liquidCapacity > 0`: dynamic bar (label `bar.liquid` when empty
    else the current liquid's name, value `current / capacity`).
  * Many blocks append their own bars (`GenericCrafter` progress? no;
    `Drill` `bar.drillspeed`, turrets `bar.ammo`, `PowerGenerator`
    `bar.poweroutput`, `Battery` `bar.powerstored`, `UnitFactory`
    `bar.progress` …) — **deferred**, list them in the commit body.
* `Liquid.barColor()` (`J/type/Liquid.java`, grep `barColor`): returns
  `barColor` if set else `color`; use the liquid's `color` (port `Liquid.color`;
  if the port has a `bar_color` field use it).

**Port today.** `hover_tile` (sim_host.rs ~3045-3078) returns
`{kind:"building", name, icon, label}` only. `BlockDef.consumes:
Vec<ConsumeSpec{consume: Consume, ..}>` with `Consume::Power{usage,
buffered}`, `Consume::Liquid{liquid, ..}`, `Consume::Liquids(Vec<LiquidStack>)`
(`core/content/registries/blocks/mod.rs` ~1057-1100); flags `has_items`,
`configurable`, `has_liquids`, `liquid_capacity`, `item_capacity`. Modules:
`Health`, `PowerModule.status`, `ItemModule.total`, `LiquidModule.liquids`
(`core/world/modules/mod.rs`). `ui/widgets/mind_bar.gd` `MindBar.setup(name,
color, fraction)` + `fraction` property (`Bar.java` port).

**Changes.**
1. `gdext/sim_host.rs`: `#[func] pub fn building_bars_json(&self, x: i32, y:
   i32) -> GString` returning `[]` unless the tile has a building of the
   player team, else
   `[{"key":"health","label":"<text>","color":"ff341c","value":0.83}, …]` in
   Java order. Build label text in Rust with the bundle used by `MindHud`?
   No — keep Rust bundle-free: return `"label_key"` and `"label_arg"`
   (`{"label_key":"bar.items","label_arg":"12"}`, or `"liquid.water.name"`
   style keys) and format in GDScript with `MindWidgets.markup_format`.
   Dynamic liquid bar: current liquid = index of the max entry of
   `LiquidModule.liquids` (the port tracks no `current()` liquid id; note the
   deviation).
2. `ui/fragments/placement_fragment.gd`:
   * `_build_hover_row(hover)`: after the icon row, when
     `hover.kind == "building"`, call `building_bars_json(tile.x, tile.y)`
     (store the hovered tile from `_mouse_tile()` in `_hover_tile`), and for
     each row add a `MindBar` (`# code-instantiated: one bar per Block.setBars
     entry`) with `custom_minimum_size = Vector2(0, 18)`, margins 4 (wrap in a
     `MarginContainer` with 4 px on all sides), `setup(text, Color(hex), value)`.
     Keep the bars in `_hover_bars: Array` and their keys.
   * In `_update_top_table`, when the state did not change and the hover is
     a building, every `TOP_REFRESH_MS` re-read `building_bars_json` and set
     each bar's `fraction` (and text for `items`/`poweramount`); rebuild if
     the key list changed.
   * Include the hovered tile in the `_top_state` key (`"h:building:%s:%d,%d"`)
     so moving between two buildings of the same type rebuilds.

**Tests.** `mind-gdext` has unit tests (`cargo test -p mind-gdext --lib`);
put the bar-list builder in a pure fn
`fn building_bar_rows(def: &BlockDef, health: f32, power_status: Option<f32>, items_total: Option<i32>, liquids: Option<&[f32]>) -> Vec<BarRow>`
in `sim_host.rs` and test: a def with `has_items=true, configurable=true,
item_capacity=10`, items 5 → rows `[health, items(0.5)]`; a conduit-like def
(`has_liquids`, capacity 10, no consumers) with liquids `[0, 4.0]` → dynamic
liquid row value 0.4, label key `liquid.<name of id 1>.name`.

**Verify in-engine.** Ground Zero: `$C.center_on_tile(129,55)`, warp the mouse
onto the core (`tile_to_screen(129,55)` + `warp_mouse`), screenshot the top
table: icon, "Core: Shard", a red `Health` bar. Damage nothing. Then
`$H.place_block(126,52,"container")`… (container is not in Ground Zero's
research? `place_block` bypasses the gate) +
`debug_add_building_items(126,52,"copper",50)`, hover → `Items: 50` bar.

**Pitfalls.** `hover_info_world` is called every frame; call
`building_bars_json` only on rebuild or every 250 ms. Do not show bars for
derelict/enemy buildings.

**Commit.** `Gameplay G03 T6: hovered buildings show health/power/items/liquid bars`

---

### T7 — Power node lasers and item-bridge links

**Symptom / acceptance.** Linked power nodes show no lasers; item bridges show
no bridge/arrows — the player cannot see the network. After: every power-node
link draws the `laser` texture strip (yellow-white, `Pal.powerLight` when
unsatisfied) between the node edges with `laser-end` caps; a linked
`bridge-conveyor` draws `<name>-bridge` between the two ends with
`<name>-end` caps and moving `<name>-arrow` chevrons.

**Java.**
* `J/world/blocks/power/PowerNode.java:496-512` `PowerNodeBuild.draw`
  (after the cached base): skip when `laserOpacity == 0`, payload or derelict;
  `Draw.z(Layer.power)`; `setupColor(power.graph.getSatisfaction())`:
  `Draw.color(laserColor1 (white).lerp(laserColor2 (powerLight fbd367),
  (1 - satisfaction) * 0.86 + absin(3, 0.1))).a(laserOpacity * Lod.alpha2)`
  (PowerNode.java:183-185); for each link: skip invalid, skip when the other
  is a `PowerNode` with `link.id >= id` (each pair once); `drawLaser(x, y,
  link.x, link.y, size, link.size)`:
  `angle = angle(x1,y1,x2,y2); len1 = size1*4 - 1.5; len2 = size2*4 - 1.5;
  Drawf.laser(laser, laserEnd, laserEnd, x1 + v*len1, y1 + v*len1, x2 - v*len2,
  y2 - v*len2, laserScale = 0.25)` (PowerNode.java:186-197; regions
  `<name>-laser` fallback `laser`, `<name>-laser-end` fallback `laser-end`).
  `laserOpacity = lasersopacity/100` (Renderer.java:166).
* `J/world/blocks/distribution/ItemBridge.java:384-432` `draw`: base, then
  `Draw.z(Layer.power)`; skip if link invalid or `bridgeOpacity == 0`;
  `i = relativeTo(other)` (0 right, 1 up, 2 left, 3 down); `Draw.alpha(
  (fadeIn ? max(warmup, 0.25) : 1) * bridgeOpacity)` (`fadeIn` true only for
  phase conveyors — `hasPower`); `endRegion` at `(x, y)` rot `i*90 + 90` and at
  the other end rot `i*90 + 270`; `Lines.stroke(bridgeWidth = 6.5)`;
  `v = (x,y) - (ox,oy)` set length 4, negated; `Lines.line(bridgeRegion, x +
  v.x, y + v.y, ox - v.x, oy - v.y, false)`; then if `Lod.l1`: `dist =
  max(|dx|,|dy|) - 1` tiles; `arrows = (int)(dist * 8 / arrowSpacing(4))`;
  for `a in 0..arrows`: `alpha = absin(a - time / arrowTimeScl(6.2),
  arrowPeriod(0.4), 1) * warmup * bridgeOpacity * Lod.alpha1`; `arrowRegion`
  at `(x + dx*(4 + a*4 + arrowOffset(2)), y + dy*(...))` rot `i*90`.
  `warmup` here is `hasPower ? this.warmup : 1` (line 400): plain bridges → 1.

**Port today.** Links: `PowerModule.links: SmallVec<[i32; 4]>` packed with
`core/world/pos.rs::pack/unpack` (`pos.rs:62-67`); satisfaction: the graph
behind `PowerModule.graph` (`core/world/blocks/power/graph.rs` `PowerGraph`,
check for a `satisfaction` field/method with `grep -n "satisfaction"
core/world/blocks/power/graph.rs`); bridge state
`ItemBridgeBuild {link: i32 (packed, -1 none), warmup, time, ..}`
(`core/world/blocks/distribution/item_bridge.rs:31-52`). Nothing draws them.

**Changes.**
1. `gdext/sim_host.rs` Rust-only seams over the visible rect:
   ```rust
   pub struct LaserLink { pub x1: f32, pub y1: f32, pub size1: i32, pub x2: f32, pub y2: f32, pub size2: i32, pub satisfaction: f32, pub block: BlockId }
   pub fn power_lasers(&self, rect: (f32, f32, f32, f32)) -> Vec<LaserLink>
   pub struct BridgeLink { pub x: i32, pub y: i32, pub ox: i32, pub oy: i32, pub warmup: f32, pub time: f32, pub has_power: bool, pub block: BlockId }
   pub fn bridge_links(&self, rect: (f32, f32, f32, f32)) -> Vec<BridgeLink>
   ```
   Power: iterate buildings with `PowerModule` whose def kind is
   `PowerNode` (and `PowerDiode`? no — only nodes draw lasers; also Battery
   etc. do not); for each link resolve the other building (`grid.entity_at
   (unpack(link))`), skip derelict team 0, skip when other is a node and its
   `EntitySeq`/entity index `>=` this one's (stable pair rule), building
   centres = `Building.x` (use the same centre formula as blocks.rs:
   `(tile + (size-1)/2) * 8 + offset`). Bridges: kinds `ItemBridge |
   BufferedItemBridge` (and `LiquidBridge` if the kind exists — it uses the
   same draw) with `link != -1` and a valid linked building of the same block.
2. New `gdext/render/links.rs` `LinkOverlay` (§0 template) under
   `Layer::Power`: draws lasers then bridges with `canvas` helpers
   (`draw_region_line` for the strips, `draw_region` for caps/ends/arrows).
   Laser width `12 * 0.25 = 3`; caps `laser-end` 72 px → `72*0.25*0.25 =
   4.5` world; `vx,vy = (cos, sin) * 8 * 0.25 * 0.25 = 0.5` (Java
   `scl = 8*scale*Draw.scl`). Opacity from settings `lasersopacity`,
   `bridgeopacity`; `Lod`: read `stats_lod_l2`/`lod_l1` from the renderer
   like `status.rs::lod_l2`.

**Tests.** `core/render/drawf.rs`: `laser_endpoints_trim_by_size`: node at
(0,0) size 1, other at (48,0) size 2 → start `(0 + 2.5, 0)`, end
`(48 - 6.5, 0)`; and `bridge_arrow_count`: bridge (10,10)→(13,10): `dist = 2`,
`arrows = (int)(2*8/4) = 4`, first arrow x = `80 + 1*(4 + 0 + 2) = 86`.

**Verify in-engine.** Ground Zero (power nodes are placeable through the API):
`$H.place_block(120,60,"combustion-generator")`, `$H.place_block(124,60,"power-node")`,
configure the link with `$I.configure_building(124,60,"int", <packed pos of
(120,60)>)` — if the node auto-linked on placement (T11 of the
building-production plan), no configure is needed; check
`power_debug(124,60)`. Screenshot: laser between them. Bridges:
`$H.place_block_rot(110,60,"bridge-conveyor",0)` and `(113,60)`, configure
the first with the second's packed pos (`configure_building(110,60,"int",
pack)` — `pack(x,y) = (x << 16) | y`, check `core/world/pos.rs:62`), screenshot.

**Pitfalls.** Iterating all buildings each frame is too slow on big maps:
restrict to tiles in `visible_rect` grown by the longest laser (surge tower
40 tiles = 320 px) using `mind_core::render::scan::visible_blocks` with a
grown `CameraView`. Never index `grid.tiles` with unpacked links without a
bounds check (`tile_at`).

**Commit.** `Gameplay G03 T7: power-node lasers and item-bridge links`

---

### T8 — Block state sprites, damage cracks and enemy team markers

**Symptom / acceptance.** Doors look closed when open, switches never show
"on", sorters/unloaders/item sources never show the configured item colour,
damaged buildings show no cracks, and enemy buildings carry no team corner
mark. After: an opened door shows `door-open`; an enabled switch shows
`switch-on`; a sorter set to copper shows a copper-coloured square under its
frame (`cross-full`/`<name>-cross` when unset); an unloader tints its
`unloader-center` with the item colour; a wall at 30% health shows dark
`cracks-1-5`; derelict/enemy buildings show a team-coloured `block-border`
corner at their bottom-left.

**Java.**
* `Door.java:132-135`: `Draw.rect(open ? openRegion("@-open") : region, x, y)`.
* `SwitchBlock.java:64-71`: `super.draw(); if(enabled) Draw.rect(onRegion("@-on"), x, y)`.
* `Sorter.java:76-88`: if `sortItem == null` `Draw.rect(cross ("@-cross"
  fallback "cross-full"), x, y)` else `Draw.color(sortItem.color);
  Fill.square(x, y, 4 - 0.00001)`; then `super.draw()` (base region on top).
* `ItemSource.java` draw: same as sorter with `outputItem`, unset →
  `cross-full`.
* `Unloader.java:263-269`: `super.draw(); Draw.color(sortItem == null ?
  clear : sortItem.color); Draw.rect(centerRegion ("@-center" fallback
  "unloader-center"), x, y)`.
* `BuildingComp.java:1236-1244` `drawCracks` (at `Layer.blockCracks`, only if
  `block.drawCracks && damaged() && size <= maxCrackSize (7)`):
  `index = clamp((int)((1 - healthf) * 8), 0, 7)`; region
  `cracks-<size>-<index>`; `Draw.colorl(0.2, 0.1 + (1 - healthf) * 0.6)` =
  colour `(0.2, 0.2, 0.2, a)`; rotation `(pos % 4) * 90` where `pos =
  Point2.pack(x, y) = (x << 16) | y` → `(tile.y % 4) * 90` for the building's
  centre tile. `damaged()` = `health < maxHealth - 0.001`
  (HealthComp.java:47).
* `BuildingComp.java:1334-1338` `drawTeam` (BlockRenderer.java ~752-757: only
  when `build.team != player team && block.drawTeamOverlay`):
  `Draw.color(team.color); Draw.rect("block-border", x - size*4 + 4, y -
  size*4 + 4)` natural size (32 px → 8 world).

**Port today.** `gdext/render/blocks.rs::update` pushes one composed region
per building (`push(...)` closure ~171-196), with special branches for
construct sites, drills, turrets, belts and team tops; `building_team(x,y)`,
`building_enabled(x,y)` (#[func] ~919) exist. State: `DoorState.open`
(`core/entities/comp/building.rs:187`), `SorterBuild.sort_item`
(`core/world/blocks/distribution/sorter.rs:23-26`), `UnloaderBuild.sort_item`
(`core/world/blocks/storage/unloader.rs:43-49`), `SandboxState.item`
(`building.rs:205`), `Building.enabled`, `Health`. `Item.color: Rgba`
(`core/content/registries/items.rs:31`).

**Changes.**
1. `gdext/sim_host.rs`: one Rust-only seam
   ```rust
   pub struct BlockStateDraw { pub door_open: bool, pub enabled: bool, pub sort_item: Option<ItemId>, pub source_item: Option<ItemId>, pub healthf: f32, pub damaged: bool }
   pub fn block_state_draw(&self, x: i32, y: i32) -> Option<BlockStateDraw>
   ```
   (reads the components above for the building at the tile; `None` without
   a building).
2. `gdext/render/blocks.rs::update`, inside the per-building loop, before the
   generic branch:
   * `BlockKind::Door | BlockKind::AutoDoor`: region `<name>-open` when open
     else `def.region`; push and `continue` (doors animate: set
     `animating = true` only if any door is visible — they change on tap, and
     a tap bumps the world revision? If not, keep `animating = true`).
   * `BlockKind::SwitchBlock`: push the normal region, then `<name>-on` when
     `enabled`.
   * `Sorter` (and inverted sorter, same kind? check `def.kind` of
     `inverted-sorter`), `ItemSource`: when an item is set, push the `white`
     region quad 8x8 tinted `item.color` **first**, then the block region;
     unset → `<name>-cross` or `cross-full`, then the block region.
   * `Unloader`: block region, then `<name>-center`/`unloader-center` tinted
     item colour (skip when None — Java draws it `clear`).
   * After the building's sprite(s): cracks (`settings` not involved): if
     `damaged && def.size <= 7` and the region `cracks-<size>-<index>` exists,
     push it into a separate `crack_groups` map that is appended after
     `item_groups` (Layer.blockCracks is above belt items' `block - 0.1`? No:
     blockCracks 30.1 > block 30 and items at 29.9 → cracks after items).
   * Enemy marker: if `def.draw_team_overlay` and `team != host.player_team()`
     push `block-border` tinted team colour at `(anchor_x - size*4 + 4,
     anchor_y - size*4 + 4)` size 8, appended with the cracks.
   * Set `animating = true` whenever a damaged building is visible (health
     changes do not bump the world revision).
3. Make the block pass rebuild when building state changes: if the world
   revision does not change on door/switch/sorter configure (check
   `world_revision()` before/after `configure_building` in-engine), add
   `animating = true` for those kinds.

**Tests.** `core/render/block_sprite.rs` (exists): add
`pub fn crack_index(healthf: f32) -> usize` and `pub fn crack_rotation(tile_y: i32) -> f32`;
tests: healthf 1.0 → 0, 0.3 → `(0.7*8) as i32 = 5`, 0.0 → 7 (clamp of 8);
`crack_rotation(6) == 180.0`, `crack_rotation(4) == 0.0`.

**Verify in-engine.** Ground Zero: `$H.place_block(126,52,"door")`, tap it
(`godot_input` left click on its screen pos) → screenshot open sprite.
`$H.place_block(127,52,"sorter")`, `$I.configure_building(127,52,"content",
"copper")` → copper square. Derelict scrap walls near (169,167) (team 0):
screenshot shows `block-border` marks and cracks on damaged ones.

**Pitfalls.** The team-top code already pushes `<name>-team` for every team;
the enemy `block-border` is additional. `player_team()` must be read once per
rebuild, not per building.

**Commit.** `Gameplay G03 T8: door/switch/sorter/unloader state sprites, cracks, enemy team marks`

### T9 — HUD residuals: boss bar, core-under-attack, drop-point warning, guardian toasts, capture announce, saving label, status icons, toggle menus

**Symptom / acceptance.** (a) Guardian waves show no boss health bar;
(b) a damaged core shows no flashing `< Core is under attack! >`; (c) flying
the alpha into the enemy drop zone shows no `LEAVE DROP POINT` warning;
(d) no `Guardian approaching in N waves` toasts; (e) capturing the current
sector opens the game-over dialog and a toast instead of Java's centred
`Sector Captured!` announce; (f) any autosave covers the screen with the
loading overlay instead of a small `Saving...` label; (g) the player's status
effects (wet, burning, boss…) never show under the wave table; (h) the
paused banner never reads `Sector Lost` after a loss; (j) announcements are a plain top-left label
instead of Java's centred fading box; (i) the
`toggle_menus` key (Java `Binding.toggleMenus`; port binding `toggle_menus`,
default `c`, `core/input/binding.rs:744-748`) does nothing (no arm in
`gdext/input/desktop.rs` binding match ~560-625). After: all
nine behave as below.

**Java** (`J/ui/fragments/HudFragment.java`):
* (d) :241-260 on `WaveEvent`: `max = 10; winWave = rules.winWave > 0 ?
  winWave : MAX`; for `i in wave-1 ..= min(wave + 10, winWave - 2)`: for each
  `group in rules.spawns` with `group.effect == boss && group.getSpawned(i) >
  0`: `diff = (i + 2) - wave`; if `diff in {1,2,5,10}` toast
  `Icon.warning`, `group.type.emoji() + " " + format("wave.guardianwarn" +
  (diff == 1 ? ".one" : ""), diff)`; then stop scanning (break outer) — the
  break happens on the first boss group found regardless of `diff`.
* (e) :262-268 `SectorCaptureEvent`: `if(e.sector.isBeingPlayed())
  ui.announce("@sector.capture.current", 5f) else showToast(format(
  "sector.capture", name))`.
* (h) :286-297 paused table label `state.gameOver && state.isCampaign() ?
  "@sector.curlost" : "@paused"`.
* (b) :596-631: `Trigger.teamCoreDamage` → `coreAttackTime = 240`; the
  collapser shows while `(coreAttackTime -= Time.delta) > 0` and not paused,
  background `Styles.black6`, text `@coreattack`, colour
  `orange.lerp(scarlet, absin(Time.time, 2, 1))`; tapping pans the camera to
  `control.lastDamagedCore`. Trigger source: `CoreBlock.java:842-848`
  `CoreBuild.damage(amount)`: if `team == player.team()` fire.
* (a) :633-656: bar `width 320 height 60`, `padTop 7`, visible when
  `rules.waves && state.boss() != null`; text = up to 6 boss unit emojis +
  (`[accent]+[]` if more) + `" " + bundle("guardian")`; colour `Pal.health`;
  value `sum(health)/sum(maxHealth)` over `state.teams.bosses` = units of
  `rules.waveTeam` with the `boss` status (`Teams.java:213-223`,
  `StatusComp.isBoss`); `blink(white)`, outline `(0,0,0,0.6)` 7.
* (c) :658-666 `nearpoint` table centred: `Styles.black6`, label
  `@nearpoint` colour `white.lerp(scarlet, absin(Time.time, 10, 1))`, table
  alpha lerps toward `spawner.playerNear() ? 1 : 0` by 0.1/tick.
  `WaveSpawner.playerNear()` (`J/ai/WaveSpawner.java:58-60`): spawns exist,
  player alive, any ground spawn within `rules.dropZoneRadius` (default 300)
  of the player, player team != wave team.
* (f) :668-673 `saving`: bottom-centred `@saving` (`Styles.outlineLabel`)
  while `control.saves.isSaving()`.
* (g) :1205-1231 `addInfoTable` second row: for each applied status effect
  with a `uiIcon`: an icon `iconMed` (32) with tooltip `name + [lightgray]
  duration`.
* (j) announce/toast visuals (`J/core/UI.java:636-652` `announce(text,
  duration)`: `Table(Styles.black3)`, `margin(8)`, label `Styles.outlineLabel`
  centred, positioned at the screen centre, removed when the HUD is hidden or
  in the menu, `actions(fadeOut(duration, Interp.pow4In), remove())`, a new
  announce replaces `lastAnnouncement`; `UI.java:405-419`
  `showInfoToast(info, duration)`: top-centred `Styles.black3` table margin 4,
  padTop 10 (+ half the coreinfo height when shown), `delay(duration*0.9)` then
  `fadeOut(duration*0.1, Interp.fade)`). G03 owns the faithful visuals; G06 T6
  only adds the `show_info_toast` entry point and signal.
* (i) :455-461: `toggleMenus()` flips `shown`; HUD tables use
  `visible(this::shown)`; first use announces `format("showui", key, 11)`
  once (`getBoolOnce("ui-hidden")`).

**Port today.** `ui/fragments/hud_fragment.gd` (337 lines) + `gdext/ui/hud.rs`
(`MindHud`, vars listed at hud.rs 60-136, `refresh_campaign` 336-495 parses
`get_hud_state` and `get_rules_json` → `Rules` every 0.2 s). Capture: hud.rs
~386-397 pushes a `sector.capture` toast; `hud_fragment.gd::_check_game_over`
opens `restart` on capture too. `_refresh_loading` shows the loading
overlay for any `io_pending()`. No boss/coreattack/nearpoint/status nodes.
Player status: `MindSimHost::player_unit_status()` (#[func], sim_host.rs
~1946) returns `{dead, healthf, type}`.

**Changes.**
1. `gdext/sim_host.rs` #[func]s:
   * `boss_status_json() -> GString`: `{"count":N,"names":["dagger",…≤6],"value":f}`
     — **seam ownership (resolved)**: G01 T19 adds `#[func] SimHost.boss_json()`
     returning `{"count":1,"names":["dagger"],"value":1.0,"first":{...}}`
     (a superset of this shape). If `grep -rn "fn boss_json" gdext/sim_host.rs`
     finds it, read `count/names/value` from `boss_json()` and do NOT add
     `boss_status_json`; if G01 T19 is not on `main` yet, add
     `boss_status_json` as written here and note that G01 T19 replaces it
     over runtime units with `TeamComp.team == wave team` (UnitRuntime
     `wave_team`) and a `StatusComp` entry whose effect is
     `content.status_by_name("boss")`; value = Σhealth/Σmax (0 when max 0).
   * `player_core_health() -> f64`: sum of `Health.health` over the player
     team's cores (iterate buildings of kind `CoreBlock` via the existing
     `player_core_blocks` pattern, sim_host.rs ~1488) — used to detect core
     damage in the view (deviation: Java fires on every `damage()` call; the
     port detects a health drop between refreshes; document it).
   * `last_damaged_core_position() -> Vector2` (world px of the core whose
     health dropped last; maintain in the HUD instead: return all player
     cores as `PackedVector2Array` positions + healths, the HUD diffs them).
   * `player_near_drop_zone() -> bool`: `wave_spawner.spawns` non-empty,
     player unit alive, any spawn with `hypot(sx*8 - px, sy*8 - py) <
     rules.drop_zone_radius` (live rules via `CampaignRuntime`; default 300),
     player team != wave team.
   * extend `player_unit_status` with `"statuses": [{"name": "...",
     "duration": ticks}]` from `StatusComp.statuses`.
2. `gdext/ui/hud.rs`:
   * capture: replace the toast at ~386-397 with
     `self.push_announce(GString::from(self.bundle.get("sector.capture.current")), 5.0)`
     (keep the `sector_event` signal).
   * guardian toasts: on `wave != self.last_wave` (same place that emits
     `wave_event`), run the Java scan over the parsed `Rules.spawns`
     (`SpawnGroup.effect: Option<String>` == `Some("boss")`,
     `get_spawned(i)`) and `push_toast(text, "warning")` with
     `bundle.format("wave.guardianwarn[.one]", &[diff])`, prefixed by the
     unit emoji (`self.iconc` lookup of `unit-<name>`; if the iconc has no
     entry, prefix nothing).
   * add `#[var] paused_text: GString` = `"@sector.curlost"` when
     `game_over && campaign` else `"@paused"`.
3. `scenes/ui/fragments/hud_fragment.tscn`: add under `CoreInfo` (after
   `CoreItems`, before `HudText`): `CoreAttack` (PanelContainer, black6
   style, child `Label` text `@coreattack`), `BossBar` (MarginContainer 10,
   min size 320x60, child `MindBar` instance); a root-level `NearPoint`
   (PanelContainer centred, child RichTextLabel) and `Saving` (Label,
   bottom-centre). Static nodes only; texts set in `_localize_banners()`.
   Add `StatusIcons` (HBoxContainer) under `WavesStack/WavesMain` after
   `Info`.
4. `ui/fragments/hud_fragment.gd`:
   * `_refresh_boss()`: parse `boss_status_json`; visible when `waves` and
     count > 0; `MindBar.setup(text, Color("ff341c"), value)` (once) then set
     `fraction`.
   * core attack: keep `_core_health_prev`; when the new total is lower set
     `_core_attack_time = 240.0` (ticks); in `_process` decrement by
     `delta * 60` unless paused; label colour
     `Color("ffa500").lerp(Color("ff2400"), absin(t, 2, 1))` (orange,
     scarlet); visible while > 0. Tapping (`gui_input` left press) pans
     `/root/Spine/World/Camera2D.pan_to` to the damaged core.
   * near point: alpha `lerp(alpha, near ? 1 : 0, 1 - pow(0.9, delta*60))`.
   * saving: `Saving.visible = io_pending > 0 and the pending request is a
     save` — `take_io_requests_json` is consumed elsewhere; add a
     `#[func] io_saving() -> bool` to sim_host.rs (true while a save request
     is queued or in flight; read the IO queue struct next to `io_pending`)
     and show the loading overlay only for loads.
   * status icons: rebuild when the status name list changes; icon texture
     `MindWidgets.icon_texture("status-%s-ui" % name)` (verify the atlas name
     with `MindWidgets.assets().probe("status-wet-ui")`; fall back to
     `status-<name>`), 32x32, tooltip `_t("@status.%s.name" % name)`.
   * paused banner text from `paused_text`.
   * toggle menus: connect `hotkey` (already connected, `_on_hotkey`) for
     `"toggle_menus"`; flip `visible` of `WavesStack`, `CoreInfo`,
     `MinimapBox`, `FpsBox` and the placement fragment
     (`/root/Spine/Ui/UiRoot/HudGroup/placement`; confirm the node path in
     `scenes/ui/ui_root.tscn`). Input side: add
     `"toggle_menus" => self.fire_hotkey(name),` to the binding match in
     `gdext/input/desktop.rs` next to the `"toggle_block_status"` arm (~603);
     one line, mention it to G08 in the summary.
   * `_check_game_over`: drop the `captured` path (T16 handles the delay).
5. `ui/ui_root.gd::_on_announce` (~393) today reuses `_toast_label` (a plain
   label at (24,24)). Replace with `_announce_box(text, duration)`:
   `# code-instantiated` `PanelContainer` with the `black3` stylebox
   (`Styles.black3` = black at alpha 0.3; reuse the style from
   `ui/styles_manifest.json` if present, else `StyleBoxFlat` bg
   `Color(0,0,0,0.3)`), content margins 8, child `MindRichLabel` centred
   (outline font), anchored at the viewport centre; free any previous
   announce box first; tween `modulate:a` 1→0 over `duration` with
   `set_trans(Tween.TRANS_QUART).set_ease(Tween.EASE_IN)` (= pow4In); hide when
   the HUD is hidden. When G06 T6's `show_info_toast` signal exists, connect
   it to `_info_toast(text, duration)`: top-centred black3 box margin 4,
   `padTop 10` (+ `CoreInfo` height / 2 when visible), wait `0.9*duration`,
   fade over `0.1*duration` (linear), free; if the signal does not exist
   yet, add the handler only.

**Tests.** `mind-core` has the wave scan logic only in hud.rs; move it to
`core/ui/hud_text.rs` as `pub fn guardian_warnings(spawns: &[SpawnGroup],
wave: i32, win_wave: i32) -> Option<(String /*unit*/, i32 /*diff*/)>` (returns
the toast only when `diff` is in {1,2,5,10}) and test with one group
`SpawnGroup { unit: "fortress", begin: 9, spacing: 100, effect:
Some("boss"), ..SpawnGroup::new("fortress") }` (`get_spawned(i) > 0` only at
i = 9, `core/game/spawn_group.rs:114-123`), `win_wave = 0` (→ unbounded):
scan `i in wave-1 ..= wave+10`, first boss hit i = 9, `diff = 11 - wave`:
wave 1 → `Some(("fortress", 10))`; wave 6 → `Some((.., 5))`; wave 8 → `None`
(diff 3, scan stops); wave 9 → `Some((.., 2))`; wave 10 → `Some((.., 1))`
(".one" key); wave 11 → `None` (i starts at 10).

**Verify in-engine.** (b) `$H.place_block` nothing; spawn 3 team-2 daggers
next to the core (`spawn_unit` op), step 120, screenshot: flashing banner.
(a) spawn a dagger then apply boss: there is no apply-status seam — use a
wave: `get_rules_json` of sector 170 has no boss group; verify (a) with a
unit test of `boss_status_json` instead (headless only, explain).
(c) headless: unit-test the `player_near_drop_zone` predicate as a pure fn
(spawn tile (10,10), radius 300, player at (80+299, 80) → true; (80+301, 80)
→ false); in-engine optional (fly the alpha toward world (1156,1844) with
held `W`/`D` keys via `godot_input`).
(e) `$MC.capture_sector()` → announce text visible, no dialog.
(f) `$H.request_save("user://saves/test.msav", false)` → small label at the
bottom, no overlay. (g) skip unless a status can be applied (no seam).

**Pitfalls.** All HUD text through bundle keys (`@coreattack`, `@nearpoint`,
`@saving`, `@guardian`, `@sector.capture.current`, `@sector.curlost`).
`MindHud.refresh_campaign` runs at 5 Hz; the core-attack timer runs in
`_process`.

**Commit.** `Gameplay G03 T9: boss bar, core-attack and drop-point warnings, guardian toasts, capture announce, saving label, status icons`

---

### T10 — PAR-0095 Shift command panel (+ PAR-0157 attack reticle)

**Symptom / acceptance.** Holding the command key keeps the block palette;
Java swaps it for the command table. After: in command mode the palette area
shows `Command Mode` (accent), an accent rule, and either `[no units]` or one
icon+count button per selected unit type (left-click keeps only that type,
right-click removes it), then the shared command buttons (50x50, checked when
every selected unit has that command) and stance buttons. Clicking `Repair`
on selected novas/polys sets their command (`unit_debug_json` shows it). In
command mode an attacking unit's target shows the rotating red `Drawf.target`
reticle.

**Java.** `J/ui/fragments/PlacementFragment.java:443-676` (transcribed in the
audit; key facts):
* swap: `mainStack` shows `commandTable` when `control.input.commandMode`
  else `blockCatTable`; the command table is forced to the block table width.
* content: `add(bundle "commandmode.name").center()`; `image().color(accent)
  .growX().pad(20).padTop(0).padBottom(4)`; then the unit table:
  if `selectedUnits.size > 0`: `counts[type.id]++`; per type with count:
  `StatValues.stack(type, count)` (icon with the count bottom-right) `pad 4`,
  7 per row; tooltip `type.localizedName`; left click →
  `selectedUnits.removeAll(u -> u.type != type)`; right click →
  `removeAll(u -> u.type == type)`; union of `type.commands` in first-seen
  order; if `commands.size > 1`: row of `ImageButton(Icon.icons.get(command.icon))`
  `Styles.clearNoneTogglei` size 50, checked when `activeCommands` has it
  (any selected unit's `CommandAI.command`), 6 per row, click →
  `Call.setUnitCommand(player, ids of units whose type allows it, command)`,
  tooltip `command.localized()`; stances similarly (separator image `Pal.gray`
  height 3 pad 7 when both rows exist), click toggles
  `setUnitStance(ids, stance, !active)`; else label `commandmode.nounits`
  colour lightGray, centred, pad 6.
* rebuild when available commands/stances or the active type set change.
* keybinds (:663-668): pressing a command's/stance's keybind applies it —
  input owner G08 already maps `unit_command_*` (desktop.rs ~574-578).
* `J/input/InputHandler.java:1225-1237` `drawCommandedTargets` at
  `Layer.overlayUI`: for selected units with `CommandAI` whose
  `attackTarget != null && currentCommand().drawTarget`:
  `Drawf.target(target.x, target.y, 6, Pal.remove)` (T1 helper).

**Port today.** `ui/fragments/placement_fragment.gd` has no command branch;
scene `Panel/Frame[TopTable, TopDivider, Layout]`. `MindInput`:
`command_mode()` (#[func]), `selection_json()` (ids), signal
`selection_changed`, `set_unit_stance(stance, enabled)` (#[func], mod.rs
~915). `DesktopBridge::command_units(command: u16)` exists but is private
(desktop.rs ~766) and emits `RemoteAction::SetUnitCommand` →
`SimCommand::UnitCommand{units, command, x: 0, y: 0}`
(`core/input/command_emit.rs:143-155`). Content: `UnitTypeDef.commands:
Vec<UnitCommandId>`, `stances: Vec<UnitStanceId>`; registry accessors
`content.unit_command(id)`, `unit_stance(id)` (`core/content/load.rs`
mappable accessors ~1012-1033); `UnitCommandDef{name, icon, localized_key,
draw_target, ..}`. Live command state:
`Sim::unit_command_state(id) -> Option<&CommandAiState>`
(`command: Option<UnitCommandId>`, `stances: StanceBits`, `attack_target:
Option<AttackTarget::{Building(packed pos), Unit(seq id)}>`).

**Changes.**
1. `gdext/input/desktop.rs`: make `command_units` `pub`; add `pub fn
   retain_selection_types(&mut self, keep: Option<i32>, drop: Option<i32>)`
   that filters `controller.state.selected_units` by unit type using
   `self.selectable` (has `type_id`) and emits the `selection_changed` the
   same way other selection edits do (find the emitter with `grep -n
   "selection_changed" gdext/input/mod.rs`).
2. `gdext/input/mod.rs` #[func]s. **Signatures are fixed by G08 T11 change 4**
   (G03 runs first, so G03 defines them; G08 T11 must reuse them, not redefine):
   ```rust
   /// `Call.setUnitCommand(player, selected ids, command)`; `command` = UnitCommand
   /// name ("move", "mine", ...). False when nothing is selected or the name is unknown.
   #[func] pub fn set_unit_command(&mut self, command: GString) -> bool
   /// `Call.setUnitStance(player, selected ids, stance, enabled)` by stance name ("stop", "holdfire", ...).
   #[func] pub fn set_unit_stance_named(&mut self, stance: GString, enabled: bool) -> bool
   ```
   Body: resolve the name to its id through the host content
   (`content.unit_command(id)` / `unit_stance(id)` by `name`), then
   `self.bridge.command_units(id)` (make `DesktopBridge::command_units` `pub(crate)`)
   / the existing `set_unit_stance(id, enabled)` path. Also add
   `select_only_type(type_id: i64)`, `deselect_type(type_id: i64)`.
   The panel passes command/stance NAMES (from `command_panel_json`), not ids.
3. `gdext/sim_host.rs` #[func] `command_panel_json(ids_json: GString) ->
   GString`:
   `{"types":[{"type":id,"name":"nova","count":3}],"commands":[{"id":1,"name":"repair","icon":"…","active":true}],"stances":[…]}`
   — types in content-id order (Java iterates `counts` by id); commands/stances
   as the union in that type order; `active` = any selected unit's
   `CommandAiState.command == id` / stance bit set.
   Attack targets: G01 §3.1 provides `#[func] SimHost.command_targets_json()
   -> [{"id":seq,"x":wx,"y":wy,"attack":bool}]` (G01 T16). Use it (filter to
   the selected ids, draw only `attack == true`). If it is not merged yet,
   add the reticle code behind a `has_method("command_targets_json")` check
   and record "reticle waits for G01 T16".
4. `scenes/ui/fragments/placement_fragment.tscn`: add `Panel/Frame/CommandPanel`
   (`PanelContainer`, visible false) after `Layout`, child `Rows`
   (VBoxContainer). Style from atlas `pane-2` like `_setup_top_style` does
   for the top table (12 px ninepatch margins).
5. `ui/fragments/placement_fragment.gd`: in `_process` poll
   `Input.command_mode()`; on change toggle `Layout.visible` /
   `CommandPanel.visible` and set `CommandPanel.custom_minimum_size.x =
   Layout.size.x`. Rebuild `Rows` on `selection_changed` and every 250 ms when
   the JSON differs: title `MindWidgets.markup("@commandmode.name")`
   centred; a 4 px `ColorRect` accent (`#ffd37f`) with 20 px side margins;
   types grid (`GridContainer columns 7`): `TextureRect`
   `unit-<name>-ui` 32x32 + count label bottom-right (outline font);
   `gui_input` left → `select_only_type`, right → `deselect_type`; commands
   grid (columns 6) of 50x50 toggle buttons (`MindWidgets.icon_button(icon,
   "")`, `set_pressed_no_signal(active)`) → `set_unit_command(name)`; stance
   grid → `set_unit_stance(id, not active)`; or the `@commandmode.nounits`
   label (lightgray). All code-built nodes carry `# code-instantiated:`.
   Map command `icon` names (Java `Icon` keys like `"move"`, `"modeAttack"`)
   through `MindWidgets.icon_button`; if an icon is missing it falls back to
   `cancel` (Java `Icon.icons.get(icon, Icon.cancel)`).
6. `gdext/render/command_overlay.rs`: draw `Drawf.target(tx, ty, 6, 1,
   Pal.remove)` (T1 helpers) for rows with an attack target and
   `draw_target`, rotation `t * 1.5` degrees.

**Tests.** `command_panel_json` core logic as a pure fn in `sim_host.rs`
(`fn command_panel(types: &[(i32, &UnitTypeDef)], states: &[Option<&CommandAiState>], content) -> serde_json::Value`)
tested with two daggers + one nova: types `[dagger×2, nova×1]` in id order,
commands = union of dagger's then nova's lists (read both from the test
registry) without duplicates.

**Verify in-engine.** Ground Zero: spawn two team-1 daggers
(`spawn_unit` with `"team":1`), `$I.toggle_command_mode()`, box-select them
through `godot_input` like the previous T16 (see its summary), screenshot:
panel with a dagger icon and `2`. Click the second command button with
`godot_input` → `$H.unit_debug_json()` shows the command change.

**Pitfalls.** `command_mode()` flips while placing (desktop.rs tests
`command_mode_ignored_while_placing`); the panel must follow `command_mode()`
only. Selection ids are `EntitySeq` values.

**Commit.** `Fixes PAR-0095: command-mode panel with unit types, commands and stances; attack reticle`

### T11 — Border / map-area darkness in the world and on the minimap (previous T19 remainder)

**Symptom / acceptance.** The map edge has no 2-tile fade to black, and once
G04 plumbs `Rules.limitMapArea` into `WorldGrid.limit_area`, everything
outside the play rectangle must be black in the world and on the minimap
(Ground Zero in Java: black outside the play area, see
`runs/20261012-overview/java-groundzero-ingame.png` minimap). After: with
`limit_area = None` the outermost 2 tiles fade (darkness 4 at the edge tile,
2 one tile in); with a limit rect, outside is fully black and the rect edge
fades. Works as soon as `grid.limit_area` is set, with no further change.

**Java.**
* `J/core/World.java:484-503` `getDarkness` border half (ported in
  `core/world/darkness.rs::border_darkness`, tests exist).
* `J/graphics/BlockRenderer.java:309-342` `updateDarkness`: FBO cleared to
  black when `limitMapArea` else white; white `Fill.crect(limitX, limitY,
  limitW, limitH)`; for tiles inside `Rect(limitX, limitY, limitW - 1,
  limitH - 1)` (all tiles without a limit): `darkness = world.getDarkness`;
  if `> 0`: grey level `1 - min((darkness + 0.5)/4, 1)`.
  (`core/render/shadow.rs::build_darkness_map(width, height, limit:
  Option<[i32;4]>, darkness_at)` already implements exactly this.)
* `J/graphics/MinimapRenderer.java:390` colour `mul(1 - clamp(getDarkness/4))`.
* `World.getDarkness` also adds the generated-sector polygon falloff
  (World.java:505-526, only `sector.preset == null`) — **deferred** with
  generated sectors (G05 PAR-0112).

**Port today.** `gdext/render/shadow.rs` ~135:
`build_darkness_map(width, height, None, |x, y| get_static_darkness(world,
content, x, y))`; rebuilt in `ShadowRenderer::update` only when its change
detection fires (read the top of `update()`; it keys on `world_revision`).
`gdext/render/minimap.rs::tile_color` ~263 uses
`darkness_multiplier(get_static_darkness(..))`. `WorldGrid.limit_area:
Option<(i32,i32,i32,i32)>` (`core/world/mod.rs:152`) is never set outside
tests (G04 owns that plumbing). `Rules.border_darkness: bool` default true
(`core/io/json/rules.rs:674,816`).

**Changes.**
1. `gdext/sim_host.rs`: Rust-only `pub fn border_darkness_enabled(&self) ->
   bool` = live `CampaignRuntime.session.rules.border_darkness`, else
   `last_sector_rules().map(|r| r.border_darkness)`, else `true`.
2. `gdext/render/shadow.rs`: pass `world.limit_area.map(|(x,y,w,h)| [x,y,w,h])`
   as `limit` and use `|x, y| get_static_darkness(world, content, x, y)
   .max(border_darkness(world, border, world.limit_area, x, y))`. Add
   `(limit_area, border)` to the change key so a later `limit_area`
   assignment rebuilds the map (store `last_limit: Option<(..)>`,
   `last_border: bool`).
3. `gdext/render/minimap.rs::tile_color`: same `max(...)`; and when
   `limit_area` is `Some` and the tile is outside the rect, return opaque
   black (Java: the minimap texture is drawn darkened by getDarkness ≥ 4 →
   multiplier 0). Rebuild the minimap texture when `limit_area` changes
   (find the provider's full-rebuild trigger and add the same key).

**Tests.** `core/render/shadow.rs` tests: `darkness_map_with_border`: 6x6
map, no limit, border on, static 0 → texel (0,3) grey `1 - min(4.5/4,1) =
0`, (1,3) `1 - 2.5/4 = 0.375`, (2,3) white `1.0`. With limit `[1,1,4,4]`:
texel (0,0) black (outside), (2,2): edgeDst = min(2-1, 2-1, -(2-4), -(2-4)) =
1 → darkness `(2-1)*2 = 2` → `1 - 2.5/4 = 0.375`.

**Verify in-engine.** Ground Zero: screenshot the map's south-west corner
(`$C.center_on_tile(2, 2)`): the last two tile rows/cols fade to black
(before: hard edge). Then simulate the limit:
`$G --eval "return $H.tile_debug(129,55)"` shows `limit_area`; if still
`null`, the limit half is headless only until G04 lands (state it).

**Pitfalls.** `border_darkness` returns values > 4 outside; the map builder
clamps via `min(.., 1)`. The minimap and world use the same helper — do not
duplicate the formula.

**Commit.** `Gameplay G03 T11: border and map-area darkness in the world and minimap`

---

### T12 — Sector rules drive the renderer; real light pass (block, core, unit and illuminator lights)

**Symptom / acceptance.** (a) `MindWorldRenderer.rules` is never fed:
`set_rules_lighting`, `set_fog_enabled`, `set_rules_env` have no caller, so a
sector with `rules.lighting` (night sectors) renders fully lit and env
renderers never switch. (b) The light pass is a placeholder (white circles of
`size*16` px for `emit_light` blocks only, ambient hard-coded, additive
blend). (c) Illuminators ignore their configured colour (handoff T10 of the
building plan). After: a sector whose rules have `lighting = true` shows the
ambient darkness from `rules.ambientLight`, with coloured light circles for
cores (accent), illuminators (configured colour, radius `lightRadius *
min(smoothTime, 2)`), unit lights (`lightRadius = max(60, hitSize*2.3)`,
colour `Pal.powerLight`, opacity 0.6 when `rules.unitLight`) and bullets with
`lightRadius > 0`.

**Java.**
* `J/core/Renderer.java` draw: `if(state.rules.lighting && drawLight)
  Draw.draw(Layer.light, lights::draw)`; `drawLight = settings.getBool(
  "drawlight", true)` (Renderer.java:175). The port has `set_draw_light`
  (#[func]); keep it, default true.
* `J/graphics/LightRenderer.java:32-46` `add(x, y, radius, color, opacity)`:
  queues a circle `(x, y, color.rgb, a = opacity, radius)`; `draw()` (:126-163):
  clear the buffer (1/4 screen res), blend `max` on alpha, draw each circle as
  the `circle-shadow` region of size `radius*2` tinted the circle colour;
  then `Shaders.light.ambient = rules.ambientLight; buffer.blit(Shaders.light)`
  with **normal alpha blending**; `light.frag`:
  `gl_FragColor = clamp(vec4(mix(ambient.rgb, color.rgb, color.a),
  ambient.a - color.a), 0, 1)` (`core/assets/shaders/light.frag`).
* Sources (enabled only when `rules.lighting`):
  * `Block.lightRadius = 60`, `lightColor = white`; `BlockRenderer.drawBlocks`
    lights loop (BlockRenderer.java ~836-860): `build.drawLight()`, or for
    environment `emitLight` blocks/floors `drawEnvironmentLight`:
    `Drawf.light(worldx, worldy, lightRadius, lightColor, lightColor.a)`.
  * `CoreBlock.java:158` `lightRadius = 30 + 20*size`; `:739-741` `drawLight`:
    `Drawf.light(x, y, lightRadius, Pal.accent, 0.65 + absin(20, 0.1))`.
  * `LightBlock.java:40,105,126-129`: `lightRadius = radius*2.5`; the
    `illuminator` (Blocks.java, `new LightBlock("illuminator")`) sets
    `radius = 140` → `lightRadius = 350`, `brightness = 0.75`;
    `smoothTime = lerpDelta(smoothTime, timeScale, 0.1)` (1 without
    overdrive), `drawLight`: `Drawf.light(x, y, lightRadius*min(smoothTime,2),
    color, brightness * efficiency)`.
  * `UnitType.java:954, 1800-1804`: `lightRadius = max(60, hitSize*2.3)` when
    unset; `drawLight`: if `lightRadius > 0 && rules.unitLight`:
    `Drawf.light(x, y, lightRadius, lightColor (powerLight), lightOpacity 0.6)`.
  * `BulletType.drawLight` (708-711): `Drawf.light(b, lightRadius, lightColor,
    lightOpacity)` when both > 0; defaults `lightRadius = -1` → `init`:
    `max(18, hitSize*5)` (BulletType.java:862-863), `lightOpacity = 0.3`,
    `lightColor = Pal.powerLight` (379-383). BulletDef has no light fields:
    use these defaults for every bullet (document; per-bullet overrides
    deferred).
  * GenericCrafter/PowerGenerator/DrawFlame lights — **deferred** with T15
    (they come from drawers).

**Port today.** `gdext/render/light.rs` (230 lines): world-sized CPU alpha map
`stamp_circle`, monochrome, `u_ambient` hard-coded `(0.01,0.01,0.04,0.99)`,
rebuilt only on `world_revision` change; `client/shaders/light.gdshader` uses
`render_mode unshaded, blend_add` (wrong: Java blits with normal blending).
`RulesRenderView` (`core/render/rules.rs:3-46`) holds lighting/ambient/fog/
env; nothing converts `Rules` into it. `light_color(x,y)` (#[func],
sim_host.rs ~895) returns the illuminator colour. `UnitTypeDef.light_radius,
light_color, light_opacity` exist; `BlockDef` has no `light_radius/
light_color` (only `emit_light`).

**Changes.**
1. `core/render/rules.rs`: `impl RulesRenderView { pub fn from_rules(r:
   &crate::io::json::Rules) -> Self }` mapping `static_fog, fog,
   dynamic_color, static_color, lighting, ambient_light, env, limit_map_area,
   limit_rect` (`ColorHex(Rgba)` → `[r,g,b,a]`). Test: default `Rules` →
   `lighting false`, `ambient_light == Rules::default().ambient_light`.
2. `gdext/render.rs::frame`: at the start, pull live rules each frame cheaply:
   add a Rust-only `MindSimHost::render_rules_revision() -> u64` (bump a
   counter whenever `sector_rules` is replaced in `load_sector`, and compare
   the CampaignRuntime rules' `lighting/ambient/fog/env` fields directly) —
   Do this: every 30 frames call `host.bind().live_rules().map(RulesRenderView::from_rules)`
   (`live_rules()` = CampaignRuntime session rules else `last_sector_rules()`;
   add it to sim_host.rs) and assign `self.rules` when different — unless a
   new field `rules_override: bool` is true. The existing `set_rules_lighting`,
   `set_fog_enabled`, `set_static_fog`, `set_rules_env` #[func]s set
   `rules_override = true` (debug override); add `#[func] clear_rules_override()`.
3. `client/shaders/light.gdshader`: `render_mode unshaded, blend_mix;`.
4. `gdext/render/light.rs`: keep the CPU light-map approach but make it a
   camera-sized RGBA map (see Pitfalls) instead of a world-sized alpha map: `LightAccumulator` already stores colour per circle
   (`core/render/light.rs`, read it); write `rgb = max-blend of colour *
   falloff`, `a = max(alpha)` per texel (Java uses `Gl.max` on alpha and
   normal blend on rgb — approximate with max on both, document), set
   `u_ambient` from `rules.ambient_light`. Rebuild every 4 frames when
   lighting is on (units move), restricted to the camera rect (grow 200 px).
   Sources, collected in `update()` through a new Rust-only seam
   `MindSimHost::light_sources(rect) -> Vec<(f32 x, f32 y, f32 radius, [f32;4] rgba, f32 opacity)>`:
   cores (`CoreBlock` kind, radius `30 + 20*size`, accent, `0.65 +
   absin(t,20,0.1)`), illuminators (`LightBlock` kind: radius 350, colour =
   `LightBehavior` colour (same read as `light_color`), opacity `0.75 *
   efficiency`), static `emit_light` blocks (radius 60 = `Block.lightRadius`
   default, white, alpha 1 — the per-block `lightRadius/lightColor`
   overrides are missing from `BlockDef`; list the vanilla overrides you find
   with `grep -n "lightRadius\|lightColor" J/content/Blocks.java` in the
   commit body as remaining), units when `rules.unit_light` (`core/io/json/rules.rs:660`, default true),
   and bullets (defaults above).
   Circle falloff: texel alpha = `circle-shadow` profile; use the existing
   `stamp_circle` radial falloff.

**Tests.** `core/render/light.rs`: `ambient_mix_matches_shader`: pure fn
`light_composite(ambient: [f32;4], light: [f32;4]) -> [f32;4]` = Java frag:
ambient `(0.01,0.01,0.04,0.99)`, light `(1,0.83,0.5,0.6)` → rgb `mix(amb,
light, 0.6)` = `(0.604, 0.502, 0.316)`, a = `0.99 - 0.6 = 0.39`.

**Verify in-engine.** Find a lighting sector: `grep -l '"lighting":true'`
won't work on binary maps — use the rules seam: after loading a sector,
`$G --eval "return $MC.get_rules_json().find('\"lighting\":true') >= 0"`.
If no unlocked sector has lighting, force it:
`$G --eval 'get_node("/root/Spine/World/Renderer").set_rules_lighting(true, 0.01, 0.01, 0.04, 0.99); return true'`
(the override flag of step 2 keeps it; `clear_rules_override()` after).
Screenshot: dark ambient, accent glow around the core, coloured illuminator
(`place_block` + `configure_building(..,"int", 0xff0000ff)`).

**Pitfalls.** The world-sized texture for a 500x500 map is 1 MB per rebuild:
restrict stamping to the camera rect and only re-upload that region
(`Image::blit_rect` or rebuild a camera-sized texture positioned at the
camera — prefer the camera-sized texture: quad at the view rect, size
`view.w/4 x view.h/4` texels like Java's 1/4 scale buffer).

**Commit.** `Gameplay G03 T12: sector rules drive the renderer; coloured core/illuminator/unit lights`

### T13 — Block inventory popup presentation (BlockInventoryFragment)

**Symptom / acceptance.** Tapping a core/container shows a column of text
buttons `copper x10` plus an `X` button, frozen at open time. Java shows a
`Tex.inventory` panel at the building's top-right corner with a 3-column grid
of 40x40 item icons, each with its live amount bottom-left (`1.2k` style),
tapping takes the whole stack, holding takes one every 20 ticks, and the panel
follows the building on screen and closes when the building is gone, empty for
120 ticks, or tapped again. After: the port matches that.

**Java.** `J/ui/fragments/BlockInventoryFragment.java` (transcribed above in
the audit): `showFor(t)` toggles off when the same building; needs `build.items
!= null && total > 0`; `holdWithdraw = 20`, `holdShrink = 120`
(BlockInventoryFragment.java:27-28); grid `cols = 3`, cell `size(8*5 = 40)
pad(4)`, `margin(4)`; image stack = `item.uiIcon` + bottom-left label
`round(amount)`: `>= 1_000_000` → `(int)(f/1e6) + "[gray]" + UI.millions`,
`>= 1000` → `(int)(f/1000) + UI.thousands`, else `(int)f`
(`UI.thousands`/`UI.millions` are the bundle values `unit.thousands = k`,
`unit.millions = mil`, bundle.properties:1404-1405);
`canPick = player alive && unit.acceptsItem(item) && !paused &&
player.within(build, itemTransferRange)`; click → `takeItem(build.items.get(item))`
→ `amount = min(requested, unit.maxAccepted(item))` → `Call.requestItem`;
hold ≥ 20 ticks → `takeItem(1)` repeatedly; position: `mouseScreen(build.x +
size*4, build.y + size*4)` aligned top-left (`Align.topLeft`; bottomLeft for
`diagonalConfigInventory` blocks); `rebuild` when an item appears or has been
absent ≥ 120 ticks; hide when `emptyTime >= 120`, building invalid, or the
grid is empty; open/close scale animation 0.07/0.06 s pow3Out (optional).

**Port today.** `ui/fragments/block_inventory_fragment.gd` (57 lines,
`open_at(tile, items_json)`, `set_items`, `_take` → `MindInput.transfer_item
(x, y, item, amount, false)`); opened by `MindInput` on
`Effect::OpenBlockInventory {x, y, screen, items}` (`gdext/input/mod.rs`
~540-556) which sets `position` to the screen point. Scene
`scenes/ui/fragments/block_inventory_fragment.tscn` (`Panel/Grid`).
Live amounts: `MindSimHost.building_items_json(x, y)` (#[func]).

**Changes** (GDScript + scene only):
1. Scene: `Panel` style = atlas `inventory` ninepatch (`Tex.inventory`;
   region `inventory` exists in the atlas; margins 4), `Grid.columns = 3`, separation 4.
2. Script:
   * `open_at(tile, items_json)`: if visible and `tile == _tile` → `hide_panel()`
     and return (toggle). Parse; if empty → do not show.
   * cells: `# code-instantiated` `Control` 40x40 with a `TextureRect`
     (`MindWidgets.icon_texture("item-%s-ui" % name)`, `EXPAND_IGNORE_SIZE`,
     keep aspect) and a bottom-left outline `Label` with `_round(amount)`.
   * `_process(delta)`: every 0.1 s re-read `building_items_json(_tile)`;
     update labels; track `_empty_time` (ticks, `delta*60`) and
     `_absent_time[item]`; rebuild when an item appears or one has been
     absent ≥ 120; hide when `_empty_time >= 120` or the tile has no building
     (`tile_debug(x,y).has_build == false`).
   * position each frame: `MindCamera2D` has `tile_to_screen(x, y)`
     (camera.rs:394) but no `world_to_screen`; add `#[func] world_to_screen(
     wx: f64, wy: f64) -> Vector2` next to it (same canvas-transform math as
     `tile_to_screen`, without the `*8`) and use it with the building's
     top-right corner `(cx + size*4, cy + size*4)` in world px; set `position` so the panel's top-left
     sits at that screen point.
   * input: `gui_input` on a cell: left press → `_holding = item`,
     `_hold_time = 0`, `_held = false`; in `_process` while holding:
     `_hold_time += delta*60`; when ≥ 20 → `_take(item, 1)`, `_hold_time = 0`,
     `_held = true`; left release → if not `_held` → `_take(item, amount)`;
     clear holding. `transfer_item` already clamps to what the unit accepts
     (check `DesktopBridge::transfer_item`; if it does not clamp, pass the
     amount and let the sim reject the excess — note it).
   * remove the `X` button (Java has none; tapping the building again or
     elsewhere closes it — G08 owns the tap-elsewhere rule).
   * `_round(f)`: as Java with the bundle keys found above.

**Tests.** GDScript only; add `_round` cases to the parse gate run by calling
nothing — no unit test framework for GDScript here. Headless: none.

**Verify in-engine.** Ground Zero: `$H.debug_add_core_items("copper", 1500)`,
tap the core with `godot_input` left click at `tile_to_screen(129,55)`:
screenshot shows a 3-column icon grid near the core's top-right with
`1k` under copper; hold the click 1 s (press, sleep 1, release): the
amount drops by ~3 and the alpha carries copper (`player_unit_status` /
`unit_debug_json`).

**Pitfalls.** The item transfer range check is sim/input-side (G08); the
fragment only disables the hand cursor. Do not call `building_items_json`
every frame.

**Commit.** `Gameplay G03 T13: block inventory popup with live icon grid, hold-to-take and follow`

---

### T14 — Overlay top: hover range (`drawSelect`), configure square, enemy core build radius, drop-zone circle, off-screen enemy indicators

**Symptom / acceptance.** Hovering your duo shows no range; the building
being configured has no accent outline; while placing, enemy cores show no
"no-build" radius; approaching the enemy spawn shows no dashed drop-zone
circle; enemies off-screen give no edge indicators around the player. After:
all five match Java.

**Java.** `J/graphics/OverlayRenderer.java` `drawTop` (transcribed in the
audit, file lines ~60-250):
* indicators (if `!player.dead()` and setting `indicators` (default true) and
  `!rules.fog`): for each unit not local, `team != player.team()`, outside the
  camera rect scaled 0.9: `v = (unit - player).setLength(indicatorLength =
  14)`; `Lines.stroke(1, unit.team.color); Lines.lineAngle(player.x + v.x,
  player.y + v.y, v.angle(), 3)`.
* `if(input.config.isShown()) config.getSelected().drawConfigure()`:
  `BuildingComp.java:1732-1737`: `color(accent); stroke(1);
  Lines.square(x, y, size*4 + 1)`.
* `buildFade = lerpDelta(buildFade, isPlacing ? 1 : 0, 0.06)`; stroke
  `buildFade * 2`; if `buildFade > 0.005` and not
  `rules.polygonCoreProtection` (default false): for each enemy core with
  `br = rules.buildRadius(core.team)` (`= teamRule.protectCores ?
  enemyCoreBuildRadius (400) + extraCoreBuildRadius : 0`, Rules.java:294-296)
  `> 0` and overlapping the camera: `color(darkGray); Lines.circle(core.x,
  core.y - 2, br); color(lerp(accent, core.team.color, 0.5 + absin(t, 10,
  0.5))); Lines.circle(core.x, core.y, br)`.
* spawns: `stroke(2); color(lerp(gray, lightGray, absin(t, 8, 1)))`; for
  each ground spawn within `dropZoneRadius + spawnerMargin (88)` of the
  player: `alpha = clamp(1 - (dst - dropZoneRadius) / 88)`;
  `Lines.dashCircle(spawn.worldx, spawn.worldy, dropZoneRadius)`.
* hover (`input.block == null && !scene.hasMouse()`): building under the
  cursor of the player team: `build.drawSelect()`; implement for
  `BaseTurret` (`Drawf.dashCircle(x, y, range, team.color)`),
  `MendProjector`/`OverdriveProjector` (`Drawf.dashCircle(x, y, range,
  baseColor)`), `PowerNode` (`Lines.stroke(1); Drawf.circles(x, y,
  laserRange*8)` in accent — PowerNode.java:445-453); skip the rest (list
  deferred in the commit). `drawDisabled` (`!enabled && drawDisabled`):
  scarlet 0.8-alpha `Icon.cancel` 6x6 at the centre — include it
  (icon `cancel` from the UI icon atlas via `MindWidgets`? It is a UI font
  icon; draw a red X with two lines instead and note the deviation).

**Port today.** `gdext/render/overlays.rs` (core edges only, `set_core_edges`
never called); `core/render/overlay.rs` has indicator geometry
(`INDICATOR_LENGTH = 14`, `spawner_margin`). Block config fragment:
`/root/Spine/Ui/UiRoot/OverlayLayer/block_config` (`.visible`, `._config`
dict — check it contains the tile `x`,`y`; if not, add them where the
fragment is opened in `gdext/input/mod.rs` `Effect::OpenBlockConfig`).

**Changes.**
1. `gdext/sim_host.rs` Rust-only seam `overlay_view(&self) -> OverlayView`:
   `{ player: Option<(f32,f32)>, player_team: u8, enemy_units: Vec<(f32,f32,u8)>,
   enemy_cores: Vec<(f32,f32,u8, f32 /*build radius*/)>, spawns: Vec<(f32,f32)>,
   drop_zone_radius: f32, fog: bool }` from the live rules/teams/runtime.
   `build radius`: `rules.teams.get(team).map_or(true, |r| r.protect_cores)`
   → `enemy_core_build_radius + extra_core_build_radius`.
2. Rust-only `hover_select(&self, x: i32, y: i32) -> Option<SelectRange>`
   `{x, y, range, color: u32, kind: Dash|Circles, size: i32, enabled: bool}` for
   the player-team building at the tile: turrets → `TurretState.config.range`
   (sim_host `turret_draw` reads `TurretState`), projectors → the T2 const
   table (`range` + 0 phase heat), power nodes → laser range table.
3. New `gdext/render/overlay_top.rs` `OverlayTop` (§0 template) under
   `Layer::OverlayUi`: `process` computes `build_fade` (placing = an input
   block selected, from `MindInput.placement_view()` of T2); `draw` executes
   the five parts in Java order: indicators, configure square, core radius,
   spawn circles, hover select. Mouse tile: `MindInput`'s cursor (T2 view);
   skip the hover when `get_viewport().gui_get_hovered_control()` has
   `MOUSE_FILTER_STOP` (same rule as placement_fragment.gd `_mouse_tile`).
   `darkGray = (0.25,0.25,0.25)`, `lightGray = (0.75,0.75,0.75)`,
   `gray = (0.5,0.5,0.5)` (Arc `Color.darkGray/lightGray/gray`).
4. Delete nothing in `overlays.rs`; core edges stay unused (polygon core
   protection is off by default).
5. **Generic hooks for other areas (G06 T12 must use these):** in
   `overlay_top.rs` expose the dispatch points as two Rust enums handled in
   one place:
   ```rust
   /// `Building.drawConfigure` extension point: G03 draws the generic accent square
   /// (BuildingComp.java:1732-1737) then calls `config_extra(kind, ..)`.
   pub(crate) enum ConfigOverlay { Generic, PowerNodeLinks, BridgeLink, Logic /* G06 T12 */ }
   /// `Building.drawSelect` extension point (hover of a player-team building).
   pub(crate) enum SelectOverlay { Range(SelectRange), MessageText /* G06 T12 */, LogicTag /* G06 T12 */, None }
   ```
   Data comes from one Rust-only seam `MindSimHost::config_overlay(x, y) ->
   ConfigOverlay` / `select_overlay(x, y) -> SelectOverlay`; G06 adds its
   variants' data and drawing as functions in `gdext/render/logic_overlay.rs`
   called from `OverlayTop::draw` (no second overlay node). The configured
   building comes from a new Rust accessor `MindInput::config_selected() ->
   Option<(i32, i32)>` (the same accessor G06 T12 asks for — add it here).
   Generic drawConfigure extras implemented here (they replace the generic
   square for these kinds):
   * `PowerNode.drawConfigure` (PowerNode.java:458-480):
     `Drawf.circles(x, y, size*4 + 1 + absin(t, 4, 1))` (accent); if
     `drawRange` (true for vanilla nodes): `Drawf.circles(x, y, laserRange*8)`
     and for every building in `tile ± (laserRange + 2)` that is linked to
     this node: `Drawf.square(link.x, link.y, link.size*4 + 1, Pal.place)`
     (rotation 45). Linked set = this node's `PowerModule.links`.
   * `ItemBridge.drawConfigure` (ItemBridge.java:282-296):
     `Drawf.select(x, y, size*4 + 2, accent)`; for `i in 1..=range`, each of
     the 4 directions: if the tile at `i` steps is a valid link target
     (same block, same team): `Drawf.select(ox, oy, size*4 + 2 + (linked ? 0
     : absin(t, 4, 1)), linked ? Pal.place : Pal.breakInvalid)`.
     `Drawf.select(x, y, r, c)` (Drawf.java:418-424): stroke 3 gray
     `Lines.square(x, y, r + 1)` then stroke 1 `c` `Lines.square(x, y, r)`;
     Arc `Lines.square(x, y, r)` = axis-aligned square of half-width `r`.
     Bridge `range` 4 (`bridge-conveyor`, Blocks.java) — read it from the
     port's bridge config (`grep -n "range" core/world/blocks/distribution/item_bridge.rs`).

**Tests.** `core/render/overlay.rs`: `indicator_segment`: player (0,0), enemy
(100,0) → segment from (14,0) to (17,0); `spawn_alpha`: drop radius 300,
distance 344 → `clamp(1 - 44/88) = 0.5`.

**Verify in-engine.** Ground Zero: place a duo (`place_block`), clear the
selection (`select_block_by_name("air")`), warp the mouse onto it →
dashed team-coloured circle r=160. Drop-zone circle and enemy core radius:
headless only on Ground Zero (the player cannot be moved to the spawn by a
seam, and Ground Zero has no enemy core) — covered by the `spawn_alpha`
test; if an unlocked sector with an enemy base exists in the imported
profile (check `$MC.get_sector_state()` for `enemyCores > 0` after loading
candidates 23, 41, 97), screenshot it while a block is selected.
Indicators: spawn a team-2 dagger 60 tiles away from the player, screenshot:
a short team-coloured line 14 px from the alpha pointing at it.

**Pitfalls.** Every part reads sim state; collect all data in one host bind,
then draw. Indicators use the camera rect scaled by 0.9 around the camera
centre (`visible_rect` from status.rs, shrunk).

**Commit.** `Gameplay G03 T14: hover range, configure outline, enemy core radius, drop-zone circle, enemy indicators`

---

### T15 — Wire the ported `DrawBlock` descriptors into the live block pass (crafter flames, glows, pistons…)

**Symptom / acceptance.** Silicon smelters, kilns, pulverizers, combustion
generators, cultivators etc. are static icons in the live game although
`core/render/draw_desc.rs` ports their `DrawBlock` chains (only the headless
`render list` uses them). After: a working silicon smelter shows its flame
glow pulsing (`DrawFlame`), a pulverizer its spinning rotator, a combustion
generator its glow, matching Java.

**Java.** `J/world/draw/*.java` (each descriptor cites its Java file in
`draw_desc.rs`); `GenericCrafter.GenericCrafterBuild.draw()` →
`drawer.draw(this)` with `warmup()`, `progress()`, `totalProgress()`.

**Port today.** `mind_core::render::{vanilla_chain(name) ->
Option<Vec<DrawDesc>>, execute_draw_desc, draw_desc::execute_extras(desc,
name, state, ids, out)}`; `DrawState::new(x, y, rotation, size)` plus fields
`warmup, progress, total_progress, time, id, power_status, liquid_amount,
liquid_capacity, liquid_color, heat, ...` (draw_desc.rs:163-202);
`execute_extras` skips the base region and emits only the extra layers
(`DescDraw{z, blend, cmd}`). Emitted command kinds (draw_desc.rs:43-150):
`Sprite{region: RegionId, x, y, w, h, ox, oy, rot, color, blend}`,
`Fill{Circle, params [x,y,r,..]}`, `Fill{Poly, [x,y,sides,r,rot,..]}`,
`Shape{CircleOutline, [x,y,r,stroke,..]}`, `Shape{Poly,
[x,y,sides,r,rot,stroke,..]}`, `Lines{Line, [x1,y1,x2,y2,..], stroke}`;
region names via `RegionIdTable::name(id)` (`core/render/ids.rs:52`).
`vanilla_chain` covers: silicon-smelter, kiln, surge-smelter,
plastanium-compressor, phase-weaver, cryofluid-mixer, melter, separator,
disassembler, spore-press, pulverizer, combustion-generator,
steam-generator, cultivator. Crafter state: `CrafterState {progress, warmup,
total_progress}` (`core/entities/comp/building.rs:129-140`).

**Changes.**
1. `gdext/sim_host.rs` Rust-only `pub fn block_draw_state(&self, x: i32, y:
   i32) -> Option<DrawStateInput>` `{warmup, progress, total_progress,
   power_status, liquid_amount, liquid_capacity, liquid_color: u32, id: i32}`
   from `CrafterState` (generators: read `GeneratorState` in
   `core/world/blocks/power/generator.rs:137` for `productionEfficiency` →
   use it as `warmup`; document), `PowerModule.status`, `LiquidModule`
   (max entry, colour from `content.liquid(id).color`), id = entity index.
2. New `gdext/render/block_extras.rs` `BlockExtras` (§0 template), **two**
   nodes added to the `Layer::Block` band after `BlockRenderer`'s nodes (one
   normal, one with a shared `CanvasItemMaterial` `BLEND_MODE_ADD` for
   `Blend::Additive`). `process`: `queue_redraw` every frame when any chain
   block is visible. `draw`: for each visible building (use
   `mind_core::render::scan::visible_blocks` with the camera view; dedupe
   multiblocks like blocks.rs) whose `vanilla_chain(&def.name)` is Some:
   build `DrawState` (`time = ticks_now()`), call `execute_extras` for each
   desc into a `Vec<DescDraw>` (one `RegionIdTable` kept on the node), sort by
   `z` (stable), then interpret: `Sprite` → `canvas::draw_region` with
   texture from `RegionCache` by `ids.name(region)`, origin `(ox, oy)` (the
   descriptors use 0.5 — assert and warn otherwise); `Fill Circle` →
   `draw_circle`; `Fill Poly` → `draw_colored_polygon(poly_ring)`; `Shape
   CircleOutline` → `draw_arc(.., 0, TAU, 32, color, stroke)`; `Shape Poly`
   → `canvas::poly_line`; `Lines Line` → `draw_line(.., stroke)`. Route
   `Blend::Additive` commands to the additive node (two passes: each node
   draws only its blend).
3. Remove nothing in blocks.rs; the composed `-full` icon stays the base.
   For chains whose base is NOT the composed icon (e.g. pulverizer's rotator
   is in the `-full` icon and also drawn as an extra → doubled static
   rotator), check each of the 14 blocks once in-engine; where the `-full`
   icon already contains a part that the chain animates, draw `def.region`
   instead of `-full` for that block in blocks.rs (add the block names to a
   `const ANIMATED_CHAIN_BASE: &[&str]` with a comment per entry).

**Tests.** `gdext/render/block_extras.rs` pure interpreter fn tested with a
hand-built `Vec<DescDraw>` (one circle, one additive sprite): returns two
normal and additive buckets in z order. Also a `mind-core` test that
`execute_extras(silicon-smelter chain, state warmup 1, time 0)` emits at
least one `Fill Circle` (flame) — add in `draw_desc.rs` tests if not present.

**Verify in-engine.** Ground Zero: `$H.place_block(120,62,"silicon-smelter")`,
power + feed it (sandbox `power-source` 1x1 adjacent via `place_block`,
`debug_add_building_items(120,62,"coal",10)`, `("sand",10)`), step 120,
screenshot twice 0.3 s apart: flame glow present and changing. Compare with a
Java smelter frame if one exists under `runs/*ref*` (search
`ls runs | grep ref`).

**Pitfalls.** Running 14 chains per visible crafter per frame is cheap; do
not run `execute` (the full chain) — the base would double. `time` must be
ticks, not seconds.

**Commit.** `Gameplay G03 T15: live DrawBlock descriptors for crafters and generators`

### T16 — Game-over dialog presentation (3 s delay, loss only, stats rows)

**Symptom / acceptance.** The `restart` dialog opens instantly, also on a
sector capture, and shows no stats. Java opens it 3 s after `GameOverEvent`
(loss in campaign; win/loss outside), never on capture, with animated stat
rows. After: losing the core (sector 98 + destroy the core with enemies, or
`$MC`-side forced game over if a seam exists) opens the dialog ~180 ticks
later with the headline `Sector Lost` formatted with the sector name (Java
`sector.lost` format) and rows `Waves Survived`, `Units Created`, `Enemies
Destroyed`, `Buildings Built`, `Buildings Destroyed`, `Buildings
Deconstructed` (+ play time), each sliding in.

**Java.**
* `J/core/Logic.java:435-440` `gameOver(winner)`: `state.stats.wavesLasted =
  state.wave; state.won = player.team() == winner; Time.run(60*3,
  () -> ui.restart.show(winner))`. Campaign win is `Call.sectorCapture()`
  (Logic.java checkGameState) — no dialog.
* `J/ui/dialogs/GameOverDialog.java` `rebuild` (transcribed in the audit):
  headline `rules.pvp && winner != null ? format("gameover.pvp", winner)
  : isCampaign ? format("sector.lost", sector.name()) : "@gameover"`;
  `highscore` row when `control.isHighScore()` (skip); stats pane
  (`Styles.black3`, margin 13): if `rules.waves` `stats.wave` →
  `wavesLasted`; `stats.unitsCreated`, `stats.enemiesDestroyed`
  (`enemyUnitsDestroyed`), `stats.built`, `stats.destroyed`,
  `stats.deconstructed`, delays 0, 0.05, 0.1, 0.15, 0.2, 0.25; then
  `stats.playtime` + accent play time when a current save exists; buttons:
  campaign `@continue` 170x60 (→ planet), else `@menu` 140x60.
  `StatLabel`: accent `whiteui` background, `outlineLabel` stat name left,
  value right; scale-x 0→1 over 0.3 s pow3Out after `delay`, the value counts
  up (read the rest of `StatLabel` at GameOverDialog.java ~140-175 and
  transcribe the counter speed).

**Port today.** `ui/dialogs/game_over_dialog.gd` (113 lines) renders
`_context.stats` (never provided) as `name: value` labels; opened by
`hud_fragment.gd::_open_game_over(captured)` on `game_over` **or** capture.
Stats live in `CampaignRuntime.session.stats: GameStats`
(`core/io/json/rules.rs:840-859`; only `waves_lasted` is ever written,
`core/game/play.rs:682` — the other counters are a G04/G01 gap, §6).

**Changes.**
1. `gdext/campaign.rs` (`MindCampaign`) #[func] `game_stats_json() -> GString`:
   `{"wave":…, "unitsCreated":…, "enemiesDestroyed":…, "built":…,
   "destroyed":…, "deconstructed":…, "waves":rules.waves}` from the session
   (find how `get_hud_state` reaches the runtime in campaign.rs and do the
   same).
2. `ui/fragments/hud_fragment.gd`: `_check_game_over` opens only on
   `game_over` (drop `captured`), after a 3.0 s timer
   (`get_tree().create_timer(3.0)`; cancel if `game_over` goes false), and
   passes `"stats": JSON.parse_string(MindCampaign.game_stats_json())`.
3. `ui/dialogs/game_over_dialog.gd`: headline as Java (campaign →
   `@sector.lost` formatted; the capture branch is removed); stats rows in
   Java order with bundle keys `stats.wave`, `stats.unitsCreated`,
   `stats.enemiesDestroyed`, `stats.built`, `stats.destroyed`,
   `stats.deconstructed` (check each exists in bundle.properties) on an
   accent background `PanelContainer` 50 px tall, label left / value right;
   animate with a `Tween`: `scale.x` 0→1 over 0.3 s `TRANS_CUBIC EASE_OUT`
   after `0.05*i` s, value counting up (Tween on a float property, format
   int). Buttons unchanged (`@continue` 170, `@menu` 140 — fix the menu
   width to 140).

**Tests.** None in Rust beyond a JSON shape test of `game_stats_json`'s
builder (pure fn over `GameStats`).

**Verify in-engine.** Sector 98 fresh landing (env §4.1), spawn 10 team-2
daggers next to the core and step until `get_hud_state().gameOver` is true,
then wait 3 s: dialog with `Sector Lost` headline and six rows. Capture
path: `$MC.capture_sector()` on another session → no dialog (T9 announce).

**Pitfalls.** Game-over LOGIC is G04's; only presentation changes here.
Use `_t("@stats.wave")` style lookups, never English literals.

**Commit.** `Gameplay G03 T16: game-over dialog after 3 s, loss only, animated stat rows`

---

### T17 — Animated shields and build beams through a screen-texture composite (PAR-0129 residual)

**Symptom / acceptance.** Shields draw Java's non-animated branch (flat
0.09-alpha fill + outline) and build beams draw flat (T3); Java desktop
default `animatedshields = true` composites them through `Shaders.shield`
(bright outline, moving diagonal stripes) and `Shaders.buildBeam`
(stripes). After: force-projector domes and unit bubbles show the shield
shader look; build beams show the stripe pattern.

**Java.** `J/core/Renderer.java` (draw, `animateShields` block):
`Draw.drawRange(Layer.shields, 1, () -> effectBuffer.begin(clear), () -> {
effectBuffer.end(); effectBuffer.blit(Shaders.shield); })` and the same for
`Layer.buildBeam` with `Shaders.buildBeam`. Shaders already ported:
`client/shaders/shield.gdshader`, `buildbeam.gdshader` (uniforms
`u_texture, u_texsize, u_invsize, u_time, u_dp, u_offset`). Under
`animateShields`, `ForceProjector.drawShield` draws `Fill.poly` at
`Layer.shields + 0.001*hit` with colour `team.color.lerp(white, hit)` at full
alpha (the shader sets the 0.18 alpha), and `UnitType.drawShield` draws
`Fill.light` (ForceProjector.java:308-330, UnitType.java:1643-1650).

**Port today.** `gdext/render/shields.rs` draws the non-animated branch.
The previous executor concluded a `CanvasGroup` cannot feed the shader; that
is not correct: Godot 4's `CanvasGroup` renders its children into the back
buffer and a custom `ShaderMaterial` on the group reads it through
`uniform sampler2D screen_texture : hint_screen_texture, filter_nearest;`
sampled at `SCREEN_UV` (Godot docs, CanvasGroup "custom shader" note).
UNVERIFIED: executor confirms by building the composite and taking a
screenshot (expected: domes visible with the stripe pattern); if the group
renders nothing, stop the task and record "CanvasGroup screen_texture path
fails in Godot 4.7" in the summary.

**Changes.**
1. New shaders `client/shaders/shield_group.gdshader` and
   `buildbeam_group.gdshader`: copies of the existing ones where
   `texture(u_texture, T)` reads `screen_texture` at `SCREEN_UV`-based
   coordinates: `T = SCREEN_UV`, `coords = T * u_texsize + u_offset`.
   Uniforms per frame exactly as `J/graphics/Shaders.java:273-282`
   (`ShieldShader.apply`; `BuildBeamShader.apply` :291-300 is identical):
   `u_dp = 1` (`Scl.scl(1)` on desktop), `u_time = Time.time` (ticks),
   `u_offset = (camera.x - view.w/2, camera.y - view.h/2)` in **world**
   units, `u_texsize = (view.w, view.h)` = camera size in world units,
   `u_invsize = 1/u_texsize`. The y-flip means SCREEN_UV.y grows downward
   while Java's v grows upward: use `T.y = 1.0 - SCREEN_UV.y` when computing
   `coords` (sampling still uses SCREEN_UV).
2. `gdext/render/shields.rs`: when the `animatedshields` setting is true
   (add the setting row `SettingRow::check("animatedshields", true)` to
   `core/ui/settings.rs` next to `animatedwater` (:245) — a settings-hub
   change; note it for G04/PAR-0164), parent the `ShieldRenderer` under a
   `CanvasGroup` (`# code-instantiated`) with `ShaderMaterial(shield_group)`;
   draw domes/bubbles opaque with the animated colours; update the shader
   uniforms each frame. Otherwise keep today's branch.
3. `gdext/render/build_beam.rs` (T3): same with `buildbeam_group`; draw the
   triangles at full alpha `build_alpha` when animated.

**Tests.** None (shader); keep the existing shields unit tests green.

**Verify in-engine.** Unit bubbles: no vanilla Serpulo early unit has a
`ForceFieldAbility`; projector domes need power (the previous executor found
`force-projector` stuck at efficiency 0 — G02 power issue). Build beams are
always available: compare T3's beam screenshot before/after. If shields
cannot be staged, state "shields headless/visual-only via beam proxy".

**Pitfalls.** `CanvasGroup` + y-flip camera: SCREEN_UV is already screen
space, no flip needed. Fit margin: set `fit_margin` 0 and `clear_margin` 0;
the group must cover the viewport (Godot sizes it from children bounds — add
a transparent full-viewport rect child if the stripes clip).

**Commit.** `PAR-0129: animated shield and build-beam composites`

---

### T18 — Landing core thrusters and post-landing retraction (PAR-0131 remainder)

**Symptom / acceptance.** The landing core drop has no thruster flames or
thruster plates, and after touchdown the core does not retract its thrusters
(Java: 90-tick retraction), no landing shake/effect. After: during the drop
four team-coloured flame circles + white cores and the `<core>-thruster1/2`
plates around the spinning core; after touchdown the plates slide in over
1.5 s and the camera shakes (5, 5).

**Java.** `J/world/blocks/storage/CoreBlock.java`:
* `:480-538` `drawLanding(x, y)`: `fin = landTimeIn; fout = 1 - fin;
  scl = Scl.scl(4)/displayScale` (= 1 at the port's zoom 4; use 1);
  `s = region.w*0.25 * scl * 3.6 * pow2Out(fout)`; `rotation = pow2In(fout) *
  135`; `thrustOpen = 0.25; thrusterFrame = fin >= 0.25 ? 1 : fin/0.25;
  thrusterSize = sample(thrusterSizes, fin)` with `thrusterSizes = {0,0,0,0,
  0.3,1,1,1,1,1,1,1,1,0}` (:44; Arc `Mathf.sample(values, t)`: linear
  interpolation over `t*(len-1)`); `lightTrail` `circle-shadow` size `s`
  (already drawn by landing_core.gd); `strength = (1 + (size-3)/2.5) * scl *
  thrusterSize * (0.95 + absin(2, 0.1))`; `offset = (size-3)*3*scl`; for
  `i in 0..4`: `v = trns(i*90 + rotation, 1)`; team-colour circle at
  `v.setLength((size*4 + 1)*scl + strength*2 + offset)` radius `6*strength`;
  white circle at length `(size*4+1)*scl + strength*0.5 + offset` radius
  `3.5*strength`; then `drawLandingThrusters(x, y, rotation, thrusterFrame)`,
  the spinning region, `alpha pow4In(frame)` second thruster pass, team
  region.
* `:540-563` `drawLandingThrusters`: `length = thrusterLength(14/4 = 3.5) *
  (frame - 1) - 1/4`; two passes; for `i in 0..4`: `reg = i >= 2 ? thruster2
  : thruster1`; `rot = i*90 + rotation % 90`; `v = trns(rot, length)`; pass 0
  draws at `(x+v.x, y+v.y)` rotation `rot`; pass 1 rotates `v` by -90,
  `alpha = (rotation % 90)/90 * alpha`, `rot -= 90`.
* `:340-355` `CoreBuild.draw` after landing: `thrusterTime > 0`:
  `drawThrusters(frame)` (:566-573: `length = 3.5*(frame-1) - 0.25`; plate
  `i` at `(x + d4x[i]*length, y + d4y[i]*length)` rotation `i*90`), base
  region, `alpha pow4In(frame)` thrusters again, team top. `thrusterTime =
  1` at touchdown (:412) and `-= delta/90` per tick (:650); also
  `launchEffect.at(this)` (`Fx.launch`) and `Effect.shake(5, 5, this)`.

**Port today.** `scenes/landing_core.gd` draws the shadow + scaled/rotated
core (header says "Thrusters and hiding the real core building are not drawn
in this slice"); `camera.rs::landing_draw_json` gives `{active, x, y, block,
fin}`.

**Changes.**
1. `scenes/landing_core.gd` `_draw`: add the flame circles and
   `drawLandingThrusters` passes (regions `<block>-thruster1/2`, fallback
   skip; natural size `* CORE_REGION_SUPERSAMPLE`), with the y-flip rule (draw
   textures with a negative-height rect or `draw_set_transform` scale
   `(1,-1)`; follow whatever the existing core sprite draw in this file
   does).
2. **Real core under the landing sprite.** Java does not hide the real
   core: `CoreBlock.drawLaunch` (CoreBlock.java:427-470) runs at
   `Layer.space` (160), above every world layer, so the dropping core,
   the `lightTrail` streak particles and the white `mixcol(white,
   pow5In(fout))` cover it. Port: give the `LandingCore` node
   (`scenes/game.tscn` `World/LandingCore`) the `Layer::Space` band z: in
   `gdext/render.rs::build_bands`, next to the block that parks `MindFx` at
   the effect band (~418-425), resolve `/root/Spine/World/LandingCore` and
   `set_z_index(self.band_plan.band(BandKey::base(Layer::Space)))` +
   `set_z_as_relative(false)` (band numbers are plan indexes, not Java layer
   values; if `Layer::Space` is not in the plan, use `Layer::FogOfWar`'s band
   and note it). While `active`, also draw the
   streaks: `pfin = pow3Out(fin)`, `pf = pow2In(fout)`, colour `lightTrail`;
   `Angles.randLenVectors(seed 1, pfin, 100, 800*pfin, …)` — 100 rays with
   seeded random angle/length (Arc `randLenVectors(seed, fin, amount, length,
   cons)`: `Rand(seed)`, per i: `v.trns(random(360), random(length*fin)…)`;
   read `ARC/arc-core/src/arc/math/Angles.java` `randLenVectors(long seed,
   float fin, int amount, float length, ParticleConsumer)` and port it into
   `core/render/drawf.rs` with the Arc `Rand` (check
   `core/math` for an Arc `Rand` port: `grep -rn "struct Rand" core/math`)):
   each ray `stroke(ffin * pf * 3)`, `lineAngle(x+ax, y+ay, angle(ax,ay),
   ffin*20 + 1)`. Clouds (`rules.cloudColor.a > 0`) — deferred.
3. After `_finish_landing()`: start a 90-tick `_thruster_time = 1.0` and keep
   drawing `drawThrusters(frame)` plates over the real core for 1.5 s;
   call `/root/Spine/MindFx.spawn_effect("launch", x, y, 0)` and
   `/root/Spine/World/Camera2D.apply_shake_event(5, 5)`.

**Tests.** GDScript only.

**Verify in-engine.** Fresh landing on sector 98 (env §4.1), screenshots at
~1 s and ~3 s and right after touchdown.

**Pitfalls.** `landing_core.gd` is UI-thread GDScript; keep the math in one
`_draw`. `Mathf.sample` clamps `t` to 0..1.

**Commit.** `PAR-0131: landing core thrusters, streaks, retraction, landing shake`

---

### T19 — Verification sweep and ledger evidence (no new code unless a check fails)

**Symptom / acceptance.** Several ledger items are fixed in code but still
`reference/fixing/blocked/seeded`. Produce in-engine evidence (screenshots +
probe JSON under `runs/<date>-sN-G03-T19/`) for each, and list the proposed
status in the summary:
* PAR-0055: screenshot the alpha at zoom 4 next to the Java tutorial frame;
  measure the body ≈ 40-50 screen px (sprite_scale 0.25).
* PAR-0070 / PAR-0128: Ground Zero wave fight with T4 bullets: effects
  (hit sparks, despawn puffs) and shake; compare with
  `runs/20261010-234250-ref-PAR-0070-combat-fx/`.
* PAR-0094: select conveyor (key line, requirements), hover ore, hover a
  unit (T5 of the previous plan), hover a building (T6 bars).
* PAR-0096: G01 §3.1 provides `#[func] SimHost.drain_build_damage_json()
  -> [[tile_x, tile_y, team], …]` (G01 T15). In `hud_fragment.gd` (or the
  minimap widget) drain it every refresh and call
  `MindMinimap.add_indicator(x, y)` for rows with the player team; this is
  the only code change in T19. If the seam is absent, "feed blocked by G01".
* PAR-0097: F6 markers (done previously) — re-screenshot.
* PAR-0126: dagger legs/weapons in motion (spawn a team-2 dagger walking).
* PAR-0129: T17 evidence or "blocked: force projector power (G02)".
* PAR-0130/0152: belts/drills/turrets still render (regression check).
* PAR-0157: command overlay + T10 reticle.
* PAR-0057: minimap terrain colours vs the Java minimap crop.
If any check fails, open a task-sized fix only if it is inside G03 and < 30
lines; otherwise record it.

**Commit.** `Gameplay G03 T19: verification evidence summary` (docs only:
the summary file).

---

### T20 — Launch pods and landing-pad arrivals (needs G04 T10/T11)

**Symptom / acceptance.** Once G04 makes launch pads launch (G04 T10) and
landing pads receive (G04 T11), nothing is drawn: no pod rises from a launch
pad, no pod lands on a landing pad, and the pads themselves show no pod or
progress lights. After: a launched pod flies up-right with an engine glow and
a shadow for 120 ticks; an arriving pod descends onto the landing pad with
engine flares; an idle launch pad shows its pod and the 4 corner progress
lights fill as items accumulate.

**Java.**
* `J/world/blocks/campaign/LaunchPad.java:227-280` `LaunchPayloadComp.draw`
  (lifetime 120, `:153`): `alpha = fout(pow5Out)`; `scale = (1 - alpha)*1.3 + 1`;
  `cx = x + fin(pow2In) * (12 + randomSeedRange(id+3, 4))`,
  `cy = y + fin(pow5In) * (100 + randomSeedRange(id+2, 30))`;
  `rotation = fin * (130 + randomSeedRange(id, 50))`; at `Layer.effect +
  0.001`: `Fill.light(cx, cy, 10, 25*(rad + scale - 1), engine@alpha,
  engine@0)` with `rad = 0.2 + fslope` (`fslope = (0.5 - |fin - 0.5|)*2`),
  then `alpha(alpha)` and 4 `Drawf.tri(cx, cy, 6, 40*(rad + scale - 1),
  i*90 + rotation)` in `Pal.engine` (`ffbb64`); at `Layer.weather - 1`: pod
  region (`<pad>-pod`, fallback `launchpod`) size `region*0.25*scale`,
  rotation, alpha; shadow at `Layer.flyingUnit + 1`: same region tinted
  `(0,0,0,0.22*alpha)` offset `trns(225, fin(pow3In)*250)`.
* `LaunchPad.java` `LaunchPadBuild.draw` (~110-135): liquid (deferred),
  base, then if `lightRegion` found: `progress = min(items.total/itemCapacity,
  launchCounter/launchTime)`; for `i in 0..4`, `j in 0..lightSteps`:
  `alpha = curve(progress, j/lightSteps, (j+1)/lightSteps)`, `offset =
  -(j - 1)*lightStep`, colour `lerp(Pal.metalGrayDark, lightColor, alpha)`,
  `lightRegion` at `(x + d8edge(i).x*offset, y + d8edge(i).y*offset)`
  rotation `i*90` (read `lightSteps`, `lightStep`, `lightColor` defaults in
  LaunchPad.java fields); then `Drawf.shadow(x, y, size*8)` and the pod
  region at the centre.
* `J/world/blocks/campaign/LandingPad.java` `draw` (~205-265): base; if
  `arriving != null`: `fin = clamp(arrivingTimer)`, `fout = 1 - fin`,
  `alpha = pow5Out(fin)`, `scale = (1 - alpha)*1.3 + 1`, `cx = x`,
  `cy = y + pow4In(fout) * (100 + randomSeedRange(id+2, 30))`,
  `rotation = fout * (90 + randomSeedRange(id, 50))`; engine light + 4 tris
  as above with `rad = 0.15 + pow5Out(slope(fin))`; pod at `Layer.weather -
  1` with `Drawf.shadow(cx, cy, size*8, fin)` and the region; dark copy at
  `Layer.flyingUnit + 1` offset `trns(225, pow3In(fout)*250)`; else if
  `cooldown > 0`: `Drawf.shadow(x, y, size*8, cooldown)`,
  `alpha(cooldown)`, `mixcol(accent, 1 - cooldown)`, pod region at the
  centre. Pod region `<pad>-pod` fallback `advanced-launch-pad-pod`.
* `Fill.light(x, y, sides, radius, centre, edge)`: triangle fan, centre
  colour at the middle, edge colour on the rim.
* `Mathf.randomSeedRange(seed, range)` = `Rand(seed).range(range)`; port as
  a deterministic hash (document) like T4's spin seed.

**Port today.** Nothing draws pods. G04 T10 adds
`#[func] MindSimHost.launch_payloads_json() -> [{x, y, time, lifetime}]`
(G04 plan T10 step 4) and G04 T11 adds `LandingPadState {config, priority,
cooldown, arriving: Option<ItemId>, arriving_timer, liquid_removed, ..}`.
`LaunchPadState`/`CampaignState` `launch_time` per G04 T10 step 1-2.

**Changes.**
1. Precondition: `grep -n "fn launch_payloads_json" gdext/sim_host.rs` and
   `grep -rn "struct LandingPadState" core/`. If either is missing, implement
   only the parts whose data exists and record the rest as "waits for G04".
2. `launch_payloads_json` lacks an id: use the array index plus `x,y` hashed
   as the seed (document: Java uses the entity id).
3. Rust-only seam `pad_draws(rect) -> Vec<PadDraw>` with `PadDraw::Launch{x,
   y, size, progress, block}` and `PadDraw::Landing{x, y, size, id,
   arriving: bool, arriving_timer, cooldown, block}`.
4. New `gdext/render/pods.rs` with three nodes (custom draw): engine glow at
   the `Layer::Effect` band, pods at `Layer::Weather` band (z - 1 → create the
   node first in that band), dark shadow copies at the `Layer::FlyingUnit`
   band. Static pad parts (lights, idle pod, cooldown pod) go into the
   `Layer::Block` band node after the block meshes.
5. `Drawf.shadow(x, y, rad, alpha)` (Drawf.java:360-364): colour
   `(0, 0, 0, 0.4*alpha)`, region `circle-shadow` drawn `rad × rad` (the
   `Draw.xscl/yscl` factors are 1).

**Tests.** `core/render/drawf.rs`: `launch_pod_path`: `fin = 0.5` with zero
seed ranges → `cx = x + 0.25*12 = x + 3`, `cy = y + 0.03125*100 = y +
3.125`, `rotation = 65`.

**Verify in-engine.** After G04 T10: G04's own verify recipe (advanced
launch pad + power + oil + 100 copper, `step(1800)`), then screenshot every
20 ticks for 120 ticks (`set_paused(true)` + `step(20)`): pod rising.
Landing pad: G04 T11's recipe, screenshots during `arriving`.

**Pitfalls.** Payload positions are pad centres; all motion is view-derived
from `time/lifetime`, never from new sim state.

**Commit.** `Gameplay G03 T20: launch pods and landing-pad arrivals`

---

### T21 — View-side unit FX: status-effect particles, floor walk ripples, landing dust

**Symptom / acceptance.** Burning, wet, sapped, tarred, overclocked … units
emit no particles; units walking through shallow water make no ripples; a
boosting mech landing makes no dust. After: a burning dagger emits `Fx.burning`
puffs, a wet unit drips (`Fx.wet`), a dagger walking in water spawns
`Fx.ripple` every `7 + hitSize/8` px moved, and a nova landing after a boost
spawns `Fx.unitLand`.

**Java.**
* `J/type/StatusEffect.java:164-167` (inside `update`): if `effect != none
  && Mathf.chanceDelta(effectChance) && !inFog`: `v.rnd(range(hitSize/2))`
  (random direction, length `range(hitSize/2)` = uniform in ±hitSize/2);
  `effect.at(x + v.x, y + v.y, 0, color, parentize ? unit : null)`.
  `chanceDelta(p) = random() < p * Time.delta`.
* `J/entities/comp/UnitComp.java:726-746`: when `isFlying()` flips from
  flying to grounded: `Fx.unitLand.at(x, y, floor.isLiquid ? 1 : 0.5,
  tile.getFloorColor())`; when `!type.hovering && isGrounded() &&
  type.emitWalkEffect`: `splashTimer += dst(deltaX, deltaY)`; when `>= 7 +
  hitSize/8`: `floor.walkEffect.at(x, y, hitSize/8, floorColor)`,
  `splashTimer = 0`. `Floor.walkEffect` = `Fx.ripple` for liquid floors
  (`Floor.java:205-207`), else `Fx.none` (no vanilla floor overrides it).

These are visual-only and use the non-deterministic `Mathf` RNG in Java, so
the port computes them in the view.

**Port today.** `StatusEffect` content has `effect: EffectId`,
`effect_chance`, `parentize_effect`, `color`
(`core/content/registries/statuses.rs:104-160`); `StatusComp.statuses`
(`core/entities/comp/unit/comp.rs:450`); floor `BlockDef.is_liquid`;
`UnitTypeDef.hovering`/`emit_walk_effect` (check names with `grep -n
"hovering\|emit_walk" core/content/registries/units/mod.rs`; if
`emit_walk_effect` is absent treat it as `true`). `MindFx.spawn_effect(name,
x, y, rotation)` (#[func], fx.rs ~672) spawns named effects but takes no
colour; the bus path `FxEvent::Effect {effect, x, y, rotation, color}` does.

**Changes.**
1. Extend `UnitDraw` (T5) with `statuses: SmallVec<[StatusId; 4]>`,
   `grounded: bool` (`!flying && elevation < 0.001`), `floor: BlockId`.
2. `gdext/render/units.rs`: per-unit view state (T5's `UnitViewState`) gains
   `was_flying: bool`, `last_pos: (f32, f32)`, `splash: f32`. Each `update()`
   (once per frame; scale probabilities by `delta_ticks`): use a view RNG
   (`rand::rngs::SmallRng` is not allowed in core; in gdext use
   `godot::global::randf()`), and push effects by calling
   `MindFx.bind_mut().bus().push(FxEvent::Effect{...})` — check `FxBus` has a
   `push`/`emit` method (`core/fx/sink.rs:133`; it implements `FxSink`, so
   call `FxSink::effect(&*bus, &EffectRef::Named(id), x, y, rotation, color)`).
   Do this in `MindWorldRenderer::frame` after `units.update()`, not inside
   the unit pass (avoid borrowing `MindFx` from the unit renderer): have
   `UnitRenderer::update` return a `Vec<(EffectId, f32, f32, f32, Rgba)>` and
   let `frame()` forward them.
   * status: for each status with `effect != EffectId::NONE` and
     `randf() < effect_chance * delta`: offset = random angle, length
     `(randf()*2-1) * hitSize/2`; emit with the status colour.
   * landing: `was_flying && grounded` → `unitLand` (`EffectId` by name via
     `fx_meta` lookup: `grep -n "fn by_name\|fn find" core/content/registries/fx_meta.rs`),
     rotation `floor.is_liquid ? 1 : 0.5`, colour = floor map colour
     (`BlockDef.map_color` or the minimap palette colour).
   * walk: grounded, liquid floor, splash ≥ `7 + hitSize/8` → `ripple`,
     rotation `hitSize/8`, floor colour.
3. Respect the `effects` setting (MindFx already gates `settings.effects` in
   `tick_view`).

**Tests.** `core/render/unit_draw.rs`: `walk_splash_threshold`: hitSize 8 →
threshold 8; moving 3 px per frame emits on the 3rd frame (9 ≥ 8) and resets.

**Verify in-engine.** Ground Zero has shallow water? (check `tile_debug` on
blue tiles); spawn a team-2 dagger on water, step 60, screenshot ripples.
Status: burning needs a fire; if no seam applies statuses, headless only.

**Pitfalls.** Never feed these through the sim's `UnitRuntime.fx` handle
(that is the deterministic sim path); they are view-only.

**Commit.** `Gameplay G03 T21: status-effect particles, walk ripples and landing dust`

---

### T22 — Construct place/break and rotate effects (sim emission + drawing)

**Symptom / acceptance.** Finishing a construction shows no `Fx.placeBlock`
square, deconstruction no `Fx.breakBlock` debris, rotating a placed block
(R) no `Fx.rotateBlock`. The sim never emits them (no `fx.effect` call in
`core/sim` or `core/world` outside combat: `grep -rn "fx\.effect\|FxSink"
core/sim core/world` → none). After: each event shows its effect.

**Java.**
* `J/world/blocks/ConstructBlock.java:59-69` `deconstructFinish`: if the tile
  is visible to the player team: `block.breakEffect.at(tile.drawx(),
  tile.drawy(), block.size, block.mapColor)` (`breakEffect = Fx.breakBlock`,
  Block.java:381).
* `ConstructBlock.java:~112-116` `constructFinish`: if visible:
  `block.placeEffect.at(tile.drawx(), tile.drawy(), block.size)`
  (`placeEffect = Fx.placeBlock`, Block.java:379). (Rotation argument = size.)
* `J/world/Build.java:88,112` (`beginPlace` replacing a same-block rotate):
  `Fx.rotateBlock.at(build.x, build.y, build.block.size)`; and
  `InputHandler.rotateBlock` (server call) — check `J/input/InputHandler.java`
  `rotateBlock` for an `Fx.rotateBlock` call and mirror it.
* Effects exist in the port: `EffectId::PLACE_BLOCK` (22), `BREAK_BLOCK`
  (25), `ROTATE_BLOCK` (254) (`core/content/registries/fx_meta.rs:109,118,805`),
  bodies in `core/fx/custom/basic.rs:271` and `wave_f56.rs:1818`.

**Port today.** Construct finish/deconstruct finish happen in
`core/sim/builder.rs` (~709 `deconstructFinish` → `tile.remove()`, ~722
`constructFinish` → `setBlock`); rotation in `core/sim/mod.rs` ~830-850
`SimCommand::Rotate`. The live FX sink is `UnitRuntime.fx: FxHandle`
(installed by the previous plan's T12 in `sim_host.rs::install_fx_sink`).

**Changes.**
1. `core/sim/builder.rs`: add `fn emit_fx(&self, id: EffectId, x: f32, y: f32,
   rotation: f32, color: Rgba)` on `Sim` that forwards to
   `self.unit_runtime().map(|rt| rt.fx.effect(&EffectRef::Named(id), x, y,
   rotation, color))` (read-only borrow; `FxSink::effect` takes `&self`).
   Call it at deconstruct finish with `(BREAK_BLOCK, centre, size as f32,
   def.map_color.unwrap_or(white))` and at construct finish with
   `(PLACE_BLOCK, centre, size as f32, white)`. Centre = `tile.drawx()` =
   `x*8 + def.offset`. Visibility gate: fog is not ported → always emit
   (document).
2. `core/sim/mod.rs` `SimCommand::Rotate`: after the rotation applies, emit
   `ROTATE_BLOCK` at the building centre with rotation `size`.
3. Determinism: FX emission must not touch the sim RNG or state; `FxSink`
   is a side channel (combat already uses it), so checksums stay unchanged —
   run `cargo test -p mind-headless --lib` and confirm no new golden diffs.
4. Drawing: nothing new — `MindFx::tick_view` already executes named effects.

**Tests.** `core/sim/builder.rs` test `construct_finish_emits_place_fx`:
install a recording `FxSink` (see the combat test that records effects:
`grep -rn "impl FxSink for" core/` for a test sink to reuse), queue a wall
via `queue_player_place` with `sim_with_player()`, tick until built, assert
one `PLACE_BLOCK` event at `(20*8, 20*8)` with rotation 1.0; deconstruct →
one `BREAK_BLOCK`.

**Verify in-engine.** Queue a conveyor through the player (T3 recipe), step
until built, screenshot the frame of completion (step 1 tick at a time near
the end; `construct_progress` shows progress near 1): the accent square
expands. `$I` rotate key on a placed conveyor → rotate effect.

**Pitfalls.** `unit_runtime()` may be `None` in headless/no-unit tests —
emission must be a no-op then.

**Commit.** `Gameplay G03 T22: construct place/break and rotate effects`

---

## 2.1 Deferred (with reasons)

| Item | Java | Why deferred |
|---|---|---|
| Liquid drawing in conduits, tanks, routers, junctions, pumps | `Conduit.draw(false)`, `LiquidBlock.drawTiledFrames` (LiquidBlock.java), `LiquidRouter.draw`, `Pump.draw`, `renderer.fluidFrames` | needs the fluid frame atlas animation + conduit blend bits; mid-game; plan as a G03 follow-up slice using T15's interpreter |
| Remaining `DrawBlock` chains (40 of 54 `drawer =` blocks: e.g. separator variants, electrolyzer, heat blocks, Erekir crafters) | `J/world/draw/*` | extend `vanilla_chain`; Erekir-heavy |
| Remaining per-block draws: Battery fill, Mend/Overdrive/Regen pulses, UnitFactory/Reconstructor unit build preview (`unitbuild` shader), MassDriver, LaunchPad, Radar, ShockMine, Incinerator, StackConveyor, Duct, LogicDisplay (G06) | the Build classes listed in §1.3 | each is a small slice; lower visibility than T7/T8 |
| `drawPlace` for drills (`Drill.drawPlace` ore count text), pumps, bridges (range), core (`CoreBlock.drawPlace`), PowerNode potential links | `Block.drawPlace` overrides | text rendering in world space + indexer queries; after T2 |
| Legged units (`drawLegs`), tank treads, unit payloads, unit parts, ability draws | `UnitType.java` | mid/late game units; `LegsComp` IK data exists |
| Inline effects (`EffectRef::Inline`, 53) and `FxEvent::Trail/Light` | `content/*` inline `new Effect(...)` lambdas | each lambda needs an individual port |
| Puddles and fires drawing | `PuddleComp.draw`, `FireComp.draw` | fires already emit Fx; puddles need the puddle mesh |
| Fog of war | `FogRenderer`, `FogControl` | `FogControl` has no live consumer (sim-side); Erekir |
| Bloom | `Renderer` bloom capture/render | cosmetic |
| Generated-sector polygon darkness | `World.java:505-526` | with PAR-0112 (G05) |
| Possession select arrows, other-player plans, schematic/rebuild selection rectangles | `OverlayRenderer.drawTop`, `InputHandler.drawOtherBuildPlans`, `drawRebuildSelection`, `drawSelection` | possession/schematic/rebuild modes are G08; multiplayer OOS |
| ChatFragment, ConsoleFragment | — | multiplayer / dev tool |

---

## 3. Execution order, dependencies, shared files

Order (one commit each):
1. T1 (helpers; everything else uses them).
2. T2 → T3 (T3 extends `plans.rs`).
3. T4 (bullets) → T5 (units; reuses the trail pattern).
4. T6 (hover bars), T8 (state sprites/cracks), T7 (lasers/bridges).
5. T9 (HUD), T10 (command panel), T16 (game over).
6. T11 (darkness), T12 (rules + lights).
7. T13 (inventory), T14 (overlay top; uses T2's view), T15 (draw descriptors).
8. T22 (construct FX emission), T21 (unit view FX), T17 (animated
   composites; UNVERIFIED gate), T18 (landing), T20 (pods; after G04
   T10/T11 are in `main`), T19 (sweep, last).

Dependencies: T20 needs G04 T10 (`launch_payloads_json`) and T11
(`LandingPadState`); T21 needs T5 (`UnitDraw` extensions); G06 T12 plugs
into T14's `ConfigOverlay`/`SelectOverlay` hooks (do T14 before G06 T12);
G06 T6's `show_info_toast` signal is consumed by T9 step 5; T3 needs T2; T14 needs T2 (`placement_view`); T17 needs T3;
T19 last. T4's lancer lasers need G01's laser-bullet fix to be visible.
T11's limit-area half needs G04's `limit_area` plumbing (the border half
works alone).

Files shared with other areas (pull `main` before editing; keep edits
additive):
* `gdext/sim_host.rs` (all areas) — add seams at the end of the
  `unit_draws_full` impl block.
* `gdext/input/desktop.rs`, `gdext/input/mod.rs` (G08) — T2 adds a read-only
  view, T9 one hotkey arm, T10 three #[func]s; nothing else.
* `core/sim/builder.rs` (G02/G08) — T2 adds `preview_can_place`.
* `ui/fragments/placement_fragment.gd/.tscn` (G08 schematic UI may touch) —
  T6, T10.
* `ui/fragments/hud_fragment.gd/.tscn` (G05 hints) — T9, T16.
* `gdext/ui/hud.rs` (G04 capture/game over) — T9 changes only the toast→
  announce and adds guardian toasts.
* `core/ui/settings.rs` (G04 PAR-0164) — T17 adds one row.
* `client/shaders/light.gdshader` — T12.

## 4. Ledger changes to report

| Item | Proposed | Note |
|---|---|---|
| PAR-0055 | verified (after T19 screenshot) | fixed in code (`units.rs:73-74` scale_marker); note says blocked — stale |
| PAR-0070 | verified (after T4+T19) | producer + regions done (previous T12); bullets now visible |
| PAR-0128 | verified for effect/decal regions | weather prims stay with PAR-0127 (G07); T1 fixes the mirrored region draw |
| PAR-0094 | verified | key line + highlight + cursor hover (previous T3-T5) + T6 bars |
| PAR-0095 | fixed by T10 | — |
| PAR-0096 | fixing → blocked on G01 (`BuildDamageEvent` never emitted) | display done (previous T15) |
| PAR-0097 | verified | previous T14; note Java gates on `hasConsumers` (no drill markers) |
| PAR-0126 | verified after T5 + T19 | engines, mining, items, hit flash added |
| PAR-0129 | fixing | non-animated done; T17 animated; in-engine blocked by projector power (G02) |
| PAR-0130, PAR-0152 | verified (regression re-check) | — |
| PAR-0131 | fixing → verified after T18 | rendering half complete |
| PAR-0157 | verified after T10 reticle | — |
| PAR-0057 | verified after T11 | border darkness; limit area pending G04 |

New items to file:
* "Placed blocks show no placement ghost, rotation arrow or range circle"
  (T2).
* "Queued build plans and the build beam are invisible" (T3).
* "Bullets are not drawn at all" (T4).
* "Mining beam, carried items and engine glows are not drawn" (T5).
* "Hovered buildings show no health/items/power bars" (T6).
* "Power-node lasers and bridge links are invisible" (T7).
* "Doors/switches/sorters show no state; no damage cracks" (T8).
* "HUD lacks boss bar, core-attack and drop-zone warnings; capture opens the
  game-over dialog" (T9).
* "Sector lighting rules never reach the renderer; light pass is a
  placeholder" (T12).
* "Crafter/generator DrawBlock animations not drawn in the live client" (T15).
* "Lancer laser bullets are removed at spawn (invisible beam)" — G01.
* "Launch/landing pods are not drawn" (T20).
* "No place/break/rotate block effects" (T22).
* "Status effects and water walking show no particles" (T21).

## 5. Done criteria

* Every task T1–T18 and T20–T22 committed or explicitly stopped with a reason in the
  summary; T19 evidence written.
* `cargo test -p mind-core --lib --features msav-import` and
  `cargo test -p mind-gdext --lib` pass; `mind-headless` failures only from
  the G00 baseline list; clippy `-D warnings` clean; GDScript parse gate
  clean.
* For each task, the "Verify in-engine" screenshot exists under
  `runs/<date>-sN-G03-T<n>/` or the summary states "headless only" with the
  reason given in the task.
* `$U/logs` newest file has 0 `panic` lines after the last in-engine step.
* Summary `docs/parity-plans/G03-rendering-hud_summary.md` committed (env
  §6 shape) with the §4 ledger table filled with real outcomes.

## 6. Cross-area findings

1. **G01** — Laser bullets are removed at spawn: `core/combat/bullet/kinds/
   laser.rs:58` sets `HIT` after `collide_line`; Java `LaserBulletType`
   (`J/entities/bullet/LaserBulletType.java:15-33,60-75`) keeps the bullet for
   `lifetime = 16` (speed 0, `collides = false`) so it can be drawn with
   `fdata = resultLength`. Without the fix the lancer beam is invisible.
2. **G01** — `BuildDamageEvent` is defined but never emitted (previous plan
   blocker 3): minimap attack indicators (PAR-0096) and the HUD core-attack
   notice (T9 uses a health-diff workaround) need it; Java
   `CoreBlock.java:842-848` also fires `Trigger.teamCoreDamage`.
3. **G04** — `GameStats` counters other than `waves_lasted` are never
   incremented (`core/game/play.rs:682` is the only writer): the game-over
   dialog shows zeros (T16).
4. **G04** — `WorldGrid.limit_area` is never set from `Rules.limitMapArea`
   (only `core/world/mod.rs:595` test); T11 draws it once set.
5. **G04/PAR-0164** — settings rows missing for `animatedshields` (Java
   desktop default true) and `drawlight`; T17 adds `animatedshields`.
6. **G02** — `force-projector` stays at efficiency 0 even when powered
   (previous plan blocker 2); blocks PAR-0129 in-engine.
7. **G01/G02** — `mind_core::world::block::kind_rotates` lists `Junction`,
   `Router`, `Sorter`, `Drill`, `Door` … as rotating (`core/world/block.rs:
   520-550`), unlike Java's `rotate = false` for those blocks; it feeds the
   replace rules in `core/world/build.rs:~100`. Check whether placement
   replacement of those blocks then differs from Java.
8. **G08** — `MindInput.placement_preview_json` (#[func]) is now unused by
   rendering (T2 reads a Rust view); keep or remove in G08's cleanup.
9. **G06** — T12 (logic overlays) must use T14's `ConfigOverlay::Logic` /
   `SelectOverlay::{MessageText, LogicTag}` hooks and the
   `MindInput::config_selected()` accessor added by T14, not a second overlay
   node; G06 T6 provides only the `show_info_toast` entry point, T9 owns the
   announce/toast visuals.
10. **G01** — boss seam: T9 defines `#[func] SimHost.boss_status_json()`
   (`{"count","names","value"}`); if G01 §3.1 adds its own boss seam, one of
   the two must be dropped (T9 checks first).
