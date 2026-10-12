# Parity plan: in-game rendering + HUD/text (session 3)

Executor-ready plan for the **IN-GAME RENDERING + HUD/TEXT** workstream of the
Mindustry-Godot 1:1 port. Every item lists the symptom, the acceptance
criterion, the Java reference, the current Godot state, ordered changes, tests,
verification recipes and pitfalls. Line numbers were checked against the trees
on 2026-10-12 (Java: `/home/clintonnguyen/g/code_examples/Mindustry`, port:
this worktree at commit `c84dcda`). If a line moved, search for the quoted
method name.

Work the items **in the order of section 3**. Items are independent unless a
"Depends on" line says otherwise.

---

## 0. Environment & workflow

| What | Value |
|---|---|
| Worktree (only place you edit/commit) | `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session3`, branch `parity/session-3` |
| Never edit | `/home/clintonnguyen/g/code_examples/Mindustry-Godot` (main) or other `-sessionN` worktrees; `chains.json` |
| Java source (read-only) | `/home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry/...`, assets `.../Mindustry/core/assets`, raw sprites `.../Mindustry/core/assets-raw/sprites` |
| Arc (read-only) | `/home/clintonnguyen/g/code_examples/Arc` |
| Godot bridge | port `6990`, X display `:12` |
| Reference frames | `runs/20261012-overview/java-groundzero-ingame.png`, `java-planet-groundzero.png`, `java-menu.png` (and the `godot-*` "before" frames next to them). Per-ledger runs: `runs/20261010-234250-ref-PAR-0070-combat-fx/`, `runs/20261009-065416-ref-ground-zero/` |
| Evidence output | `runs/20261012-session3/` (save every before/after PNG here) |

### 0.1 Generated assets (do this first in any fresh worktree)

The asset-pipeline outputs are gitignored. A worktree without them loads **no
atlas, no fonts, no icons** (`[assets] ready ok=false ... fonts=0` in the log)
and every sprite is a placeholder. Regenerate them (≈10 min, release build):

```bash
cd /home/clintonnguyen/g/code_examples/Mindustry-Godot-session3
CARGO_BUILD_JOBS=6 bash tools/pack.sh pack      # NOTE: `bash`, tools/pack.sh is not executable
```

Expected tail: `pack: 5141 regions on 4 page(s) -> ./assets/sprites` and `EXIT 0`.
Check: `python3 -c "import json;d=json.load(open('assets/sprites/sprites.atlas.json'));print(len(d['regions']))"` → `5141`.
Any change under `client/rust/mind-tools/` needs a re-pack to show in-game.

### 0.2 Build / restart / drive

```bash
cd /home/clintonnguyen/g/code_examples/Mindustry-Godot-session3
CARGO_BUILD_JOBS=6 tools/build.sh            # mind-gdext + mind-headless, ~20 s incremental
T=200 tools/session-play.sh 3                # restart editor (REQUIRED after every build: the .so is mmap'd)
T=200 tools/session-play.sh 3 --play         # replay the scene only (GDScript/.tscn-only changes, or a pinned eval loop)
export OPEN_GODOT_MCP_PORT=6990
python3 tools/gmcp.py --eval 'return <single gdscript expression>'   # evals MUST `return`
python3 tools/gmcp.py --screenshot runs/20261012-session3/<name>.png # 1152x648 viewport; view with the Read tool
python3 tools/gmcp.py godot_input '{"action":"mouse_motion","params":{"position":{"x":780,"y":470},"coords":"viewport"}}'
```

Eval rules (a bad eval pins the frame loop for 15 s and blocks later evals;
recover with `tools/session-play.sh 3 --play`):
- one statement, no loops, no lambdas, never call a method that does not exist
  (e.g. `SimHost.block_at` does NOT exist — it pinned the loop once);
- only call `#[func]` methods from GDScript (plain `pub fn` in Rust are invisible);
- software GL: the game runs at 10–40 fps, so give `sleep 2` before screenshots.

Ground Zero launch (the way the UI does it; a bare `start_sector` gives "Game Over / no core"). Save as `/tmp/s3_enter.sh`:

```bash
cd /home/clintonnguyen/g/code_examples/Mindustry-Godot-session3
OPEN_GODOT_MCP_PORT=6990 python3 tools/gmcp.py \
  --eval 'get_node("/root/Spine/SimHost").load_sector("serpulo", 15); return true' \
  --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 15)' \
  --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true' >/dev/null
sleep 2
```

Camera then sits at tile (129,55) (`MindHud.position_text` = `"129,55"`).
Useful probes:

```bash
python3 tools/gmcp.py --eval 'return get_node("/root/Spine/SimHost").place_block(126, 53, "conveyor")'   # true
python3 tools/gmcp.py --eval 'return str(get_node("/root/Spine/World/Renderer").get_stats())'           # missing_regions, dynamic_sprites, unit_sprites...
python3 tools/gmcp.py --eval 'return str(MindWidgets.assets().probe("block-duo-full"))'                  # atlas lookup
python3 tools/gmcp.py --eval 'return get_node("/root/MindUi").open_dialog("planet", "{}")'              # planet page
python3 tools/gmcp.py --eval 'return str(get_node("/root/Spine/SimHost").player_unit_status())'         # {dead, healthf, type}
```

Logs: game process `…/Mindustry-Godot/.opencode/parity-session/session-3/xdg-data/godot/app_userdata/Mindustry-Godot/logs/godot.log`;
editor `…/Mindustry-Godot/.opencode/parity-session/session-3/editor-console.log`.

### 0.3 Tests and gates

```bash
cd client/rust
CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib            # expect "1853+ passed; 0 failed"
CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib -- ui::text # targeted
CARGO_BUILD_JOBS=6 cargo clippy -p mind-gdext -p mind-core -- -D warnings
CARGO_BUILD_JOBS=6 cargo check -p mind-gdext
```
RAM is shared (14 GB, 4 agents): one cargo job at a time, never run `pack` and a `build` concurrently.

### 0.4 Conventions

- Rust render code: `client/rust/mind-gdext/src/render/*.rs` (+ `render.rs`). Pure, testable logic goes in `client/rust/mind-core/src/...` with `#[cfg(test)]` unit tests. Do not change sim/gameplay logic in mind-core (other agents own combat/units/building/campaign); read-only accessors are fine.
- GDScript is UI-only (reads state, never mutates sim). Static UI lives in `.tscn`; any node built in code needs a `# code-instantiated: <reason>` comment.
- Every source file starts with `SPDX-License-Identifier: GPL-3.0-only` and cites the ported Java source.
- User-visible strings go through bundle keys (`MindWidgets.markup("@key")` / `_bundle(key, fallback)`).
- One commit per fix, in the worktree only, never push:
  `Fixes PAR-XXXX: <summary>` (or a descriptive subject), body explaining Java vs port, last line
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

## 1. Already done (do not redo)

| Commit | What | Evidence |
|---|---|---|
| `a3f6e46` | **Doubled/garbled text root cause fixed.** Autoloads + main scene enter the tree in one pass before any `_ready`, so `UiRoot._enter_tree` built the theme before `MindAssets._ready` loaded fonts → theme had no default font → Godot's fallback font (doubled glyphs, `D፤culty`, stray glyphs). `MindAssets::enter_tree` now calls `load_assets()` (`client/rust/mind-gdext/src/assets/atlas.rs`). All UI now uses `font.woff` (the Java pixel font). | `runs/20261012-session3/after-font-menu.png`, `after-font-ingame.png`, `after-font-planet.png` |
| `a479081` | Content-icon glyphs in the UI font (`Fonts.loadContentIcons`/`registerIcon`): bitmap `FontFile` from the UI atlas page chained as fallback of `font.woff` (+ `icon.ttf`). `Iconc::entries()` in mind-core. HUD-local fallback override removed. | `after-icons-ingame.png` (copper icon before "Copper") |
| `13eb10b` | Objective world label keeps Arc markup: `mind_core::ui::text::markup_runs` (coloured runs + icons); `render/objective_markers.rs` draws runs. | `after-marker-ingame.png` ("Move near the [icon] copper ore" in accent) |
| `b213cc2` | `PerformanceFragment` hidden unless `showperformance` (Java default off). `MindUi.settings_get_bool`. | `after-minimap.png` (no fps/cpu/mem) |
| `7c43a42` | **PAR-0057** minimap: `block_colors.png` palette (`ContentLoader.loadColors`), camera-following crop (`MinimapRenderer.getRegion`), team colours (`Team.java` palette), `Tex.pane` frame, position label centred under the minimap. | `after-minimap.png`, `after-minimap2.png` |
| `d2462d6` | Block palette opens on `Category.distribution`, reset on world load. | `after-palette.png` |
| `88f0e6c` | Generator emits `block-armored-conveyor-*`, `block-plated-conduit-*`, `block-reinforced-conduit-*` icons (needs re-pack). | `after-player-status.png` (4th distribution icon) |
| `40dae2d` | HUD status table: player portrait in the health hexagon (`client/ui/widgets/player_status.gd`, `MindSimHost.player_unit_status()`). | `after-player-status.png` vs Java top-left |
| `c84dcda` | **WIP** for PAR-0152 (see §3.1): resolver `find`, block region chain, rotated block quads, construct-site drawing, `building_rotation`/`construct_draw` seams. Builds/runs; the placeholder is NOT gone yet. | `after-construct.png` |
| (pre-existing) | PAR-0055 sprite scale: `render/units.rs` already divides by `scale_marker` width (`sprite_scale`). Only re-verify. | — |

---

## 2. Remaining items at a glance

| § | Item | Size |
|---|---|---|
| 3.1 | PAR-0152 placed buildings draw "Wrong texture folder" | M (diagnose first) |
| 3.2 | PAR-0130 items on conveyors invisible | M |
| 3.3 | Ore/building/unit hover info + PAR-0094 block info panel (PlacementFragment top table) | L |
| 3.4 | PAR-0126 / PAR-0053 unit composition (shadow, legs, outline, cell, weapons) | L |
| 3.5 | PAR-0070 FX never produced + PAR-0128 FX regions drawn as flat rects | L |
| 3.6 | PAR-0097 F6 block status markers | S |
| 3.7 | PAR-0096 minimap attack indicators + minimap unit icons | M |
| 3.8 | PAR-0129 shields (force projector + unit shields) | M |
| 3.9 | PAR-0157 command-mode selection overlay | M |
| 3.10 | PAR-0095 Shift command panel | L |
| 3.11 | Small HUD/menu gaps found this session | S each |

---

## 3. Items

### 3.1 PAR-0152 — placed buildings render the "Wrong texture folder" placeholder

**Symptom.** Placing a conveyor, duo or mechanical drill (UI or `SimHost.place_block`) draws the atlas `error` region ("Wrong texture folder") at the block's footprint. Copper walls and the core render correctly. Renderer stats after placing conveyor×2 + duo + drill + copper wall: `dynamic_sprites: 5, missing_regions: 4`.

**Acceptance.** After `/tmp/s3_enter.sh` and the five `place_block` calls below, the screenshot shows real sprites: conveyors as grey belts (Java `conveyor-0-0` region, facing right for rotation 0), duo = grey base `block-1` + copper turret, drill = `mechanical-drill` + rotator + top; `get_stats().missing_regions == 0`.

```bash
python3 tools/gmcp.py --eval 'return get_node("/root/Spine/SimHost").place_block(126, 53, "conveyor")' \
  --eval 'return get_node("/root/Spine/SimHost").place_block(127, 53, "conveyor")' \
  --eval 'return get_node("/root/Spine/SimHost").place_block(126, 51, "copper-wall")' \
  --eval 'return get_node("/root/Spine/SimHost").place_block(130, 50, "mechanical-drill")' \
  --eval 'return get_node("/root/Spine/SimHost").place_block(132, 53, "duo")'
sleep 2; python3 tools/gmcp.py --screenshot runs/20261012-session3/par0152-after.png
```
Crop `(440,340)-(720,500)` to compare. Java reference for belt/turret look: `runs/20261009-065416-ref-ground-zero/` frames.

**Java reference.**
- `world/blocks/distribution/Conveyor.java:144-177` `ConveyorBuild.draw`: `Draw.z(Layer.block - 0.2f); Draw.rect(regions[blendbits][frame], x, y, tilesize*blendsclx, tilesize*blendscly, rotation*90)`; regions are `conveyor-<blendbits>-<frame>` (`conveyor-0-0` … `conveyor-4-3`).
- Turrets: `world/blocks/defense/turrets/Turret.java` → `DrawTurret` (`world/draw/DrawTurret.java`): base `block-<size>` (`block-1`) then the turret region rotated by `rotation - 90`.
- Drills: `world/blocks/production/Drill.java` `DrillBuild.draw`: `region`, then `rotatorRegion` rotated by `timeDrilled * rotateSpeed`, then `topRegion`.
- Generated full icons: `block-<name>-full` (= `getGeneratedIcons()` stacked).
- Construct sites: `world/blocks/ConstructBlock.java:251-276` (`ConstructBuild.draw`, blockbuild shader, `Draw.color(Pal.accent, Pal.remove, constructColor)`), shader `core/assets/shaders/blockbuild.frag`.

**Current Godot state (after WIP `c84dcda`).**
- `client/rust/mind-gdext/src/render/blocks.rs` `BlockRenderer::update`: for each tile from `mind_core::render::scan::visible_blocks` (only `cache_layer == Normal && draw_dynamic` blocks), resolves `block_region(def)` = `block-<name>-full` → `def.region` → `<name>-0-0` via `RegionResolver::find` (no error fallback), falls back to `resolver.resolve(&def.region)` (error fallback). Construct sites (`def.kind == BlockKind::ConstructBlock`) draw an accent ghost + bottom-up reveal using `MindSimHost::construct_draw`. Rotation via `units::build_unit_mesh`.
- The regions exist (`probe("block-duo-full")` → found, page 0, 32×32; `probe("conveyor-0-0")` found). So `missing_regions: 4` means the `def` the pass resolves for those tiles is **not** duo/conveyor/drill — most likely `build1`/`build2` (`ConstructBlock`, `client/rust/mind-core/src/content/registries/blocks/environment.rs:58`) whose `construct_draw` returned `None`, or a content-id mismatch between the grid and `host.content_registry()`.
- Checked and ruled out: a fresh `Blocks::new()` and `create_base_content()` give identical ids for duo/conveyor/copper-wall/mechanical-drill/core-shard.

**Ordered changes.**
1. **Diagnose (add a read-only probe).** In `client/rust/mind-gdext/src/sim_host.rs`, inside `#[godot_api] impl MindSimHost`, add:
   ```rust
   /// Debug probe: block name/kind at a tile and whether it has ConstructState.
   #[func]
   pub fn tile_debug(&self, x: i32, y: i32) -> VarDictionary {
       let mut d = VarDictionary::new();
       let tile = self.sim.grid.tile(x, y);
       let def = self.content_snapshot.as_ref().and_then(|c| c.block(tile.block));
       d.set("raw", tile.block.raw() as i64);
       d.set("name", &GString::from(def.map(|d| d.name.as_str()).unwrap_or("?")));
       d.set("kind", &GString::from(def.map(|d| format!("{:?}", d.kind)).unwrap_or_default().as_str()));
       d.set("sim_name", &GString::from(self.sim.block_name_of(tile.block).as_str()));
       d.set("has_build", tile.build.is_some());
       d.set("construct", self.construct_draw(x, y).is_some());
       d
   }
   ```
   Build, restart, launch, place the five blocks, then
   `python3 tools/gmcp.py --eval 'return str(get_node("/root/Spine/SimHost").tile_debug(132, 53))'` (and 126,53 / 130,50).
2. **Branch on the result:**
   - `name == "build1"`/`"build2"` and `construct == false`: the `ConstructState` is not on `tile.build`. Find where the live runtime inserts it: `client/rust/mind-core/src/world/harness.rs:268` (`.insert(ConstructState::construct(BlockId::AIR, block, cost))`) — read which entity it is inserted on and fix `construct_draw` (sim_host.rs, right above `building_team`) to read the same entity (e.g. via the multiblock centre tile: `world.tile(x,y)` vs the build's `BuildingComp.pos`). If `ConstructState.current` is AIR, use the block the plan targets (read `BuildingComp.block`). Re-verify; expect the accent ghost.
   - `name != sim_name`: content-id mismatch after `load_sector` (sim_host.rs ~1067-1122 takes/rebuilds the registry). Make the renderer use the same registry the sim uses; do not change sim logic — expose the right registry read-only.
   - `name == "duo"` etc.: then `block_region` is failing; add a unit test for `RegionResolver::find` key handling (`client/rust/mind-gdext/src/render/atlas_bind.rs`, `find` uses cache key `"\u{0}" + name`), and log the first miss with `log::warn!` once per name.
3. **Construct sites should finish building.** Java placement by the player creates a `ConstructBuild` that the player's builder unit completes. If sites stay at progress 0 forever with no builder nearby, that is gameplay (session "building/production" agent) — only render it; note it in the report.
4. **Blockbuild shader (proper look, after 1–2 work).** Replace the accent-ghost approximation with the port shader `client/shaders/blockbuild.gdshader` (already exists; uniforms mirror `blockbuild.frag`: `u_progress`, `u_time`, `u_uv`, `u_uv2`, `u_texsize`). Construct sites need per-quad uniforms, so draw each site as its own `MeshInstance2D` (pooled, `# code-instantiated: one node per construct site`) with a `ShaderMaterial` whose `u_uv/u_uv2` = the region UV rect, `u_progress` = progress, `u_time` = sim tick, modulate = `Pal.accent` (`0xffd37f`). Put these nodes under the `Layer.blockBuilding` (40) band: `self.band_node_at(BandKey::base(Layer::BlockBuilding))` in `render.rs` (`band_node_at`, ~line 403); pass the node into `BlockRenderer::new` like `block_band` (render.rs ~463).
5. **Sizes.** The pass currently forces every sprite to `size*8` world px square. Java draws `region.width * Draw.scl` (`Draw.scl = 1/scale_marker.width`, `scale_marker` is 4 px → 1/4). For `-full`/`-0-0` regions (32 px per tile, unstripped) `size*8` is correct; for stripped regions (e.g. `duo` is 26×28 with `offsets [x,y]`, `original [32,32]`) you must use `original` size and offsets. `MindAssets.region_geometry` returns only `[x,y,w,h,page]`; extend it (or add `region_meta`) to return offsets/original from the atlas manifest (`mind_core::assets::atlas::Region.offsets`) before drawing stripped regions at their true size.
6. Remove the `tile_debug` probe or keep it documented as a debug probe in `client/AGENTS.md`'s playtest notes.

**Tests.** mind-core: none needed for the probe. If you add a region-chain helper as a pure fn (e.g. `fn block_region_candidates(name, region, kind) -> Vec<String>` in `client/rust/mind-core/src/render/`), unit-test it: duo → `["block-duo-full","duo","duo-0-0"]`, conveyor → contains `conveyor-0-0`.

**Pitfalls.**
- `visible_blocks` skips `draw_dynamic == false` and non-`Normal` cache layers; walls go through `render/floor.rs` (`CacheLayerId::Walls`) and `drawCached` blocks through `render/building_cache.rs`, which still use plain `def.region` (`building_cache.rs:197`) — apply the same `block_region` chain there if a cached block shows the error sprite.
- `BlockRenderer::needs_rebuild` early-outs on camera key + `world_revision`; the WIP keeps rebuilding while construct sites are visible (`animating`). Conveyor animation (`frame = (int)(Time.time*speed*8*timeScale*efficiency) % 4`) also needs per-frame rebuilds — gate on "any animated block visible".
- The y-flip camera: `build_unit_mesh` maps the quad's world-bottom edge to `v1`. A rotation of `+90°` is CCW in world space. Java conveyor rotation 0 = facing +x.

**Status 2026-10-12:** done — commit `3a25763`. Diagnosis: the base `-full` chain resolved fine; the placeholder was the team overlay drawing `resolve`'s `error` fallback for every block without a `<name>-team` region, plus an `air` floor painting one permanent miss. Verified in Ground Zero: five `place_block` calls → `missing_regions: 0`, real belt/wall/drill/duo sprites. Step 4 (blockbuild shader for construct sites) stays open — Ground Zero placement is instant (`tile_debug.construct == false`), so sites are unreachable there; the WIP accent-ghost remains.

---

### 3.2 PAR-0130 — items on conveyors are invisible

**Symptom.** Belts carry items (core count rises) but no item icons move on them.
**Acceptance.** With a drill on copper ore feeding a belt into the core (or `SimHost.place_block` a belt line next to an item source), screenshots 0.5 s apart show copper icons (`item-copper` region, 5 world px = `Vars.itemSize`) moving along the belt, matching Java frames in `runs/20261009-065416-ref-ground-zero/`.
**Depends on** §3.1 (belts must render first).

**Java reference.** `world/blocks/distribution/Conveyor.java:162-177`:
```java
Draw.z(Layer.block - 0.1f);
for(int i = 0; i < len; i++){
    Item item = ids[i];
    Tmp.v1.trns(rotation * 90, tilesize, 0);
    Tmp.v2.trns(rotation * 90, -tilesize / 2f, xs[i] * tilesize / 2f);
    float ix = (x + Tmp.v1.x * ys[i] + Tmp.v2.x), iy = (y + Tmp.v1.y * ys[i] + Tmp.v2.y);
    Draw.rect(item.fullIcon, ix, iy, itemSize, itemSize);   // itemSize = 5f (Vars.java:115)
}
```
`item.fullIcon` region = `item-<name>` (e.g. `item-copper`, 30×30 in the atlas). Ducts: `Duct.java:172`, same `itemSize`.

**Godot state.** Sim state exists: `client/rust/mind-core/src/world/blocks/distribution/conveyor.rs:35` `ConveyorBuild { ids: [ItemId; 3], xs: [f32;3], ys: [f32;3], len: u8, ... }` (ECS `Component` on the building entity). Nothing reads it for drawing.

**Changes.**
1. `client/rust/mind-core/src/render/` add `conveyor_items.rs` (pure): 
   ```rust
   /// Conveyor.draw item placement: world-px centres for each belt item.
   pub fn belt_item_positions(x: f32, y: f32, rotation: u8, xs: &[f32], ys: &[f32]) -> Vec<(f32, f32)>
   ```
   Implement `trns(angle, a, b)` = rotate vector `(a, b)` by `angle` degrees; `ix = x + v1.x*ys[i] + v2.x` with `v1 = trns(rot*90, 8, 0)`, `v2 = trns(rot*90, -4, xs[i]*4)`. Register in `render/mod.rs`. Unit tests: rotation 0, `ys=0.5, xs=0` → `(x+4-4, y)` = `(x, y)`; rotation 1 rotates by 90°.
2. `client/rust/mind-gdext/src/sim_host.rs`: Rust-only seam
   `pub fn conveyor_items(&self, x: i32, y: i32) -> Option<(u8 /*rot*/, Vec<(ItemId, f32, f32)>)>` reading `BuildingComp.rot` and `ConveyorBuild` from `self.sim.ecs.0` for `self.sim.grid.tile(x,y).build`.
3. `client/rust/mind-gdext/src/render/blocks.rs` `update`: after pushing the belt quad for `BlockKind::Conveyor | ArmoredConveyor`, call `host.conveyor_items(tile.x, tile.y)`, compute positions with `belt_item_positions`, resolve `item-<name>` (item name via `content.item(id).name`) with `self.resolver.find`, push a `UnitQuad { w: 5.0, h: 5.0, rot: 0.0, .. }` per item. Items must draw above belts: keep them in a separate group map and append after belts (same page → put them in a second mesh node, or sort quads so items come after belts inside the page group).
4. Rebuild every frame while any conveyor with `len > 0` is visible (extend the `animating` flag).

**Verify.** `get_stats().dynamic_sprites` increases by the number of belt items; screenshot shows icons.
**Pitfalls.** Item ids → names need the content registry (`content.item(id)`); the band for items is `Layer.block - 0.1` which is the same `Layer::Block` band — order inside the mesh decides overlap.

**Status 2026-10-12:** done — commit `4c813b0`. `belt_item_positions` + the `conveyor_items` seam; item quads draw above the belt and the pass rebuilds while items are visible. Verified with a drill on `ore-copper` feeding a nine-belt line (`par0130-n1.png` vs `par0130-n3.png`). Duct/stack-conveyor items not done (their `current`/segment state is separate); belt frame animation (`regions[b][frame]`) still static frame 0.

---

### 3.3 Hover info (ore "Copper" popup) + PAR-0094 block info panel

**Symptom.** Java shows, above the block palette, a top table: hovering ore shows `[copper icon] Copper`; selecting a block shows `[icon] Conveyor [hotkey]`, a `?` button, and requirement rows `[icon] Copper  0/1`. Godot shows nothing above the palette.
**Acceptance.** Compare against `runs/20261012-overview/java-groundzero-ingame.png` (top of the palette at x≈840-1150, y≈345-395: "Copper" with icon while hovering ore). Selecting conveyor shows name, `[1,1]`-style key combo, `?` and "Copper 0/1" (scarlet when lacking).

**Java reference.** `ui/fragments/PlacementFragment.java`:
- `:345-441` top table (`Tex.buttonEdge2`), refresh logic, block header (`Image(displayBlock.uiIcon)` 32 px, `labelWrap` width 190, `?` button `Styles.flatBordert` size 40 → `ui.content.show(block)`), requirements rows (16 px icon, `Color.lightGray` name, amount label colours `[scarlet]` if `< amount/2`, `[accent]` if `< amount`, else `[white]`; text `"<have>[white]/<need>"`, `"*/<need>"` with infinite resources), unplaceable reason collapser (`Icon.cancel` scarlet).
- `:784-816` `hasInfoBox()` / `hovered()`: hovered unit (`Units.closestOverlap(..., 5f, ...)`) → building (`displayable`, not in fog) → tile with a drop.
- `world/Tile.java:721-733` `Tile.display`: shows `block` if it has `itemDrop`, else `overlay` (ore) if it drops or `wallDrop()`, else `floor`; `getDisplayIcon` / `getDisplayName` (an ore displays its item icon and the item's localized name, e.g. "Copper").
- `frame.image().color(Pal.gray).height(4)` separator under the top table (`:441`).

**Godot state.** `client/scenes/ui/fragments/placement_fragment.tscn` has `Panel/Layout/{Left/{Scroll/Blocks, Divider, InputBar}, Categories}` — no top table. `client/ui/fragments/placement_fragment.gd` builds categories and blocks. Camera has `#[func] screen_to_tile(x, y) -> Vector2i` (`client/rust/mind-gdext/src/camera.rs:279`). The selected block is `SimHost.selected_block()`; `MindUi.block_catalog_json()` gives categories/blocks with `name`/`localized`.

**Changes.**
1. `client/rust/mind-gdext/src/sim_host.rs`: `#[func] pub fn hover_info(&self, x: i32, y: i32) -> VarDictionary` returning `{kind: "unit"|"building"|"tile"|"", name, icon_region, team}` implementing `hovered()`'s order (unit within 5 world px using `unit_draws()` positions; building at tile; tile drop: if `block.item_drop` → block, elif `overlay.item_drop` → overlay, elif floor `item_drop` → floor; icon = `item-<drop>-ui` for ores (OreBlock display icon is the item), name = localized item name via the bundle key `item.<name>.name`).
2. `#[func] pub fn block_requirements(&self, name: GString) -> Array<VarDictionary>` → `[{item, amount, have}]` using `def.requirements` × `rules.build_cost_multiplier` and the player core's items (there is a core-items read path used by `CoreItemsDisplay`; reuse it).
3. `placement_fragment.tscn`: add, as the FIRST child of `Panel/Layout/Left`, a `PanelContainer` `TopTable` (stylebox: atlas ninepatch region `button-edge-2` = `Tex.buttonEdge2`; it exists in the atlas) containing `VBoxContainer` `Rows`, followed by a 4 px `ColorRect` `TopDivider` (`Pal.gray` = `#454545`). Visibility = `hasInfoBox()`.
4. `placement_fragment.gd`: in `_process`, every frame:
   - `display_block = SimHost.selected_block()` (empty → none);
   - else compute hover: if `get_viewport().gui_get_hovered_control()` is non-null → none; else `tile = Camera2D.screen_to_tile(mouse)` → `SimHost.hover_info(tile.x, tile.y)`;
   - rebuild `Rows` only when `(display_block, hover name)` changed (Java caches `lastDisplayState`). Header row = `TextureRect` 32×32 (`MindWidgets.assets().find_region("block-<name>-ui")`) + label (width 190, autowrap) + `?` button (40×40, opens the database content dialog: `MindUi.open_dialog("content_info", "{\"block\":\"<name>\"}")` — check the dialog name in `client/ui/dialogs_manifest.json`).
   - requirement rows refresh their amount labels every frame with the colour rules above (use BBCode in a `RichTextLabel`).
   - hover-only rows: icon 32 px + name label.
5. Key combo text: bundle key `placement.blockselectkeys` (`assets/bundles/bundle.properties`), using `MindInput.keybinds_json()` for `block_select_*` binds.

**Tests.** mind-core pure helper for the requirement colour: `fn requirement_color(have: i32, need: i32) -> &'static str` (`"scarlet"|"accent"|"white"`), unit-tested at boundaries (`have < need/2`, `have < need`, `>=`).
**Verify.** Hover ore: `godot_input mouse_motion` to the screen point of an ore tile (`Camera2D.tile_to_screen(x,y)` → viewport coords), screenshot; select conveyor via `get_node("/root/Spine/Input").select_block_by_name("conveyor")`, screenshot.

**Status 2026-10-12:** done — commit `9e450a1`. `hover_info` + `block_requirements_json` seams, `requirement_color` (+tests), `BlockCategoryView.ordinal`, `TopTable`/`TopDivider` scene nodes and the fragment's per-frame top table. Verified: conveyor shows "Key: [3,1]" + "Copper 0/1" scarlet (`par0094-d-selected.png`); ore hover shows the copper icon + "Copper" (`par0094-c4-hover.png`). Notes: the `[accent]/[white]` colours and infinite-resources `*/n` path are not reachable in Ground Zero (fresh core); the `?` opens the `content` dialog; unit-hover path untested in-engine (no enemy unit in range). The top table reuses `mouse_filter == STOP` (not raw `gui_get_hovered_control != null`) because the HUD fragment root is a full-rect PASS control.
**Pitfalls.** `gui_get_hovered_control` must be read from the root viewport (`get_tree().root.gui_get_hovered_control()`); the top table itself must not count as "UI has mouse" when the cursor is over the world. The fragment root has `mouse_filter` ignore regions — do not block world clicks.

---

### 3.4 PAR-0126 / PAR-0053 — units are a single flat body sprite

**Symptom.** Units (player alpha, daggers) render as one rotated body quad: no shadow, no mech legs/walk, no outline, no team cell, no weapons.
**Acceptance.** Player alpha in Ground Zero shows the yellow-orange team cell, side legs animating while moving (WASD via `godot_input` key events), a soft shadow under it; a dagger (spawn with the wave or `MindFx`/sim spawn API) shows legs, two `large-weapon` barrels and the red cell. Compare with `runs/20261010-234250-ref-PAR-0070-combat-fx/` Java frames.

**Java reference.** `type/UnitType.java`:
- `:1471-1560` `draw(Unit)` order: shadow (flying/shadowElevation, `Layer` z-1) → `drawMech` (legs) → `drawTank`/`drawLegs` → `drawPayload` → `drawSoftShadow` → `drawOutline` → `drawWeaponOutlines` → engines/trail → `drawBody` → `drawCell` → `drawWeapons` → `drawItems` → `drawLight` → `drawShield` → parts.
- `:1652-1664` `drawShadow`: `Pal.shadow` tint, `shadowRegion = fullIcon`, offset `shadowTX/TY * elevation`.
- `:1674-1680` `drawSoftShadow`: `circle-shadow` region, colour `(0,0,0,0.4)`, size `max(region.w,h) * region.scl() * softShadowScl * 1.6`.
- `:1763-1785` `drawOutline` (`<name>-outline`), `drawBody` (`region`, rotation `unit.rotation - 90`).
- `:1787-1797` `drawCell`: region `<name>-cell` (fallback `power-cell`), colour `black.lerp(team.color, f + absin(time, max(f*5,1), 1-f))`, `f = healthf`.
- `:1939-1970` `drawMech`: `legRegion` (`<name>-leg`) drawn twice (`i = ±1`), offset `trns(baseRotation, extension*i - boostTrns, -boostTrns*i)`, width × `i` (mirrored), height × `(1 - max(-sin*i,0)*0.5)`, rotation `baseRotation - 90 + 35*i*e`.
- Regions loaded at `:1154-1184` (`-leg`, `-joint`, `-foot`, `-treads`, `-base`, `-cell`, `-outline`).
- Weapons: `type/Weapon.java` `draw`/`drawOutline` (`<weapon>` and `<weapon>-outline` regions, mirrored for `mirror` weapons, positioned `x,y` relative and rotated by mount rotation).

**Atlas reality (checked).** Present: `alpha` 46×46, `alpha-cell` 12×38, `dagger` 48×32, `dagger-leg` 40×36, `dagger-base` 26×24, `large-weapon` + `large-weapon-outline`, `small-basic-weapon`, `circle-shadow` 201×201, `power-cell`, `scale_marker` 4×4. **Missing:** `alpha-outline`, `dagger-outline`, `dagger-cell`, `alpha-leg` (Java generates `-outline` regions at pack time in `tools/.../Generators`/`UnitType.createIcons`; check the Java raw sprites dir `core/assets-raw/sprites/units/` for which exist as files).

**Godot state.** `client/rust/mind-gdext/src/render/units.rs`: `UnitRenderer::update` draws one `UnitQuad` per unit from `MindSimHost::unit_draws()` (`sim_host.rs` `unit_draws`, returns `(x, y, rotation, name, flying, low_altitude)`), region = unit name, size = region px × `sprite_scale` (1/4). Mech state lives in `client/rust/mind-core/src/entities/comp/unit/comp.rs` (`MechComp` line 120, `LegsComp` 175, `TankComp` 195); health in `entities/comp/health.rs`; team in `TeamComp`.

**Changes (ordered, each its own commit).**
1. Extend `unit_draws` into a struct (Rust-only): in `sim_host.rs` add `pub struct UnitDraw { x, y, rotation, name: String, flying, low_altitude, team: u8, healthf: f32, elevation: f32, mech: Option<(f32 /*base_rotation*/, f32 /*walk_extension*/)>, weapons: Vec<(String, f32 /*x*/, f32 /*y*/, f32 /*rotation*/, bool /*mirror flip*/)> }` and `pub fn unit_draws_full(&self) -> Vec<UnitDraw>`; keep the old fn for callers. Read `MechComp`'s fields (open `comp.rs:120` to get the names) and weapon mounts (`entities/comp/unit/weapon_mount.rs`).
2. `units.rs`: per unit push, in this order, into the same page bucket (quad order = draw order):
   1. soft shadow: `circle-shadow`, colour `0x00000066`, size `max(w,h)*scale*1.6` (softShadowScl default 1).
   2. mech legs (if `mech`): two `<name>-leg` quads per Java `drawMech` (mirror by negative width → swap `u0/u1`).
   3. outline `<name>-outline` if present (`resolver.find`).
   4. body `<name>`.
   5. cell `<name>-cell` else `power-cell`, colour = `lerp(black, team colour, f + absin(...))` (team colours: `mind_core::game::team::Team::get(team).color`).
   6. weapons: `<weapon>` regions at mount offsets rotated with the unit (`rotation - 90`), mirrored for the left mount.
   All rotations are `unit.rotation - 90` (Java sprites face up).
3. Packer: generate `-outline` regions for units/weapons (port of Java `Generators` unit outline: `Pixmaps.outline(region, outlineColor, outlineRadius)`; the port already has outline code for blocks in `client/rust/mind-tools/src/generate/blocks.rs` — reuse it for units in `client/rust/mind-tools/src/generate/` (look for a `units.rs`); then re-pack (§0.1).
4. Ground shadow for flying units (`drawShadow`): `fullIcon` tinted `Pal.shadow` (`new Color(0,0,0,0.22)` → `0x00000038`) offset `(shadowTX, shadowTY) * e` with the static constants `shadowTX = -12, shadowTY = -13` (`UnitType.java:44`), `e = clamp(elevation, shadowElevation, 1) * shadowElevationScl`.

**Tests.** mind-core pure helpers in `client/rust/mind-core/src/render/unit_draw.rs`: `mech_leg_transforms(base_rot, extension, sin, elevation) -> [(dx, dy, sx, sy, rot); 2]` and `cell_color(team_rgba, healthf, time) -> u32`, with tests at `extension = 0` (legs at the centre) and `healthf = 1` (cell = team colour).
**Verify.** `get_stats().unit_sprites` rises from 1 to ~5 per unit; screenshot the player after `godot_input` key `W` held.

**Status 2026-10-12:** done (partial evidence) — commit `8ce2941`. `unit_draw.rs` helpers + tests, `unit_draws_full`/`UnitDraw`/`WeaponDraw`, and the composed pass (soft shadow, legs, body, cell, weapons) in `units.rs`. Alpha verified in-engine (body + team cell + soft shadow, `par0126-offcore.png`); the alpha has no `<name>-leg` region in vanilla (`assets-raw/sprites/units` has only `alpha.png`/`alpha-cell.png`), so "alpha legs" in the acceptance is upstream-inaccurate. Dagger legs/weapons not framed in-engine: wave enemies spawn ~175 tiles away, are inert (units/AI gap owned elsewhere), and the campaign camera re-centers on the player each frame, so `center_on_tile`/`spectate` do not stick. Step 3 (packer `-outline` generation + repack) is skipped: no unit/weapon `-outline` regions ship and the acceptance does not gate on outlines. `unit_debug_positions` kept as a debug probe.
**Pitfalls.** The atlas is the 4× sprite set — always multiply region px by `sprite_scale` (already computed from `scale_marker`). Stripped regions need the offsets fix described in §3.1 step 5 (bodies are stripped: `dagger` 48×32 is not square). The units band nodes are per (layer bucket, page) — legs/body/weapons of one unit can be on different pages; order across pages is then by node order, so keep a single page where possible (all unit sprites are on page 0 today).

---

### 3.5 PAR-0070 / PAR-0128 — no impacts/explosions/smoke/shake; FX regions drawn as flat rects

**Symptom.** Combat shows no hit sparks, explosions, smoke, wreck/spawn effects or screen shake; when an effect does render (dev probe `MindFx.spawn_effect`) region sprites are solid rectangles.
**Acceptance.** `python3 tools/gmcp.py --eval 'return get_node("/root/Spine/MindFx").spawn_effect("hitBulletSmall", 1032, 440, 0)'` then a screenshot within 0.2 s shows the Java-like spark sprites (not filled rects); during a wave, duo fire produces `shootSmall` muzzle flashes and hits produce `hitBulletSmall`; unit death produces an explosion and the camera shakes. `MindFx.live_effect_count()` > 0 during combat.

**Java reference.** `entities/Effect.java` (`Effect.create`, `Effect.shake`), `content/Fx.java` (effect bodies), `core/Renderer.java:403-414` (effect/shield buffers), `graphics/Layer.java` (`effect = 110`, `bullet = 100`).

**Godot state.**
- Producer missing: `client/rust/mind-core/src/sim/unit_runtime.rs:97` `pub fx: FxHandle` initialised to `noop_fx()` at `:135`; `FxHandle = Arc<dyn FxSink>` (`mind-core/src/combat/view.rs:84`). Nothing replaces it, so every sim `fx.effect/shake` call is dropped.
- Consumer: `client/rust/mind-gdext/src/fx.rs` `MindFx` owns `bus: FxBus` (by value, `fx.rs:47`), `tick_view` drains it (`:427`), `draw` executes the program; `draw_prim` (`:136`) draws `PrimKind::Region` as `draw_rect_ex` filled (`:273-283`) and ignores `NoiseLayer/ShaderBlit/Polyline` (`:285-287`).

**Changes.**
1. Share the bus: change `MindFx.bus` to `Arc<FxBus>` (`FxBus` is `Sync` — it wraps a `Mutex`, `mind-core/src/fx/sink.rs:133`), add `pub fn bus_handle(&self) -> Arc<FxBus>`.
2. In `sim_host.rs`, wherever a fresh sim/unit runtime is created (search `install_sim_seams` ~line 297 and the `load_sector` path), resolve `/root/Spine/MindFx` and set `if let Some(mut rt) = self.sim.unit_runtime_mut() { rt.fx = bus.clone() as FxHandle; }` (`unit_runtime_mut` at `mind-core/src/sim/unit_runtime.rs:321`). This is wiring, not sim logic. Also check `mind-core/src/combat/harness.rs:810` (`self.fx = fx`) for the combat harness path.
3. Region execution in `fx.rs` `draw_prim`: resolve `region.0` through `MindAssets` (`find_region(name) -> AtlasTexture`, `assets/atlas.rs:225`) cached in a `HashMap<&'static str, Gd<Texture2D>>`, then `base.draw_set_transform(Vector2(x,y), rotation_deg.to_radians(), Vector2::ONE)` + `draw_texture_rect(tex, Rect2(-w*origin.0, -h*origin.1, w, h), false, modulate=color)` + reset transform. `mix` (mixcol) → use a `CanvasItemMaterial`-free approximation: lerp colour toward mix colour. Units: the FX program is in world px; region sizes from the program are already world units.
4. Shake: on `FxEvent::Shake` drained in `tick_view`, call the camera `apply_shake_event(intensity, duration)` (`camera.rs:409`), honouring the `screenshake` setting (`settings_get_i32("screenshake")`).
5. `Polyline` → `draw_polyline_ex`; `NoiseLayer/ShaderBlit` stay with plan 16 (weather) — note in the commit.

**Tests.** mind-core: a test that a `UnitRuntime` with an installed `FxBus` records an event when a bullet hits (find an existing combat test that asserts on `NoopFx` and mirror it with a recording sink).
**Verify.** `--eval 'return get_node("/root/Spine/MindFx").live_effect_count()'` during a wave (`MindCampaign` skip-wave button or wait), screenshots.
**Pitfalls.** `MindFx` is a sibling node; `sim_host` must re-install the handle after every sim replacement (load_sector creates a new sim). Do not hold a `bind_mut` of MindFx while the sim steps.

---

### 3.6 PAR-0097 — F6 block status markers

**Symptom.** F6 does nothing; Java toggles small diamonds at the bottom-right corner of every consuming building (green active, orange noOutput, red noInput, purple logic-disabled, grey inactive).
**Acceptance.** After F6 (or `MindUi.settings_set("blockstatus", true)`), a drill on ore shows a green diamond at its bottom-right corner, a drill off ore shows red; F6 again hides them.

**Java reference.** `input/DesktopInput.java:455-457` (F6 → toggle `blockstatus`), `core/Renderer.java:170` (`drawStatus = settings.getBool("blockstatus")`), `graphics/BlockRenderer.java:825-833` (draw for `tileWithConsumerView` if `Lod.l2`), `entities/comp/BuildingComp.java:1221-1234` `drawStatus` (corner `brcx = x + size*4 - 8*m/2`, `brcy = y - size*4 + 8*m/2`, `m = size>1 ? 1 : 0.64`; `Pal.gray` square radius `2.5m` rotated 45, then `status().color` square `1.5m`; `Layer.power + 1` = 71), `:638-652` `status()`, `world/meta/BlockStatus.java` colours: active `5ce677`, noOutput `Color.orange` (`ffa500`), noInput `Pal.remove` (`e55454`, `Pal.java:86`), logicDisable `8a73c6`, inactive `Color.lightGray` (`bfbfbf`); `Pal.gray` = `454545`.

**Godot state.** Setting row exists (`mind-core/src/ui/settings.rs:231` `blockstatus`), binding exists (`mind-core/src/input/binding.rs:763` `toggle_block_status`) but no handler. `mind-core/src/world/status.rs` has `BlockStatus` with **wrong colours** (`color()` returns white/`ff1d1d`/`ffd37f`) and no `logicDisable/inactive` — fix there is fine (presentation constant), but ask/announce since session "building" may own it; add a separate `java_color()` if unsure.

**Changes.**
1. Input: where `MindInput` maps bindings to actions (`client/rust/mind-gdext/src/input/mod.rs`, effect emission ~line 495 `emit_signal("hotkey", …)`), handle `toggle_block_status` by flipping the `blockstatus` setting (through `MindUi.settings_set`) — or emit `hotkey("toggle_block_status")` and handle it in `client/ui/fragments/hud_fragment.gd` (`_on_hotkey` pattern exists in `placement_fragment.gd:129`).
2. `sim_host.rs`: `pub fn building_statuses(&self, view_rect) -> Vec<(x, y, size, status)>` for player-team buildings with consumers (`def.consumes` non-empty), status per Java `status()` (needs `enabled`, `shouldConsume`, `efficiency` — expose what the sim has; if efficiency is not available, use `world/status.rs::block_status`).
3. New render pass `client/rust/mind-gdext/src/render/status.rs` drawing two diamond quads per building (use `draw_colored_polygon` on a `Node2D` under the `Layer::Power` band, or white-region quads tinted) when `settings_get_bool("blockstatus")`.

**Tests.** Pure `status_corner(x, y, size) -> (f32, f32, f32 /*m*/)` with size 1 and 2 cases.
**Pitfalls.** Lod: Java hides markers when zoomed out (`Lod.l2`); the port has `lod_l2` in stats.

---

### 3.7 PAR-0096 — minimap attack indicators (+ unit icons on the minimap)

**Symptom.** Damaged player buildings never flash on the minimap; units are not drawn on the minimap either (Java draws team-tinted unit icons).
**Acceptance.** Force damage to a player building (sandbox: enemy unit, or a debug damage call if one exists) → a pulsing orange→scarlet square appears on the HUD minimap at that tile for 15 s. Units appear as small team-coloured icons on the minimap (Java frame shows the player's alpha in the minimap centre).

**Java reference.** `game/AttackIndicators.java` (duration `15*60` ticks, `add(x,y)`, `update()`), `core/Control.java:68-72` (`BuildDamageEvent` for the player team → `indicators.add`), `graphics/MinimapRenderer.java:253-275` (pulse: `fin = (globalTime/30) % 1`, `rad = fin*5 + tilesize - 2`, stroke `(1-fin)*4 + 0.5`, colour `orange.lerp(scarlet, clamp(time/70))`, `Lines.square((ix+0.5+offset)*8, …, rad)`), units: `MinimapRenderer.drawEntities` (`:157-165`: `unit.icon()` with `Draw.mixcol(team.color, 1)`, size `scale`, rotation `rotation - 90`).

**Godot state.** Widget `client/ui/widgets/mind_minimap.gd` (`_draw_cropped` draws the crop of `MinimapProvider.get_texture()` using `region()`). Provider `client/rust/mind-gdext/src/render/minimap.rs`.

**Changes.**
1. Sim/UI-side store: `client/rust/mind-gdext/src/render/minimap.rs` (provider) gets `indicators: Vec<(i32 pos, f32 time)>` with `add_indicator(x, y)` and age-out in `process`. Feed: the sim's building-damage event (search `BuildDamageEvent` in `mind-core/src/game/events*`; drain via `MindSimHost` events into the provider). If no damage event exists in the sim, add a read-only event emission request to the combat owner and stub with a debug `#[func] add_indicator`.
2. `#[func] indicators_json()` → `[[x, y, time], …]`; `#[func] minimap_units_json()` → `[[x, y, rot, name, team], …]` from `unit_draws_full`.
3. `mind_minimap.gd` `_draw_cropped`: after the texture, convert tile→widget coords with `(tile - region.position) * scale` (remember the texture is top-row-first: widget y = `(height-1-tile_y) - region.position.y`), draw `draw_rect(rect, color, false, stroke)` per indicator with the pulse formula; draw unit icons (`find_region("unit-<name>-ui")` modulated with team colour).

**Pitfalls.** Texture row 0 = map top (`height-1`); the crop `region()` is in texture pixels.

---

### 3.8 PAR-0129 — shields invisible

**Symptom.** Force-projector domes and unit shield bubbles never render.
**Acceptance.** Place a `force-projector` with power in a sandbox/custom game: a team-coloured hexagon dome (radius `realRadius()`) with the shield shader look appears; a unit with a `ForceFieldAbility` shows its bubble.

**Java reference.** `world/blocks/defense/ForceProjector.java:292-330` (`drawShield`: `Draw.color(team.color, white, clamp(hit))`, `Layer.shields`, `Fill.poly(x, y, sides, radius, shieldRotation)` when `animateShields`, else alpha `0.09 + 0.08*hit` fill + `Lines.poly` stroke 1.5), `type/UnitType.java:1643-1650` (`drawShield`: `Fill.light` radius `hitSize*1.3`, colour `shieldColor.lerp(white, hitTime/2)` alpha `0.7*shieldAlpha`), `core/Renderer.java:403-414` (shield layer rendered to `effectBuffer` and blitted with `Shaders.shield`), `Layer.shields = 125`.

**Godot state.** `client/shaders/shield.gdshader` exists; `MindWorldRenderer.shield_available()` (`render.rs:738`) only reports it. No pass.

**Changes.**
1. Read-only seams: `sim_host.rs` `pub fn force_shields(&self) -> Vec<(x, y, radius, sides, rotation, team, hit)>` (ForceProjector build state — find its component under `mind-core/src/world/blocks/defense/`), `unit_shields()` from `ShieldComp` (`entities/comp/unit/comp.rs:328`).
2. New `client/rust/mind-gdext/src/render/shields.rs`: draw filled polygons into a `SubViewport`-free approach: a `Node2D` under the `Layer::Shields` band with a `CanvasGroup` parent whose material is `shield.gdshader` (CanvasGroup renders children to a buffer then applies the material — the Godot analogue of `effectBuffer.blit(Shaders.shield)`). Children draw `draw_colored_polygon(hexagon, team colour)`.
3. Fallback when the shader is missing: Java's non-animated path (alpha fill + outline).

**Pitfalls.** CanvasGroup margins must cover dome radius; update every frame (domes animate radius on build-up).

---

### 3.9 PAR-0157 — command-mode selection overlay

**Symptom.** In command mode (Shift held / `MindInput.toggle_command_mode()`), selected units get no hexagon ring, no target line or move square.
**Acceptance.** Select the player's units in command mode (or set selection via `MindInput.set_selectable_units_json` + selection path): each selected unit shows an accent (`ffd37f`) hexagon ring of radius `hitSize/unitSelectRadScl + 1` with inner light fill alpha 0.7→0, and a line + 3.5 square to its move target.

**Java reference.** `input/InputHandler.java:1211-1300` `drawCommanded`/`drawCommandedTargets` (`sides = 6`, `alpha = 0.5`, `lineLimit = 6.5`, `Fill.lightInner(x, y, 6, rad*0.8, rad, 0, color a0, color a0.7)`, `Lines.poly(x, y, 6, rad + 0.5)` stroke 1, `Drawf.limitLine`, `Drawf.square(dest, 3.5)`, `Drawf.target(attackTarget, 6, Pal.remove)`; layers `Layer.plans` (flying) / `Layer.groundUnit - 1` (ground) / `Layer.overlayUI`). `unitSelectRadScl = 1f` (`InputHandler.java:56`).

**Godot state.** `client/rust/mind-gdext/src/input/mod.rs`: `command_mode()` (`:792`), `selection_json()` (`:804`, `selected_units` ids), `selection_changed` signal. No draw.

**Changes.** Add `client/rust/mind-gdext/src/render/command_overlay.rs` (or a GDScript `Node2D` under `World` — Rust preferred because it needs unit positions): each frame if `MindInput.command_mode()`, for each selected id get the unit position/hitSize/target from a new read-only `sim_host` seam `unit_command_view(ids) -> Vec<(x, y, hit_size, flying, target: Option<(x,y)>, attack: Option<(x,y)>)>`, then draw with `draw_colored_polygon` (fill), `draw_polyline` (ring), `draw_line` (target line, alpha 0.5), `draw_rect` (square). World→screen: draw in a world-space `Node2D` under the band (`Layer::Plans` / `Layer::GroundUnit`), like `render/overlays.rs`.
**Depends on** the units/AI agent feeding selection (PAR-0140); verify with a hand-set selection.

---

### 3.10 PAR-0095 — Shift command panel

**Symptom.** Command mode keeps the block palette; Java swaps it for a command table: title "Command Mode", accent rule, unit-type icon buttons with counts, command buttons (`Icon` per `UnitCommand`), stance buttons; "No units" text when nothing selected.
**Acceptance.** Enter command mode with no selection → the palette area shows `@commandmode.name` title + `@commandmode.nounits`; with units selected → one button per unit type (icon + count) and the shared command buttons (50×50, `Styles.clearNoneTogglei`).

**Java reference.** `ui/fragments/PlacementFragment.java:443-676` (`commandTable = new Table(Tex.pane2)`, swap in `mainStack` when `control.input.commandMode` changes `:447-456`, contents `:471-670`, command buttons `:571-590` call `Call.setUnitCommand`, keybinds `:663-668`).

**Godot state.** `placement_fragment.gd` has no command branch; `MindInput.command_mode()`, `selection_json()`, `set_unit_stance(stance, enabled)` (`input/mod.rs:822`) exist; there is no `set_unit_command` func — add a `#[func]` that forwards to the sim's command path (owned by the units/AI agent; coordinate).

**Changes.**
1. `placement_fragment.tscn`: add sibling `CommandPanel` (PanelContainer, ninepatch region `pane-2` = `Tex.pane2`; exists in the atlas), hidden by default, same rect as `Panel/Layout/Left`.
2. `placement_fragment.gd` `_process`: poll `Input.command_mode()`; on change toggle `Left`/`CommandPanel` visibility. Rebuild the panel on `selection_changed` signal: group selected units by type (`selection_json` → ids; need a `unit_types_for(ids)` read seam), create buttons (`# code-instantiated: one button per selected unit type`).
3. Bundle keys: `commandmode.name` (`[accent]Command Mode`) and `commandmode.nounits` (`[no units]`) exist in `assets/bundles/bundle.properties:455-456`.

---

### 3.11 Small gaps found this session

Each is one small commit.

1. **Main menu button labels centred** (`runs/20261012-overview/java-menu.png`: "Play", "Database"… are centred in a 170 px column right of the icon; Godot left-aligns them next to the icon, `runs/20261012-session3/after-font-menu.png`). Java: `ui/fragments/MenuFragment.java` `buttons(...)` (`Styles.flatToggleMenut`, label `.growX()` centred). Fix `client/ui/fragments/menu_fragment.gd` (`_make_button` or equivalent, ~line 230 where `_icon_font` is applied): set the label's `horizontal_alignment = CENTER` and make it expand (`size_flags_horizontal = EXPAND_FILL`). Also the Discord banner bottom-right: Java draws the Discord icon in a slanted pane (`discordBanner`), Godot shows a plain blue shape.
2. **Planet page** (`runs/20261012-session3/after-font-planet.png` vs `runs/20261012-overview/java-planet-groundzero.png`): text is fixed. Remaining rendering gaps: sector title shows `serpulo-sector-15` instead of the preset name "Ground Zero" (campaign agent likely — check `client/ui/dialogs/planet_dialog.gd` title source: it should use the preset bundle name `sector.groundZero.name`); no sector hex grid lines, lock icons, `[ Ground Zero ]` label or attack markers over the globe (Java `ui/dialogs/PlanetDialog.java` `renderSectors`/`drawSelection`/labels); no "Sectors / N under attack" top-right panel; info pane lacks "Resources:" icons and "Stats" row; button reads "Resume" (Java "Go" for a not-yet-landed sector — verify state).
3. **Status table**: Java's `wavepane` box hugs the right edge with a slanted corner and shows the skip-wave button inside the table (`HudFragment.java:491-510`, right button `buttonRight` style, width 40). Godot shows `WaveButtons` below. Move the skip button into `StatusRow/Row` as the last child when `canSkipWave` (see `hud_fragment.gd` `_build_wave_buttons`).
4. **Hint placement** ("Use [WASD] to move." at y≈300 left): Java `ui/fragments/HintsFragment.java` places hints top-left under the status table (`t.top().left()` with `Tex.buttonSideRight`?). Read `HintsFragment.build` and align `client/scenes/ui/fragments/hints_fragment.tscn`.
5. **Minimap scope**: Java's Ground Zero minimap shows only the play area (small island, black elsewhere) — the port shows the whole 256×256 map. Likely `rules.limitMapArea` darkness (`world/darkness` / `Tile.inMapArea`) not applied by the sim or the minimap `get_static_darkness`; check `state.rules.limitMapArea/limitX/limitY/limitWidth/limitHeight` in the loaded rules and darken tiles outside it in `render/minimap.rs` `tile_color` (Java `MinimapRenderer.colorFor` → `world.getDarkness`). Also verify the in-world darkness at the area border.
6. **`BlockStatus` colours** in `client/rust/mind-core/src/world/status.rs:26-33` do not match Java (see §3.6).
7. **Stripped-region offsets** (§3.1 step 5) affect every pass that sizes quads from `region w/h`: units, blocks, FX.

---

## 4. Files touched by session 3 that other agents may also touch

- `client/rust/mind-gdext/src/sim_host.rs` (added read-only `player_unit_status`, `building_rotation`, `construct_draw`).
- `client/rust/mind-gdext/src/ui/ui_host.rs` (`settings_get_bool`).
- `client/rust/mind-gdext/src/campaign.rs` (`localized_marker_text` keeps markup; removed `strip_arc_markup`).
- `client/rust/mind-core/src/io/map/preview.rs` (`apply_block_colors`, `team_color` palette).
- `client/rust/mind-tools/src/generate/metadata.rs` (armored conveyor/conduit icons) + `assets/parity/region_names.txt`.
- `client/scenes/ui/fragments/hud_fragment.tscn`, `client/ui/fragments/hud_fragment.gd`, `placement_fragment.gd`.
- Other checkouts need `bash tools/pack.sh pack` to pick up the generator change and to have assets at all (session worktrees start without the gitignored pack outputs).
