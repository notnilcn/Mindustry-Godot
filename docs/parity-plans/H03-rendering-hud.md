<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H03 — In-sector world rendering + HUD: audit and execution plan (2026-10-14)

Area: everything the player SEES after entering a sector — world draw passes
(`client/rust/mind-gdext/src/render/**`, `render.rs`), view seams on
`MindSimHost`, the in-game HUD fragments (`client/ui/fragments/hud_fragment.gd`,
`placement_fragment.gd`, the minimap widget) and the shaders those use. Not
in this area: sim rules (H01/H02/H09), save/campaign loop (H04), meta screens
and the planet globe (H05), logic displays (H06), weather drawing + audio
(H07), input semantics (H08).

**Read `docs/H-environment.md` first** (paths `REPO WT core/ gdext/ headless/
ui/ scenes/ J/ ARC/`, session/build/test recipes, the gmcp shorthands
`$G $H $I $C $MC`, §4.1 "Entering a sector", §4.2 seam table, §6 commit
rules). They are not repeated here. Commit subjects in this plan:
`Gameplay H03 T<n>: ...` or `Fixes PAR-XXXX: ...` / `PAR-XXXX: ...` (the task
says which).

**Start state.** `main` contains the H00 marker:
`git log main --oneline | grep -m1 'H00'` must print a line. After H00 the
player alpha spawns on Ground Zero (`$H.has_player_unit()` = true) and
`player:Some` placements queue a plan instead of applying instantly. If that
command prints nothing, stop and report.

**Drift rule.** Java line numbers were checked 2026-10-14 against
`/home/clintonnguyen/g/code_examples/Mindustry` (read-only). Port line
numbers are from `main` @ `ffbe84dd`; if one moved, `grep -n` the quoted
function name. If a symbol is missing, a seam already exists with a different
shape, or code differs from what a task says by more than "moved lines": stop
that task, write what you saw in your summary, and continue with the next task
that does not depend on it. Do not improvise designs — where a choice exists,
this plan made it.

Shared conventions from the round-G plan still hold (summarised so you do not
need the old plan):

* **World coordinates** = Java's (tile `(x,y)` centre = world `(x*8, y*8)`,
  +y up). The camera flips y (`gdext/camera.rs`, `set_zoom(Vector2(z,-z))`).
  Angles are Java degrees (CCW). Any textured custom draw must go through
  `canvas::draw_region` (`gdext/render/canvas.rs:30`), never
  `draw_texture_rect*` directly (it would appear upside down).
* **Atlas scale**: region pixels are 4x world units. A region drawn "at
  natural size" in Java is `pixels * 0.25` world units.
* **Layers** (`J/graphics/Layer.java`): floor 0, debris 20, blockUnder 29.5,
  block 30, blockCracks 30.1, blockOver 35, blockBuilding 40, turret 50,
  groundUnit 60, power 70, legUnit 75, darkness 80, plans 85,
  flyingUnitLow 90, bullet 100, effect 110, flyingUnit 115, overlayUI 120,
  buildBeam 122, shields 125, weather 130, light 140, playerName 150,
  fogOfWar 155, space 160. One `Node2D` band per layer; inside
  `MindWorldRenderer` get one with
  `self.band_node_at(BandKey::base(Layer::X))`.
* **New custom-draw pass template**: copy `gdext/render/status.rs` (a
  `#[derive(GodotClass)] #[class(base=Node2D)]` with `init/ready/
  on_notification(EXTENSION_RELOADED)/process(queue_redraw)/draw`,
  `bootstrap()` resolving `/root/Spine/SimHost`, `visible_rect()` camera
  culling). Create it in `MindWorldRenderer::build_floor` (`gdext/render.rs`
  ~478-650) exactly like the `bullets::BulletRenderer` block (~606-614):
  `new_alloc()`, `set_name`, `# code-instantiated` comment,
  `band.add_child(&node)`, keep the `Gd` in a new `Option<Gd<..>>` field of
  `MindWorldRenderer` (declare next to `bullets`, initialise `None` in
  `init`). Register the module in the `mod xxx;` list at the top of
  `gdext/render.rs`.
* **Canvas helpers that exist** (`gdext/render/canvas.rs`): `draw_region`,
  `draw_region_line`, `dash_circle`, `circles`, `poly_line`, `rect_line`,
  `fill_tri`, `fill_rect`, `color_u32`, `color_f4`. Colours: `core/render/
  drawf.rs::pal` (`[f32;4]`). Java `Pal` values you need below are quoted in
  each task.
* **Time**: sim-tied animation uses the sim tick; pure view pulses use
  `Time::singleton().get_ticks_msec() as f32 * 0.06` (ticks at 60/s).
  `absin(x, scl, mag) = ((sin(x/scl)+1)/2)*mag`.
* **Settings from Rust**: copy `StatusRenderer::enabled` (status.rs ~120:
  `/root/MindUi` `settings_get_bool`). Read once per `process()`, cache.
* **New seams**: Rust-only `pub fn` on `MindSimHost` unless the task says
  `#[func]`. Put them at the end of the `impl MindSimHost` block that holds
  `block_state_draw` (sim_host.rs ~4872). Use `self.tile_at(x, y)` for
  bounds-checked tile reads.
* Never call `MindRender.get_stats()` or `get_state_json()` while a sector is
  loaded (H-environment §4). T6 adds a small `light_debug_json` probe instead.

---

## 1. Inventory (Java -> port -> status, re-verified 2026-10-14)

Legend: **W** wired & working, **W?** wired but not confirmed in-engine,
**P** partial, **M** missing, **BUG** wrong, **OOS** other area.

### 1.1 Renderer / graphics

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `Renderer` rules -> lighting/fog/env (`Renderer.java:419`, `:175`) | `render.rs::frame` polls `MindSimHost::live_rules()` every 30 frames | W | `render.rs` ~719-735 | — |
| `Renderer.drawLight` setting (`drawlight`) | `set_draw_light` #[func] exists; the `drawlight` settings row is never read | **M** | `grep -rn set_draw_light client/ui` = 0 | T6 |
| `LightRenderer` composite | `render/light.rs` camera-sized RGBA map + `light` shader | **BUG (invisible)** | G03 T12 summary; see T6 diagnosis | T6 |
| `ShaderRegistry` source of truth | prefers `assets/shaders/godot/*.gdshader` (packed copy) over `res://shaders/` | **BUG** | `assets/shaders/godot/light.gdshader` still `blend_add`; `client/shaders/light.gdshader` is `blend_mix` | T6 |
| `FogRenderer` (Erekir fog of war, `FogRenderer.java`) | `render/fog.rs` = flat placeholder quad; `core/game/fog.rs::FogControl` not live (no consumer) | **M** | fog.rs:83-94 | T12 (needs H09) |
| `OverlayRenderer.drawTop` (indicators, drawConfigure, enemy core radius, drop zone, drawSelect) | — | **M** | no overlay-top pass | T5 |
| `InputHandler.drawCommandedTargets` reticle | `render/command_overlay.rs` rings/lines only | **M** | grep `target(` none | T3 |
| `MinimapRenderer` attack indicators | widget draws `indicators_json`; nobody calls `add_indicator`; lifetime/colour use seconds instead of ticks | **BUG** | minimap.rs:116-123, mind_minimap.gd:113-136 | T4 |
| Minimap all-black crop after a limit-area change (G03 T11 finding) | open | **?** | G03 summary | T4 |
| Shields/buildBeam `effectBuffer` composite (`Renderer.java` animateShields) | non-animated branch only; no `animatedshields` setting row | P | shields.rs header | T11 |
| `BlockRenderer` cracks + enemy team border (`BlockRenderer.java:747-757`) | blocks.rs ~630-680 | W? (code matches Java, not seen in-engine) | — | T2 |
| `Door/Sorter/Switch` state sprites | blocks.rs ~354 | W? (configure path suspect, PAR-0188/0190 = H02) | — | T2 |
| `PowerNode` lasers / `ItemBridge` links | `render/links.rs` | lasers W; bridges W? (staging needs H02's configure fix) | PAR-0244 | T2 |
| `DrawBlock` descriptors (`J/world/draw/*`) | `core/render/draw_desc.rs` ported, **not wired** into the live pass | **NW** | only `core/render/*` references `vanilla_chain` | T8 |
| `FireComp.draw`, `PuddleComp.draw` / `Liquid.drawPuddle` | — | **M** | no fire/puddle drawing in gdext | T7 |
| `UnitFactory.draw` / `Reconstructor.draw` (`Drawf.construct`, `Shaders.build`) | static `-full` icon; `unitbuild.gdshader` exists but unused | **M** | grep `unitbuild` gdext = 0 | T9 |
| `StatusEffect` particles, `Floor.walkEffect` ripples, `Fx.unitLand` | — | **M** | — | T10 |
| `LaunchPayload` / `LaunchPad` / `LandingPad` draws | — | **M**, data seams absent (H04) | `launch_payloads_json`, `LandingPadState` absent | T13 |
| `Bloom`, `Pixelator` | bloom stub; pixelator W | deferred (cosmetic) | — | — |
| `fx_golden` test `fx_program_explosion` | fails on 1 ULP float noise (`prims[14].kind.y2` 95.36211395 vs 95.36210632) | **BUG (gate)** | ran `cargo test -p mind-headless --test fx_golden` 2026-10-14 | T1 |

### 1.2 HUD

| Java | Port | Status | Task |
|---|---|---|---|
| `PlacementFragment` command table (`PlacementFragment.java:443-676`) | none | **M** (PAR-0095) | T3 |
| `HudFragment` status icons, `@nearpoint`, `toggleMenus` | code in `hud_fragment.gd` (G03 T9) | W? — never seen with a live player unit | T2 |
| `HudFragment` boss bar, `@coreattack`, guardian toasts, capture announce, `@saving` | hud_fragment.gd | W (G03 T9 screenshots) | — |
| `BlockInventoryFragment`, `GameOverDialog` | G03 T13/T16 | W | — |
| `PlayerListFragment`, `ChatFragment` | multiplayer only (`net.active()` gate, `DesktopInput.java:233`) | OOS | — |

### 1.3 Done — do not redo (spot-checked 2026-10-14)

* Round-G G03 T1-T7, T9, T11, T13, T16, T18, T22 commits (`git log main
  --oneline | grep "G03 T"`). Ledger: PAR-0184/0185/0186/0187/0240/0241/
  0242/0243/0245 verified.
* Spot-checks performed for this plan (do not re-audit):
  1. **Cracks + enemy border** (blocks.rs ~630-680): region
     `cracks-<size>-<i>`, `i = clamp((1-healthf)*8, 0, 7)`, colour grey 0.2
     with alpha `0.1 + (1-healthf)*0.6`, rotation `(pos%4)*90` = `(y%4)*90`
     — matches `BuildingComp.java:1236-1244`. `block-border` at
     `(x - size*4 + 4, y - size*4 + 4)` in team colour for non-player teams
     — matches `BlockRenderer.java:753-757`. Correct code; only in-engine
     evidence is missing (T2).
  2. **Bullets** (T4 screenshot `runs/20261013-s3-G03-T4/duo-bullets.png`):
     duo bullets visible. OK.
  3. **Boss bar** (`hud_fragment.gd::_refresh_boss`, `boss_json` seam): OK.
  4. **Status icons** (`runs/20261013-s3-G03-T9/status-icons.png`): no icons
     visible — there was no player unit. Not a false claim, unverified (T2).
  5. **Minimap indicators** (G00 b03aed9 claim "indicators done"):
     **FALSE in timing** — `AttackIndicators.duration = 15f * 60f` is in
     ticks (900 ticks = 15 s, `Time.delta` is ticks), the port keeps rows
     900 **seconds** (`minimap.rs:116-123` comment says "seconds") and the
     widget's scarlet blend uses `age / 70` with age in seconds (Java: 70
     ticks). Fixed in T4.
  6. **G03 T12 "pass constructed twice at boot"**: **FALSE diagnosis**. The
     two `MeshInstance2D` children of `Band_light_0` are the
     `LightRenderer` node and the `EnvRenderer` node (both attach to the
     `Layer::Light` band, render.rs ~554 and ~639). Not a double
     construction. Real root-cause candidates in T6.
  7. **G03 T12 commit "light.gdshader is blend_mix"**: **FALSE at runtime** —
     `ShaderRegistry::load` (render/shaders.rs ~84-97) tries
     `<assets_dir>/shaders/godot/light.gdshader` before `res://shaders/`,
     and that packed copy (gitignored, written Oct 12 by `tools/pack.sh`)
     is still `blend_add` in REPO and in the session-3 worktree. Every
     future `client/shaders/*.gdshader` edit is silently shadowed the same
     way. Fixed in T6.
  8. **`mind_label.gd:24 refresh()` "Parameter mesh is null"**: the GDScript
     frame in the backtrace is incidental (the error is raised by
     `mesh_get_surface_count` in the renderer while some script frame is
     active; backtraces in `.opencode/parity-session/session-*/
     editor-console.log` show `_eval`, `_launch`, `refresh` interchangeably).
     The label is not the cause. Timeboxed diagnosis in T14.

---

## 2. Tasks

Ordered by what a player notices first (T1 is a quick gate fix that every
later task's test run needs). One task = one commit.

---

### T1 — `fx_golden`: compare FX goldens with a float tolerance (quick gate fix)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED. `cargo test -p mind-headless --test fx_golden` (build lock): `fx_audit_and_smoke_pass` ok, `fx_scenarios_match_committed_goldens` FAILED with `fx golden mismatch`; a JSON diff of a fresh `fx program explosion --tick 3 --json` against the golden shows exactly one difference, `.prims[14].kind.y2` 95.36211395263672 -> 95.36210632324219. `write_or_check` is at `headless/fx_scenarios.rs:610-632` as stated. Evidence `runs/20261014-s2-verify2/`: `H03-T1-diff.txt`, `H03-T1-fx_program_explosion-now.json`.

**Symptom / acceptance.** `cargo test -p mind-headless --test fx_golden`
fails: `fx program explosion --tick 3 --check .../fx_program_explosion.json`
reports "fx golden mismatch". The only difference is one float:
`prims[14].kind.y2` golden `95.36211395263672`, now `95.36210632324219`
(1 ULP of an f32 trig result; `explosion` body in
`core/fx/custom/basic.rs:446-461` is a verbatim port of `Fx.java:1656-1675`
and its prim count/order/colours are unchanged). After: the test passes and
a real geometry change (> 1e-3) still fails.

**Java.** Not applicable (test harness). The effect matches
`J/content/Fx.java:1656-1675`.

**Port today.** `headless/fx_scenarios.rs:610-632` `write_or_check`
compares `golden.trim_end() != text.trim_end()` (exact string).

**Changes.**
1. `headless/fx_scenarios.rs`: replace the string comparison with a JSON
   comparison:
   ```rust
   /// Structural JSON equality with an absolute float tolerance (FX goldens
   /// hold f32 trig results; 1-ULP drift between toolchains is not a change).
   fn json_close(a: &Value, b: &Value, eps: f64) -> bool {
       match (a, b) {
           (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
               (Some(x), Some(y)) => (x - y).abs() <= eps,
               _ => x == y,
           },
           (Value::Array(x), Value::Array(y)) => {
               x.len() == y.len() && x.iter().zip(y).all(|(u, v)| json_close(u, v, eps))
           }
           (Value::Object(x), Value::Object(y)) => {
               x.len() == y.len()
                   && x.iter().all(|(k, u)| y.get(k).is_some_and(|v| json_close(u, v, eps)))
           }
           _ => a == b,
       }
   }
   ```
   In `write_or_check`: parse the golden with `serde_json::from_str::<Value>`
   (on parse error keep the old "mismatch" path) and fail only when
   `!json_close(&golden_value, value, 1e-4)`. Keep the `eprintln!` text.
2. Do not regenerate the golden.

**Tests.** Unit test at the end of `fx_scenarios.rs`:
`json_close_tolerates_ulp_but_not_geometry`: `{"y":95.36211395263672}` vs
`{"y":95.36210632324219}` -> true; `{"y":95.0}` vs `{"y":96.0}` -> false;
arrays of different length -> false. Then
`cargo test -p mind-headless --test fx_golden` must pass.

**Verify in-engine.** None (headless only: test harness).

**Pitfalls.** Other headless golden checkers (render/audio/logistics) are not
in scope; touch only `fx_scenarios.rs`.

**Commit.** `Gameplay H03 T1: FX goldens compare with a 1e-4 float tolerance`

---

### T2 — Post-H00 re-verification of player-unit-gated visuals (evidence first, small fixes only)

> **In-engine verification (2026-10-14, session 2):** PARTLY (rows a, d, h pass; c, g, j fail; e, f partly; b, i not decisive). Run with the H00 T1 env-bit fix applied locally (player alpha present). Per row:
> * (a) PASS: `debug_apply_status(1000000000, "wet", 6000)` + `"overclock"` (the player row of `unit_debug_json` has `"kind":"Player"`, id 1000000000) shows both icons under the wave pane (`H03-T2a-status-icons.png`).
> * (b) NOT CHECKABLE on Ground Zero: the enemy spawn (1156,1844) is tile (144,230), outside the limit area x 90-159 / y 20-79, and the player is clamped at the limit edge (WASD west stopped at x=720 = tile 90). Headless only.
> * (c) FAIL, cause found (fix < 30 lines in H03): `hud_fragment.gd::_toggle_menus` keeps the hidden state in a `ui-hidden` setting that has no settings row, so `settings_set` drops it, `settings_get_bool` stays false and `shown` is always true; and the paths `WavesStack/CoreInfo/MinimapBox/FpsBox` are children of `HudGroup/hud`, not `HudGroup` (only `placement` resolves). Result: pressing C (bound key, `keybinds_json` value `c`) never hides anything; the `showui` announce appears with raw markup `Press [accent][[C][] to show UI.` (see T17). Java keeps a local `shown` field (`HudFragment.java:875-877`) and uses `getBoolOnce("ui-hidden")` only to gate the announce (`:453-459`). Evidence `H03-T2c-menus-hidden.png`, `H03-T2c-second-press.png`.
> * (d) PASS: the alpha's build beam fans to the plan (`H03-T2d-build-beam.png`). Row recipe corrected in place: `apply_sim_command_json` takes a NUMERIC `block` id and the key `rotation` (a string `block` parses as 0 = air).
> * (e) PARTLY: crack overlay visible on the core shard at 35% health (`H03-T2e-core-cracks-crop.png`). Walls (and every `drawDynamic = false` block: walls, doors, sorters, storage, unloader, logic blocks) never get cracks: `core/render/scan.rs:231` `visible_blocks` drops `!draw_dynamic` blocks and `render/building_cache.rs` draws no cracks, while Java draws them through `tileExtraCachedView` (`BlockRenderer.java:582-586,802-821`). **Owner: T18** (coordinator decision 2026-10-14). So the row's copper-wall recipe cannot pass; no damage seam exists (a team-2 dagger shoots the core, not the wall; stop it with `debug_apply_status(id, "disarmed", 5000)` — `melting` does not kill quickly).
> * (f) PARTLY: a team-2 conveyor shows the red corner mark (`H03-T2f-enemy-border-dynamic-crop.png`); a team-2 duo shows none. Java draws `drawTeam` only for `drawDynamic` blocks (`BlockRenderer.java:582-586,752-757`; `Wall.java:75-77` sets `drawDynamic = false`), so walls correctly have no mark and the row's derelict scrap-wall recipe was wrong (and (169,167) is outside the limit area, i.e. dark). Missing marks: turrets and every kind that `continue`s early in `render/blocks.rs` (drill ~254-296, door ~356, switch ~382, sorter/item source ~406, unloader ~465) because the crack/border code (~650-680) comes after them.
> * (g) FAIL, render (not configure): configure works (`building_config_json` door `"true"`, sorter `{"type":"item","id":0}`) but the open door draws exactly like an unconfigured one and the sorter shows no item icon (`H03-T2g-doors-crop.png`, `H03-T2g-sorter-crop.png`). Cause: door/sorter are `draw_dynamic = false`, so the `blocks.rs` Door/Sorter/Switch/Unloader branches are unreachable; the static `building_cache` has no state sprites and no recache on config (Java `Door.java:54-55` `entity.recache()`). Not blocked by H02. **Owner: T18** (coordinator decision 2026-10-14). Coordinates used: free ground (133,66) closed door, (135,66) open door, (137,66) sorter.
> * (h) PASS: bridges (139,66) -> (142,66) linked with `configure_building(139,66,"int", str((142 << 16) | 66))` (Point2.pack = x<<16 | y; readback `{"type":"point","x":3,"y":0}`), line + arrows drawn (`H03-T2gh-crop.png`); no hang.
> * (i) NOT DECISIVE: port alpha ~33x44 px at scale 4 vs Java ~43x50 px in `r02-03-stable.png` (different facing; `H03-T2i-alpha-compare.png`). Also every unit carries a permanent bubble (new T15).
> * (j) FAIL: with `duo` selected the ghost follows the cursor on free ground but no dashed range circle is drawn (`H03-T2j-duo-range3.png`; cause not isolated: `render/plans.rs:270` `block_place_range` branch). `godot_input` `mouse_motion` works (`{"action":"mouse_motion","params":{"position":{..},"coords":"viewport"}}`); `placement_preview_json()` stays `"[]"` and `get_input_state_json().cursor` is null while the ghost is drawn. **Moved to T5 step 6** (coordinator decision 2026-10-14).

**Symptom / acceptance.** Round G could not see several finished visuals
because no player unit existed (H00 fixed that) or because configure was
suspect. Produce in-engine evidence for each item below under
`runs/<date>-s<N>-H03-T2/` and list the outcome per row in your summary.
If a check fails and the fix is inside H03 and < 30 lines, fix it in this
commit; otherwise record it (name the owner) and continue.

| # | Item | How (all on Ground Zero, §4.1 "Entering a sector") | Pass looks like |
|---|---|---|---|
| a | HUD status icons (`HudFragment.addInfoTable`) | `$G --eval "return $H.debug_apply_status(<seq>, \"wet\", 600)"` with `<seq>` from `JSON.parse_string($H.unit_debug_json())` row whose `"player"`/controller is the player (read the row keys first: `return $H.unit_debug_json().substr(0, 600)`); screenshot | a wet icon next to the wave pane (`ui/fragments/hud_fragment.gd::_refresh_status_icons`) |
| b | `@nearpoint` banner | the player cannot be teleported by a seam; move the alpha with WASD via `godot_input` toward the enemy spawn (world ~(1156,1844); ~ 175 tiles north) — if it takes > 3 min of key holding, mark "headless only (`player_near_drop_zone` unit test)" | red `@nearpoint` text centred |
| c | `toggle_menus` (G03 skipped it) | `python3 REPO/tools/gmcp.py godot_input '{"action":"key","params":{"key":"C","pressed":true}}'` — first read the bound key: `return $I.keybinds_json()` and find `toggle_menus` | HUD hides; press again -> HUD returns |
| d | Build beam from the real player | `$H.debug_add_core_items("copper", 200)`; `apply_sim_command_json('{"op":"place","x":134,"y":58,"block":<id>,"rotation":0,"team":1,"player":0}')` with `<id>` = `$H.content_list("block").find("conveyor")` (`block` is a NUMERIC id, the key is `rotation`; corrected 2026-10-14); screenshot within 30 ticks | yellow beam fan from the alpha to the plan |
| e | Cracks | `$H.place_block(126,60,"copper-wall")`, damage it: spawn a team-2 dagger next to it (`spawn_unit`, §4.2) and step 120; or use `debug_kill_building` only to check removal. Screenshot at zoom 4 (`$C.set_camera_scale(4)` if that #[func] exists; check with grep) | grey crack overlay on the wall |
| f | Enemy team border | derelict `scrap-wall`s near (169,167): `$C.center_on_tile(169,167)`; screenshot | small team-coloured corner mark bottom-left of each derelict block |
| g | Door / sorter state | `place_block` a `door` at (124,58), `$I.configure_building(124,58,"bool","true")`; `place_block` a `sorter` (124,60), `configure_building(124,60,"content","copper")` (check the `content` kind syntax in `gdext/input/mod.rs::configure_building`); read back `$H.building_config_json(x,y)` | door shows `door-open`; sorter shows the copper icon. If the readback did not change, configure is broken: record "blocked by H02 (PAR-0190)" — do not fix configure here |
| h | Bridge links | two `bridge-conveyor`s at (120,62) and (123,62); link with `configure_building(120,62,"int", <packed>)` ONLY if H02 has landed its configure fix (`git log main --oneline \| grep -m1 "PAR-0190"`); otherwise skip with "blocked by H02" | bridge line + arrows between them |
| i | PAR-0055 sprite scale | screenshot the alpha at camera scale 4; compare with Java `runs/20261009-065416-ref-ground-zero/java/r02-03-stable.png` (alpha at the core) | alpha body ≈ same screen size (within 10%) |
| j | Duo range circle radius | `$I.select_block_by_name("duo")`, warp the cursor next to the core, screenshot; measure the dashed circle radius in screen px and divide by the camera zoom (`return $C.camera_state_json()`) | radius ≈ 110 world units (`Blocks.java` duo `range = 110`) |

**Java.** Facts quoted in the table; no new code expected.

**Port today.** Code for every row exists (round-G G03 T5/T7/T8/T9).

**Changes.** Only fixes < 30 lines inside H03 files; every other failure goes
into the summary with an owner.

**Tests.** None new unless you fix something (then add a unit test next to the
code you change).

**Verify in-engine.** The table is the verification. Panic gate after the
session (`grep -c panic` = 0).

**Pitfalls.** A wrong-typed argument wedges the frame loop (H-environment §4);
check every `#[func]` with `grep -rn -B1 "fn <name>"` before calling it.
`configure_building` on a bridge hung a round-G session — only do row (h)
after H02 lands.

**Commit.** If nothing was fixed: no commit; record evidence in the summary.
Otherwise `Gameplay H03 T2: <what you fixed>`.

---

### T3 — PAR-0095 command-mode panel (+ PAR-0157 attack reticle)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED. `$I.toggle_command_mode()` -> `command_mode()` true, the block palette stays (`H03-T3-command-mode.png`); none of `set_unit_command`, `set_unit_stance_named`, `select_only_type`, `deselect_type`, `command_panel_json` exist. Java NOT observed: in the seeded Java Ground Zero no keyboard input reached the client on `:11` (Shift hold and WASD had no effect, clicks worked); source citations stand.

**Symptom / acceptance.** Holding the command key (Shift by default) keeps the
block palette; Java swaps it for the command table (the Ground Zero tutorial
has a command-mode objective). After: in command mode the palette area shows
`Command Mode` (accent), an accent rule, and either `[no units]` (light grey)
or one icon + count per selected unit type (left-click keeps only that type,
right-click removes it), then the command buttons (50x50, checked when any
selected unit runs that command) and the stance buttons. Clicking a command
changes the selected units' command (`$H.unit_debug_json()` shows it). In
command mode an attacking selected unit's target shows the rotating red
`Drawf.target` reticle.

**Java.**
* `J/ui/fragments/PlacementFragment.java:443-453`: `commandTable = new
  Table(Tex.pane2)`; `mainStack.addChild(control.input.commandMode ?
  commandTable : blockCatTable)`; command table forced to the block table
  width.
* `:474-476`: `commandTable.add(Core.bundle.get("commandmode.name"))
  .fill().center()`; `commandTable.image().color(Pal.accent).growX()
  .pad(20f).padTop(0f).padBottom(4f)`.
* `:478-611` unit table: counts per type id (iterate content order); per
  type with count > 0: `StatValues.stack(type, count)` (icon with the count
  bottom-right) `.pad(4)`, 7 per row, tooltip `type.localizedName`; left
  click -> `selectedUnits.removeAll(u -> u.type != type)`, right click ->
  `removeAll(u -> u.type == type)`. Command union over the selected types in
  first-seen order; if `commands.size > 1`:
  `coms.button(Icon.icons.get(command.icon, Icon.cancel),
  Styles.clearNoneTogglei, () -> Call.setUnitCommand(player, ids of units
  whose type allows it, command)).checked(i -> activeCommands.get(command.id))
  .size(50f).tooltip(command.localized())` (`:579-581`), 6 per row; stances
  the same (`:602-603`, toggles `!activeStances.get(stance.id)`), separated
  by `image().color(Pal.gray).height(3).pad(7)` when both rows exist; with no
  selection: `u.add(Core.bundle.get("commandmode.nounits"))
  .color(Color.lightGray).growX().center().pad(6)` (`:611`).
* `:619-630`: `activeCommands` = commands of the selected units' CommandAI.
* Bundle (`REPO/assets/bundles/bundle.properties:455-456`):
  `commandmode.name = [accent]Command Mode`, `commandmode.nounits = [no units]`.
* `J/input/InputHandler.java:1225-1237` `drawCommandedTargets` (at
  `Layer.overlayUI`, called from `:1221`): for selected units whose
  `CommandAI.attackTarget != null && currentCommand().drawTarget`:
  `Drawf.target(attackTarget.x, attackTarget.y, 6f, Pal.remove)`.
* `J/graphics/Drawf.java:279-289` `target(x, y, rad, alpha, color)`:
  stroke 3, `Pal.gray` (`454545`): `Lines.poly(x, y, 4, rad, t*1.5)` +
  `Lines.spikes(x, y, 3/7*rad, 6/7*rad, 4, t*1.5)`; then stroke 1 in
  `color` (`Pal.remove` = `e55454`) the same two calls. Arc
  `Lines.spikes(x,y,radius,length,n,rot)` (`ARC/arc-core/src/arc/graphics/
  g2d/Lines.java:372-383`): for `i in 0..n`: line from
  `trns(i*360/n + rot, radius)` to `trns(i*360/n + rot, radius + length)`.
  `Lines.poly(x,y,4,rad,angle)`: closed polygon of 4 vertices at
  `angle + i*90`.

**Port today.**
* `ui/fragments/placement_fragment.gd` has no command branch (line 6 comment
  only). Scene `scenes/ui/fragments/placement_fragment.tscn`
  `Panel/Frame[TopTable, TopDivider, Layout]`.
* `MindInput` (`gdext/input/mod.rs`): `command_mode()` (#[func] ~1032),
  `toggle_command_mode()` (~1025), `selection_json()` (~1050),
  `set_unit_stance(stance: i64, enabled)` (~1068), signal
  `selection_changed(units_json)` (~258). NOT present: `set_unit_command`,
  `set_unit_stance_named`, `select_only_type`, `deselect_type`
  (`grep -n "fn set_unit_command\|fn set_unit_stance_named" gdext/input/mod.rs`
  -> nothing).
* `DesktopBridge::command_units(command: u16)` is private
  (`gdext/input/desktop.rs:965`); `set_stance(stance, enabled)` is `pub`
  (~980). Both emit `RemoteAction::SetUnitCommand/SetUnitStance` over
  `self.controller.state.selected_units` (`EntitySeq` ids).
* Content: `content.unit_command(id)`, `unit_command_by_name(name)`,
  `unit_stance(id)`, `unit_stance_by_name(name)` (`core/content/load.rs`
  ~1018-1037); `UnitCommandDef { name, icon, draw_target, localized_key, ..}`
  (`core/content/registries/commands.rs:43-74`); `UnitTypeDef.commands:
  Vec<UnitCommandId>`, `stances: Vec<UnitStanceId>`, `default_command`
  (`core/content/registries/units/mod.rs:1238-1242`);
  `core/ai/unit_command_runtime.rs`: `allows_command(def, id)`,
  `default_command(..)`, `get_unit_stances(def, content, &mut out)`.
* Live state: per-unit stances = component
  `core/ai/unit_stance_runtime.rs::UnitStancesComp(StanceBits)` (`.0.get(id)`);
  per-unit command: find how `unit_debug_json` reports a unit's command
  (`grep -n "command" core/sim/*.rs | grep -i json`) and read it the same way;
  `Sim::unit_command_state(id) -> Option<&CommandAiState>` (`command:
  Option<UnitCommandId>`) is the relay state; when both exist prefer what
  `unit_debug_json` uses. Unit with no explicit command = its type's
  `default_command` (Java `CommandAI.currentCommand()`).
* G01 T16 landed `#[func] command_targets_json()` (sim_host.rs:952,
  `[{"id","x","y","attack"}]`, `attack` = has an attack target). Unused.
* `gdext/render/command_overlay.rs` draws rings/lines at `Layer.plans`; no
  reticle.

**Changes.**
1. `gdext/input/desktop.rs`: make `command_units` `pub(crate)`. Add:
   ```rust
   /// `PlacementFragment` unit-type filter (left click keeps `keep`, right
   /// click drops `drop`); `type_of` maps a selected id to its unit type id.
   pub(crate) fn retain_selection_types(&mut self, type_of: &dyn Fn(i32) -> Option<i32>,
       keep: Option<i32>, drop: Option<i32>) -> bool
   ```
   that filters `self.controller.state.selected_units` and returns whether
   anything changed.
2. `gdext/input/mod.rs`, new `#[func]`s (**signatures are fixed — H08 reuses
   them, it must not redefine them**):
   ```rust
   /// `Call.setUnitCommand(player, selected ids, command)` by UnitCommand name
   /// ("move", "repair", ...). False when nothing is selected or the name is unknown.
   #[func] pub fn set_unit_command(&mut self, command: GString) -> bool
   /// `Call.setUnitStance(player, selected ids, stance, enabled)` by stance name.
   #[func] pub fn set_unit_stance_named(&mut self, stance: GString, enabled: bool) -> bool
   /// Left click on a type icon: keep only units of `type_id`.
   #[func] pub fn select_only_type(&mut self, type_id: i64) -> bool
   /// Right click on a type icon: drop units of `type_id`.
   #[func] pub fn deselect_type(&mut self, type_id: i64) -> bool
   /// The building whose config panel is open (`input.config.getSelected()`),
   /// as (x, y) tile; None when the panel is hidden. Rust-only accessor used by
   /// T5 `OverlayTop` and H06 T12; also exposed for probes:
   pub fn config_selected(&self) -> Option<(i32, i32)>
   #[func] pub fn config_selected_json(&self) -> GString  // "[x,y]" or "null"
   ```
   Name resolution through the SimHost content snapshot
   (`host.bind().content_registry()`), then
   `self.bridge.command_units(def.id.raw())` / `self.bridge.set_stance(..)`
   (check the bridge field name: `grep -n "bridge" gdext/input/mod.rs | head`).
   `select_only_type`/`deselect_type` build `type_of` from
   `host.bind().selectable_units_json()` rows (`id`, `type`) parsed once per
   call, call `retain_selection_types`, then emit `selection_changed` exactly
   like the existing selection edits (find the emitter:
   `grep -n "selection_changed" gdext/input/mod.rs`).
   `config_selected`: add a field `config_tile: Option<(i32, i32)>`; set it
   in the `Effect::OpenBlockConfig { x, y, .. }` arm (mod.rs ~600); in the
   accessor return it only if the node at `BLOCK_CONFIG_PATH` exists and its
   `visible` property is true (else None). Never hold a bind across the node
   read.
3. `gdext/sim_host.rs` `#[func] command_panel_json(ids_json: GString) ->
   GString` returning
   `{"types":[{"type":id,"name":"dagger","count":2}],
     "commands":[{"name":"move","icon":"move","key":"command.move","active":true}],
     "stances":[{"name":"stop","icon":"…","active":false}]}`.
   Put the logic in a pure fn (testable without Godot):
   ```rust
   pub(crate) fn command_panel(content: &ContentRegistry,
       units: &[(i32 /*type id*/, Option<UnitCommandId> /*current*/, StanceBits)])
       -> serde_json::Value
   ```
   Types sorted by type id with counts; commands = union of
   `def.commands` in the order of the sorted types, deduped; `active` = any
   unit whose current command (explicit, else `default_command`) equals it;
   stances = union of `get_unit_stances(def, content, ..)`; `active` = any
   unit's bits contain it. Return `"commands": []` when the union has ≤ 1
   entry (Java shows the row only when `commands.size > 1`); same rule for
   stances (`stances.size > 1`, check `PlacementFragment.java:590-600` for
   the exact condition and copy it).
4. `scenes/ui/fragments/placement_fragment.tscn`: add
   `Panel/Frame/CommandPanel` (`PanelContainer`, `visible = false`) after
   `Layout`, child `Rows` (`VBoxContainer`). Stylebox: the atlas `pane-2`
   ninepatch (12 px margins) — copy how `_setup_top_style` styles the top
   table.
5. `ui/fragments/placement_fragment.gd`:
   * `_process`: poll `get_node("/root/Spine/Input").command_mode()`; on a
     change toggle `Layout.visible` / `CommandPanel.visible` and set
     `CommandPanel.custom_minimum_size.x = Layout.size.x`.
   * Rebuild `Rows` on `selection_changed` and at most every 250 ms when
     `command_panel_json(selection_json())` text differs from the last one:
     title `MindWidgets.label(MindWidgets.markup("@commandmode.name"))`
     centred; a 4 px `ColorRect` `Color("ffd37f")` with 20 px side margins
     (wrap in a `MarginContainer`); a `GridContainer` (`columns = 7`) of type
     cells: `TextureRect` with `MindWidgets.icon_texture("unit-%s-ui" % name)`
     32x32 and a count `Label` (outline font) anchored bottom-right;
     `gui_input`: left press -> `Input.select_only_type(type)`, right press ->
     `deselect_type(type)`; commands `GridContainer columns = 6` of
     `MindWidgets.glyph_button(icon, "?", "clearNoneTogglei")`,
     `custom_minimum_size = Vector2(50, 50)`, `toggle_mode = true`,
     `set_pressed_no_signal(active)`, tooltip `_t("@" + key)`,
     `pressed -> Input.set_unit_command(name)`; a 3 px `ColorRect`
     `Color("454545")` separator (7 px margins) when both rows exist; stance
     grid the same with `set_unit_stance_named(name, not active)`. With no
     types: `MindWidgets.label(MindWidgets.markup("@commandmode.nounits"))`,
     `modulate = Color(0.75,0.75,0.75)`, centred, 6 px margin. Every node
     built in code carries `# code-instantiated: count driven by the
     selection`.
6. `gdext/render/canvas.rs`: add
   ```rust
   /// `Drawf.target` (Drawf.java:279-289): gray stroke-3 then `color` stroke-1
   /// square (`Lines.poly(x,y,4,rad,rot)`) + 4 spikes from 3/7 rad to 9/7 rad.
   pub(crate) fn target(canvas: &mut Node2D, x: f32, y: f32, rad: f32, rot: f32, alpha: f32, color: Color)
   ```
   (gray = `Color::from_rgba(0.271, 0.271, 0.271, alpha)`; spikes: inner
   radius `3/7*rad`, outer `3/7*rad + 6/7*rad`).
7. `gdext/render/command_overlay.rs`: when `command_mode()` is true, read
   the targets. `command_targets_json()` is a private `fn` (#[func] for
   GDScript only), so add a Rust-only `pub fn command_targets(&self) ->
   Vec<(i32, f32, f32, bool)>` on `MindSimHost` forwarding to
   `self.sim.command_targets()` and use that (no JSON). Filter to selected ids
   (`MindInput.selection_json()` already read by this pass — reuse its
   source) and rows with `attack == true` whose unit's current command has
   `draw_target` (look the command up as in step 3). Draw
   `canvas::target(x, y, 6.0, t * 1.5, 1.0, color_u32(0xe55454ff))` with
   `t` = view ticks. This must draw at `Layer.overlayUI`: the existing
   overlay node is in the `Plans` band, so add a second `CommandOverlay`
   instance in the `OverlayUi` band with a mode flag `reticle_only = true`
   (copy how `unit_overlays::OverlayMode` selects behaviour).

**Tests.**
* `sim_host.rs` tests (end of file): `command_panel_orders_types_and_unions_commands`:
  build the test registry (`create_base_content()`), units
  `[(dagger,None,0),(dagger,None,0),(nova,None,0)]` -> types `[dagger x2,
  nova x1]` in id order; commands = dagger's list then nova's extra entries,
  no duplicates (read both lists from the registry in the test, do not
  hard-code); `active` true for each type's default command.
* `canvas.rs` has no Godot-free tests; put the spike geometry in a pure fn
  `target_spikes(x, y, rad, rot) -> [(f32,f32,f32,f32); 4]` and test
  `rad 7, rot 0` -> first spike `(x+3, y) .. (x+9, y)`.

**Verify in-engine.** Ground Zero after H00. Spawn two team-1 daggers next to
the core (`spawn_unit` with `"team":1`, unit index from
`content_list("unit").find("dagger")`). `$I.toggle_command_mode()`. Select
them with a box drag through `godot_input` mouse events (press at the screen
point of tile (127,53), release at (131,57); points from
`$C.tile_to_screen`). Screenshot `panel.png`: dagger icon + `2`, command row.
Then `$G --eval 'return get_node("/root/Spine/Input").set_unit_command("move")'`
-> true, and `$H.unit_debug_json()` shows the command. Attack reticle: spawn
a team-2 dagger 10 tiles away, right-click it in command mode
(`godot_input` `MOUSE_BUTTON_RIGHT` at its screen point), step 10, screenshot
`reticle.png`: red rotating square with spikes on the enemy. Compare panel
layout against Java `runs/20261010-234250-ref-PAR-0070-combat-fx/java/` (no
command-panel frame exists; describe differences instead).

**Pitfalls.** `command_mode()` is false while a block is selected
(`desktop.rs` test `command_mode_ignored_while_placing`); the panel follows
`command_mode()` only. Selection ids are `EntitySeq` values, not ECS
entities. A synthetic GUI click may not reach Controls (H-environment §4):
call the button's handler from an eval to prove the wiring if clicks fail,
and note it.

**Commit.** `Fixes PAR-0095: command-mode panel with unit types, commands and stances; PAR-0157 attack reticle`

---

### T4 — PAR-0096 minimap attack indicators: feed, Java timing, black-crop diagnosis

> **In-engine verification (2026-10-14, session 2):** CONFIRMED, and the black-crop root cause is found (it is NOT a limit-area change). After the core took fire, `$H.drain_build_damage_json()` returned 1174 undrained rows and `MinimapProvider.indicators_json()` was `"[]"` (no feed); `minimap.rs:116-123` ages in seconds as stated. The minimap is black on a plain Ground Zero entry (`H03-T4-gz-baseline-black-minimap.png`; Java shows terrain, `java-05-gz-in.png`). Probe: `region()` = `{x:65.5, y:128, w:128, h:128}` (top-first rows), texture pixel (129,55) = core colour `ffd37f`, pixel (129, 255-55) black, (120,60) snow. Cause: `MindMinimap::rebuild_all` (`gdext/render/minimap.rs` ~181-188) appends rows bottom-first (`for y in 0..height`; the comment "Top row first" is false) while `update` (~251) writes `height - 1 - y` and `region_for_camera` (`core/render/minimap.rs:26-38`) returns top-first rows -> the crop samples world y 128..255 (outside the play area, dark). Fix: build rows top-first in `rebuild_all` (`for y in (0..height).rev()`); add the `limit_area_rows_match_texture_rows`-style test on the row order. Step 4's recipe cannot run as written: `MindCampaign.set_rules_json` refuses campaign edits, and `grid_limit_area_json`/`limit_area_json` do not exist — `$H.tile_debug(x,y).limit_area` already returns `[90,20,70,60]` on Ground Zero.

**Symptom / acceptance.** Enemy fire on your buildings never pulses the
minimap. After: a damaged player building shows an orange square pulsing on
the minimap that turns scarlet over ~1.2 s (70 ticks) and disappears 15 s
(900 ticks) after the last hit; indicators freeze while the game is paused.
Also: explain (and fix if inside H03) the all-black minimap crop seen after a
limit-area change.

**Java.**
* `J/core/Control.java:68-72`: `Events.on(BuildDamageEvent.class, e -> { if
  (e.build.team == Vars.player.team()) indicators.add(e.build.tileX(),
  e.build.tileY()); })`.
* `J/game/AttackIndicators.java:11` `duration = 15f * 60f`; `:24-36` `add`
  restarts an existing row's time; `:38-55` `update`: `time += Time.delta`
  (ticks), remove at `>= duration`. Called only `if(!state.isPaused())`
  (`Control.java:709-712`).
* `J/graphics/MinimapRenderer.java:253-274`: `fin = (Time.globalTime / 30f)
  % 1f`; `rad = fin * 5 + tilesize - 2`; stroke `(1 - fin) * 4 + 0.5`;
  colour `Color.orange.lerp(Color.scarlet, clamp(time / 70f))`; square at
  `(ix + 0.5 + offset) * tilesize` where `offset = build.block.offset /
  tilesize` (multiblock centre).

**Port today.**
* Feed: `#[func] drain_build_damage_json()` (sim_host.rs:977,
  `[[tile_x, tile_y, team], ...]`, G01 T15) has no caller;
  `core/world/events.rs` caps the log when undrained.
  `MindMinimap::add_indicator(x, y)` (#[func], `gdext/render/minimap.rs:390`).
* Timing BUG: `minimap.rs:116-123` ages rows by `delta` **seconds** with
  lifetime `15*60` = 900 s; `ui/widgets/mind_minimap.gd:113-136` blends
  `age / 70` with age in seconds; no multiblock offset; ages while paused.

**Changes.**
1. `gdext/render/minimap.rs::process`: age in ticks:
   `*age += delta as f32 * 60.0` and lifetime `INDICATOR_DURATION_TICKS:
   f32 = 15.0 * 60.0` (cite `AttackIndicators.java:11`); skip ageing when
   `host.bind().is_paused()` (check the Rust accessor name:
   `grep -n "fn is_paused" gdext/sim_host.rs`). Fix the comment.
2. Same `process`: once per frame, when the host exists, drain the log
   through a new Rust-only `pub fn drain_build_damage(&mut self) ->
   Vec<(i32, i32, u8)>` on `MindSimHost` (forward to
   `self.sim.drain_build_damage_events()`; keep the #[func] JSON one for
   probes) and `add_indicator(x, y)` for rows with
   `team == host.player_team()`. Take the events inside a `bind_mut()`
   block, drop the bind, then mutate `self` (no bind held across).
   This makes the minimap the single drain consumer (document it on both
   seams; the T9 core-attack banner uses a health diff and does not drain).
3. `indicators_json` rows gain a 4th value `offset` (`def.offset /
   TILESIZE`, 0 for odd sizes; read the block at the tile through the host).
   `mind_minimap.gd`: `_map_point(.., row[0] + offset, row[1] + offset)` and
   the colour lerp `clampf(float(row[2]) / 70.0, ..)` stays (age is now in
   ticks). `fin` stays `fmod(now_s * 2.0, 1.0)` (globalTime/30 at 60 tps).
4. **Black crop diagnosis (no blind fix).** (2026-10-14: root cause found without a rules change — see the verification note above: `rebuild_all` row order. `set_rules_json` refuses campaign edits, so the reproduction below cannot run.) Reproduce: Ground Zero, then
   force a limit area through the rules seam
   `$MC.set_rules_json(...)` with `limitMapArea=true, limitX=100,
   limitY=30, limitWidth=60, limitHeight=50` (read the current JSON first
   with `get_rules_json()` and edit only those keys; check the key names in
   `core/io/json/rules.rs` `limit_*` fields' serde names). Then probe:
   * `return get_node("/root/Spine/World/MinimapProvider").region()`
   * `return $H.grid_limit_area_json()` — if no such seam exists, read
     `world.limit_area` through `tile_debug` or add a Rust `#[func]
     limit_area_json()` on `MindSimHost` returning `[x,y,w,h]` or `null`
     (one-line probe, keep it).
   * `minimap_color_at(cx, cy)` at the region centre.
   Expected: the region rect (texture rows, top-first) intersects the limit
   rect after converting the limit rect to texture rows
   (`ty = height - 1 - y`). If the region lies outside the limit rect the
   crop is legitimately black (the camera is outside the play area — Java
   clamps the camera to the limit area: check `J/core/Renderer.java` /
   `J/input/InputHandler` for `limitMapArea` camera clamping with
   `grep -rn "limitMapArea\|limitX" J/core J/input J/graphics`); if Java
   clamps the camera and the port does not, that is an H08 camera finding
   (record it). If the region overlaps the play area but pixels are black,
   the y-flip of `limit_area` in `MindMinimap::tile_color` vs
   `region_for_camera` is wrong: fix it here and add a unit test.

**Tests.** `core/render/minimap.rs` (or `minimap.rs` gdext pure helpers):
`indicator_expires_after_900_ticks` (pure fn
`age_indicators(rows, dt_ticks) -> rows`: a row at 899 survives +0.5, a row
at 900 is removed). If step 4 finds a flip bug, add
`limit_area_rows_match_texture_rows`.

**Verify in-engine.** Ground Zero: `place_block(140,60,"copper-wall")`,
spawn a team-2 dagger at tile (146,60) (`spawn_unit`, world px = tile*8),
step 120. Screenshot the HUD minimap `indicator.png` (pulsing square near the
wall). `return get_node("/root/Spine/World/MinimapProvider").indicators_json()`
shows a row with age < 900. Step 1000 more -> the row is gone. Java
reference for the look: `runs/20261010-234250-ref-PAR-0070-combat-fx/java/
step-231-eng6-minimap.png` (no indicator visible there; use it for the crop
frame only).

**Pitfalls.** `process` runs every frame; drain every frame (the log is
per-tick and capped). Do not route this through GDScript.

**Commit.** `Fixes PAR-0096: minimap attack indicators fed from BuildDamageEvent with Java tick timing`

---

### T5 — Overlay top: enemy indicators, configure outline, enemy core radius, drop-zone circle, hover range (+ H06 hooks)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED. No overlay-top pass (`gdext/render/` has no `overlay_top.rs`); hovering a placed player duo at (137,74) with no block selected draws no range (`H03-T5-hover-duo.png`).

**Symptom / acceptance.** Hovering your duo shows no range; the building
whose config panel is open has no accent outline; while placing, enemy cores
show no "no-build" circle; approaching the enemy spawn shows no dashed
drop-zone circle; off-screen enemies give no edge indicators around the
player. After: all five match Java, and H06 can plug logic overlays into the
same pass.

**Java.** `J/graphics/OverlayRenderer.java:91-226` (`drawTop`), constants
`:24-25` `indicatorLength = 14f`, `spawnerMargin = tilesize*11f` (= 88):
* `:93` everything below the indicators needs `!player.dead() &&
  ui.hudfrag.shown()`.
* `:110-121` enemy indicators: setting `indicators` (default true, port
  `core/ui/settings.rs:243`) and `!state.rules.fog`: for each unit not local,
  `team != player.team()`, outside the camera rect scaled 0.9 around the
  camera centre: `v = (unit - player).setLength(14)`;
  `Lines.stroke(1, unit.team.color)`; `Lines.lineAngle(player.x + v.x,
  player.y + v.y, v.angle(), 3)`.
* `:94-108` player indicators (other players) — multiplayer, skip.
* `:153-157` `if(input.config.isShown()) config.getSelected().drawConfigure()`;
  generic `BuildingComp.drawConfigure` (`BuildingComp.java:1732-1737`):
  `Draw.color(Pal.accent); Lines.stroke(1f); Lines.square(x, y, block.size *
  tilesize / 2f + 1f)`.
* `:164-198` `buildFade = lerpDelta(buildFade, isPlacing ? 1 : 0, 0.06)`;
  stroke `buildFade * 2`; if `buildFade > 0.005` and not
  `rules.polygonCoreProtection` (default false): for each enemy core with
  `br = rules.buildRadius(core.team)` (`teamRule.protectCores ?
  enemyCoreBuildRadius (400) + extraCoreBuildRadius : 0`, Rules.java) `> 0`
  overlapping the camera: `color(Color.darkGray)` `Lines.circle(core.x,
  core.y - 2, br)`, then `color(Pal.accent.lerp(core.team.color, 0.5 +
  absin(Time.time, 10, 0.5)))` `Lines.circle(core.x, core.y, br)`.
* `:200-211` spawns: `stroke 2`; colour `Color.gray.lerp(Color.lightGray,
  absin(Time.time, 8, 1))`; for each ground spawn within `dropZoneRadius +
  88` of the player: `alpha = clamp(1 - (dst - dropZoneRadius) / 88)`;
  `Lines.dashCircle(spawn.worldx, spawn.worldy, dropZoneRadius)`.
* `:213-229` hover (`input.block == null && !scene.hasMouse()`): the player-
  team building under the cursor: `build.drawSelect()`; if `!build.enabled
  && block.drawDisabled`: `drawDisabled()`.
  `drawSelect` overrides in scope: `BaseTurret` `Drawf.dashCircle(x, y,
  range(), team.color)`; `MendProjector`/`OverdriveProjector`
  `Drawf.dashCircle(x, y, range, baseColor)` (mender/mend-projector
  `84f491`, overdrive `feb380`); `PowerNode` (`PowerNode.java:445-453`)
  `Lines.stroke(1f); Draw.color(Pal.accent); Drawf.circles(x, y, laserRange *
  tilesize)`.
* Arc colours: `Color.darkGray (0.25,0.25,0.25)`, `gray (0.5,0.5,0.5)`,
  `lightGray (0.75,0.75,0.75)`.

**Port today.** No overlay-top pass. `gdext/render/overlays.rs` (core edges,
unused — polygon protection is off by default, leave it). `core/render/
overlay.rs` holds `INDICATOR_LENGTH = 14` / spawner-margin geometry. Data:
`MindSimHost::player_unit_position()` (sim_host.rs:2519),
`player_team()` (:2545), `live_rules()` (:2746, clones `Rules` — expensive,
never per frame), spawns via `self.sim.unit_runtime().wave_spawner.spawns`
(see `player_near_drop_zone` :2715), core enumeration: copy the core loop of
`light_sources` (:2761). Place-range table: `block_place_range(block)`
(:4732) — reuse it for the hover range (turrets from the runtime cache,
projector table, power-node laser ranges). Placing state:
`MindInput::placement_view()` (mod.rs ~1038, `cursor`, `block`).
`MindInput::config_selected()` comes from T3 step 2.

**Changes.**
1. `core/render/rules.rs` `RulesRenderView`: append fields
   `drop_zone_radius: f32`, `enemy_core_build_radius: f32`,
   `polygon_core_protection: bool`, `unit_light: bool` and a
   `protect_cores: [bool; 256]`-free representation: `core_build_radius:
   Vec<(u8 /*team*/, f32)>` filled in `from_rules` from `rules.teams` (team
   rule `protect_cores` -> `enemy_core_build_radius +
   extra_core_build_radius`, else 0; default team rule when absent = protect
   true). Default view: Java defaults (400, false, 300 drop-zone — read
   `Rules::default()` values, do not hard-code). The renderer already polls
   this every 30 frames.
2. `gdext/sim_host.rs` Rust-only seam:
   ```rust
   pub struct OverlayView {
       pub player: Option<(f32, f32)>, pub player_team: u8,
       pub enemy_units: Vec<(f32, f32, u8)>,      // non-player-team units
       pub enemy_cores: Vec<(f32, f32, u8)>,      // core centre, team
       pub spawns: Vec<(f32, f32)>,               // ground spawn world px
   }
   pub fn overlay_view(&self) -> OverlayView
   ```
   (units from `unit_draws_full()`/the same rows `minimap_units_json` uses;
   one host bind per frame).
3. Same file: the extension-point enums and one seam each (**H06 T12 adds
   its variants' data and drawing; keep the enum names exactly**):
   ```rust
   /// `Building.drawConfigure` dispatch (BuildingComp.java:1732-1737 generic square).
   pub(crate) enum ConfigOverlay { Generic { x: f32, y: f32, size: i32 },
       PowerNodeLinks { x: f32, y: f32, size: i32, laser_range: f32, links: Vec<(f32, f32, i32)> },
       BridgeLink { x: f32, y: f32, size: i32, range: i32, targets: Vec<(f32, f32, bool /*linked*/)> },
       Logic /* H06 T12 fills */ }
   /// `Building.drawSelect` dispatch for the hovered player-team building.
   pub(crate) enum SelectOverlay { Range { x: f32, y: f32, range: f32, color: u32, circles: bool },
       MessageText /* H06 T12 */, LogicTag /* H06 T12 */, None }
   pub(crate) fn config_overlay(&self, x: i32, y: i32) -> Option<ConfigOverlay>
   pub(crate) fn select_overlay(&self, x: i32, y: i32) -> SelectOverlay
   ```
   `config_overlay`: power nodes -> `PowerNodeLinks` (links from the node's
   power-graph link list: `grep -n "links" core/world/power*.rs` /
   `PowerModule`); item bridges (`BlockKind::ItemBridge` or the bridge kind
   name in `core/world/blocks/distribution/item_bridge.rs`, `range` from its
   config) -> `BridgeLink` with candidate tiles `1..=range` in 4 directions
   that hold the same block and team, `linked` = equals the configured link;
   everything else -> `Generic`. `select_overlay`: `block_place_range` hit ->
   `Range{circles: kind == PowerNode}`; else `None`.
4. New `gdext/render/overlay_top.rs` `OverlayTop` (§0 template) in the
   `Layer::OverlayUi` band. Fields: `build_fade: f32`, cached settings
   `indicators: bool`. `process`: `build_fade += (target - build_fade) *
   (1 - (1-0.06)^delta_ticks)` with target 1 when
   `placement_view().block` is a real block (not air/None); queue_redraw.
   `draw` (Java order): indicators -> configure -> core radius -> spawns ->
   hover select. Mouse tile for hover: `placement_view().cursor`; skip hover
   when a block is selected or `get_viewport().gui_get_hovered_control()`
   is a Control whose `mouse_filter == STOP` (same rule as
   `placement_fragment.gd::_mouse_tile`). Configure: `MindInput::
   config_selected()` -> `config_overlay` -> draw:
   * Generic: accent `ffd37f` stroke 1 square half-size `size*4 + 1`.
   * PowerNodeLinks (`PowerNode.java:458-480`): `canvas::circles(x, y,
     size*4 + 1 + absin(t,4,1), accent)`; `circles(x, y, laser_range*8,
     accent)`; for each link `square(lx, ly, lsize*4 + 1, 45°, Pal.place
     6335f8)` (Drawf.square: stroke 3 gray then stroke 1 colour, radius+1,
     `Lines.square(x,y,r,rot) = poly(x,y,4,r,rot-45)`; add
     `canvas::square(canvas,x,y,r,rot,color)` per `Drawf.java:426-438`).
   * BridgeLink (`ItemBridge.java:282-296`): `canvas::select(x, y,
     size*4 + 2, accent)`; each target `select(tx, ty, size*4 + 2 + (linked
     ? 0 : absin(t,4,1)), linked ? Pal.place : Pal.breakInvalid d44b3d)`;
     `Drawf.select` (`Drawf.java:418-424`): stroke 3 gray axis-aligned square
     half-width `r+1`, then stroke 1 colour half-width `r` (add
     `canvas::select`).
   * Logic: call `super::logic_overlay::draw_config(..)` only if that module
     exists (H06 creates it) — until then the arm draws the Generic square.
   Indicators: camera rect = `visible_rect()` shrunk to 0.9 around the
   centre; skip when `rules.fog`. Core radius: `stroke = build_fade * 2`,
   colours as Java; `Lines.circle` = 64-segment `draw_arc`. Spawns: alpha per
   Java; `canvas::dash_circle` with stroke 2 (check whether `dash_circle`
   takes a stroke — add a `_stroke` variant if not). Hover: `Range` ->
   `dash_circle(x, y, range, team colour or projector colour)` or
   `circles(..)` for power nodes; `drawDisabled`: two scarlet lines
   `(ff341c, a 0.8)` forming an X of half-size 3 at the centre (deviation:
   Java draws the `Icon.cancel` glyph; note it).
5. Do not delete `overlays.rs`.
6. **Placement range circle (T2 row j, folded in 2026-10-14).** Symptom: with
   `duo` selected the ghost follows the cursor but no dashed range circle is
   drawn (`runs/20261014-s2-verify2/H03-T2j-duo-range3.png`); Java
   `drawPlace` of turrets draws `Drawf.dashCircle(x, y, range, Pal.placing)`
   (read `J/world/blocks/defense/turrets/BaseTurret.java` `drawPlace` for the
   exact call). Diagnose in this order (stop at the first hit, fix there,
   unit-test the fix):
   a. Probe `block_place_range(<duo id>)` (temporary `log::debug!` in
      `render/plans.rs` ~270, or a debug `#[func]` on `MindSimHost`). `None`
      -> the turret-config lookup (`sim_host.rs` ~4761-4795, `config_for`
      against the runtime content, then the snapshot) fails for `duo`: fix it.
   b. `Some((110, Turret, ..))` but nothing visible -> follow the
      `DrawCmd::DashCircle` from the cursor branch (`plans.rs` ~259-290) to the
      draw loop (~348 `canvas::dash_circle`): check it is not filtered, its
      colour alpha/stroke, and that the pass node's band is visible.
   c. `placement_preview_json()` returns `"[]"` and
      `get_input_state_json().cursor` is null while the ghost is drawn —
      confirm which cursor the plans pass reads (`view.cursor`) and that the
      range branch uses the same one.
   Verify: `$I.select_block_by_name("duo")`, `godot_input` `mouse_motion` to
   `$C.tile_to_screen(122,62)`, screenshot `place-range.png`: dashed circle of
   radius ~110 world (~440 px at camera scale 4).

**Tests.** `core/render/overlay.rs`: `indicator_segment` (player (0,0),
enemy (100,0) -> segment (14,0)..(17,0)); `spawn_alpha` (drop radius 300,
distance 344 -> 0.5); `core/render/rules.rs`: `from_rules_core_radius`
(default rules -> 400 for team 2; a team rule with `protect_cores = false`
-> 0).

**Verify in-engine.** Ground Zero: `place_block(132,52,"duo")`,
`$I.select_block_by_name("air")`, move the mouse over it (`godot_input`
`mouse_motion` if supported, else a left-press/release elsewhere first —
check `tools/gmcp.py --help`), screenshot `hover-range.png`: dashed circle
radius ≈ 110 world. Indicators: spawn a team-2 dagger 60 tiles north of the
player, screenshot `indicator.png`: a 3-px line 14 px from the alpha pointing
north. Configure: place a `power-node` at (128,60), open its config
(left-click its screen point), screenshot `config.png`: pulsing accent
circles. Drop-zone/enemy core: headless only on Ground Zero (no enemy core;
spawn far) unless an unlocked preset has `enemyCores > 0`: try 23, 41, 97
(`$MC.get_sector_state()` after loading); if found, select a block and
screenshot `core-radius.png`.

**Pitfalls.** Collect all data in one host bind, drop it, then draw.
`config_overlay` must bounds-check tiles (`tile_at`). Java hides
everything after `:160` when the player is dead — gate on `player.is_some()`.

**Commit.** `Gameplay H03 T5: overlay top (indicators, configure outline, core radius, drop zone, hover range) + ConfigOverlay/SelectOverlay hooks`

---

### T6 — PAR-0246 light composite never visible: diagnose, fix, wire `drawlight`

> **In-engine verification (2026-10-14, session 2):** CONFIRMED, diagnosis SETTLED: the composite is not invisible, it is ADDITIVE. Before: `$R.set_rules_lighting(true, 0.01, 0.01, 0.04, 0.99)` brightens (white glow around the core, nothing darkens), `(true,1,0,0,1)` washes the screen pink (`H03-T6-ambient-forced-before.png`, `H03-T6-ambient-red-before.png`); the light `MeshInstance2D` under `Band_light_0` is visible, in tree, has a ShaderMaterial and a mesh. `WT/assets/shaders/godot/light.gdshader` (gitignored packed copy) is `blend_add`, `client/shaders/light.gdshader` `blend_mix`. Copying the client shader over the packed copy (no rebuild, `--play`) gives a dark screen with the core glow and a red screen for red ambient (`H03-T6-ambient-forced-fixed.png`, `H03-T6-ambient-red-fixed.png`). So step 1 alone fixes PAR-0246; the step-3 table is optional. `set_draw_light` callers in `client/ui`: 0. Correction in place: the MindUi setter is `settings_set(key, value)` (there is no `settings_set_bool`).

**Symptom / acceptance.** Forcing `set_rules_lighting(true, 1, 0, 0, 1)`
(full red ambient) leaves the frame unchanged although `light_rebuilds`
grows (G03 T12). After: with lighting on, the screen is darkened by
`rules.ambientLight` and the core glows accent, lit units/illuminators show
coloured circles; the `drawlight` setting toggles the pass. Applies to
night/dark sectors (any preset whose rules have `lighting`).

**Java.**
* `J/core/Renderer.java:175` `drawLight = settings.getBool("drawlight",
  true)`; `:419` region: `if(state.rules.lighting && drawLight)
  Draw.draw(Layer.light, lights::draw)`.
* `J/graphics/LightRenderer.java:126-163` `draw()`: buffer at 1/4 screen
  res, circles drawn with `Gl.max` alpha blending as the `circle-shadow`
  region size `radius*2`; then `Shaders.light.ambient = rules.ambientLight;
  buffer.blit(Shaders.light)` with normal alpha blending;
  `core/assets/shaders/light.frag`:
  `gl_FragColor = clamp(vec4(mix(ambient.rgb, color.rgb, color.a),
  ambient.a - color.a), 0, 1)`.

**Port today.**
* `gdext/render/light.rs`: camera-sized RGBA `Image` -> `ImageTexture`,
  `ShaderMaterial(light)` on one `MeshInstance2D` in the `Layer::Light` band,
  quad from `atlas_bind::build_mesh(&[quad], 0.0, false)` centred on the
  camera, `set_texture(&texture)` added by G03 "found in-engine".
* Same band also holds `EnvRenderer`'s `MeshInstance2D` (env.rs:43, attached
  at render.rs ~639) — that is the "second child", not a double build.
* The working reference: `ShadowRenderer`'s darkness composite
  (`render/shadow.rs` ~165-180, shader `darkness.gdshader`): identical
  pattern (`ImageTexture` -> `ShaderMaterial.u_texture`, world quad via
  `build_mesh`, `set_material`) and it IS visible in every screenshot. It
  does **not** call `set_texture`.
* `render/shaders.rs::load` candidates order: `<assets_dir>/<manifest gdshader
  path>` (= `assets/shaders/godot/light.gdshader`), then
  `<assets_dir>/shaders/godot/<name>.gdshader`, then
  `res://shaders/<name>.gdshader`. The packed copy is stale (`blend_add`).
* `drawlight` row exists (`core/ui/settings.rs:229`); nothing calls
  `MindWorldRenderer::set_draw_light`.

**Changes.**
1. **Shader source of truth.** `render/shaders.rs::shader_candidates`: put
   `res://shaders/{name}.gdshader` FIRST, then the asset-tree paths (they
   stay as a fallback for exported builds without `res://shaders`). Update
   the module doc ("committed `client/shaders` wins; the packed copy is a
   fallback"). Add a boot warning: if both exist and their text differs, log
   `[W] [shaders] packed copy of '{name}' is stale; using res://shaders` once
   per name.
2. **Diagnosis probe** (keep it; debug-only): `#[func] light_debug_json(&self)
   -> GString` on `MindWorldRenderer` returning
   `{"active", "node_visible", "visible_in_tree", "band_visible", "z_index",
   "mesh_surfaces", "mesh_aabb":[x,y,w,h], "has_material", "shader_head"
   (first 120 chars of the material's shader code), "u_ambient",
   "texture_size":[w,h], "view":[x,y,w,h], "global_position"}` read from
   `self.light` (add a `pub fn node(&self) -> Gd<MeshInstance2D>` and
   `pub fn material(&self)` accessor on `LightRenderer`).
3. Rebuild, `tools/session-play.sh N`, enter Ground Zero, then:
   ```bash
   R='get_node("/root/Spine/World/Renderer")'
   $G --eval "$R.set_rules_lighting(true, 0.01, 0.01, 0.04, 0.99); return true"
   sleep 1; $G --eval "return $R.light_debug_json()"
   $G --screenshot runs/<date>-s<N>-H03-T6/ambient-forced.png
   ```
   Decide with this table (stop at the first row that matches; write the
   probe output into the commit body):
   | Observation | Cause | Fix |
   |---|---|---|
   | `shader_head` contains `blend_add` | stale packed shader (step 1 not effective) | fix step 1 |
   | `visible_in_tree` false or `band_visible` false | a parent hidden | find who hides it (`set_layer_visible` calls, `apply_visibility`) and fix |
   | `mesh_surfaces` 0 or `mesh_aabb` far from `view` | quad not built / wrong place | fix the quad (must cover `view`) |
   | all above fine, frame still unchanged | canvas path: compare with the darkness node | remove `set_texture` (shadow does not use it), then if still invisible temporarily `set_material(None)` on the light node and set the quad colour red: a red screen proves mesh OK -> the shader output is the problem (check `u_texture` binding / `texture()` returning 0 alpha everywhere -> COLOR alpha = ambient.a should still show) |
   Note: G03 also observed that a script error "freezes the frame loop while
   renders still work" — before trusting "unchanged", confirm
   `$H.get_tick()` advances between two calls 1 s apart; if not, run
   `tools/session-play.sh N --play` and retry.
4. Wire `drawlight`: in `MindWorldRenderer::frame`, every 30 frames (the
   rules poll), read `settings_get_bool("drawlight")` from `/root/MindUi`
   (copy `StatusRenderer::enabled`) into `self.draw_light`.
5. After the fix: `clear_rules_override()` and confirm the live feed turns it
   off again on Ground Zero (rules lighting false).

**Tests.** `render/shaders.rs`: `candidates_prefer_res_shaders`
(`shader_candidates("light", Some("shaders/godot/light.gdshader"),
"/a")[0] == "res://shaders/light.gdshader"`). Keep
`ambient_mix_matches_shader` green.

**Verify in-engine.** Step 3 screenshots: `ambient-forced.png` (dark screen,
accent core glow), then `set_rules_lighting(true, 1, 0, 0, 1)` ->
`ambient-red.png` (red screen). Illuminator: `place_block(126,52,
"illuminator")` + power (`place_block` a `power-source` adjacent — sandbox,
API bypass) + `$I.configure_building(126,52,"int","4278190335")` (0xff0000ff;
check `light_color(126,52)` reads red) -> `illuminator.png` red circle. Then
`clear_rules_override()`; after 1 s the composite is hidden (Ground Zero
rules lighting false). Settings: toggle `drawlight` off via
`get_node("/root/MindUi").settings_set("drawlight", false)` (corrected 2026-10-14: there is no `settings_set_bool`; check the
setter name with grep in `gdext/ui/ui_host.rs`) -> forced lighting no longer
draws.

**Pitfalls.** Never leave the debug override on in a commit's default path.
`light_debug_json` must not bind the host. Java uses `Gl.max` on alpha only;
the port's max on rgb too is a documented approximation (keep it).

**Commit.** `Fixes PAR-0246: light composite visible (res://shaders wins over the stale packed copy), drawlight setting wired`
(If the root cause turned out to be different from the shader, say so in the
subject: `Fixes PAR-0246: <actual cause>`.)

---

### T7 — Fires and puddles are invisible

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (code only): no fire/puddle drawing in `gdext`; `core/combat/fires.rs:172` `fire.animation = (fire.animation + 1.0) % 40.0`; no debug fire/puddle seam (`grep "fn debug_.*fire"` none). In-engine not checked (no seam; the 20261010 Java `step-250-fire*` frames are duo shots, not fires).

**Symptom / acceptance.** Burning tiles (incendiary hits, scorch, spilled oil)
and liquid puddles (leaking conduits, destroyed tanks, water bullets) draw
nothing; only the occasional named FX shows. After: fires show the animated
`fire0..fire39` flame sprite fading in over 20 ticks with a light, and
puddles show liquid-coloured blobs.

**Java.**
* `J/entities/comp/FireComp.java:23,26` `frames = 40`, `duration = 90`,
  `ticksPerFrame = duration / frames` (2.25), `warmupDuration = 20`;
  `:41-43` `animation += Time.delta / ticksPerFrame; animation %= frames`;
  `:103-116` `draw`: `Draw.color(1,1,1, clamp(warmup / warmupDuration))`;
  `Draw.z(Layer.effect)`; `Draw.rect(regions[min((int)animation, 39)], x +
  randomSeedRange((int)y, 2), y + randomSeedRange((int)x, 2))`;
  `Drawf.light(x, y, 50 + absin(5, 5), Pal.lightFlame (ffdd55), 0.6 *
  clamp(warmup / 20))`.
* `J/entities/comp/PuddleComp.java:126-130` `Draw.z(Layer.debris - 1)`
  (= 19); `liquid.drawPuddle(self)`.
* `J/type/Liquid.java:124-149` `drawPuddle`: `f = clamp(amount /
  (maxLiquid / 1.5))` (`maxLiquid = 70`); `smag = tile.floor().isLiquid ?
  0.8 : 0`, `sscl = 25`; `color = liquid.color.shiftValue(-0.05)`;
  `Fill.circle(x + sin(Time.time + id*532, 25, smag), y + sin(Time.time +
  id*53, 25, smag), f * 8)`; `length = f * 6`; `rand.setSeed(id)`; 3 times:
  `v.trns(rand.random(360), rand.random(length))`; `Fill.circle(x + v.x +
  sin(t + i*532, 25, smag), y + v.y + sin(t + i*53, 25, smag), f * 5)`;
  light when `liquid.lightColor.a > 0.001 && f > 0`: `Drawf.light(x, y,
  30*f, lightColor, color.a * f * 0.8)`.

**Port today.**
* `core/combat/fires.rs:42-61` `FireState { tile, time, lifetime, warmup,
  animation, .. }` on fire entities; `:172-173` `animation = (animation +
  1.0) % 40.0` per tick — **BUG**: Java advances `1 / 2.25` per tick (the
  flame cycles 2.25x too fast).
* `core/combat/puddles.rs:28-45` `PuddleState { tile, liquid, amount, .. }`
  on puddle entities.
* No gdext drawing; `T12` light sources do not include fires/puddles.

**Changes.**
1. `core/combat/fires.rs`: `const FRAMES: f32 = 40.0; const DURATION: f32 =
   90.0;` and `fire.animation = (fire.animation + 1.0 / (DURATION / FRAMES))
   % FRAMES;` (cite `FireComp.java:41-43`). Check whether `animation` feeds
   a checksum or save: `grep -rn "animation" core/io core/sim/checksum*`;
   if goldens change, regenerate per H-environment §2 and name the reason.
2. `gdext/sim_host.rs` Rust-only:
   ```rust
   pub struct FireDraw { pub x: f32, pub y: f32, pub frame: i32, pub alpha: f32 }
   pub struct PuddleDraw { pub x: f32, pub y: f32, pub id: i32, pub f: f32,
       pub color: [f32; 4], pub on_liquid: bool, pub light: Option<([f32;4], f32)> }
   pub fn fire_draws(&self, rect: (f32,f32,f32,f32)) -> Vec<FireDraw>
   pub fn puddle_draws(&self, rect: (f32,f32,f32,f32)) -> Vec<PuddleDraw>
   ```
   Iterate entities with `FireState`/`PuddleState` (`world.query::<(Entity,
   &FireState)>()`, sort by `entity.index()` for stable order). Fire
   centre = tile centre `(tx*8, ty*8)` + `seed_range((ty) as i32, 2)`,
   `seed_range(tx, 2)` where `seed_range(seed, r)` = deterministic hash in
   `[-r, r]` (reuse the T4 bullet spin-seed helper if one exists:
   `grep -n "fn seed_range\|randomSeedRange" core/render/*.rs`; else add it to
   `core/render/drawf.rs` with a test). `alpha = clamp(warmup / 20)`.
   Puddle: `f = clamp(amount / (70/1.5))`, colour = liquid colour with value
   shifted by -0.05 (HSV value; add `shift_value` to `core/render/drawf.rs`
   with a test), `on_liquid` = floor is a liquid (`BlockDef.is_liquid` or
   kind `ShallowLiquid/DeepLiquid` as `minimap::is_liquid` does), `id` =
   entity index, light from the liquid's `light_color` field if the
   `Liquid` def has one (`grep -n "light_color" core/content/registries/
   liquids.rs`; if absent, no light and note it).
3. New `gdext/render/hazards.rs` with two nodes (§0 template):
   `PuddleRenderer` in the `Layer::Debris` band if it exists, else the floor
   band directly above the last floor cache layer (check `BandPlan` in
   `core/render/bands.rs` for a `Debris` entry; if none, use
   `Layer::BlockUnder` and document); `FireRenderer` in the `Layer::Effect`
   band. Fire: `canvas::draw_region` of `fire{frame}` (natural size =
   region px * 0.25) modulated by alpha. Puddle: the 4 `draw_circle`s per
   Java with `t` = view ticks; `rand` for the 3 satellites: a tiny
   seeded LCG in the view (document: Java `Rand(id)`; positions need not be
   bit-identical).
4. `MindSimHost::light_sources` (sim_host.rs ~2761): add fires
   (`50 + absin(t,5,5)`, `ffdd55`, `0.6*alpha`) and puddle lights.

**Tests.** `core/combat/fires.rs`: `fire_animation_cycles_every_90_ticks`
(start 0, 90 updates -> back to ~0 within 1e-3). `core/render/drawf.rs`:
`shift_value_darkens` (`(0.5,0.5,1.0)` value 1.0 -> 0.95, hue kept),
`seed_range_is_bounded_and_stable`.

**Verify in-engine.** Ground Zero: start a fire — find a seam:
`grep -rn "fn debug_.*fire\|create_fire" gdext/sim_host.rs`; if none, add a
debug `#[func] debug_create_fire(x: i32, y: i32) -> bool` forwarding to
`core/combat/fires.rs::create` (debug-only, documented). Fire at (128,60),
step 30, screenshot `fire.png`; compare with Java
`runs/20261010-234250-ref-PAR-0070-combat-fx/java/step-250-fire*.png` if
they show fires (open one first). Puddle: same approach with
`core/combat/puddles.rs` deposit fn (`debug_deposit_puddle(x,y,"water",
40.0)`), screenshot `puddle.png`.

**Pitfalls.** Querying the World every frame: collect only inside the camera
rect (grow 16 px). `FireState` lives in the sim World, not `UnitRuntime` —
check where `update_fires` reads it before writing the query.

**Commit.** `Gameplay H03 T7: draw fires and puddles; Java fire animation rate`

---

### T8 — Crafter/generator `DrawBlock` animations in the live block pass

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: powered silicon smelter (efficiency 1.0, warmup 0.38), pulverizer and combustion generator on free ground (134,69)/(138,71)/(138,69) with sandbox `power-source`s: two frames 9 ticks apart are pixel-identical over the crafters (`H03-T8-T9-a.png`, `-b.png`, crop `H03-T8-T9-crop.png`); `vanilla_chain`/`execute_extras` unused in gdext. Coordinates corrected in place: use free ground (132..147, 57..72); `debug_add_building_items` caps at the block's item capacity (10).

**Symptom / acceptance.** Silicon smelters, kilns, pulverizers, combustion
generators, cultivators, … are static icons in the live game although
`core/render/draw_desc.rs` ports their `DrawBlock` chains. After: a working
silicon smelter shows its pulsing flame (`DrawFlame`), a pulverizer its
spinning rotator, a combustion generator its glow, matching Java.

**Java.** `J/world/draw/*.java` (each descriptor cites its Java file in
`draw_desc.rs`); `GenericCrafter.GenericCrafterBuild.draw()` ->
`drawer.draw(this)` with `warmup()`, `progress()`, `totalProgress()`.
Chains (Blocks.java lines cited at `draw_desc.rs:1829-1832`).

**Port today.** `mind_core::render::draw_desc::{vanilla_chain(name),
execute_extras(desc, name, &DrawState, &mut RegionIdTable, &mut
Vec<DescDraw>)}` (draw_desc.rs:1807-1833); `DrawState` fields at :163-200
(`x, y, rotation, rotdeg, size, warmup, progress, total_progress, time, id,
power_status, liquid_capacity, liquid_amount, liquid_color, heat, …`; read
the struct for the full list and its constructor). Emitted command kinds at
draw_desc.rs:43-150 (`Sprite{region,x,y,w,h,ox,oy,rot,color,blend}`,
`Fill{Circle|Poly}`, `Shape{CircleOutline|Poly}`, `Lines{Line}`); region
names via `RegionIdTable::name(id)` (`core/render/ids.rs`). Nothing in
`mind-gdext` calls them (`grep -rln "vanilla_chain\|execute_extras"
client/rust/mind-gdext` -> nothing). Crafter state `CrafterState {progress,
warmup, total_progress}` (`core/entities/comp/building.rs`, grep it).

**Changes.**
1. `gdext/sim_host.rs` Rust-only `pub fn block_draw_state(&self, x: i32, y:
   i32) -> Option<DrawStateInput>` `{warmup, progress, total_progress,
   power_status, liquid_amount, liquid_capacity, liquid_color: u32, heat,
   id: i32}` from `CrafterState` (generators: the generator state's
   `productionEfficiency` as `warmup` — find it with `grep -n
   "production_efficiency" core/world/blocks/power/*.rs`; document),
   `PowerModule.status`, the dominant `LiquidModule` entry (colour from
   `content.liquid(id).color`), `id` = entity index.
2. New `gdext/render/block_extras.rs` `BlockExtras` (§0 template); **two**
   nodes in the `Layer::Block` band created after `BlockRenderer`'s nodes
   (one normal, one with a `CanvasItemMaterial` `BLEND_MODE_ADD` for
   `Blend::Additive` commands). `draw`: for each visible building (reuse the
   visible-tile scan `blocks.rs` uses; dedupe multiblocks by entity) whose
   `vanilla_chain(&def.name)` is `Some`: build `DrawState` (`time` = view
   ticks), `execute_extras` each desc into a `Vec<DescDraw>` (one
   `RegionIdTable` kept on the node), stable-sort by `z`, interpret:
   `Sprite` -> `canvas::draw_region` (texture by `ids.name(region)` through
   the same region resolver `BlockRenderer` uses), `Fill Circle` ->
   `draw_circle`, `Fill Poly` -> `draw_colored_polygon` (ring of `sides`
   points), `Shape CircleOutline` -> `draw_arc(.., 0, TAU, 32, color,
   stroke)`, `Shape Poly` -> `canvas::poly_line`, `Lines Line` ->
   `draw_line(.., stroke)`. Each node draws only its blend.
3. Base sprite doubling: for every chain whose `-full` composed icon already
   contains a part the chain animates (e.g. pulverizer rotator), make
   `blocks.rs` draw `def.region` instead of the `-full` icon: add
   `const ANIMATED_CHAIN_BASE: &[&str]` in `blocks.rs` with one comment per
   entry. Decide per block by checking the 14 blocks once in-engine
   (screenshot each; list them in the commit body).

**Tests.** `block_extras.rs` pure interpreter bucketing fn tested with a
hand-built `Vec<DescDraw>` (one circle, one additive sprite) -> two buckets in
z order. `draw_desc.rs`: `silicon_smelter_extras_emit_flame` (warmup 1, time
0 -> at least one `Fill Circle`) if not already present.

**Verify in-engine.** Ground Zero (free ground (132..147, 57..72); verified 2026-10-14 at (134,69) with a `power-source` at (136,69)): `place_block(134,69,"silicon-smelter")`,
a sandbox `power-source` adjacent, `debug_add_building_items(120,62,"coal",
10)` and `("sand",10)`, step 120, two screenshots 0.3 s apart
(`smelter-a.png`, `smelter-b.png`): flame present and changing. Repeat for
`pulverizer` (+ scrap) and `combustion-generator` (+ coal).

**Pitfalls.** Never run the full `execute` (the base would double). `time` is
ticks, not seconds. Do not run chains for off-screen buildings.

**Commit.** `Gameplay H03 T8: live DrawBlock descriptors for crafters and generators`

---

### T9 — Unit factories and reconstructors show no unit being built

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: `ground-factory` at (142,70) with `power-source` (144,70), `configure_building(142,70,"int","0")` (readback `"0.0"`), 10 silicon + 10 lead -> a team-1 dagger appeared after ~1000 ticks while the factory showed only its static icon (`H03-T8-T9-a.png`); `unitbuild` unused in gdext. Coordinates corrected in place (free ground).

**Symptom / acceptance.** A powered ground factory with a plan selected looks
identical to an idle one; Java shows the unit's silhouette filling in with
the scanning `Shaders.build` pattern and a sweeping accent line, plus the
rotated output arrow. After: the same in the port (Frozen Forest is where a
player first builds daggers).

**Java.**
* `J/world/blocks/units/UnitFactory.java:380-397` `draw`: `Draw.rect(region,
  x, y)`; `Draw.rect(outRegion, x, y, rotdeg())`; if `currentPlan != -1`:
  at `Layer.blockOver` `Drawf.construct(this, plan.unit, rotdeg() - 90,
  progress / plan.time, speedScl, time)`; `drawPayload()`; at
  `Layer.blockOver + 0.1` `Draw.rect(topRegion, x, y)`. `:410-415`
  `time += edelta * speedScl * unitBuildSpeed`, `speedScl = lerpDelta(speedScl,
  efficiency > 0 && plan ? 1 : 0, 0.05)`. Regions: `outRegion =
  find(name + "-out", "factory-out-" + size)`, `topRegion = find(name +
  "-top", "factory-top-" + size)` (check `UnitFactory.load`).
* `J/graphics/Drawf.java:586-603` `construct(t, region, color=Pal.accent,
  rotation, progress, alpha, time, size = block.size*8 - 4)`:
  `Shaders.build.{region, progress, color(a = alpha), time = -time/20}`;
  `Draw.rect(region (= unit fullIcon), t.x, t.y, rotation)` through the
  shader; then `Draw.color(Pal.accent); Draw.alpha(alpha);
  Lines.lineAngleCenter(t.x + sin(time, 20, size/2), t.y, 90, size)`.
* `J/world/blocks/units/Reconstructor.java:287-317`: region, `inRegion`
  (`find(name+"-in", "factory-in-"+size)`) rotated toward each blending
  input side (fallback `rotation*90`), `outRegion` at `rotdeg()`; when
  constructing and arrived: payload unit icon at alpha `1 - progress /
  constructTime` (`constructTime` default 120) plus `Drawf.construct(this,
  upgrade(type), payRotation - 90, progress / constructTime, speedScl,
  time)`; else `drawPayload()`; `topRegion` at blockOver + 0.1.
* `Shaders.build` = `client/shaders/unitbuild.gdshader` (already ported;
  uniforms `u_time, u_progress, u_color, u_uv, u_uv2, u_texsize`).

**Port today.** `core/world/blocks/units/mod.rs:145-160` `UnitFactoryBuild
{ current_plan, progress }`, `:245-257` `ReconstructorBuild { payload,
progress, command, move_in }`; no `time`/`speedScl` (view-only in this
plan). `UnitFactory.plans` with `time` per plan (same file). Atlas regions
present: `ground-factory`, `factory-out-3/5/7/9`, `factory-top-3/5`,
`factory-in-3/5/7/9`, `additive-reconstructor(-top)`, `unit-<name>-full`.
`blocks.rs` draws the static `-full` icon.

**Changes.**
1. `gdext/sim_host.rs` Rust-only `pub fn unit_build_draw(&self, x: i32, y:
   i32) -> Option<UnitBuildDraw>` with `{kind: Factory|Reconstructor, cx, cy,
   size, rotation, unit: Option<UnitTypeId> /*plan or upgrade target*/,
   payload: Option<UnitTypeId>, fraction: f32, working: bool, id: i32}`;
   `fraction = progress / plan.time` (factory) or `progress / 120`
   (reconstructor; read the port's construct time constant first), `working`
   = `efficiency > 0 && unit.is_some()` (read efficiency like other seams).
2. `gdext/render/blocks.rs`: for `BlockKind::UnitFactory |
   BlockKind::Reconstructor`, draw `def.region` + (`-in` per Java for
   reconstructors: blending sides = neighbours that output into it — if no
   cheap test exists, draw the fallback `rotation*90` only and note it) +
   out region at `rotation*90` + top region (Layer.blockOver + 0.1 -> the
   `BlockBuilding` band is above; use a node in the `Layer::BlockOver` band
   if present, else draw top in the block band after the construct node) and
   `continue` (like the T8 door branch at blocks.rs ~354). Region lookup:
   `<name>-out` else `factory-out-<size>`; same for top/in.
3. New `gdext/render/unit_build.rs` `UnitBuildRenderer` (§0 template) at
   `Layer::BlockOver` (or `BlockBuilding` if no blockOver band; document):
   per visible factory: view state `HashMap<i32 id, (time, speed_scl,
   last_fraction)>`; `speed_scl += (target - speed_scl) * (1 - 0.95^dt)`
   with `target = working ? 1 : 0`; `time += dt * speed_scl`. Draw the unit
   `unit-<name>-full` region rotated `rotation*90 - 90` through a
   `ShaderMaterial(unitbuild)` child node (`Sprite2D` per factory is fine:
   count driven by buildings -> `# code-instantiated` pooled), uniforms:
   `u_progress = fraction`, `u_color = accent with a = speed_scl`, `u_time =
   -time/20`, `u_uv/u_uv2` = the region's atlas UV rect, `u_texsize` = atlas
   page size. Then the accent line: `draw_line((cx + sin(time/20)*size/2,
   cy - size/2), (.., cy + size/2))` with `size = block.size*8 - 4`, alpha
   `speed_scl`. Reconstructor: first the payload icon at alpha `1 -
   fraction`.
4. If `ShaderMaterial` on a `Sprite2D` with an `AtlasTexture` gives wrong UVs,
   pass the page texture + `region_rect` instead (Sprite2D `region_enabled`),
   so `UV` is page-space and `u_uv/u_uv2` match Java's `region.u/v`.

**Tests.** `core/render/` pure helper `construct_line_x(cx, time, size)`
(`time = 20*PI/2` -> `cx + size/2`). `sim_host.rs` test for
`unit_build_draw` fraction on a factory with a plan at half progress
(use `production_tests.rs` helpers; `place(&mut sim, "ground-factory", ..)`,
configure plan 0, set progress).

**Verify in-engine.** Ground Zero (free ground; verified 2026-10-14 at (142,70) with a `power-source` at (144,70)): `place_block(142,70,"ground-factory")`
(3x3), sandbox `power-source`s around it, `configure_building(142,70,"int",
"0")` (plan 0 = dagger; if configure is broken -> H02, note it), add silicon
+ lead with `debug_add_building_items`, step 300, screenshot `factory.png`:
dagger silhouette partly revealed with scan stripes and the accent line.

**Pitfalls.** Payload drawing (`drawPayload`) of a finished unit waiting to
exit is deferred (payload rendering = H01/H09 area); note it.

**Commit.** `Gameplay H03 T9: unit factory and reconstructor build preview (Drawf.construct)`

---

### T10 — View-side unit FX: status particles, walk ripples, landing dust

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: no live consumer of `StatusEffect.effect_chance` (only `fx/parts` and registries); a burning + wet team-2 dagger shows no `Fx.burning`/`Fx.wet` particles (`H03-T2e-core-cracks-T10-burning.png`). Walk ripples not checked.

**Symptom / acceptance.** Burning, wet, sapped, tarred, overclocked units emit
no particles; units walking through shallow water make no ripples; a mech
landing after a boost makes no dust. After: a burning dagger emits
`Fx.burning`, a wet unit drips (`Fx.wet`), a dagger walking in water spawns
`Fx.ripple` every `7 + hitSize/8` px moved, a boosting nova landing spawns
`Fx.unitLand`.

**Java.**
* `J/type/StatusEffect.java:164-167` (in `update`): if `effect != none &&
  Mathf.chanceDelta(effectChance) && !inFog`: `v.rnd(Mathf.range(hitSize/2))`
  (random direction, length uniform in ±hitSize/2); `effect.at(x + v.x,
  y + v.y, 0, color, parentize ? unit : null)`. `chanceDelta(p) = random() <
  p * Time.delta`.
* `J/entities/comp/UnitComp.java:726-746`: flying -> grounded transition:
  `Fx.unitLand.at(x, y, floor.isLiquid ? 1 : 0.5, tile.getFloorColor())`;
  `!type.hovering && isGrounded() && type.emitWalkEffect`: `splashTimer +=
  dst(deltaX, deltaY)`; at `>= 7 + hitSize/8`:
  `floor.walkEffect.at(x, y, hitSize/8, floorColor)`, reset.
  `Floor.walkEffect = Fx.ripple` for liquid floors (`Floor.java:205-207`).

**Port today.** Status content: `effect`, `effect_chance`,
`parentize_effect`, `color` (`core/content/registries/statuses.rs`); unit
statuses `StatusComp.statuses` (`core/entities/comp/unit/comp.rs`, grep);
`UnitTypeDef.hovering` / `emit_walk_effect` (grep; treat a missing
`emit_walk_effect` as true). FX bus: `FxSink::effect(&*bus,
&EffectRef::Named(id), x, y, rotation, color)` (`core/fx/sink.rs`);
`MindFx` owns the bus (`gdext/fx.rs`). Effect ids by name: `grep -n "fn
by_name\|fn find" core/content/registries/fx_meta.rs`. G03 T5 added a
per-unit view state in `gdext/render/units.rs` / `unit_overlays.rs` (find
it: `grep -n "struct .*ViewState\|HashMap<i32" gdext/render/unit*.rs`).

**Changes.**
1. Extend the unit draw row (`UnitDraw` or the struct `unit_draws_full`
   returns) with `statuses: SmallVec<[StatusId; 4]>`, `grounded: bool`
   (`!flying && elevation < 0.001`), `floor: BlockId`.
2. In the unit view state add `was_flying`, `last_pos`, `splash`. Each
   frame, with `dt` in ticks and `godot::global::randf()` as the view RNG,
   collect `Vec<(EffectId, f32 x, f32 y, f32 rotation, Rgba)>`:
   status particles (`randf() < effect_chance * dt`; offset = random angle,
   length `(randf()*2 - 1) * hitSize/2`; status colour); landing
   (`was_flying && grounded` -> `unitLand`, rotation `is_liquid ? 1 : 0.5`,
   floor map colour); walk (`grounded && floor liquid && splash >= 7 +
   hitSize/8` -> `ripple`, rotation `hitSize/8`).
3. Return that Vec from the unit pass's `update()`; in
   `MindWorldRenderer::frame` forward each to `MindFx` (one `bind_mut` after
   the unit pass, never while holding the unit renderer).
4. Respect the `effects` setting (MindFx already gates in `tick_view`).

**Tests.** `core/render/unit_draw.rs` (or the module that owns the view
helpers): `walk_splash_threshold` (hitSize 8 -> threshold 8; 3 px per frame
emits on frame 3 and resets).

**Verify in-engine.** Ground Zero: `debug_apply_status(<dagger seq>,
"burning", 600)` on a spawned team-2 dagger, step 60, screenshot
`burning.png`. Water: find shallow water near the core with `tile_debug`
on blue tiles (Ground Zero has `shallow-water`/`deep-water` patches north-
east, see the screenshots in `runs/20261013-s3-G03-T2/`), spawn a dagger on
it with a move command, step 60, screenshot `ripples.png`.

**Pitfalls.** Never use the sim `UnitRuntime.fx` handle (deterministic path)
for these; view-only.

**Commit.** `Gameplay H03 T10: status-effect particles, walk ripples and landing dust`

---

### T11 — PAR-0129 animated shields and build beams (effect-buffer composite)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (by design): `force-projector` (118,62) + `power-source` (120,62) reaches efficiency 1.0 (not blocked by H02) and draws the flat non-animated dome, radius ~101 world (`H03-T11-force-projector.png`); no `animatedshields` settings row.

**Symptom / acceptance.** Shields draw Java's non-animated branch; Java
desktop default `animatedshields = true` composites them through
`Shaders.shield` (bright outline + moving stripes), and build beams through
`Shaders.buildBeam`. After: domes/bubbles/beams show the shader look when the
setting is on.

**Java.** `J/core/Renderer.java` (animateShields block, ~403-414):
`Draw.drawRange(Layer.shields, 1, () -> effectBuffer.begin(clear), () -> {
effectBuffer.end(); effectBuffer.blit(Shaders.shield); })`, same for
`Layer.buildBeam` with `Shaders.buildBeam`. `J/graphics/Shaders.java:273-300`
uniforms: `u_dp = Scl.scl(1)` (1 on desktop), `u_time = Time.time`,
`u_offset = (camera.x - camera.width/2, camera.y - camera.height/2)`,
`u_texsize = (camera.width, camera.height)`, `u_invsize = 1/u_texsize`.
Under animateShields: `ForceProjector.drawShield` draws `Fill.poly` in
`team.color.lerp(white, hit)` at full alpha; `UnitType.drawShield`
`Fill.light` (`ForceProjector.java:308-330`, `UnitType.java:1643-1650`).
Setting default: `graphics.checkPref("animatedshields", !mobile)`
(`J/ui/dialogs/SettingsMenuDialog.java`, grep `animatedshields`).

**Port today.** `gdext/render/shields.rs` (non-animated); `build_beam.rs`
flat; `client/shaders/shield.gdshader`, `buildbeam.gdshader` sample
`u_texture` at `UV`. No `animatedshields` settings row (`grep -n
animatedshields core/ui/settings.rs` -> nothing).

**Changes.**
1. `core/ui/settings.rs`: add `SettingRow::check("animatedshields", true)`
   next to `animatedwater` (~245) and to the graphics list (~384); note it for
   H05/PAR-0164 (settings hub) in the commit.
2. New shaders `client/shaders/shield_group.gdshader` /
   `buildbeam_group.gdshader`: copies whose source sample is
   `uniform sampler2D screen_texture : hint_screen_texture, filter_nearest;`
   read at `SCREEN_UV`; Java's `T` = `vec2(SCREEN_UV.x, 1.0 - SCREEN_UV.y)`
   for the `coords = T * u_texsize + u_offset` stripe math only (sampling
   offsets keep using SCREEN_UV, with the y offsets negated). Register both
   names in `core/render/shaders.rs` `AUXILIARY` (check the constant).
3. `shields.rs` / `build_beam.rs`: when `animatedshields` is on, parent each
   pass node under a `CanvasGroup` (`# code-instantiated`, `fit_margin = 0`,
   `clear_margin = 0`) with `ShaderMaterial(shield_group / buildbeam_group)`;
   draw opaque Java colours; set uniforms each frame from the camera view.
   Off -> today's branch.
4. If the `CanvasGroup` renders nothing in Godot 4.7 (the G03 executor
   suspected this): stop, keep the code behind the setting defaulting to the
   old branch, and record "CanvasGroup screen_texture path fails" with a
   screenshot.

**Tests.** None for shaders; keep shields tests green; settings test that
`animatedshields` exists with default true.

**Verify in-engine.** Build beams: T2(d) recipe before/after
(`beam-flat.png`, `beam-animated.png`). Shields: `place_block` a
`force-projector` + sandbox power; if it stays at efficiency 0 (G02 issue)
write "shields: blocked by projector power (H02)".

**Pitfalls.** Run `godot4 --headless --path client --script
res://tools/shaders_check.gd` after adding shaders.

**Commit.** `PAR-0129: animated shield and build-beam composites`

---

### T12 — Erekir fog of war rendering (`FogRenderer`)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED and worse than described: on Onset (`load_sector("erekir", 10)` + `start_sector`; rules `fog` true, `staticFog` true, `dynamicColor 0000007f`, `staticColor 000000ff`) the `fogOfWar` band paints the whole view opaque black (pixels (0,0,0); `H03-T12-onset-fog.png`); hiding the layer with `$R.set_layer_visible("fogOfWar", false)` shows the world (`H03-T12-onset-nofog.png`). H09's seams are absent, so the task still waits; coordinator: Erekir is visually unplayable until then — consider a stopgap (hide the placeholder quad). `java/step-204-erekir-sector.png` is a planet frame (no in-world Java reference). Unrelated: the Onset palette lists Serpulo blocks.

**Symptom / acceptance.** On Erekir (rules `fog = true`, often
`staticFog = true`) Java hides undiscovered terrain in black and darkens areas
outside current vision; the port draws a flat quad over the whole (2026-10-14: on Onset it is OPAQUE black, the world is invisible)
screen (or nothing). After: dynamic fog (area outside unit/building fog
radii dimmed by `rules.dynamicColor`) and static fog (never-discovered tiles
in `rules.staticColor`) match Java. **Depends on H09** making
`FogControl` live (discovered bitsets per team).

**Java.** `J/graphics/FogRenderer.java`:
* `drawFog` (called at `Layer.fogOfWar` when `rules.fog`,
  `Renderer.java:419`): return when `fogControl.getDiscovered(team) == null`.
  Dynamic buffer (world tiles size, cleared black): white `Fill.poly(x, y,
  20, radius)` for each player-team building flagged `hasFogRadius`
  (`build.fogRadius() * 8`) and each unit (`type.fogRadius * 8`), scissor 1
  px border. Static buffer: on team change `copyFromCpu()` (white 1x1 rect
  for each discovered tile, skipping the 1-tile border); new static events
  drawn as `Fill.poly(x + 0.5 + o, y + 0.5 + o, 20, radius + 0.3)`.
* Composite: `Draw.shader(Shaders.fog)`; `Draw.color(rules.dynamicColor,
  max(0.5, dynamicColor.a))`; `Draw.fbo(dynamic, w, h, tilesize)`; if
  `staticFog`: `Draw.color(rules.staticColor, 1)`; `Draw.fbo(static, w, h,
  tilesize, tilesize/2)`.
* `core/assets/shaders/fog.frag`: `QUANT 0.3`; `gl_FragColor = vec4(1,1,1,
  (1 - floor(c.r / QUANT) * QUANT) * step(c.r, 0.99)) * v_color` (note GLSL
  `step(edge, x)`: 1 when `0.99 >= c.r`).

**Port today.** `gdext/render/fog.rs:83-94` flat quad of
`rules.dynamic_fog_color()`; `core/game/fog.rs` `FogControl` with
`get_discovered(team) -> Option<&FogBits>`, `is_visible_tile`,
`push_static_blocks`, `update`, ... but no live owner
(`grep -rn "FogControl" core/sim core/game/runtime*.rs` shows none);
`MindCampaign.fog_query` returns `discovered: false` always
(`gdext/campaign.rs:2211`). `client/shaders/fog.gdshader` exists.

**Changes (only once H09 lands a live FogControl; precondition below).**
0. Precondition: `grep -rn "fn fog_view\|fn fog_bits" gdext/sim_host.rs
   core/sim` — H09 provides (seam name fixed here; H09 implements it):
   ```rust
   /// Discovered (static) bits for `team` as one byte per tile (0/255,
   /// row-major, y up) or None when fog is off / not tracked.
   pub fn fog_static_bytes(&self, team: u8) -> Option<(i32, i32, Vec<u8>)>
   /// Dynamic vision sources for `team`: (world x, y, radius in world px).
   pub fn fog_sources(&self, team: u8) -> Vec<(f32, f32, f32)>
   ```
   If absent: skip steps 1-3 ("waits for H09") but DO step 0b.
0b. **Stopgap (coordinator decision 2026-10-14; applies even when H09 has not
   landed).** Today the placeholder quad in `gdext/render/fog.rs::update`
   (~70-95) paints the whole view opaque black on Onset
   (`runs/20261014-s2-verify2/H03-T12-onset-fog.png`; with the `fogOfWar` layer
   hidden the world is visible, `H03-T12-onset-nofog.png`). Change `update` to
   keep the node hidden (`set_visible(false)`, no mesh) while the step-0 fog
   seams are absent — draw nothing for fog until H09 lands; keep the event
   bookkeeping and `stats.active = rules.fog`. Document the deviation ("fog
   not rendered until FogControl is live") in the doc comment.
   Test: split the decision into a pure fn
   `fog_placeholder_visible(fog_enabled: bool, has_fog_data: bool) -> bool`
   and assert `(true, false) -> false`, `(false, _) -> false`.
   Verify on Onset (`$H.load_sector("erekir", 10)` + `$MC.start_sector("erekir", 10)`
   + `UiRoot.set_menu_visible(false)`; `$C.set_process(true)` first if an
   earlier step disabled camera follow): screenshot `onset-stopgap.png` shows
   the core bastion and terrain; the centre pixel is not (0,0,0).
   Commit separately: `Gameplay H03 T12: stopgap — no opaque fog placeholder
   until FogControl is live`. **Steps 1-3 below still wait for H09.**
1. `gdext/render/fog.rs`: two `MeshInstance2D` quads covering the world
   (copy `ShadowRenderer`'s `world_quad` + `make_texture` pattern), both
   with `ShaderMaterial(fog)`; dynamic map rebuilt every 2 frames on the CPU
   at 1 texel per tile (stamp filled 20-gon circles = discs, 255 inside,
   1-tile black border), static map rebuilt when the static bytes' hash
   changes. Linear filtering on both textures (`texture_filter =
   LINEAR` on the two nodes; Java sets `TextureFilter.linear`). Modulate:
   dynamic `rules.dynamic_color` with alpha `max(0.5, a)`; static
   `rules.static_color` alpha 1, quad offset by +4 px (half tile) like Java.
2. Shader `client/shaders/fog.gdshader`: verify it is the Java math above
   (`COLOR = vec4(1,1,1, (1.0 - floor(c.r/0.3)*0.3) * step(c.r, 0.99)) *
   COLOR_vertex`); fix if not.
3. Feed `rules.fog/static_fog/dynamic_color/static_color` from
   `RulesRenderView` (already polled).

**Tests.** Pure fn `fog_alpha(r: f32) -> f32` in `core/render/fog.rs`
(r 0 -> 1.0; r 0.35 -> 0.7; r 1.0 -> 0.0).

**Verify in-engine.** Erekir first sector (onset): `$H.load_sector("erekir",
<id>)` — find the id with `$MC.get_tech_state()` or the planet dialog;
screenshot `fog.png` vs Java
`runs/20261010-234250-ref-PAR-0070-combat-fx/java/step-204-erekir-sector.png`
(open it first; if it is a planet-page frame, there is no in-world Java
reference — say so).

**Pitfalls.** Units/buildings hidden inside fog (`inFog` draw gating of enemy
units) is a sim visibility question: ask H09 for `fog_visible(team, x, y)`
and gate unit/building drawing only if that seam exists; otherwise record it.

**Commit.** `Gameplay H03 T12: Erekir fog of war rendering (static + dynamic)`

---

### T13 — Launch pods and landing-pad arrivals (needs H04)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED absent: `fn launch_payloads_json` and `struct LandingPadState` have 0 hits; waits for H04.

**Symptom / acceptance.** Once H04 makes launch pads launch and landing pads
receive, nothing is drawn: no pod rises, none lands, and pads show no pod or
progress lights. After: a launched pod flies up-right with an engine glow and
shadow for 120 ticks; an arriving pod descends with engine flares; an idle
launch pad shows its pod and the corner progress lights.

**Java.** (verified 2026-10-14)
* `J/world/blocks/campaign/LaunchPad.java:227-280` `LaunchPayloadComp.draw`
  (lifetime 120, `:153`): `alpha = fout(pow5Out)`; `scale = (1 - alpha)*1.3
  + 1`; `cx = x + fin(pow2In) * (12 + randomSeedRange(id+3, 4))`;
  `cy = y + fin(pow5In) * (100 + randomSeedRange(id+2, 30))`;
  `rotation = fin * (130 + randomSeedRange(id, 50))`; at `Layer.effect +
  0.001`: `Fill.light(cx, cy, 10, 25*(rad + scale - 1), engine@alpha,
  engine@0)` with `rad = 0.2 + fslope` (`fslope = (0.5 - |fin - 0.5|)*2`),
  then `alpha(alpha)` and 4 `Drawf.tri(cx, cy, 6, 40*(rad + scale - 1), i*90
  + rotation)` in `Pal.engine` (`ffbb64`); at `Layer.weather - 1`: pod region
  (`<pad>-pod`, fallback `launchpod`) size `region*0.25*scale`, rotation,
  alpha; shadow at `Layer.flyingUnit + 1`: same region tinted
  `(0,0,0,0.22*alpha)` offset `trns(225, fin(pow3In)*250)`.
* `LaunchPad.java` `LaunchPadBuild.draw` (~110-135): base; if `lightRegion`
  found: `progress = min(items.total/itemCapacity, launchCounter/launchTime)`;
  for `i in 0..4`, `j in 0..lightSteps`: `alpha = curve(progress,
  j/lightSteps, (j+1)/lightSteps)`, `offset = -(j - 1)*lightStep`, colour
  `lerp(Pal.metalGrayDark, lightColor, alpha)`, `lightRegion` at
  `(x + d8edge(i).x*offset, y + d8edge(i).y*offset)` rotation `i*90` (read
  `lightSteps`, `lightStep`, `lightColor` defaults in the field list); then
  `Drawf.shadow(x, y, size*8)` and the pod at the centre.
* `J/world/blocks/campaign/LandingPad.java` `draw` (~205-265): base; if
  `arriving != null`: `fin = clamp(arrivingTimer)`, `alpha = pow5Out(fin)`,
  `scale = (1 - alpha)*1.3 + 1`, `cy = y + pow4In(1-fin) * (100 +
  randomSeedRange(id+2, 30))`, `rotation = (1-fin) * (90 +
  randomSeedRange(id, 50))`; engine light + 4 tris with `rad = 0.15 +
  pow5Out(slope(fin))`; pod at `Layer.weather - 1` with `Drawf.shadow(cx, cy,
  size*8, fin)`; dark copy at `Layer.flyingUnit + 1` offset `trns(225,
  pow3In(1-fin)*250)`; else if `cooldown > 0`: `Drawf.shadow(x, y, size*8,
  cooldown)`, `alpha(cooldown)`, `mixcol(accent, 1 - cooldown)`, pod at the
  centre. Pod region `<pad>-pod`, fallback `advanced-launch-pad-pod`.
* `Fill.light(x, y, sides, radius, center, edge)`
  (`ARC/.../g2d/Fill.java:171-191`): triangle fan, `sides` rounded up to
  even, centre colour at the middle, edge on the rim.
* `Drawf.shadow(x, y, rad, alpha)` (`Drawf.java:350-364`): `circle-shadow`
  region `rad x rad`, colour `(0,0,0,0.4*alpha)`.

**Port today.** Nothing draws pods. Atlas has `launchpod`,
`advanced-launch-pad-pod`, `launch-pad-light`, `circle-shadow`. H04 provides
the data seams (names fixed by the round-G G04 plan; H04 confirms):
`#[func] MindSimHost.launch_payloads_json() -> [{x, y, time, lifetime}]` and
`LandingPadState {config, priority, cooldown, arriving: Option<ItemId>,
arriving_timer, ..}`. Both are absent on `main` today.

**Changes.**
1. Precondition: `grep -n "fn launch_payloads_json" gdext/sim_host.rs` and
   `grep -rn "struct LandingPadState" core/`. Implement only the half whose
   data exists; record the rest as "waits for H04".
2. Seed: `launch_payloads_json` lacks an id -> hash `(index, x, y)` as the
   `randomSeedRange` seed (document; Java uses the entity id). Reuse the T7
   `seed_range` helper.
3. Rust-only seam `pad_draws(rect) -> Vec<PadDraw>` with
   `PadDraw::Launch{x, y, size, progress, block}` and
   `PadDraw::Landing{x, y, size, id, arriving, arriving_timer, cooldown,
   block}`.
4. New `gdext/render/pods.rs`: engine glow node in the `Layer::Effect` band;
   pods node first in the `Layer::Weather` band; shadow copies in the
   `Layer::FlyingUnit` band; static pad parts (lights, idle pod, cooldown
   pod) in the `Layer::Block` band after the block meshes. Add
   `canvas::fill_light(canvas, x, y, sides, radius, center, edge)` (Arc fan).

**Tests.** `core/render/drawf.rs` `launch_pod_path`: `fin = 0.5`, zero seed
ranges -> `cx = x + 0.25*12 = x + 3`, `cy = y + 0.03125*100 = y + 3.125`,
`rotation = 65`.

**Verify in-engine.** After H04: H04's launch recipe, then
`set_paused(true)` + `step(20)` six times with a screenshot each
(`pod-0..5.png`). Landing pad: H04's arrival recipe.

**Pitfalls.** All motion is view-derived from `time/lifetime`; never add
sim state for it.

**Commit.** `Gameplay H03 T13: launch pods and landing-pad arrivals`

---

### T14 — "Parameter mesh is null" render noise (timeboxed: 45 min)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: 6 `Parameter "mesh" is null` errors in `REPO/.opencode/parity-session/session-2/editor-console.log` during this session (bursts of 4 and 2, each with an `_eval` frame on the stack). Bisection not run.

**Symptom / acceptance.** Every session log has `ERROR: Parameter "mesh" is
null. at: mesh_get_surface_count (mesh_storage.cpp:691)` bursts around sector
loads (16 in session-5, 30 in session-3 `editor-console.log`). The GDScript
frame shown (`mind_label.gd:24`, `planet_dialog.gd:_launch`, `_eval`) is just
whatever script was on the stack. After: zero occurrences on a Ground Zero
load + 600 ticks, or a written root cause if the timebox runs out.

**Java.** n/a.

**Port today.** All world meshes go through `MeshInstance2D.set_mesh` with
`ArrayMesh`es from `SurfaceTool::commit()` (`atlas_bind::build_mesh`,
`floor.rs::build_mesh` ~538, blocks/units/building_cache passes). A mesh RID
that is freed or never got a surface triggers this error when the canvas
renderer queries it.

**Changes (procedure).**
1. Reproduce: fresh `tools/session-play.sh N`, Ground Zero entry recipe,
   `grep -c 'Parameter "mesh" is null' REPO/.opencode/parity-session/
   session-N/editor-console.log` before and after.
2. Bisect by pass: temporarily (not committed) call
   `$G --eval 'get_node("/root/Spine/World/Renderer").set_layer_visible("<layer>", false); return true'`
   for each layer name in `core/render/layer.rs` `Layer::name()` BEFORE the
   sector load, and re-run step 1; the layer whose hiding stops the error
   owns the bad mesh.
3. In that pass look for `set_mesh` with a mesh that can have 0 surfaces
   (empty quad list) or a mesh whose `Gd` is dropped while still assigned
   (e.g. `RenderingServer` RIDs kept beyond the `Gd`); fix: when the quad
   list is empty call `node.set_mesh(Gd::null)`/`set_visible(false)`
   instead of assigning an empty mesh (`build_mesh` returning `Some` with 0
   vertices — check `SurfaceTool::commit` on an empty tool).
4. If not found in 45 min: commit nothing, write findings.

**Tests.** If the fix is in a pure helper (e.g. `build_mesh` returning `None`
for an empty slice), a unit test for that.

**Verify in-engine.** Step 1 count = 0.

**Commit.** `Gameplay H03 T14: no empty/freed meshes on render passes (mesh is null noise)`

---

### T15 — Every unit draws a permanent shield bubble (added after the 2026-10-14 in-engine verification)

**Symptom / acceptance.** Every unit (player alpha/mega, daggers of any team)
carries a translucent team-coloured disc (radius `hitSize * 1.3`) at all times
(`runs/20261014-s2-verify2/H03-T15-T16-circles-strip.png`: red disc on a team-2
dagger, yellow disc on the alpha; hiding the `shields` layer removes it,
`H03-T15-T16-circles-hide-strip.png`). Java draws the bubble only for ~15 ticks
after a hit lands on a unit whose `shield > 0`. After: no bubble on unshielded
units; a wave unit with `shield > 0` flashes the bubble when hit and it fades
over 15 ticks.

**Java.**
* `J/type/UnitType.java:1555-1556` (in `draw`): `if(unit.shieldAlpha > 0 &&
  drawShields) drawShield(unit);`
* `:1643-1650` `drawShield`: `alpha = unit.shieldAlpha(); radius =
  unit.hitSize() * 1.3f; Fill.light(x, y, Lines.circleVertices(radius),
  radius, Color.clear, shieldColor(unit).lerp(Color.white,
  clamp(unit.hitTime() / 2f)).a(0.7f * alpha))`.
* `J/entities/comp/ShieldComp.java:57-63` `rawDamage`: `hadShields = shield >
  0.0001f; if(hadShields) shieldAlpha = 1f;` then the shield absorbs;
  `:84-87` `update`: `shieldAlpha -= Time.delta / 15f; if(shieldAlpha < 0)
  shieldAlpha = 0f;`. `shieldAlpha` is `transient` (not saved).

**Port today.**
* `gdext/sim_host.rs:4223-4257` `unit_shields()` pushes one row for EVERY
  entity with `ShieldComp` (all units: `core/entities/comp/unit/lifecycle.rs:194`
  inserts `ShieldComp::default()`) with a constant alpha 0.55 and hit 0; it
  never reads `shield`.
* `core/entities/comp/unit/comp.rs:419` `ShieldComp { shield }` — no
  `shield_alpha`. Shield absorb sites: `core/combat/damage/area.rs:289`
  (main unit damage path), `core/combat/abilities.rs:109`,
  `core/combat/damage/status.rs` (interval damage). Read each before editing.
* `gdext/render/shields.rs` consumes `(x, y, radius, alpha, team, hit)`.

**Changes.**
1. `ShieldComp`: add `pub shield_alpha: f32` (doc: `ShieldComp.shieldAlpha`,
   transient — confirm it is not written by the save writer or the checksum:
   `grep -rn "ShieldComp" core/io core/sim/checksum*`; if the checksum walks the
   struct field-by-field, leave it out there).
2. At each absorb site, before subtracting: `if shield.shield > 0.0001 {
   shield.shield_alpha = 1.0; }` (Java sets it on every `rawDamage` with
   shields, even when the hit is fully absorbed).
3. Decay once per tick in the unit update (the same system that ticks
   `StatusComp`; find it with `grep -n "fn update_statuses" core/combat/damage/status.rs`
   and its caller): `shield_alpha = (shield_alpha - delta / 15.0).max(0.0)`.
4. `unit_shields()`: skip rows with `shield_alpha <= 0.0`; pass
   `shield_alpha` as the alpha and the unit's hit-flash time (the field the
   unit renderer already uses for `hitTime`, `grep -n "hit_time" gdext/render/units.rs core/entities/comp/unit/*.rs`;
   0 if none) as `hit`. Keep the `0.7 *` factor where `shields.rs` applies it
   (read `shields.rs` once; do not double it).

**Tests.** `core/combat/damage/area.rs` tests: `shield_hit_sets_shield_alpha`
(dagger with `shield = 5`, 3 damage -> `shield_alpha == 1.0`; a dagger with
`shield = 0` -> stays 0); decay test: 15 updates with delta 1 -> 0.

**Verify in-engine.** Ground Zero: the alpha and a spawned team-2 dagger show
no disc (screenshot `no-bubble.png`). Then spawn a dagger, set its shield via a
wave-shield test fixture if one exists (else headless only for the flash) and
hit it: disc visible for < 0.3 s.

**Pitfalls.** Do not touch `ForceProjector` domes (same pass, separate
data). Determinism: `shield_alpha` is view-only state on a sim component —
keep it out of the checksum.

**Commit.** `Gameplay H03 T15: unit shield bubble only while shieldAlpha > 0 (ShieldComp.shieldAlpha)`

---

### T16 — FX light prims draw opaque discs with lighting off (added after the 2026-10-14 in-engine verification)

**Symptom / acceptance.** With `rules.lighting = false` (Ground Zero), every
`ParticleEffect`-based hit/shoot effect draws a large translucent coloured disc
(the orange disc around duo/dagger bullet impacts,
`H03-T15-T16-circles-strip.png`). Disabling the `PrimKind::Light` arm in
`gdext/fx.rs` removes the discs (`H03-T16-no-fx-light-discs.png`). Java: lights
are added only when the light pass is enabled. After: no discs while lighting
is off; with lighting on, the effect lights feed the T6 light pass.

**Java.** `J/graphics/Drawf.java:301-304` `light(...) ->
renderer.lights.add(...)`; `J/graphics/LightRenderer.java:32-33` `add(...)`:
`if(!enabled() || radius <= 0f) return;`; `:186-188` `enabled() =
state.rules.lighting && state.rules.ambientLight.a > 0.0001f &&
renderer.drawLight`. `ParticleEffect.render` calls `Drawf.light`
(`J/entities/effect/ParticleEffect.java:109,119`).

**Port today.** `gdext/fx.rs` `draw_prim`, `PrimKind::Light` arm (~272-285):
"Plan 16 owns the additive light FBO; draw a faint debug disc" —
`draw_circle(radius, color * opacity)` unconditionally. Emitters:
`core/fx/resolve.rs:234,271` (ParticleEffect), `core/fx/custom/emit.rs:306`,
`core/fx/parts/draw.rs:417`.

**Changes.**
1. `gdext/fx.rs`: the `Light` arm draws nothing (return `false`, keep the
   prim count stats honest).
2. Collect the frame's light prims into a `Vec<(x, y, radius, rgba, opacity)>`
   on `MindFx` (cleared each frame) exposed by a Rust-only accessor; in
   `MindWorldRenderer::frame`, when the light pass is active (T6), append them
   to the circles the `LightRenderer` stamps (find where `light_sources` rows
   are consumed in `gdext/render/light.rs`). If T6 has not landed, do step 1
   only and note it.

**Tests.** None for Godot drawing; if a pure collector is added, a unit test
that a `Light` prim lands in the collector and not in the draw list.

**Verify in-engine.** Ground Zero, a team-2 dagger shooting the core (stop it
with `debug_apply_status(id,"disarmed",5000)` before the core dies): no orange
discs (`no-discs.png`); with `$R.set_rules_lighting(true,0.01,0.01,0.04,0.99)`
small glows appear at impacts.

**Pitfalls.** Do not drop `Light` prims from the deterministic FX recording
(goldens); only the gdext drawing changes.

**Commit.** `Gameplay H03 T16: FX light prims feed the light pass instead of drawing debug discs`

---

### T17 — Toasts and announces show raw Mindustry colour markup (added after the 2026-10-14 in-engine verification)

**Symptom / acceptance.** The game-over toast reads `Sector [accent]groundZero[white] lost!`
(`H03-T4-core-under-fire.png`, top-left) and the toggle-menus announce reads
`Press [accent][[C][] to show UI.` (`H03-T2c-menus-hidden.png`): Java colour
tags and the `[[` escape are printed literally. Java renders them as colours
(`[[` is a literal `[`). The sector name is the internal `groundZero` (Java
`Sector.name()` = localized "Ground Zero"; same in the Game Over dialog title).
After: both texts are coloured, `[C]` shows as accent text, the sector name is
localized.

**Java.** `J/core/UI.java:636-652` `announce(text, duration)` (an
`outlineLabel`, markup on); `UI.showInfoToast`/`HudFragment.showToast` labels
parse markup; `bundle.properties` `sector.lost = Sector [accent]{0}[white] lost!`,
`showui = UI hidden.\nPress [accent][[{0}][] to show UI.`; `J/core/Control.java`
sector-lost toast passes `state.getSector().name()`.

**Port today.** `ui/ui_root.gd:453-461` `_toast_label` builds a plain
`MindWidgets.label(text)`; `:417-450` `_announce_box` puts
`"[center]" + text` into a `MindRichLabel`, which by design never parses
Mindustry markup (`ui/text/mind_rich_label.gd:6-7`, "must be pre-translated by
`mind-core::ui::text::render_markup`"). Producers pass raw bundle text:
`gdext/ui/hud.rs:429` `self.bundle.format("sector.lost", &[sector_name])`
(internal name), `ui/fragments/hud_fragment.gd:131-132` `bundle_get("showui")`.
Translation helpers exist only by key: `MindAssets.bundle_markup(key)`,
`bundle_markup_format(key, args)` (`gdext/assets/atlas.rs:443,451`).

**Changes.**
1. `gdext/assets/atlas.rs`: `#[func] pub fn markup_text(&self, text: GString)
   -> GString` forwarding to `mind_core::ui::text::render_markup(s, &Iconc)`
   (`core/ui/text.rs:293`; obtain the `Iconc` the same way `bundle_markup` does).
2. `ui/mind_widgets.gd`: `static func markup_text(text: String) -> String`
   calling it (same fallback pattern as `markup`).
3. `ui/ui_root.gd`: `_toast_label` uses a `MindRichLabel` with
   `MindWidgets.markup_text(text)`; `_announce_box` uses
   `"[center]" + MindWidgets.markup_text(text)`. Producers keep passing Java
   markup (single translation point).
4. `gdext/ui/hud.rs:429`: pass the localized sector name (coordinator
   decision 2026-10-14): use H05's sector-name helper if it is on `main`
   (`git log main --oneline | grep -m1 "H05"` and `grep -rn "fn sector_display_name\|fn sector_name" client/rust`);
   otherwise reuse the helper the planet views use today,
   `MindCampaign::sector_localized_name(registry, SectorId)`
   (`gdext/campaign.rs:1714`, private — make it `pub(crate)` or move the body
   to a shared pure fn; do not write a third name lookup). Overlap noted in §6.

**Tests.** If `render_markup` lacks a `[[` escape test, add one in its module:
`"[accent][[C][]"` -> accent-coloured literal `[C]`.

**Verify in-engine.** Press C on Ground Zero (after T2 row c's fix): the
announce shows `Press [C] to show UI.` with `[C]` in accent; trigger a sector
loss on a throwaway profile (destructive: re-import afterwards) or call
`get_node("/root/MindUi").toast("Sector [accent]Ground Zero[white] lost!", "warning")`
and screenshot.

**Pitfalls.** Do not translate twice (texts already produced by
`MindWidgets.markup(...)` are BBCode; only raw producers go through
`markup_text`; check each `toast`/`announce` caller with
`grep -rn "\"toast\"\|\"announce\"" client/ui client/rust/mind-gdext/src`).

**Commit.** `Gameplay H03 T17: toasts and announces render Mindustry markup; localized sector name in the loss toast`

---

### T18 — Static block cache redraws on config/damage changes: door/sorter/switch/unloader state sprites, cracks on cached blocks (added after the 2026-10-14 in-engine verification)

**Symptom / acceptance.** Every `drawCached` / `drawDynamic = false` block is
baked once into the static building cache and never redrawn for its state:
an opened door looks closed, a configured sorter shows no item colour, and
damaged walls (and doors, sorters, containers/vaults, unloaders, logic
blocks) never show cracks. Evidence 2026-10-14 (`runs/20261014-s2-verify2/`):
`H03-T2g-doors-crop.png` (door (135,66) configured `true`, readback `"true"`,
drawn identical to the closed door at (133,66)), `H03-T2g-sorter-crop.png`
(sorter (137,66) readback `{"type":"item","id":0}`, no item square),
T2 row (e) (cracks appear on the dynamic core shard, never on walls). After:
the open door uses `door-open`, the sorter shows a copper-coloured square,
the switch shows `-on`, and a damaged copper wall shows the crack overlay;
state changes appear within one frame.

**Java.**
* Cache membership: `J/graphics/BlockRenderer.java:568-590` — `drawCached`
  blocks mark their chunk for the cache; `if(!block.drawDynamic){ if(build !=
  null) tileExtraCachedView.add(build); return; }` (`:582-586`): non-dynamic
  buildings never enter `tileview`, so `build.draw()` for them happens only
  inside the cache bake.
* Cracks for cached buildings: `:802-821` loop over `tileExtraCachedView`:
  `if(build.damaged()){ Draw.z(Layer.blockCracks); build.drawCracks(); }`
  (no `drawTeam` there — non-dynamic blocks get no team border, see T2 row f).
  `BuildingComp.drawCracks` (`BuildingComp.java:1236-1244`) — same math the
  port's dynamic pass already uses (`blocks.rs` ~630-660).
* Recache triggers: `BuildingComp.recache()` (`BuildingComp.java:1290-1292`)
  -> `BlockRenderer.recacheBuilding(layer, tile)` (`BlockRenderer.java:152-167`,
  marks the chunk dirty and re-bakes it when in view). Callers:
  `Door.java:38-56` config handler (`entity.open = open; entity.recache();`),
  `Sorter.java:64-72` `configured` (`recache(); renderer.minimap.update(tile)`),
  `Unloader.java:281`, `BuildingComp.java:1913` (proximity update of a
  `drawCached` block), `:2291` (first time visible).
* Baked sprites (the bake calls the block's `draw()`): `Door.java:133-135`
  `Draw.rect(open ? openRegion : region, x, y)` (`openRegion = "@-open"`,
  `:30`); `Sorter.java:76-90` `sortItem == null ? Draw.rect(cross, x, y) :
  { Draw.color(sortItem.color); Fill.square(x, y, tilesize/2 - 0.00001f);
  Draw.color(); }` then `super.draw()` (check the exact order in the file);
  `SwitchBlock` draws `-on` while enabled (`SwitchBlock.java:64-71`).

**Port today.**
* `core/render/scan.rs:206-235` `visible_blocks` skips `!draw_dynamic`
  blocks, so the `render/blocks.rs` branches for Door/AutoDoor (~356),
  SwitchBlock (~382), Sorter/ItemSource (~406) and Unloader (~465), and the
  crack/border code (~630-680), never run for them (dead code for these kinds).
* `gdext/render/building_cache.rs`: `update` (~99-133) rebakes ALL chunks only
  when `MindSimHost::world_revision()` changes; `bake_chunk` (~154-240) draws
  only the base region from `block_region_candidates(&def.name, &def.region)`
  — no state sprite, no cracks. Config and health changes do not bump
  `world_revision` (the `blocks.rs` comments "Doors toggle on tap without a
  revision bump", "Health changes do not bump the world revision").
* State readers that exist: the `state` value used by `blocks.rs` (door
  `door_open`, switch `enabled`; find its source with
  `grep -n "door_open" gdext/sim_host.rs gdext/render/blocks.rs`), sorter item
  via `building_config_json` / the same accessor `blocks.rs` uses for
  Sorter, health fraction via the accessor the crack code uses (~630).

**Changes.**
1. `gdext/sim_host.rs` Rust-only `pub fn cached_state_key(&self, x: i32, y:
   i32) -> u64`: a cheap hash of the draw-relevant state of a cached building
   (door open, switch enabled, sorter/unloader item id, and the crack index
   `clamp((1 - healthf) * 8, 0, 7)` + `damaged()`). Put it next to
   `block_state_draw`.
2. `gdext/render/building_cache.rs`: keep `HashMap<(i32 /*chunk x*/, i32 /*chunk
   y*/), u64>` of the XOR/sum of `cached_state_key` over the chunk's buildings,
   recomputed for VISIBLE chunks each frame (cheap: only chunks overlapping
   the view, only `draw_cached` buildings); a changed sum marks that chunk
   dirty (`self.grid.recache_building(layer, ...)` per layer) — the port's
   equivalent of Java's per-building `recache()`.
3. `bake_chunk`: draw the state sprite instead of the base region where Java
   does: Door/AutoDoor -> `<name>-open` when open; SwitchBlock -> base then
   `<name>-on` when enabled; Sorter/ItemSource -> base, then the cross region
   (`cross`) when no item, else a `tilesize - eps` square in the item colour
   (`content.item(id).color`), following `Sorter.java:76-90` order;
   Unloader per `Unloader.java` `draw` (read it). Move (do not duplicate) the
   region-name logic out of the dead `blocks.rs` branches into a shared pure
   helper (`core/render/` or a private fn used by both) and delete the dead
   branches in `blocks.rs` with a comment pointing here.
4. Cracks for cached buildings: a small dynamic node (§0 template,
   `Layer::BlockCracks` band) that, for visible `draw_cached` buildings with
   `healthf < 1`, draws `cracks-<size>-<i>` exactly like `blocks.rs` (reuse its
   helper: `crack_rotation`, colour `0x333333` + alpha `0.1 + (1-healthf)*0.6`).
   Java draws these every frame (not baked), so do not put cracks in the cache.

**Tests.** Pure helper tests: `door_state_region` (closed -> `door`, open ->
`door-open`), `sorter_bake_cmds` (None -> cross; copper -> square with the
copper colour), `cached_state_key_changes_on_config_and_damage` (if the key
fn is split into a pure part taking the state tuple).

**Verify in-engine.** Ground Zero (free ground (132..147, 57..72)):
`place_block(133,66,"door")`, `place_block(135,66,"door")`,
`$I.configure_building(135,66,"bool","true")`, `place_block(137,66,"sorter")`,
`$I.configure_building(137,66,"content","copper")`, `place_block(139,66,"switch")`
+ toggle it, step 3, screenshot `state-sprites.png`: 135 open, 133 closed,
sorter copper square, switch on. Cracks: no damage seam — spawn a team-2
dagger next to a wall (it targets the core first; place the wall between it
and the core), or test headless; neutralise it with
`debug_apply_status(id,"disarmed",5000)` before the core dies. Toggle the door
back (`"false"`) -> closed within one frame.

**Pitfalls.** Never re-bake all chunks per frame (perf); only chunks whose
state sum changed. `world_revision` stays the trigger for structural changes.
Determinism: view-only; no sim state. Keep T2 row (f)'s finding: non-dynamic
blocks get NO team border (Java).

**Commit.** `Gameplay H03 T18: static block cache redraws state sprites on config, cracks for cached blocks`

---

## 2.1 Deferred (with reasons)

| Item | Java | Why |
|---|---|---|
| Liquid drawing in conduits/tanks/routers/pumps | `Conduit.draw`, `LiquidBlock.drawTiledFrames`, `renderer.fluidFrames` | needs the fluid frame animation + conduit blend bits; mid-game; follow-up slice on T8's interpreter |
| Remaining `DrawBlock` chains (40 of 54) | `J/world/draw/*` | extend `vanilla_chain`; Erekir-heavy |
| Battery fill, Mend/Overdrive/Regen pulses, MassDriver, Radar, ShockMine, Incinerator, StackConveyor | per-block `draw` | small slices, lower visibility |
| `drawPlace` for drills (ore count text), pumps, bridges, core | `Block.drawPlace` overrides | world-space text |
| Legged units, tank treads, unit payload drawing, unit parts, ability draws | `UnitType.java` | mid/late units |
| Inline effects (`EffectRef::Inline`), `FxEvent::Trail/Light` | `content/*` lambdas | one port per lambda |
| Rotate-placed highlight, drop-item overlay, possession arrows, LogicAI link square | `OverlayRenderer.java:224-260` | input modes owned by H08; add after H08 exposes `rotate_placed_held`/`dropping_item` view state |
| Bloom | `Renderer` bloom | cosmetic |
| Player names, player list, chat | `PlayerComp.drawName`, `PlayerListFragment` | multiplayer only |

---

## 3. Execution order and hot files

Order (updated 2026-10-14 after the in-engine verification): **T1 -> T2 ->
T3 -> T4 -> T5 (incl. step 6 placement range) -> T6 -> T18 -> T12 step 0b
(stopgap) -> T7 -> T8 -> T9 -> T10 -> T11 -> T15 -> T16 (step 2 needs T6) ->
T17 -> T12 steps 1-3 (if H09 landed) -> T13 (if H04 landed) -> T14.** T18
comes right after T6 because players see the door/sorter/crack state
constantly; T12 step 0b does not wait for H09. T5 needs T3's
`config_selected`. T12/T13 are skipped (not reordered) when their providers
are absent; re-check at the end of the session (`git merge --no-edit main`
per H-environment §1.0).

Hot files (pull `main` before editing, keep edits additive):
* `gdext/sim_host.rs` (every area) — append seams at the end of the impl
  block holding `block_state_draw`.
* `gdext/render.rs` — `build_floor` (new passes), `frame` (T6 drawlight),
  field list.
* `gdext/input/mod.rs`, `gdext/input/desktop.rs` (H08) — T3 adds four
  #[func]s + `config_selected`; nothing else.
* `gdext/render/blocks.rs` (T8 base list, T9 factory branch).
* `ui/fragments/placement_fragment.gd/.tscn` (H08 schematic UI) — T3.
* `gdext/render/minimap.rs`, `ui/widgets/mind_minimap.gd` — T4.
* `core/render/rules.rs` — T5 fields.
* `core/ui/settings.rs` (H05 settings hub) — T11 one row.
* `core/combat/fires.rs` (H01/H09 sim) — T7 one line.
* `gdext/render/shaders.rs` — T6.

## 4. Ledger changes to report (coordinator applies)

| Item | Proposed | Evidence |
|---|---|---|
| PAR-0095 | verified after T3 | panel.png |
| PAR-0157 | verified after T3 (reticle) | reticle.png |
| PAR-0096 | verified after T4 | indicator.png + indicators_json probe |
| PAR-0246 | verified after T6 (state the real root cause) | ambient-forced/red/illuminator PNGs |
| PAR-0129 | fixing -> verified only if T11 shows the composite; else note the CanvasGroup/power blocker | beam-animated.png |
| PAR-0188 | verified if T2(e,g) shows cracks + door/sorter state; else note H02 | T2 PNGs |
| PAR-0244 | verified if T2(h) shows bridge links | T2 PNG |
| PAR-0055 | verified if T2(i) matches the Java frame | alpha comparison |
| New | "Minimap attack indicators last 15 minutes instead of 15 seconds" -> fixed by T4 | unit test |
| New | "Packed shader copies shadow client/shaders edits (light stays blend_add)" -> fixed by T6 | test + probe |
| New | "Fires and puddles are invisible; fire animation 2.25x fast" -> T7 | fire.png, puddle.png |
| New | "Unit factories show no unit being built" -> T9 | factory.png |
| New | "Crafter/generator animations not drawn" -> T8 | smelter PNGs |
| New | "Erekir fog of war not rendered" -> T12 (or blocked: H09) | fog.png |
| New | "Launch/landing pods not drawn" -> T13 (or blocked: H04) | pod PNGs |
| New | "fx_golden explosion fails on 1-ULP float noise" -> T1 | test |
| New | "Overlay top missing (hover range, configure outline, indicators, drop zone, core radius)" -> T5 | T5 PNGs |
| New | "Every unit draws a permanent shield bubble" -> T15 (found 2026-10-14) | `runs/20261014-s2-verify2/H03-T15-T16-circles-hide-strip.png` |
| New | "Bullet/shoot effects draw big translucent discs (FX light prims with lighting off)" -> T16 (found 2026-10-14) | `H03-T16-no-fx-light-discs.png` |
| New | "Toasts/announces print raw [accent] markup; sector-lost toast uses the internal sector name" -> T17 (found 2026-10-14) | `H03-T2c-menus-hidden.png`, `H03-T4-core-under-fire.png` |
| New | "HUD toggle (C) never hides the HUD" -> T2 row (c) fix (found 2026-10-14) | `H03-T2c-second-press.png` |
| New | "Minimap black on sector entry (rebuild_all row order)" -> T4 (found 2026-10-14) | `H03-T4-gz-baseline-black-minimap.png` |
| New | "Door open state / sorter item / switch state never drawn; no cracks on walls and other non-dynamic blocks" -> T18 | `H03-T2g-doors-crop.png`, `H03-T2g-sorter-crop.png`; after T18: `state-sprites.png` |
| New | "Erekir fog placeholder paints the screen opaque black" -> T12 step 0b (stopgap); real fog -> T12 steps 1-3 (waits for H09) | `H03-T12-onset-fog.png`; after: `onset-stopgap.png` |
| New | "No range circle while placing a turret" -> T5 step 6 | `H03-T2j-duo-range3.png`; after: `place-range.png` |

## 5. Done criteria

* T1-T11, T14, T15-T18 and T12 step 0b committed or stopped with a reason; T12/T13 committed or
  "waits for H09/H04"; T2 evidence table filled.
* `cargo test -p mind-core --lib --features msav-import`,
  `cargo test -p mind-gdext --lib`, `cargo test -p mind-headless --test
  fx_golden` pass; other `mind-headless` failures only from the H00 baseline
  list; clippy `-D warnings` clean; GDScript parse gate and
  `tools/shaders_check.gd` clean.
* Each task's screenshots exist under `runs/<date>-s<N>-H03-T<n>/` or the
  summary says "headless only" with the reason.
* Newest `$U/logs` file has 0 `panic` lines after the last in-engine step.
* Summary `REPO/docs/H03-rendering-hud_summary.md` written (H-environment §6
  shape) with §4 outcomes.

## 6. Cross-area seams

Provided by H03 (signatures fixed; other plans reuse, never redefine):
* **H08**: `MindInput.set_unit_command(command: GString) -> bool`,
  `set_unit_stance_named(stance: GString, enabled: bool) -> bool`,
  `select_only_type(type_id: i64) -> bool`, `deselect_type(type_id: i64)
  -> bool` (#[func], T3). Keybind application of commands/stances
  (`PlacementFragment.java:655-668`) is H08's and should call these.
* **H06 T12**: `MindInput::config_selected() -> Option<(i32,i32)>`
  (+ `#[func] config_selected_json`), `ConfigOverlay::Logic`,
  `SelectOverlay::{MessageText, LogicTag}` in `gdext/sim_host.rs`, dispatched
  from `gdext/render/overlay_top.rs`; H06 adds
  `gdext/render/logic_overlay.rs::{draw_config, draw_select}` and fills the
  variants — no second overlay node (T5).
* **All**: `res://shaders/*.gdshader` is now the shader source of truth
  (T6); `canvas::{target, square, select, fill_light}` helpers (T3/T5/T13).
* **H05/PAR-0164**: settings rows `animatedshields` added (T11); `drawlight`
  now read (T6).
* **H05 (sector names, T17)**: the sector-lost toast needs the localized
  sector name; T17 uses H05's helper when it exists on `main`, else
  `MindCampaign::sector_localized_name` (`gdext/campaign.rs:1714`). If H05
  later adds its own helper, it should replace that call (one lookup only).
  The Game Over dialog title shows the same internal name (`ui/dialogs/game_over_dialog.gd:102-110`; H05's surface).

Needed by H03:
* **H09**: live `FogControl` + `MindSimHost::fog_static_bytes(team)` /
  `fog_sources(team)` (shapes in T12 step 0); optionally
  `fog_visible(team, x, y)` for `inFog` draw gating.
* **H04**: `#[func] launch_payloads_json()` and `LandingPadState` (T13), and
  whether Java clamps the camera to `limitMapArea` (T4 step 4 finding goes
  to H08 if it is a camera clamp).
* **H02**: `configure_building` reaching `configured()` (PAR-0190) for T2
  rows (g)/(h) and T9's plan selection; force-projector power for T11.
* **H01**: unit command/stance live state as read by `unit_debug_json`
  (T3); `drain_build_damage_events` stays the single minimap drain (T4) —
  H01 must not add a second drain consumer.
* **H07**: weather drawing stays H07; T6's light pass is shared — weather
  lights (if any) go through `light_sources`.
