# Audit + executor plan: in-game rendering and HUD (2026-10-12)

This plan is self-contained. It records the audit of the rendering/HUD work merged into `main` (commits listed in §2) and gives ordered, executor-ready tasks for every defect found and for every rendering/HUD item still not done. Do the tasks in the order of §5. Do not make judgement calls: where a choice exists, this plan makes it.

Java source (read-only): `/home/clintonnguyen/g/code_examples/Mindustry` (`core/src/mindustry/...`, assets `core/assets`, raw sprites `core/assets-raw/sprites`). Arc (read-only): `/home/clintonnguyen/g/code_examples/Arc`. Line numbers were checked on 2026-10-12; if one moved, search for the quoted method name.

---

## 1. Environment & workflow

### 1.1 Checkout and session

- Never edit the main checkout `/home/clintonnguyen/g/code_examples/Mindustry-Godot` directly while another executor uses it. Use a session worktree:
  ```bash
  cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
  bash tools/parity-session.sh --session N          # N = 2..5; creates ../Mindustry-Godot-sessionN, bridge 6970+10(N-1), display :(9+N)
  cd ../Mindustry-Godot-sessionN && git merge --ff-only main
  ```
  The audit fixes in §3 live on branch `parity/session-3` (worktree `../Mindustry-Godot-session3`). If they are not yet in `main`, merge that branch first: `git merge parity/session-3` (one conflict is expected in `client/rust/mind-gdext/src/sim_host.rs` with session 4's `8b6ee23` "bounds-check MindSimHost tile accessors": keep ONE bounds helper, see §3).
- Generated assets are gitignored. In every new worktree, and after any change under `client/rust/mind-tools/`, run (≈5–10 min, release build):
  ```bash
  CARGO_BUILD_JOBS=6 bash tools/pack.sh pack       # `bash` is required: tools/pack.sh is not executable
  ```
  Expect `pack: 5141 regions on 4 page(s)` and exit 0. Without it the client shows placeholder sprites and fallback fonts.
- Build and (re)start:
  ```bash
  CARGO_BUILD_JOBS=6 tools/build.sh                # mind-gdext + mind-headless (~20 s incremental, ~2 min after mind-core changes)
  T=300 tools/session-play.sh N --import           # first start: restart editor + import the full-tech-tree Java profile
  T=200 tools/session-play.sh N                    # after EVERY build (the editor mmaps the .so)
  T=200 tools/session-play.sh N --play             # GDScript/.tscn-only change, or to unpin a stuck eval loop
  ```
  `--import` must print `{"nodes": 375.0, "unlocked": 282.0}`.

### 1.2 Driving the client (gmcp)

```bash
export OPEN_GODOT_MCP_PORT=$((6970 + 10*(N-1)))
python3 tools/gmcp.py --eval 'return <one GDScript expression>'      # every eval MUST `return`
python3 tools/gmcp.py --screenshot runs/<dir>/<name>.png             # 1152x648 viewport PNG
python3 tools/gmcp.py --eval 'get_tree().root.warp_mouse(Vector2(600, 560)); return true'   # move the cursor (godot_input mouse_motion does NOT move get_mouse_position)
```
Eval rules: one statement; no loops/lambdas; only call methods that exist and are `#[func]` (a missing method pins the frame loop for 15 s — recover with `tools/session-play.sh N --play`); software GL runs at 10–40 fps, so `sleep 2` before screenshots.

Ground Zero launch (sector **170**; the import remaps legacy 15 → 170). Save as `/tmp/enter170.sh`:
```bash
OPEN_GODOT_MCP_PORT=$PORT python3 tools/gmcp.py --timeout 140 \
  --eval 'get_node("/root/Spine/SimHost").load_sector("serpulo", 170); return true' \
  --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 170)' \
  --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true' >/dev/null
sleep 6     # the landing zoom (PAR-0131) runs ~3-5 s on software GL
```
Check: `--eval 'return str(get_node("/root/MindHud").get("position_text"))'` → `"129,55"`.

Useful probes (all exist):
```bash
--eval 'return get_node("/root/Spine/SimHost").place_block(126, 53, "conveyor")'          # true
--eval 'return str(get_node("/root/Spine/SimHost").tile_debug(126, 53))'                  # {name, kind, sim_name, floor, overlay, has_build, construct}
--eval 'return str(get_node("/root/Spine/World/Renderer").get_stats())'                   # dynamic_sprites, missing_regions, unit_sprites ...
--eval 'return str(get_node("/root/Spine/SimHost").hover_info(130, 50))'                  # placement hover target
--eval 'return get_node("/root/Spine/Input").select_block_by_name("conveyor")'            # select a block like the palette
--eval 'return str(MindWidgets.assets().probe("block-duo-full"))'                         # atlas lookup
--eval 'return str(get_node("/root/Spine/World/Camera2D").tile_to_screen(130, 50))'       # tile -> viewport px
```
Reference frames (Java, captured on this host): `runs/20261012-overview/java-groundzero-ingame.png`, `java-planet-groundzero.png`, `java-menu.png`; combat: `runs/20261010-234250-ref-PAR-0070-combat-fx/`; tutorial/belts: `runs/20261009-065416-ref-ground-zero/`. Audit evidence: `../Mindustry-Godot-session3/runs/20261012-audit-s3/`.

### 1.3 Tests, gates, commits, RAM

```bash
cd client/rust
CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib            # baseline 1866+ passed, 0 failed
CARGO_BUILD_JOBS=6 cargo clippy -p mind-core -p mind-gdext -- -D warnings
```
- Pure, testable logic goes into `client/rust/mind-core/src/render/` (or `ui/`) with `#[cfg(test)]` tests; Godot glue in `client/rust/mind-gdext/src/render/*.rs`; GDScript is UI-only and never mutates sim state; static UI in `.tscn`; code-built nodes carry `# code-instantiated: <reason>`; user-visible strings through bundle keys; SPDX header `GPL-3.0-only` citing the Java source on new files.
- One commit per task: `Fixes PAR-XXXX: <summary>` (or `Audit fix PAR-XXXX: ...`), body = Java vs port, last line `Co-Authored-By: <your model line>`. Never push.
- "Fixes" requires an in-engine before/after screenshot compared with the named Java frame; say so in the body. Unit tests alone are not verification.
- RAM: 14 GB shared by up to 4 sessions (~1.4 GB per software-GL editor). One cargo job at a time; never run `pack` and `build` concurrently; never start a second editor.

---

## 2. Audit verdicts (commits merged into `main`)

Evidence PNGs are under `../Mindustry-Godot-session3/runs/20261012-audit-s3/` (abbreviated `audit/` below).

| Commit | Item | Verdict | Evidence / notes |
|---|---|---|---|
| `3a25763` | PAR-0152 placed buildings' sprite | **correct** | Root cause was the team-overlay step falling back to the `error` region; now `find` (no fallback). `place_block` conveyor×2, copper wall, drill, duo → `missing_regions: 0`, real sprites: `audit/par0152-blocks.png`. Left open: per-part drawing (drill rotator, turret aim), belt autotiling/animation, blockbuild shader (tasks T8–T10). `tile_debug` lacked a bounds check (fixed in `ded4e48`). |
| `4c813b0` | PAR-0130 belt items | **partial → fixed** | Placement math matches `Conveyor.java:162-177`, but items never appeared with a static camera: the mesh only kept rebuilding if a belt already had items at the last rebuild (`audit/belt-items-1.png`: belt holds `copper:2`, nothing drawn). Fixed in `1a0f758` (`audit/belt-items-fixed-1.png`). Weak test: `rotation_one_rotates_the_axis_ninety_degrees` asserts `points[0].0 - 10.0 < 1e-4` without `abs()` (T11). Items share the belt mesh, so the next belt's quad can cover an item near the tile edge (T11). |
| `9e450a1` | PAR-0094 top table + ore hover | **wrong (layout) → fixed; partial** | The table sat inside the block column: native-size icon (96 px core icon), 190 px label → the category rail was clipped off-screen and the input bar pushed below the viewport (`audit/ingame-170.png`). Requirement `have` counts never refreshed. Hover ignored Java rules (own unit, other teams; ore showed the item icon instead of the ore icon). Fixed in `8067a5a` + `ded4e48` (`audit/palette-fixed-nohover.png`, `audit/ore-hover-fixed.png`, `audit/selected-conveyor.png`). Still open: key-combo line missing, hard-coded `Key:` text, selected palette cell not highlighted, unit hover uses the tile centre (T3–T5). `hover_info` off-map panic was fixed by the coordinator in `9aa3ef7`. |
| `8ce2941` | PAR-0126 composite units | **partial** | Math (`mind-core/src/render/unit_draw.rs`) matches `UnitType.drawMech`, `MechComp.walkExtend`, `Weapon.draw`, `cellColor`; trimmed regions are centre-stripped in this atlas (checked: `dagger 48x32 offsets [0,8] of 48x48`), so no offset compensation is needed. Missing: flying-unit ground shadow `drawShadow` (the player `alpha` is flying; Java draws its tinted silhouette down-left, see the grey blob next to the core in `runs/20261012-overview/java-groundzero-ingame.png`), unit/weapon `-outline` regions (Java alpha has a dark outline; port does not: `audit/unit-walking.png`), soft shadow drawn under the legs instead of over them. Dagger legs/weapons never verified in-engine (T6, T7). |
| `15ae932` | chore (font metrics casts) | **correct** | No behaviour change. |
| `bdc29ec` | PAR-0131 landing zoom (rendering side) | **correct (zoom only)** | Measured: mid-landing `zoom 2.136` at `land_time_in 0.527`, `4.0` after; matches `Renderer.java:180-199` (`fin = 1 - landTime/160`, `Interp.pow3` 0.02→4, `CoreBlock.java:57-65,655-657`). 160 ticks = 2.67 s at 60 fps; on software GL it takes 3–5 s because Java clamps `Time.delta` at 6 per frame too, so this is expected. Missing (campaign owner): scaled/rotated core sprite, white fade, landing announce. |
| `30765c8` | merge of session 3 | **correct** | Clean merge of the above; no conflict edits in render code. |
| docs commits (`cb555d3`, `88e16cc`, `3cfcf89`, `a4a0a27`) | plan status lines | **overstated** | They mark PAR-0130/0094/0126 "done"; see the verdicts above for what remained. |

Regression check of earlier HUD/text work (fonts, inline icons, objective colours, portrait hexagon, minimap colours/crop, coordinates, palette default): **no regressions** (`audit/ingame-170.png`, `audit/selected-conveyor.png`). One pre-existing defect became more visible: the world objective label draws **over** the HUD (T1).

## 3. Audit fixes already made (branch `parity/session-3`, not yet in `main`)

| SHA | Fix |
|---|---|
| `8067a5a` | Top table moved above the palette: `placement_fragment.tscn` is now `Panel/Frame(VBox)[TopTable, TopDivider, Layout]`, Panel anchored bottom-right growing up/left; icons `EXPAND_IGNORE_SIZE` (32/16 px); requirement `have` re-read every 250 ms. |
| `ded4e48` | `MindSimHost::tile_at(x, y) -> Option<&Tile>` bounds helper used by `conveyor_items`, `construct_draw`, `construct_at`, `building_team`, `building_rotation`, `tile_debug`; `hover_info` follows Java (`Units.closestOverlap(player.team(), …, !u.isLocal())`, ore = own `block-<ore>-ui` icon + item name, liquid floor = liquid icon/name). **Overlaps session 4's `8b6ee23`** (bounds-check MindSimHost tile accessors on `parity/session-4`): when both are merged keep one helper (prefer whichever is already in `main`, delete the other, re-run `cargo check -p mind-gdext`). |
| `1a0f758` | Belt items: any visible belt keeps the dynamic block pass rebuilding. |

Merge these before starting §4: `git merge parity/session-3` in your worktree, then `cargo test -p mind-core --lib` and `tools/build.sh`.


## 4. Tasks

Each task: symptom → acceptance (vs a Java frame) → Java reference → current port code → ordered changes → checks → verify recipe → pitfalls. `PORT=6970+10(N-1)`; `G='python3 tools/gmcp.py'`. Save every before/after PNG under `runs/<YYYYMMDD>-<task>/`.

### Part A — defects in the merged work

#### T1. World objective label draws over the HUD

- **Symptom.** The Ground Zero marker label "Move near the [ore] copper ore on the ground…" is painted on top of the block palette / top table (`../Mindustry-Godot-session3/runs/20261012-audit-s3/selected-conveyor.png`). In Java the UI covers it (`runs/20261012-overview/java-groundzero-ingame.png`: the label is cut by the "Copper" top table).
- **Acceptance.** Launch Ground Zero, select conveyor (`--eval 'return get_node("/root/Spine/Input").select_block_by_name("conveyor")'`): the top table and palette fully cover the label where they overlap.
- **Java.** `mindustry/game/MapObjectives.java` `ShapeTextMarker.draw` draws in world space through `WorldLabel` during `Renderer.draw` (before `Core.scene.draw()`), so all UI is above it.
- **Port.** `client/rust/mind-gdext/src/render.rs` (`ensure_objective_markers`-style block around line 325-343) creates a `CanvasLayer` named `ObjectiveMarkers` at runtime with the default `layer = 1`; `client/scenes/game.tscn` line 54 `Ui` is also a `CanvasLayer` with the default `layer = 1`. Equal layers sort by creation order, and the runtime layer is created later, so it draws above the UI.
- **Change.** In `client/scenes/game.tscn`, add `layer = 2` to the `[node name="Ui" type="CanvasLayer" …]` block (the `Planet` CanvasLayer stays at 1, so the UI stays above the planet view). Do not change the Rust code.
- **Checks.** None in Rust. Grep that no other code calls `set_layer` on `Ui`: `grep -rn "set_layer\|\.layer = " client/ui client/rust/mind-gdext/src` → no hits.
- **Verify.** `tools/session-play.sh N --play`, launch, select conveyor, screenshot: the label is hidden behind the palette. Also open the planet page (`$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "{}")'`) and confirm the dialog is still above the 3D planet.
- **Pitfall.** Tooltips/popups are viewport-level and unaffected.

#### T2. Block shadow/darkness maps are offset half a tile up-right

- **Symptom.** Every block's soft shadow sits up-right of the block (`audit/par0152-blocks.png`, the core in `audit/ingame-170.png`); in Java it is centred under the block and spreads mostly down-left (`java-groundzero-ingame.png`, the core).
- **Acceptance.** The core's dark halo is centred on the core sprite (equal spread on all four sides, ±4 screen px at zoom 4); boulder shadows hug the boulders.
- **Java.** `graphics/BlockRenderer.java:484-492` writes one texel per tile at `Fill.rect(tile.x + 0.5f, tile.y + 0.5f, 1, 1)`; `drawShadows` (`:503-518`) samples with `x = camera.position.x + tilesize/2f` (a −4 world px shift); `drawDarkness` (`:449-450`) uses `Draw.fbo(dark.getTexture(), world.width(), world.height(), tilesize, tilesize/2f)` (also shifted −tilesize/2). A tile's centre is `x*tilesize` (`Tile.worldx()`), so texel `x` must cover world `[x*8-4, x*8+4]`.
- **Port.** `client/rust/mind-gdext/src/render/shadow.rs` `world_quad(width, height)` (line ~163) places the quad from world `(0,0)` to `(W*8, H*8)`, so texel `x` covers `[x*8, x*8+8]`: +4 px in x and y.
- **Change.** In `world_quad`, subtract half a tile from the centre:
  ```rust
  let half = size / 2.0;
  Quad { cx: width as f32 * size / 2.0 - half, cy: height as f32 * size / 2.0 - half, w: width as f32 * size, h: height as f32 * size, uv: [0.0, 0.0, 1.0, 1.0], color: 0xffff_ffff }
  ```
  Leave the rest of `update()` unchanged (the same quad mesh is used for shadow and darkness, and both are shifted −tilesize/2 in Java).
- **Checks.** Add a unit test in `shadow.rs` (`#[cfg(test)]`): `world_quad(2, 2)` → `cx == 4.0, cy == 4.0, w == 16.0`.
- **Verify.** Build, restart, launch, screenshot; crop around the core `(500,240)-(660,400)` and compare with the same crop of `java-groundzero-ingame.png`.
- **Pitfall.** Do not flip `flip_v`: the y orientation is already right (the error is a translation).

#### T3. Top table: key-combo line, bundle text, label sizing

- **Symptom.** Selecting conveyor shows only "Conveyor" (`audit/selected-conveyor.png`); Java shows the name plus a second line `Key: [3,1]` in light grey (bundle `placement.blockselectkeys = \n[lightgray]Key: [{0},`).
- **Acceptance.** Header shows `Conveyor` and, below it, `Key: [<category key>,<digit>]` in light grey; the `?` button is fully visible at the right edge, 40×40.
- **Java.** `ui/fragments/PlacementFragment.java:371-395`: `keyCombo = Core.bundle.format("placement.blockselectkeys", blockSelect[currentCategory.ordinal()].value.key.toString()) + (i < 10 ? "" : blockSelect[(i + 1) / 10 - 1].value.key.toString() + ",") + blockSelect[i % 10].value.key.toString() + "]"`; label `labelWrap(...).left().width(190f).padLeft(5)`; `?` button `size(8*5).padTop(-5).padRight(-5)`.
- **Port.** `client/ui/fragments/placement_fragment.gd` `_build_block_header` (label = `MindRichLabel`, no `fit_content`, so the second line is clipped) and `_key_combo` (hard-coded `"[color=#9a9a9a]\nKey: [%s][/color]"`).
- **Changes.**
  1. In `_build_block_header` set `label.fit_content = true`, `label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART`, `label.custom_minimum_size = Vector2(190, 0)`, `label.size_flags_horizontal = Control.SIZE_FILL` (not EXPAND), and keep the spacer as the only EXPAND child.
  2. In `_key_combo` build the text from the bundle: `var fmt := _bundle("placement.blockselectkeys", "\n[lightgray]Key: [{0},")`, replace `{0}` with the category key, append `(index < 10 ? "" : binds[(index + 1) / 10 - 1] + ",") + binds[index % 10] + "]"`, then pass the whole string through `MindWidgets.markup(...)` (it converts `[lightgray]` to BBCode). Remove the hard-coded colour.
  3. If `_block_select_displays()` returns fewer than 10 non-empty strings, log once with `push_warning` and return "" (today it silently returns "").
- **Verify.** `$G --eval 'return get_node("/root/Spine/Input").select_block_by_name("conveyor")'`, `sleep 1`, screenshot, crop `(800,250)-(1152,420)`.
- **Pitfall.** `MindWidgets.markup` expects `@key` for bundle lookups; pass the already-formatted string (no `@`).

#### T4. Selected palette cell is not highlighted

- **Symptom.** After selecting a block (palette click or `select_block_by_name`) the block cell has no accent frame (Java `Styles.selecti` checked state draws the `button-select` accent frame).
- **Acceptance.** The selected cell shows the accent (`#ffd37f`) frame; selecting another block moves it; deselecting (`select_block_by_name("air")`) clears it.
- **Java.** `PlacementFragment.java:300-330` `button.update(() -> { ... button.setChecked(control.input.block == block) ... })` — per frame.
- **Port.** `placement_fragment.gd` `_rebuild()` sets `button.button_pressed = name == selected` once; nothing updates it later.
- **Change.** In `_process`, read `SimHost.selected_block()`; when it differs from a cached `_last_selected`, iterate `blocks.get_children()` (skip `is_queued_for_deletion()`), set `button_pressed = (child.name == "block-%s" % selected)` with `set_pressed_no_signal`. Cache the new value.
- **Verify.** Select conveyor, screenshot the palette; select junction, screenshot again.
- **Pitfall.** Use `set_pressed_no_signal` so the ButtonGroup does not fire `pressed` (which would re-select).

#### T5. Unit hover uses the tile centre, not the cursor

- **Symptom.** `hover_info(x, y)` takes a tile and measures unit distance from the tile centre; Java uses the cursor world position (`Core.input.mouseWorldX/Y()`, radius 5 world px + unit hit radius).
- **Acceptance.** With the cursor on a friendly non-player unit that stands between tile centres, the top table shows that unit.
- **Java.** `PlacementFragment.java:797` `Units.closestOverlap(player.team(), Core.input.mouseWorldX(), Core.input.mouseWorldY(), 5f, u -> !u.isLocal() && u.displayable())` (overlap = distance < 5 + hitSize/2).
- **Changes.** (1) `client/rust/mind-gdext/src/sim_host.rs`: add `#[func] pub fn hover_info_world(&self, wx: f32, wy: f32) -> Dictionary<GString, Variant>` that does the unit check against `(wx, wy)` with radius `5 + hit_size/2` (add `hit_size` to `UnitDraw` from `HitboxComp` if missing), then falls back to `self.hover_info(floor((wx+4)/8), floor((wy+4)/8))` for the tile part. (2) `placement_fragment.gd` `_mouse_tile()` → return the world position too: add a `screen_to_world` `#[func]` on `MindCamera2D` (`client/rust/mind-gdext/src/camera.rs`, next to `screen_to_tile` at ~279) and call `hover_info_world`.
- **Verify.** Unit tests are not practical; in-engine: spawn or find a friendly unit, `warp_mouse` onto it, screenshot.

#### T6. Flying units lack the ground shadow (`drawShadow`)

- **Symptom.** The player `alpha` (a flying unit, `UnitTypes.java:2494-2500`: `flying = true; lowAltitude = true`) shows no silhouette shadow; Java draws the full icon tinted `Pal.shadow` offset down-left (`java-groundzero-ingame.png`: the grey alpha-shaped blob beside the core at about `(515,375)`).
- **Acceptance.** A grey (alpha 0.22) alpha-shaped silhouette appears down-left of the alpha, offset ≈ `(-12, -13) * e` world px (×4 screen px at zoom 4), drawn under ground units/blocks above it.
- **Java.** `type/UnitType.java:1494-1497` (draw shadow at `Draw.z(Math.min(Layer.darkness, z - 1f))` when `unit.isFlying() || shadowElevation > 0`), `:1652-1664` `drawShadow`: `e = clamp(unit.elevation, shadowElevation, 1) * shadowElevationScl * (1 - drownTime)`, position `(x + shadowTX*e, y + shadowTY*e)`, `shadowTX = -12, shadowTY = -13` (`UnitType.java:44`), colour `Pal.shadow` = `(0,0,0,0.22)` (`Pal.java:63`) times `shadowAlpha` (1 when the floor `canShadow`), region `shadowRegion = fullIcon` (`:1184`; atlas `unit-<name>-full` if present, else `<name>`), rotation `unit.rotation - 90`. `shadowElevation` defaults to -1 and is resolved in `UnitType.init()` (`:931-933` sets 0.11 for naval units; flying units have `elevation = 1`, so `e = 1`), `shadowElevationScl = 1` (`:135`).
- **Port.** `client/rust/mind-gdext/src/render/units.rs` `UnitRenderer::update` pushes soft shadow → legs → body → cell → weapons into the unit's band bucket; no ground shadow.
- **Changes.**
  1. In `update`, before the soft shadow and only when `unit.flying || unit.elevation > 0`, push a quad into **bucket 3** (the new `Layer::Darkness` band, see the layering note below) with region `resolver.find(&format!("unit-{}-full", name)).or_else(|| resolver.find(&name))`, centre `(x + -12*e, y + -13*e)`, `e = unit.elevation.clamp(0, 1)`, size `region.w/h * sprite_scale`, rotation `rotation - 90`, colour `0x0000_0038`.
  2. Add a pure helper `pub fn flying_shadow_offset(elevation: f32) -> (f32, f32)` in `client/rust/mind-core/src/render/unit_draw.rs` with tests (`e=1 → (-12,-13)`, `e=0 → (0,0)`).
- **Verify.** Launch Ground Zero; screenshot crop `(520,280)-(640,400)`; compare with the same crop of the Java frame.
- **Pitfall / layering.** Java draws this shadow at `Math.min(Layer.darkness, z - 1)`: for a flying unit (`z = flyingLayer`) that is `Layer.darkness = 80`, i.e. above blocks (30) and ground units (60), below `plans` (85) and flying units (90/115). The port's unit buckets are `[ground, low, flying]` (`render.rs` ~466 `unit_bands`). Add a 4th band `self.band_node_at(BandKey::base(Layer::Darkness))` to `unit_bands` (index 3) and push the ground shadow into bucket 3. Check that `UnitRenderer::update` tolerates a missing band (`bands.get(key.0)` already returns `None` → skip).

#### T7. Soft shadow order and unit outlines

- **7a Soft shadow above legs.** Java order (`UnitType.java:1500-1527`): legs at `z - 0.02`, then `drawSoftShadow` at `z - 0.01` (over the legs), then body at `z`. Port pushes the soft shadow first. Change `units.rs::update` to push the soft-shadow quad after the leg quads and before the body.
- **7b Outlines.** Java packs `<unit>-outline` and `<weapon>-outline` regions (dark outline around the body; visible on the Java alpha). The port atlas has `large-weapon-outline` but no unit outlines (`alpha-outline`, `dagger-outline` missing).
  - Java: outline generation in `tools/src/mindustry/tools/Generators.java` (search `outline` / `makeOutline`), colour `UnitType.outlineColor = Pal.darkerMetal` = `#565666` (`UnitType.java:373`, `Pal.java:35`), radius `outlineRadius = 3` (`UnitType.java:375`).
  - Port: block icon outlines exist in `client/rust/mind-tools/src/generate/blocks.rs` (search `outline_color`, `outline_radius`). Add a unit pass in `client/rust/mind-tools/src/generate/` (new `units.rs` if there is none; register it where `blocks::run` is called) that, for every unit type's `region` (and each weapon region), writes `<name>-outline` with the same outline routine. Re-pack (`bash tools/pack.sh pack`) and confirm `probe("alpha-outline")` is found.
  - Draw: in `units.rs` push `<name>-outline` (if found) after the soft shadow and before the body, same transform as the body; push `<weapon>-outline` for each weapon before the body (`drawWeaponOutlines`, `UnitType.java:1539`).
- **Verify.** Screenshot the alpha; compare edge darkness with the Java crop.

#### T8. Belt autotiling and animation

- **Symptom.** Every belt draws the straight `conveyor-0-0` frame: corners and joins look wrong and belts do not animate.
- **Acceptance.** An L-shaped belt shows the curved corner (`conveyor-1-*`/`-2-*`), belts feeding into another from the side show the join, and the chevrons scroll while items move.
- **Java.** `world/blocks/distribution/Conveyor.java:144-160`: `frame = enabled && clogHeat <= 0.5f ? (int)((Time.time * speed * 8f * timeScale * efficiency) % 4) : 0`; `Draw.rect(regions[blendbits][frame], x, y, tilesize * blendsclx, tilesize * blendscly, rotation * 90)`; `blendbits/blendsclx/blendscly` from `onProximityUpdate` → `buildBlending(...)` (`world/blocks/Autotiler.java` `buildBlending`, `blendsclx/y` flips).
- **Port.** `render/blocks.rs` draws `block_region(def)` (the `-full` icon) for belts.
- **Changes.**
  1. Port `Autotiler.buildBlending` as a pure function in `client/rust/mind-core/src/render/autotile.rs`: input = the belt's rotation and, for each of the 4 neighbours, whether it is a belt-like block facing into this tile (`Conveyor.blends`), output `(blendbits 0..4, sclx ±1, scly ±1)`; port the Java unit cases (straight, left/right corner, T, both sides) as tests.
  2. Read-only seam in `sim_host.rs`: `belt_blend(x, y) -> (u8, f32, f32)` computing neighbours via `tile_at` + `BuildingComp.rot`.
  3. `blocks.rs`: for `Conveyor | ArmoredConveyor`, region = `format!("{}-{}-{}", def.name, blendbits, frame)` with `frame = ((sim_time_ticks * speed * 8 * efficiency) as i32).rem_euclid(4)`, quad size `8*sclx, 8*scly` (negative = mirror: swap UVs like units do).
- **Verify.** Place belts: `(126,53)` rot 0 then `(127,53)`… plus a side feeder; screenshot twice 0.5 s apart.
- **Pitfall.** `place_block` has no rotation parameter (always 0). For rotated test belts add `#[func] place_block_rot(x, y, block, rot)` in `sim_host.rs` that calls the same `Command::Place` path with `rot` (check `Sim::place_for_team` takes `rot`).

#### T9. Construct sites: blockbuild shader instead of the accent ghost

- **Symptom.** Construct sites draw an accent-tinted ghost plus a bottom-up reveal (`render/blocks.rs`, `BlockKind::ConstructBlock` branch); Java uses the `blockbuild` shader (outline scan + diamond reveal).
- **Java.** `world/blocks/ConstructBlock.java:251-276` (`Draw.color(Pal.accent, Pal.remove, constructColor)`, `Shaders.blockbuild` with `region`, `time`, `progress`, one pass per `current.getGeneratedIcons()` region, `Layer.blockBuilding` = 40), shader `core/assets/shaders/blockbuild.frag`.
- **Port.** Shader exists: `client/shaders/blockbuild.gdshader` (check its uniform names first: `grep uniform client/shaders/blockbuild.gdshader`).
- **Changes.** Draw each construct site as its own pooled `MeshInstance2D` (`# code-instantiated: one node per visible construct site`) under the `Layer::BlockBuilding` band (`render.rs` `band_node_at(BandKey::base(Layer::BlockBuilding))`), quad = target block region at footprint size, `ShaderMaterial(blockbuild)` with uniforms `u_uv`/`u_uv2` = region UV corners, `u_texsize` = page size, `u_progress` = progress, `u_time` = sim ticks, modulate = `Pal.accent` (`#ffd37f`) lerped to `Pal.remove` (`#e55454`) by `constructColor` (0 unless deconstructing). Remove the ghost/reveal quads from the main mesh.
- **Verify.** Construct sites appear when a builder works a plan (building plan of session "building"): `$G --eval 'return str(get_node("/root/Spine/SimHost").tile_debug(x, y))'` shows `construct: true`; screenshot over ~3 s.

#### T10. Per-part block drawing (drill rotator, turret aim, team top)

- **Symptom.** Drills and turrets are static composed `-full` icons: the drill rotator does not spin, turrets do not aim, the turret base is not separated.
- **Java.** `world/blocks/production/Drill.java` `DrillBuild.draw()` (base `region`, `rotatorRegion` rotated `timeDrilled * rotateSpeed`, `topRegion`); turrets `world/draw/DrawTurret.java` `draw(Building)` (base `block-<size>` via `basePrefix`, the turret region at `build.recoilOffset`, rotation `build.rotation - 90`, heat region).
- **Changes.** Add read-only seams `drill_draw(x, y) -> Option<f32 /*time_drilled*/>` and `turret_draw(x, y) -> Option<(f32 /*rotation*/, f32 /*recoil*/, f32 /*heat*/)>` in `sim_host.rs` (read the drill/turret build components in `mind-core/src/world/blocks/production/drill*.rs` and `defense/turrets/mod.rs`). In `blocks.rs`, for `BlockKind::Drill` push `region`, `<name>-rotator` (rotation `time_drilled * rotate_speed`; `rotate_speed` default 2 in `Drill.java`), `<name>-top`; for `ItemTurret|PowerTurret|LiquidTurret` push `block-<size>` then `<name>` at the recoil-offset position, rotation `rotation - 90`. Keep the `-full` path for all other kinds.
- **Verify.** Screenshot a drill twice 0.5 s apart (rotator angle changes); a duo during a wave (aims at enemies).

#### T11. Belt item ordering and the weak test

- **11a.** Items must draw above **all** belts (`Layer.block - 0.1` vs belts at `Layer.block - 0.2`). In `render/blocks.rs` collect belt item quads into a separate `Vec<UnitQuad>` per page and append them to the page group **after** the tile loop finishes (so no later belt covers them).
- **11b.** `client/rust/mind-core/src/render/conveyor_items.rs` test `rotation_one_rotates_the_axis_ninety_degrees`: change the first assertion to `assert!((points[0].0 - 10.0).abs() < 1e-4 && (points[0].1 - 20.0).abs() < 1e-4);`.
- **Verify.** `cargo test -p mind-core --lib render::conveyor_items`; screenshot a saturated belt line (items no longer clipped at tile edges).


### Part B — rendering/HUD items not started yet

#### T12. PAR-0070 / PAR-0128 — no impacts/explosions/smoke/shake; FX regions drawn as flat rectangles

- **Symptom.** Combat shows no hit sparks, muzzle flashes, explosions, smoke, wrecks or screen shake. When an effect does run (dev probe) its sprite regions are solid rectangles.
- **Acceptance.** (1) `$G --eval 'return get_node("/root/Spine/MindFx").spawn_effect("hitBulletSmall", 1032, 440, 0)'` then a screenshot within 0.2 s shows spark sprites (not filled rects). (2) During a wave (duo near enemies) `$G --eval 'return get_node("/root/Spine/MindFx").live_effect_count()'` > 0 and screenshots show muzzle flashes / hits; a unit death shakes the camera. Compare with `runs/20261010-234250-ref-PAR-0070-combat-fx/` Java frames.
- **Java.** `entities/Effect.java` (`Effect.create`, `Effect.shake`), `content/Fx.java` (effect bodies), `graphics/Layer.java` (`bullet = 100`, `effect = 110`), `core/Renderer.java` `shake()`.
- **Port.**
  - Producer missing: `client/rust/mind-core/src/sim/unit_runtime.rs:136` initialises `fx: noop_fx()`; `FxHandle = Arc<dyn FxSink>` (`mind-core/src/combat/view.rs:84`); nothing installs a real sink, so every `fx.effect/shake` from the sim is dropped. Same in `mind-core/src/combat/harness.rs:115` (setter at `:810`).
  - Consumer: `client/rust/mind-gdext/src/fx.rs` `MindFx` owns `bus: FxBus` by value (`:47`, `:71`), `bus()` (`:421`), `tick_view` drains it; `draw_prim` draws `PrimKind::Region` as a filled rect (`:273-283`) and ignores `NoiseLayer/ShaderBlit/Polyline`.
- **Changes.**
  1. `fx.rs`: change the field to `bus: Arc<FxBus>` (`FxBus` wraps a `Mutex`, `mind-core/src/fx/sink.rs:133`, so it is `Send + Sync`), initialise `Arc::new(FxBus::new())`, keep `bus()` returning `&FxBus` (`&self.bus`), add `pub fn bus_handle(&self) -> Arc<FxBus>`.
  2. `sim_host.rs`: add `fn install_fx_sink(&mut self)` that resolves `/root/Spine/MindFx` (`try_get_node_as::<MindFx>`), takes `bus_handle()`, and sets `rt.fx = handle as FxHandle` through `self.sim.unit_runtime_mut()` (`mind-core/src/sim/unit_runtime.rs:321`). Call it at the end of `install_sim_seams` (`sim_host.rs:358`) and after every `self.sim = sim` assignment (`sim_host.rs` ~1280 and ~1376; grep `self.sim = ` for others) — a new sim has a new unit runtime.
  3. `fx.rs::draw_prim` `PrimKind::Region { region, x, y, w, h, rotation_deg, origin, color, mix, .. }`: resolve `region.0` through `/root/MindAssets` `find_region(name)` (returns an `AtlasTexture`; cache in `HashMap<&'static str, Option<Gd<Texture2D>>>` on `MindFx`), then
     ```rust
     base.draw_set_transform(Vector2::new(*x, *y), rotation_deg.to_radians(), Vector2::ONE);
     base.draw_texture_rect_ex(&tex, Rect2::new(Vector2::new(-w * origin.0, -h * origin.1), Vector2::new(*w, *h)), false).modulate(to_color(final_color)).done();
     base.draw_set_transform(Vector2::ZERO, 0.0, Vector2::ONE);
     ```
     where `final_color` = `color` lerped toward `mix` (if `Some`) by the mix alpha (Arc `Draw.mixcol` approximation). Missing region → keep the old filled rect.
  4. `PrimKind::Polyline` → `draw_polyline_ex(points, color).width(stroke)`. `NoiseLayer`/`ShaderBlit` stay unimplemented (weather/env owner).
  5. Screen shake: in `tick_view`, for each drained `FxEvent::Shake { intensity, duration }` call `/root/Spine/World/Camera2D` `apply_shake_event(intensity, duration)` (`camera.rs` `#[func] apply_shake_event`, ~409), multiplied by `MindUi.settings_get_i32("screenshake") / 4.0` (Java `Renderer.shake` scales by the `screenshake` setting / 4).
- **Checks.** mind-core: add a test next to an existing combat test that installs a recording `FxSink` on `UnitRuntime.fx` and asserts at least one event after a bullet hit.
- **Verify.** Recipes in Acceptance. Expected: `live_effect_count()` returns >0 for a few frames after `spawn_effect`.
- **Pitfalls.** Do not hold `MindFx.bind_mut()` while the sim steps (borrow panic). The world is y-up through the camera: effect regions must use the same UV flip as other world sprites — if sprites appear upside down, pass `flip_v` like `build_mesh` (`render/atlas_bind.rs`).

#### T13. PAR-0129 — shields invisible

- **Symptom.** Force-projector domes and unit shield bubbles never render.
- **Acceptance.** In a custom/sandbox game, a powered `force-projector` shows a team-coloured hexagonal dome (radius `realRadius()`), brighter when hit; units with a `ForceFieldAbility` show their bubble.
- **Java.** `world/blocks/defense/ForceProjector.java:292-330` (`drawShield`: `Draw.color(team.color, Color.white, clamp(hit))`; with `renderer.animateShields`: `Draw.z(Layer.shields + 0.001f * hit); Fill.poly(x, y, sides, radius, shieldRotation)`; else `Layer.shields`, alpha `0.09 + clamp(0.08*hit)` fill + `Lines.poly` stroke 1.5); `type/UnitType.java:1643-1650` (`drawShield`: `Fill.light(x, y, circleVertices(r), r, clear, shieldColor.lerp(white, clamp(hitTime/2)).a(0.7*shieldAlpha))`, `r = hitSize * 1.3`); `core/Renderer.java:403-414` (shield layer rendered into `effectBuffer`, blitted with `Shaders.shield`); `Layer.shields = 125`.
- **Port.** Shader `client/shaders/shield.gdshader`; `MindWorldRenderer.shield_available()` (`render.rs:738`) only reports it; no pass.
- **Changes.**
  1. Read-only seams in `sim_host.rs`: `pub fn force_shields(&self) -> Vec<(f32 x, f32 y, f32 radius, u8 sides, f32 rotation, u8 team, f32 hit)>` (find the force-projector build state under `mind-core/src/world/blocks/defense/`), `pub fn unit_shields(&self) -> Vec<(f32, f32, f32 radius, f32 alpha, u8 team, f32 hit)>` from `ShieldComp` (`mind-core/src/entities/comp/unit/comp.rs:328`).
  2. New `client/rust/mind-gdext/src/render/shields.rs`: a `CanvasGroup` node placed under `band_node_at(BandKey::base(Layer::Shields))` with `material = ShaderMaterial(shield.gdshader)` (a `CanvasGroup` renders its children to a buffer and applies the material — the Godot equivalent of `effectBuffer.blit(Shaders.shield)`); a child `Node2D` whose `_draw` (custom draw via `queue_redraw` each frame) calls `draw_colored_polygon(hexagon, team colour)` per dome and a radial fan (`draw_polygon` with centre colour transparent, rim colour α 0.7) per unit shield. Set the CanvasGroup `fit_margin` ≥ the largest radius.
  3. If `shield_available()` is false, draw Java's non-animated path directly (fill α `0.09`, `draw_polyline` stroke 1.5).
- **Verify.** Sandbox: place `force-projector` + power source (sandbox blocks from the building owner), screenshot.

#### T14. PAR-0097 — F6 block status markers

- **Symptom.** F6 does nothing. Java toggles small diamonds at the bottom-right corner of every consuming building.
- **Acceptance.** After pressing F6 (`$G godot_input '{"action":"key","params":{"key":"F6","pressed":true}}'` then `pressed:false`), a working drill on ore shows a green diamond at its bottom-right corner and a drill off ore a red one; F6 again hides them.
- **Java.** `input/DesktopInput.java:455-457` (F6 → `Core.settings.put("blockstatus", !…)`); `core/Renderer.java:170` (`drawStatus = settings.getBool("blockstatus")`); `graphics/BlockRenderer.java:825-833` (only with `Lod.l2`, for `tileWithConsumerView`); `entities/comp/BuildingComp.java:1221-1234` `drawStatus()`: `multiplier = size > 1 ? 1 : 0.64`, `brcx = x + size*4 - 8*multiplier/2`, `brcy = y - size*4 + 8*multiplier/2`, `Draw.z(Layer.power + 1)`, `Pal.gray` (`#454545`) square radius `2.5*multiplier` rotated 45°, then `status().color` square radius `1.5*multiplier`; `status()` (`:638-652`): `!enabled → logicDisable`, `!shouldConsume() → noOutput`, `efficiency <= 0 || !productionValid() → noInput`, else `active` (blinking: `((state.tick/30) % 1) < efficiency ? active : noInput`); `world/meta/BlockStatus.java`: active `#5ce677`, noOutput `Color.orange` `#ffa500`, noInput `Pal.remove` `#e55454`, logicDisable `#8a73c6`, inactive `Color.lightGray` `#bfbfbf`.
- **Port.** Setting row `blockstatus` (`mind-core/src/ui/settings.rs:231`), binding `toggle_block_status` (`mind-core/src/input/binding.rs:763`) with no handler; `mind-core/src/world/status.rs:24-34` `BlockStatus::color()` returns the wrong colours (white / `ff1d1d` / `ffd37f`) and lacks `logicDisable`/`inactive`.
- **Changes.**
  1. `status.rs`: do NOT change `color()` (other code may use it); add `pub const fn java_color(self) -> u32` returning the Java colours above as `0xRRGGBBAA` (`None` → lightGray). Unit test each value.
  2. Input: in `client/rust/mind-gdext/src/input/mod.rs` where bindings become `Effect`s and `emit_signal("hotkey", …)` (~495), make `toggle_block_status` emit `hotkey("toggle_block_status")`; in `client/ui/fragments/hud_fragment.gd` connect `/root/Spine/Input` `hotkey` and on that action call `MindUi.settings_set("blockstatus", not MindUi.settings_get_bool("blockstatus"))`.
  3. `sim_host.rs` read seam: `pub fn building_statuses(&self) -> Vec<(i32 x, i32 y, i32 size, u32 rgba)>` for player-team buildings whose `def.consumes` is non-empty, using the building's `enabled`/`efficiency` (the same fields `building_debug` already exposes, `sim_host.rs` ~715) and the Java `status()` order; colour via `java_color()`.
  4. New pass `client/rust/mind-gdext/src/render/status.rs` (`Node2D` under `band_node_at(BandKey::base(Layer::Power))`, custom `_draw`): when `MindUi.settings_get_bool("blockstatus")`, draw two 45°-rotated squares per building with the formula above. Skip when the camera's LOD stat `lod_l2` is false.
- **Checks.** Pure helper `status_corner(x, y, size) -> (f32, f32, f32)` in `mind-core/src/render/` with tests for size 1 (`m = 0.64`) and size 2.
- **Pitfall.** Java only draws buildings `wasVisible`; use the visible-tile scan (`mind_core::render::scan::visible_blocks`) to limit the work.

#### T15. PAR-0096 — minimap attack indicators and unit icons

- **Symptom.** Damaged player buildings never flash on the minimap, and units are not drawn on it (Java draws team-tinted unit icons; the player alpha is visible at the centre of the Java minimap).
- **Acceptance.** The HUD minimap shows a small team-coloured icon at the player's position; after a player building takes damage, a pulsing orange→scarlet square appears at its tile for 15 s.
- **Java.** `game/AttackIndicators.java` (duration `15 * 60` ticks, `add(x, y)`, `update()`), `core/Control.java:68-72` (`BuildDamageEvent` for `Vars.player.team()` → `indicators.add(tileX, tileY)`), `graphics/MinimapRenderer.java:253-275` (`fin = (Time.globalTime / 30f) % 1f`, `rad = fin*5 + tilesize - 2`, stroke `(1 - fin)*4 + 0.5`, colour `Color.orange.lerp(Color.scarlet, clamp(time/70))`, `Lines.square((ix + 0.5 + offset)*tilesize, (iy + 0.5 + offset)*tilesize, rad)` with `offset = build.block.offset / tilesize`), units `MinimapRenderer.drawEntities` (`:157-165`: `unit.icon()`, `Draw.mixcol(unit.team.color, 1f)`, size `scale`, rotation `rotation - 90`).
- **Port.** Widget `client/ui/widgets/mind_minimap.gd` (`_draw_cropped` draws the camera crop from `MinimapProvider.region()` in texture px; texture row 0 = map top); provider `client/rust/mind-gdext/src/render/minimap.rs`.
- **Changes.**
  1. Provider (`minimap.rs`): `indicators: Vec<(i32, i32, f32 /*age ticks*/)>`, `#[func] add_indicator(x, y)` (reset age if present), age-out in `process` at 900 ticks; `#[func] indicators_json() -> GString` `[[x,y,age],…]`; `#[func] minimap_units_json() -> GString` `[[x,y,rot,name,team],…]` from `MindSimHost::unit_draws_full()`.
  2. Feed: find the sim's building-damage event (`grep -rn "BuildDamage" client/rust/mind-core/src`). If it exists, drain it in `sim_host.rs` and call the provider's `add_indicator` for the player team. If it does not, add only the `#[func]` and record "needs BuildDamageEvent from the combat owner" in the commit body.
  3. `mind_minimap.gd` `_draw_cropped`: after the texture, map tile `(tx, ty)` to widget px: `px = inner.position + (Vector2(tx + 0.5, (tex_h - 1 - ty) + 0.5) - region.position) * scale`; draw `draw_rect(Rect2(px - Vector2(r, r), Vector2(2r, 2r)), color, false, stroke)` with Java's pulse values scaled by `scale / 8` (`rad` is in world px). Units: `MindWidgets.icon_texture("unit-%s-ui" % name)` drawn at the unit's mapped position, 8×8 px, `modulate = team colour` (`mind_core` team palette: sharded `#ffd37f`, crux `#f25555`).
- **Verify.** `$G --eval 'get_node("/root/Spine/World/MinimapProvider").add_indicator(129, 55); return true'` then screenshot the minimap twice 0.25 s apart (the square pulses).

#### T16. PAR-0157 — command-mode selection overlay

- **Symptom.** In command mode, selected units get no hexagon ring, target line or move square.
- **Acceptance.** With units selected in command mode, each shows an accent (`#ffd37f`) hexagon ring, a translucent inner fill, and a line + 3.5-unit square to its move target.
- **Java.** `input/InputHandler.java:1211-1300` `drawCommanded` / `drawCommandedTargets`: `sides = 6`, `alpha = 0.5`, `lineLimit = 6.5`, `rad = unit.hitSize / unitSelectRadScl + 1` (`unitSelectRadScl = 1`, `:56`), `Fill.lightInner(x, y, 6, rad*0.8, rad, 0, color.a(0), color.a(0.7))`, `Lines.stroke(1); Lines.poly(x, y, 6, rad + 0.5)`, `Drawf.limitLine(unit, dest, rad, lineLimit, color.a(0.5))`, `Drawf.square(dest, 3.5, color.a(0.5))` when no attack target, `Drawf.target(attackTarget, 6, Pal.remove)`; layers `Layer.plans` (flying units), `Layer.groundUnit - 1` (ground), `Layer.overlayUI` (targets). Colour `Team.malis.color` for `LogicAI` units.
- **Port.** `client/rust/mind-gdext/src/input/mod.rs`: `command_mode()` (~792), `selection_json()` (~804, `selected_units` ids), signal `selection_changed`. No draw.
- **Changes.** New `client/rust/mind-gdext/src/render/command_overlay.rs`: a world-space `Node2D` under `band_node_at(BandKey::base(Layer::Plans))` drawing each frame when `/root/Spine/Input` `command_mode()`: parse `selection_json()`, get positions/hit sizes/targets from a new read seam `sim_host.rs::unit_command_view(ids: &[i64]) -> Vec<(f32 x, f32 y, f32 hit_size, bool flying, Option<(f32,f32)> target, Option<(f32,f32)> attack)>` (read the CommandAI state the units owner added; if absent, target = None). Draw: hexagon fan (centre α0 → rim α0.7) with `draw_polygon`, ring `draw_polyline` width 1 radius `rad + 0.5`, target line `draw_line` α0.5 shortened by `rad` at the unit end, square `draw_rect` 7×7 outline.
- **Verify.** Toggle command mode (`$G --eval 'return get_node("/root/Spine/Input").toggle_command_mode()'`), select units through the real input path (box-drag with `godot_input mouse_button` + `mouse_motion`), screenshot.
- **Depends on** the units owner feeding the selection (PAR-0140).

#### T17. PAR-0095 — Shift command panel

- **Symptom.** In command mode the block palette stays; Java swaps it for the command table.
- **Acceptance.** Command mode with nothing selected: the palette area shows `[accent]Command Mode` (bundle `commandmode.name`), an accent rule, and `[no units]` (`commandmode.nounits`); with units: one button per unit type (icon + count) and the shared command buttons (50×50).
- **Java.** `ui/fragments/PlacementFragment.java:443-676` (`commandTable = new Table(Tex.pane2)`; swap in `mainStack` when `control.input.commandMode` changes, `:447-456`; content `:471-670`; command buttons `:571-590` → `Call.setUnitCommand`; keybinds `:663-668`).
- **Port.** `client/ui/fragments/placement_fragment.gd` has no command branch; `MindInput.command_mode()`, `selection_json()`, `set_unit_stance(stance, enabled)` exist; there is no `set_unit_command` `#[func]`.
- **Changes.**
  1. `client/scenes/ui/fragments/placement_fragment.tscn`: add `Panel/Frame/CommandPanel` (PanelContainer, hidden) as a sibling of `Layout`, with a VBox `Rows`. Stylebox from atlas `pane-2` (`Tex.pane2`; ninepatch margins 12).
  2. `placement_fragment.gd` `_process`: poll `/root/Spine/Input`.`command_mode()`; on change swap `Layout.visible` / `CommandPanel.visible`; rebuild `Rows` on the `selection_changed` signal: title label (`MindWidgets.markup("@commandmode.name")`), 4 px accent `ColorRect`, then either `@commandmode.nounits` or one button per unit type (`# code-instantiated: one button per selected unit type`), counts from grouping the selection by type (add read seam `unit_types_for(ids_json) -> GString` in `sim_host.rs`).
  3. Command buttons need `#[func] set_unit_command(ids_json, command)` in `input/mod.rs` forwarding to the units owner's command path; if it does not exist, render the buttons disabled and note it in the commit.

### Part C — small visible gaps (one commit each)

#### T18. Main menu labels are left-aligned
- **Acceptance.** Menu "Play/Database/Editor/Mods/Settings/Quit" labels are centred in the button like `runs/20261012-overview/java-menu.png`.
- **Java.** `ui/fragments/MenuFragment.java:267-271` `t.button(b.text, b.icon, Styles.flatToggleMenut, …).marginLeft(11f)`: an Arc `TextButton` centres its label; the icon sits left.
- **Port.** `client/ui/fragments/menu_fragment.gd` `_make_button` (~218): `label.size_flags_horizontal = EXPAND_FILL` but no alignment.
- **Change.** Add `label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER`. Screenshot the menu (`tools/session-play.sh N --play` boots to it) and compare.

#### T19. Minimap (and world) ignore the map-area limit
- **Symptom.** The Ground Zero minimap shows the whole 256×256 map; Java shows only the play area, black outside (`java-groundzero-ingame.png` minimap). The player can also fly out of the play area into black (`audit/unit-walking.png`).
- **Java.** `core/World.java:484-503` `getDarkness`: with `rules.borderDarkness`, `edgeDst` from the limit rect (`limitX/Y/Width/Height`) when `limitMapArea`, `dark = max((2 - edgeDst) * 2, dark)` for `edgeDst <= 2` (≥4 = black); `MinimapRenderer.java:390` `color.mul(1 - clamp(world.getDarkness(x, y) / 4))`.
- **Port.** `client/rust/mind-gdext/src/render/minimap.rs` `tile_color` uses only `get_static_darkness` (`mind-core/src/world/darkness.rs:119`); `WorldGrid.limit_area: Option<(x,y,w,h)>` exists (`mind-core/src/world/mod.rs:151`).
- **Changes.** Add `pub fn border_darkness(grid: &WorldGrid, border: bool, x: i32, y: i32) -> f32` in `darkness.rs` porting the `borderDarkness` half (use `grid.limit_area` when `Some`, else the map bounds), with tests (inside → 0, on the edge → 4, outside → ≥4). In `tile_color`, use `max(static, border)`. Before changing the minimap, add `limit_area` (as `[x,y,w,h]` or `null`) to the `tile_debug` dictionary in `sim_host.rs` and run `$G --eval 'return str(get_node("/root/Spine/SimHost").tile_debug(129, 55))'` to confirm Ground Zero sets it; if it is `null`, the loaded rules drop `limitMapArea` — report that to the campaign owner and stop this task after the darkness helper + tests. The movement clamp is the units owner's (note it in the commit).

#### T20. Hint placement and the skip-wave button
- **Hint.** "Use [WASD] to move." sits at mid-left (y≈300). Java `ui/fragments/HintsFragment.java` `build`: read where the hint table is anchored (`top().left()` under the status table) and move `client/scenes/ui/fragments/hints_fragment.tscn` accordingly.
- **Skip wave.** Java puts the skip button inside the status table, right side (`HudFragment.java:491-510`, style with `buttonRight`, width 40, `growY`). Port: `hud_fragment.gd` `_build_wave_buttons` adds it to `WaveButtons` below the status row. Move it into `WavesStack/WavesMain/StatusRow/Row` as the last child, 40 px wide, `size_flags_vertical = FILL`, shown only when `canSkipWave` (existing visibility logic).

## 5. Execution order, dependencies, shared files

Order (each line is one commit; do not reorder):

1. Merge `parity/session-3` (§3) and resolve the `sim_host.rs` bounds-helper overlap with `8b6ee23`.
2. T1 (CanvasLayer order) — trivial, unblocks clean HUD screenshots.
3. T2 (shadow/darkness half-tile offset).
4. T3, T4 (top table text + selection highlight) — same file, two commits.
5. T11 (belt item order + test), then T8 (belt autotile/animation) — T8 reuses the item-after-belts structure from T11.
6. T6 (flying shadow), T7a (soft-shadow order), T7b (unit/weapon outlines; needs a re-pack).
7. T12 (FX producer + region execution + shake) — largest gameplay-visible win.
8. T10 (drill rotator / turret parts), T9 (blockbuild shader).
9. T14 (F6 status), T15 (minimap indicators/units), T19 (map-area darkness), T18 (menu), T20 (hint/skip).
10. T13 (shields) — mid-game reachability; needs a sandbox.
11. T16, T17 (command overlay/panel) — depend on the units owner's selection/commands (PAR-0140); T5 last.

Dependencies: T8 after T11; T9 and T10 touch the same `render/blocks.rs` loop — do them in sequence and rebuild between; T15 needs a building-damage event from the combat owner (stub otherwise); T16/T17 need the units owner's command state.

Files shared with other areas (pull `main` before touching, keep commits small):
- `client/rust/mind-gdext/src/sim_host.rs` (every read seam in this plan; also edited by building/combat/campaign work — add new seams at the end of the `#[godot_api] impl` block; reuse the bounds helper).
- `client/rust/mind-gdext/src/render/blocks.rs` (building owner also touches construct sites).
- `client/rust/mind-gdext/src/fx.rs`, `client/rust/mind-core/src/sim/unit_runtime.rs` (T12; combat owner).
- `client/rust/mind-core/src/world/status.rs` (T14; building owner) — add `java_color`, do not change `color()`.
- `client/rust/mind-tools/src/generate/` (T7b) — every checkout must re-pack afterwards.
- `client/scenes/game.tscn` (T1).
- `client/ui/fragments/placement_fragment.gd/.tscn` (T3, T4, T5, T17), `hud_fragment.gd/.tscn` (T14, T20).
