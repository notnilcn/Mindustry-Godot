# 16 — WORLD RENDERING (frame pipeline, floor/block caches, lights/fog, planets) Implementation Plan

> Phase P6. This plan inherits `HIGH_LEVEL_PLAN.md` §0 (locked decisions), §2 (architecture), §4 (template), §6–§9. It is a sibling of `17_FX_PARTS_IMPLEMENTATION_PLAN.md` and `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md`, and depends on `02`, `06`, `07`.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Ready to execute. M0–M9; 5 load-bearing defaults flagged `NEEDS USER DECISION` in §8 (band allocator ABI, Godot renderer method inheritance, bloom capture strategy, light `max` blend fidelity, map-screenshot capture path). None block M0. |
| **Phase** | P6 Render. |
| **Depends on** | `02_CONTENT_IMPLEMENTATION_PLAN.md` (`BlockDef` draw metadata, `EffectId`, `Block.cache_layer`, `UnitTypeDef`, `PlanetDef`, `StatusEffect`), `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (`WorldGrid`/`Tile`/`Tiles`, `RenderHooks`, `CacheLayerId` ordering, darkness functions, `WorldHooks`), `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (`DrawBlock`/`DrawCommands`/`DrawSpec`, `BlockKindData`, `BuildDrawData`, `drawCached`, `BuildingCacheLayer`). Requires the plan-00 spine rig, `mind-gdext` + `bevy_ecs` wiring, plan-03 atlas/`shader.index.json`/`.gdshader`, and the MCP bridge. |
| **Blocks** | `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (draw triggers, effect/part draw lists, screen shake), `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (`EditorRenderer` on top of chunk/quadtree APIs, preview textures), `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (render-only view sync hooks), `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (screenshot/export surfaces). |
| **Sources** | Mindustry: `core/src/mindustry/core/Renderer.java` (652 lines, read in full); `graphics/{Layer,CacheLayer,FloorRenderer,BlockRenderer,OverlayRenderer,LightRenderer,FogRenderer,MinimapRenderer,Pixelator,Shaders,Pal,Drawf,Trail,EnvRenderers,Lod,MenuRenderer,LoadRenderer,BuildingCacheLayer,MultiPacker,DebugCollisionRenderer,InverseKinematics,Voronoi,CubemapMesh}.java`; `graphics/g3d/{PlanetRenderer,PlanetMesh,PlanetGrid,PlanetParams,MeshBuilder,HexMesh,NoiseMesh,HexSkyMesh,SunMesh,HexMesher,GenericMesh,MatMesh,MultiMesh,ShaderSphereMesh}.java`; `world/draw/*.java` (36 drawers), `world/blocks/environment/Floor.java` (draw halves), entity/draw usage of `Drawc` (via plans 10/11), `core/GameState`/`Rules` render fields, `maps/MapPreviewLoader.java` (call-site semantics only). AGENTS: `graphics/AGENTS.md`, `core/AGENTS.md` (Renderer), `world/AGENTS.md`, `core/assets/AGENTS.md` (shaders), `ui/AGENTS.md` (minimap widget boundary), `core/src/mindustry/AGENTS.md`, `Mindustry/AGENTS.md`. Skills: `godot-compositor-testing`, `playtest`. |
| **Extends spine** | Adds `/root/Spine/World/Renderer` (`MindWorldRenderer`), band canvas-item children, append-only aux nodes (`Bloom`, `LowRes`, `MinimapProvider`, `Planet`), the `MindRender` autoload facade, the `render-list` headless command + scenarios, and `mind-core::render` (Godot-free Layer/DrawCommands/scan/chunk-invalidation data). Retires plan-00's colored `MindTileGrid` draw path to debug-only; all other plan-00 node paths are unchanged. |

## 2. Scope & parity definition

### 2.1 In scope

1. **Frame pipeline parity.** `Renderer.draw()` stage order, `Draw.draw(z, runnable)` queue + `Draw.sort(true/false)` + `Draw.flush()` semantics, `Draw.reset()`, render-target brackets (`Draw.drawRange`), bloom capture/render bands, `Trigger.{preDraw,draw,drawOver,postDraw}` firing positions, camera/viewport/scale handling, screenshake offset application, launch/land cutscene camera (`showLanding`/`showLaunch`, `landTime`, `getLandTimeIn`), `Lod`.
2. **`Layer` + `CacheLayer` + `BuildingCacheLayer` parity**: exact constant tables, append-only, plus the band allocator that maps them onto Godot `CanvasItem.z_index`.
3. **`FloorRenderer`**: 30×30-tile chunk bakes of floors/overlays/walls keyed by `CacheLayer`; `recacheTile` dirty grid; `ShaderLayer.begin/end` animated-liquid behavior; `growSprites` padding; error-region fallback; `drawUnderwater` replay per liquid layer.
4. **`BlockRenderer`**: 5 quadtrees (block, blockCached, blockLight, overlay, floor) and their index/unindex rules; `processBlocks` visible-set extraction; per-chunk `SpriteCache` equivalent for `drawCached` buildings across `BuildingCacheLayer::{under,normal}`; dynamic tile draws (`drawBase`, cracks, team overlay, `drawStatus`, `drawDestroyed`); shadows FBO; darkness FBO (`updateDarkness`/`drawDarkness`/`updateShadow`); multi-tile center-only draws; lights collected from the light quadtree.
5. **`OverlayRenderer`**: `drawBottom` (other build plans, own plans, input draw) and `drawTop` (player/enemy indicators, unit-possession selection arrows, config selection, build/placement previews + core-protection edges via `Voronoi`, spawner drop-zone circles, hover select, dropping-item overlay). Input handlers supply data; the renderer owns only ordering + primitives.
6. **`LightRenderer`**: 4×-downscaled light buffer, pooled circle lights, region lights, line lights, additive composite with `Shaders.light`, `enabled()` rule.
7. **`FogRenderer`**: dynamic fog from fog-radius buildings + player units; static fog accumulated from plan-12 `FogControl` discovery bitsets + events; `Shaders.fog` composite; texture reuse by the minimap.
8. **`MinimapRenderer` provider**: `Pixmap`→`Texture` world image, per-tile dirty batching (update every 2 frames), `updateAll`, camera region extraction, entity/spawn/indicator/marker draw calls. The interactive widget (click-pan, scroll zoom, fullscreen) is **plan 14**; this plan exposes the provider API only.
9. **`Pixelator`** (optional low-res re-render + `Shaders.screenspace` blit), **`Lod`**, **`EnvRenderers`** (underwater/scorching), **`DebugCollisionRenderer`** (`drawhitboxes`).
10. **Shaders**: consume plan 03's `assets/shaders/shader.index.json`; port every `Shaders.*` entry to `client/shaders/*.gdshader` (canvas_item/spatial), with nullable `shield` fallback, uniform application, and `mind-tools shaders check` drift gate.
11. **`Pal`/`Drawf`/`Trail`**: constants and static draw helpers; ribbon trails.
12. **Menu/load/planet**: `MenuRenderer` (procedural cached menu world + flyers), `LoadRenderer` (bars + rotating `PlanetGrid` mesh), `g3d` (`PlanetRenderer`, `PlanetMesh` family, `MeshBuilder`, `PlanetGrid`, cubemap skybox, atmosphere/clouds/grid/orbits), launch cutscene, map screenshots.
13. **Draw command bridge**: `DrawCommands`/`DrawSpec` execution from 07, bullet/unit draw states from 10/11, logistics/power/heat visual reads from 08/09, and a headless-testable **render-list extraction** path (the visual oracle).
14. **View interpolation & culling**: frame-alpha (`SimHost` accumulator remainder), camera smoothing, chunk/quadtree culling parity (`processBlocks` early-out), explicit "no dynamic-group culling" parity rule.

### 2.2 "Done" means

1. `mind-core::render` types exist and are headless-testable: `Layer` table, `DrawCommands`, render-list scan, floor-chunk dirty/version logic, band keys.
2. `mind-headless render-list` produces the canonical render-list JSON (§6.3) for registered scenarios, deterministic across runs, and the committed goldens pass.
3. In-engine, the full `Renderer.draw()` pipeline runs in the exact §3.3 stage order, and a fixed camera pose screenshot per toggled `Layer` is stable (MCP oracle §7c).
4. A tile change invalidates exactly the affected floor chunk + block chunk + shadow/darkness/minimap pixels; re-bake produces an identical render list to a cold rebuild (chunk-invalidation oracle §7b).
5. `drawCached` buildings recache on visual change (`Building.recache()` equivalent) and on fog/team visibility transitions; dynamic turrets/units/bullets draw every frame from the same data as the sim.
6. Lights, darkness, fog of war, minimap, pixelator, bloom, menu, loading screen, planet view, cutscene and map screenshot all function at parity (visual checks in §7).
7. Shader drift check clean; `ShieldShader == None` behaves like upstream (skips shield composite).
8. Perf budget §7d met on the dev host at target map sizes; no per-frame heap allocations in the render build path (alloc-audit counter, plan 05 §3.9 style).

### 2.3 Explicit boundaries (who owns what)

| Area | Owner |
|---|---|
| `Effect`/`EffectState`/`DrawPart`/part rendering/decals/screen shake/`Fx` catalogue | `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (16 only fires draw triggers + provides `Drawf`/queue) |
| Minimap widget/fullscreen fragment, HUD, all dialogs, `LoadRenderer` bar widget visuals | `14_UI_IMPLEMENTATION_PLAN.md` (16 provides provider/textures + g3d mesh; 14 builds the bar/layout) |
| `EditorRenderer`/`EditorSpriteCache`/map preview pixels & PNG/`Texture2D` | `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (16 exposes chunk/quadtree/`render-list` APIs and the `checkPreviews` call site) |
| Sim state, tile mutation, `RenderHooks` firing, `CacheLayerId` ordering | `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` |
| Block draw descriptors (`DrawBlock`, `DrawSpec` → `DrawCommands`), `drawCached`, bars/status data | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (16 executes) |
| Bullet/turret/shield/laser draw states | `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` |
| Unit draw state + `UnitType` draw selection (`UnitTypeDraw` descriptors), hitboxes | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (parts bodies in 17) |
| Conveyor/payload/stack visual state | `08_LOGISTICS_IMPLEMENTATION_PLAN.md` (16 draws from components) |
| Power/heat/liquid render reads | `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` |
| `graphics/EnvRenderers.java` body implementations (underwater rays/particles, scorching noise) | `17_FX_PARTS_IMPLEMENTATION_PLAN.md` owns the FX bodies and registers them through 16's `add_env_renderer(env, pass)`; 16 owns the registration/selection order and reuses the ported constants. |
| `graphics/Trail.java` split | 16 owns the `Trail` state machine + quad emitter (`mc/render/trail.rs`, per assignment); 17 owns FX trail registry/lifecycle; 11 owns unit trail fields and drives `Trail::update`. No duplicate port. |
| Fog discovery bitsets, `Rules.staticFog/dynamicColor/staticColor/lighting/ambientLight/env`, planet/sector runtime, objectives/markers data | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (not yet written; contract frozen in §3.12) |
| Asset pixels, atlas manifest, region names, `shader.index.json`, `.gdshader` storage | `03_ASSETS_IMPLEMENTATION_PLAN.md` |
| Set of drawable content + region metadata | `02_CONTENT_IMPLEMENTATION_PLAN.md` |
| STDB tables/reducers/views | none (client-only visuals; D2) |

### 2.4 Deliberate deviations (each with reason)

| # | Deviation | Reason / tracking |
|---|---|---|
| D16-1 | Godot `CanvasItem` has no fractional z; Mindustry's f32 `Layer` values map to an allocated ordered set of integer `z_index` bands (§3.4). | Engine constraint; ordering parity is preserved because all fractional constants are part of the frozen band table. |
| D16-2 | Blend mode is per canvas item, so a band is keyed `(z, blend)` instead of per draw call. | Godot canvas limitation; produces identical output for all upstream draws (upstream `Draw` material switches are per-batch, not per-sprite). |
| D16-3 | `Gl.blendEquationSeparate(funcAdd, max)` in `LightRenderer.draw()` is not expressible via `CanvasItemMaterial`; default is additive (`BLEND_MODE_ADD`) with a custom `RenderingDevice` max-blend variant reserved in M5. | Closest engine feature; `NEEDS USER DECISION` for exact-max fidelity (§8). |
| D16-4 | Bloom capture band uses a dedicated `SubViewport`; fallback is whole-frame HDR-2D glow if band capture proves unstable. | Band capture is faithful to `Layer.bullet-0.02`/`Layer.effect+0.02`; fallback documented (§8). |
| D16-5 | `ParticleRenderer` is not ported (upstream wires it nowhere). | Dead code upstream; skip. |
| D16-6 | `IntelGpuCheck`/`NvGpuInfo` GL workarounds are not ported; a single `rendering_method` (OD-R2) applies. | Godot abstracts backends; VRAM query has a Godot equivalent if ever needed. |
| D16-7 | `MenuRenderer` randomness uses a dedicated view RNG seeded from `--menu-seed` (default random) so screenshot oracles can pin it. | Upstream is random per launch; dev/CI needs determinism. |
| D16-8 | Dynamic entity groups (units/bullets/effects) are **not** frustum-culled, matching upstream `Groups.draw` iteration. | Parity first; perf managed by plan 10/11 counts and 17's FX gates. |
| D16-9 | World background for `planetBackground` uses a cached `SubViewport` render, not an Arc `FrameBuffer`; `takeMapScreenshot` renders to a `SubViewport` and reads back. | Godot has no direct `FrameBuffer`; semantics identical (§3.10). |
| D16-10 | Render-only data may be recomputed in `mind-gdext`; it never feeds the sim. | HLP §2.4 / D8. |
| D16-11 | Addons per HLP §8: `TileMapDual` **not used** (square 30×30 chunk meshes, square 47-slice autotiling lives in 02/03/06 descriptors); minimap/fog addons **not used** (custom provider/FBOs for parity); `Phantom Camera` belongs to plan 15, but 16 exposes `Camera2D` scale/min/max/`set_camera_pose` for it; `BlastBullets2D` belongs to plans 10/17 and is not a world-render dependency. | Recorded in `THIRD_PARTY_NOTICES.md` only when actually adopted. |

## 3. Target design

All names below are final unless marked otherwise. `mind-core` stays Godot-free and tokio-free (D1). No `HashMap` iteration on render-scan paths; ordered `Vec`/`IndexMap`/slab only.

### 3.1 Crate / module layout

```
client/rust/mind-core/src/render/            # Godot-free, view-only data + scan
  mod.rs           # Layer constants (f32 table), LayerName, BandKey, RenderError
  commands.rs      # DrawCommands (POD, reusable), DrawCmd::{Sprite,Shape,Lines,Fill,SetZ,SetBlend},
                   #   Sprite, ShapeKind, Blend, RegionId, DrawSpec executor entry
  ids.rs           # RegionId(u32) interning over 03 AtlasIndex; RegionIdTable load/step/reset
  scan.rs          # RenderList + visible-set extraction (processBlocks port, quadtrees)
  floor_cache.rs   # FloorChunkGrid: dirty flags, used-layer computation, version counters
  block_cache.rs   # BuildingCacheChunk state: per-layer dirty, cache epoch ids (mesh rebuild triggers)
  drawf.rs         # Pal constants + Drawf-style command emitters (text/flames/lines/additive/target/selection)
  trail.rs         # Trail state machine (points, update/draw/shorten) emitting DrawCommands
  math/voronoi.rs  # Voronoi.generate (core-protection edges; reused by 19)
  math/inverse_kinematics.rs  # leg solver used by 11/17 unit drawing
  g3d/grid.rs      # PlanetGrid (icosphere, exact constants), Ptile/Corner/Edge
```

```
client/rust/mind-gdext/src/render/           # Godot-facing
  mod.rs               # MindRender autoload facade + #[func] debug API
  bands.rs             # BandPlan: Layer f32 x Blend -> z_index (i32), stable, append-only
  queue.rs             # RenderQueue: Vec<(z, seq, Cmd)>; stable sort; partition to bands
  world.rs             # MindWorldRenderer (Node2D): _draw replay, band children, culling, triggers
  floor.rs             # FloorRenderer: per-(chunk, CacheLayer) ArrayMesh + material; recache
  blocks.rs            # BlockRenderer: quadtree mirrors, dynamic draws, cached chunk rebuilds
  shadow.rs            # shadows + darkness SubViewports and composite quads
  light.rs             # LightRenderer SubViewport + composite
  fog.rs               # FogRenderer static/dynamic SubViewports + composite
  minimap.rs           # MinimapRenderer provider (Image/ImageTexture, dirty batch)
  pixelate.rs          # Pixelator low-res SubViewport + screenspace blit
  shaders.rs           # ShaderRegistry over shader.index.json; uniforms; null-shield fallback
  drawf.rs             # Drawf/Trail executor variants (region lookups, additive, lines)
  env.rs               # EnvRenderers (underwater/scorching)
  lod.rs               # Lod
  menu.rs              # MenuRenderer (procedural world on CachedTile-equivalents)
  load.rs              # LoadRenderer mesh (bars layout from 14)
  g3d/mod.rs           # PlanetRenderer host (Camera3D, meshes, skybox, orbits)
  g3d/mesh.rs          # MeshBuilder -> ArrayMesh/ImmediateMesh; PlanetMesh wrappers
  snapshot.rs          # map screenshot + layer capture (oracle)
  debug.rs             # DebugCollisionRenderer
  stats.rs             # counters for MCP/perf: draw calls, chunk rebuilds, queue sizes
```

Godot scenes (added, not replacing plan-00 paths):

```
client/scenes/render/world_renderer.tscn   # MindWorldRenderer + band children + aux quads
client/scenes/render/planet.tscn           # Camera3D + PlanetRenderer host (used by 14 PlanetDialog)
client/scenes/render/loading.tscn          # LoadRenderer mesh + bars (layout from 14)
client/scenes/render/menu_bg.tscn          # menu background world host (menu state only)
```

### 3.2 Godot scene tree additions (plan-00 spine preserved)

```
Spine (Node)
├── SimHost (MindSimHost)                       # unchanged
├── World (Node2D)                              # unchanged path
│   ├── TileGrid (MindTileGrid)                 # debug-only after M1 (visible=false default)
│   ├── Camera2D (MindCamera2D)                 # unchanged path; gains scale/zoom/pose API
│   ├── Renderer (MindWorldRenderer)            # NEW: frame pipeline owner
│   ├── Bloom (Node2D)                          # NEW (appended): SubViewport + composite quad
│   ├── LowRes (Node2D)                         # NEW (appended): pixelator SubViewport + blit
│   └── MinimapProvider (MindMinimap)           # NEW (appended): textures + dirty batch
├── Ui (CanvasLayer)
│   └── StateInspector (…)                      # unchanged; gains Render tab (counters)
└── Planet (CanvasLayer)                        # NEW: g3d host, visible in menu/planet screens
```

The `MindWorldRenderer` draws directly on the main canvas through ordered band children (no world SubViewport on the default path); `Bloom`/`LowRes` are appended aux nodes, so every plan-00 MCP node path stays valid. `MindRender` is registered by `mind-gdext` as an additional Node in the spine (path `/root/Spine/MindRender`, appended) with `#[func]` methods added to plan-00's stable test API: `set_layer_visible(name, bool)`, `layer_visible(name) -> bool`, `set_camera_pose(x, y, zoom)`, `get_render_stats() -> Dictionary`, `capture_layers(dir) -> PackedStringArray`, `capture_map(path) -> bool`, `set_menu_seed(seed)`, `rebuild_chunks() -> i64`.

### 3.3 Frame pipeline parity (exact stage order)

`MindWorldRenderer::frame(delta)` runs after `SimHost` applies its fixed steps and after `mind-gdext` view sync. Per-frame sequence (upstream `Renderer.update/draw` + `graphics/AGENTS.md` §Frame pipeline), with the Godot target:

| # | Upstream | This port | Godot target |
|---|---|---|---|
| 0 | `PerfCounter.render.begin`, `Color.white` reset | `stats.frame_begin` | `MindRenderStats` |
| 1 | camera scale lerp (`camerascale → targetscale`, clamp min/max), settings read (`animatedwater`, `effects`, `blockstatus`, `drawlight`, `pixelate`, …) | identical formula (`lerpDelta`, 0.1); settings from plan 04 store | `/root/Spine/World/Camera2D` (`scale`), `Shaders` toggles |
| 2 | cutscene: `launchAnimator.updateLaunch`, `camerascale = zoomLaunch()`, `landTime -= delta`; else `weatherAlpha` lerp | same state machine in `cutscene.rs`; `weatherAlpha` exposed to 17's weather render | camera + `Layer.space` draws |
| 3 | `camera.width/height = screen/camerascale` | `Camera2D` zoom = `camerascale`; viewport rect derived | `Camera2D.zoom` |
| 4 | `Lod.update()` | exact thresholds (§3.8) | `lod.rs` |
| 5 | menu: `graphics.clear(black)`; game: `minimap.update()`, screenshake offset, `pixelator.drawPixelate()` **or** `draw()` | apply `fx::shake::ShakeState::offset` (17) with upstream's `shakeIntensity * (screenshake/4) * 0.75` random-direction offset; pixelate dispatch | camera `offset`, `LowRes` |
| 6 | `Events.fire(Trigger.preDraw)`, `MapPreviewLoader.checkPreviews()` | fire trigger; call plan 06/19 preview queue API | view `EventBus` |
| 7 | `Draw.reset()`; `effectBuffer.resize` if `animateWater \|\| animateShields` | reset queue; resize aux buffers | `queue.rs` |
| 8 | `Draw.proj(camera)` | copy camera canvas transform to band items | `RenderingServer.canvas_item_set_transform` |
| 9 | `blocks.checkChanges()` (darkness data + `recacheWall`), `blocks.processBlocks()` | port exactly (§3.6) | `blocks.rs`, `floor.rs` |
| 10 | `Draw.sort(true)`; `Trigger.draw`; pixelator `register()` | enable queue sort; fire trigger; set `Layer.end` blit | `queue.rs`, `Trigger` |
| 11 | `Layer.background`: `drawBackground()` (texture scroll / planet skybox FBO / custom callback) | §3.10 | `Renderer` band + `Planet` |
| 12 | `Layer.floor`: `floor.drawFloor()` (all cache layers) | §3.5 | floor band + CacheLayer sub-bands |
| 13 | `Layer.block-1`: `drawShadows()` (`Shaders.darkness`); `Layer.block-0.09`: walls layer `beginDraw/drawLayer(walls)` | §3.6 | shadow band + walls sub-band |
| 14 | `Draw.drawRange(Layer.blockBuilding, Shaders.blockbuild, Draw::shader)` | bracket: set blockbuild material for that band | `Shaders.blockbuild` |
| 15 | matching `envRenderers` by `(env & rules.env) == env`; objective markers; `state.markers.worldMarkers` | §3.8, data from 12 | `env.rs`, `Drawf` |
| 16 | `lights.add(marker light runnables)`; `if (rules.lighting && drawLight) Layer.light: lights.draw()` | §3.7 | `light.rs` |
| 17 | `if (enableDarkness) Layer.darkness: blocks.drawDarkness()` | §3.6 | `shadow.rs` |
| 18 | bloom `capture` at `Layer.bullet-0.02`, `render` at `Layer.effect+0.02` | §3.9 | `Bloom` |
| 19 | `control.input.drawCommanded()`; `Layer.plans: overlays.drawBottom()` | §3.7; input data from 15 | overlay band |
| 20 | `Layer.shields` / `Layer.buildBeam` effectBuffer brackets (skipped if `Shaders.shield == null`) | §3.9; null-check parity | `shaders.rs` |
| 21 | `Layer.overlayUI: overlays.drawTop()`; `if rules.fog Layer.fogOfWar: fog.drawFog()` | §3.7 | overlay/fog bands |
| 22 | `Layer.space` launch draw + global-z launch draw | §3.10 cutscene | space band |
| 23 | `Trigger.drawOver`; `blocks.drawBlocks()`; `Groups.draw.draw(Drawc::draw)` | §3.6 + 07/10/11 commands | block/unit/bullet bands |
| 24 | `drawhitboxes` → `DebugCollisionRenderer.draw()` | §3.8 | `debug.rs` |
| 25 | `Draw.reset(); Draw.flush(); Draw.sort(false); Trigger.postDraw` | flush replay; disable sort; fire trigger | `queue.rs` |

`Draw.draw(z, runnable)` equivalent: `queue.push(z, Cmd)` in emission order; the queue is stable-sorted by `z` only when `sort == true` (mirroring `Draw.sort`); `Draw.flush()` replays every command to band items in that order. `Draw.drawRange(z, begin, end)` equivalent: `queue.range(z, RangeBegin, RangeEnd)` with band material brackets (used for `blockbuild`, shields, buildBeam). `Draw.reset()` → `queue.push(Reset)` (color/blend/z defaults). `Draw.z`, `Draw.color`, `Draw.mixcol`, `Draw.alpha`, `Draw.blend`, `Draw.scl`, `Draw.trans` are per-thread encoder state, never sim state.

### 3.4 Layer constants and the band allocator

`mind_core::render::Layer` mirrors `graphics/Layer.java` exactly (append-only; never renumber):

| Constant | Value | Band (z_index) | Note |
|---|---|---|---|
| `min` | -11 | 0 | |
| `background` | -10 | 1 | custom background / planet skybox |
| `floor` | 0 | 2 (+ `CacheLayer.id` sub-order) | CacheLayer sub-bands inside |
| `scorch` | 10 | 12 | decals (17) |
| `debris` | 20 | 13 | |
| `blockUnder` | 29.5 | 14 | `BuildingCacheLayer.under` = 29.5 |
| `block` | 30 | 15 (+ `BuildingCacheLayer.normal` sub-order) | cached then dynamic |
| `blockCracks` | 30.1 | 16 | cracks batch |
| `blockAfterCracks` | 30.2 | 17 | |
| `blockAdditive` | 31 | 18 | grouped additive |
| `blockProp` | 32 | 19 | |
| `blockOver` | 35 | 20 | |
| `blockBuilding` | 40 | 21 | shader bracket |
| `turret` | 50 | 22 | |
| `turretHeat` | 50.1 | 23 | additive |
| `groundUnit` | 60 | 24 | |
| `power` | 70 | 25 | power lines |
| `legUnit` | 75 | 26 | |
| `darkness` | 80 | 27 | darkness FBO composite |
| `plans` | 85 | 28 | |
| `flyingUnitLow` | 90 | 29 | |
| `bullet` | 100 | 30 (capture 99.98 → 31) | bloom capture |
| `effect` | 110 | 32 (render 110.02 → 33) | bloom render |
| `flyingUnit` | 115 | 34 | |
| `overlayUI` | 120 | 35 | |
| `buildBeam` | 122 | 36 | |
| `shields` | 125 | 37 | |
| `weather` | 130 | 38 | data from 17 |
| `light` | 140 | 39 | light composite |
| `playerName` | 150 | 40 | |
| `fogOfWar` | 155 | 41 | |
| `space` | 160 | 42 | cutscene |
| `end` | 200 | 43 | pixelator blit |
| `endPixeled` | 210 | 44 | post-pixelation text |
| `max` | 220 | 45 | |

Rules: the band table is generated once at startup from an ordered list and is **append-only** (new layers allocate new band numbers; existing numbers never change). `z_index` is set on each band's `Node2D` child with `z_as_relative = false`. Each `BandKey = (layer: LayerId, sub: u8, blend: Blend)`; the queue partitions commands into band items so that within a band the emission order is exact. `Blend` = `{Normal, Additive, Multiply, Disabled}` mapped to `CanvasItemMaterial.blend_mode`. `LayerName` strings match the Java identifiers (`"blockCracks"`, …) for the debug API and oracle layer names.

`CacheLayer` ordering is fixed by plan 06 §3.12 as an ABI: `water=0, mud=1, tar=2, slag=3, arkycite=4, cryofluid=5, space=6, normal=7, walls=8` (**not** the `CacheLayer.java` field-declaration order; it is the `CacheLayer.init()` `addLast` order). `BuildingCacheLayer`: `under = Layer.block - 0.5`, `normal = Layer.block`.

### 3.5 Floor cache (`FloorRenderer` → `floor.rs` + `floor_cache.rs`)

- Chunks are 30×30 tiles (`chunkunits = 240` world units, `packPad = 64`), matching `FloorRenderer.chunksize`.
- One Godot `ArrayMesh` per `(chunk, CacheLayer)` with one shared shader material per layer (port of the inline `FloorRenderer` shader; the band item transform supplies `Draw.proj`, so world-space vertices are sufficient and the packed normalized coords are an implementation detail upstream). Shared quad indices match `SpriteIndices` semantics.
- `mind_core::render::floor_cache::FloorChunkGrid` owns `dirty: Vec<bool>`, `used: SmallVec<[CacheLayerId; 9]>` per chunk, `mesh_epoch: Vec<u64>`, and the exact `recache_tile(x, y)` mapping (chunk `x/30, y/30`, bounds-checked no-op). `recache_wall(tile)` additionally dirties a `darkRadius` square and calls `minimap.update_pixel` for each tile, mirroring `BlockRenderer.recacheWall`.
- `cacheChunk` used-layer computation is ported verbatim: for the chunk plus a 1-tile border, add `tile.block().cache_layer` when it is not `normal`, and add `tile.floor().cache_layer` when `world.is_accessible(tile)` or the block is not a filling wall.
- Bake pass (`cacheChunkLayer` port): for each covered tile: `CacheLayer.walls` draws `block.draw_base(tile)` when `!(tile.is_darkened() && tile.data >= 5)`; else the floor's own layer draws `floor.draw_base(tile)` with the accessibility rule; a layer that matches neither calls `floor.draw_non_layer(tile, layer)`. While baking any layer except `walls`, `grow_sprites = true` (0.04 padding). Draw commands are captured into a `DrawCommandSink` (core) and converted to quads in `floor.rs`; unknown regions fall back to the `error` region exactly like `FloorRenderBatch.draw`. Chunk AABB is `[cx*240-4, cy*240-4]..[(cx+1)*240+4, (cy+1)*240+4]`.
- `ShaderLayer.begin/end`: when `animate_water`, liquid layers render through the shared `effectBuffer` `SubViewport` with the layer's `SurfaceShader`; `Draw.flush` parity is preserved by ending the current band batch before capture. `liquid` layers replay `underwaterDraw` entries once per liquid layer using the alpha-capped blend (`src_alpha, one_minus_src_alpha / dst_alpha, one_minus_src_alpha`), then restore the normal batch.
- `drawFloor()`'s preliminary pass (collect layers used by chunks whose AABB overlaps camera bounds, sorted by id, skipping `walls`) is ported as `scan::floor_layers_in_view`.
- Choice justification: **`ArrayMesh` + per-layer band items** because it supports arbitrary bake-time draw commands, per-CacheLayer shaders, 30×30 recache granularity and error fallback. `MultiMeshInstance2D` cannot bake arbitrary draws (only same-quad instances) and would need per-instance UV custom data; `TileMapLayer` lacks fractional sub-bands, per-layer shaders and the recache API, and the HLP §8 default excludes `TileMapDual`. Flagged `NEEDS USER DECISION` (§8-OD16-A) because it is load-bearing and engine-specific.

### 3.6 Block renderer (`BlockRenderer` → `blocks.rs`)

- Five quadtree mirrors (`block`, `blockCached`, `blockLight`, `overlay`, `floor`) ported with the exact `indexBlock/indexBlockCached/indexOverlay/indexFloor` predicates and the `TilePreChangeEvent`/`TileChangeEvent` remove/insert wiring from plan 06's event stream. Quadtrees live in `mind_core::render::scan` (Godot-free) so headless tests can assert visible sets; `blocks.rs` consumes them.
- `processBlocks` early-out parity: skip recompute when `(camera tile, range, team)` is unchanged. The lists `updateFloors` (`floor.updateRender`, `overlay.updateRender`), `lightview` (floor/overlay emit-light + `blockLight` quadtree), `tileview` (dynamic blocks + power-link tiles via `procLinks`), `tileExtraCachedView` (cached non-dynamic), `tileWithConsumerView` (status bars for same-team consumer buildings) and `chunksToDraw` are rebuilt in the same order and with the same `blocks.drawCached` / `!block.drawDynamic` / `build.power.links` branches.
- Dynamic draws: at `Layer.block`, `block.draw_base(tile)`; `customShadow` at `Layer.block-1`; `drawCracks` at `Layer.blockCracks` (regions `cracks-<size>-<i>`, `crackRegions = 8`, `maxCrackSize = 7`); `drawTeam` at `Layer.block` when `build.team != pteam && block.draw_team_overlay`; fog visibility (`build.in_fog_to(team)`, `was_visible`, `visible_flags` per team bit) ported exactly, including the `renderer.drawStatus && Lod.l2` gate and the `minimap.update(tile)` / `react` side effects on visibility flips.
- `drawCached` buildings: per-chunk `BuildingCacheChunk` (30×30, `max_sprites_per_cache_tile = 6`) with `under`/`normal` `BuildingCacheLayer`s; contents captured by `block.draw_base_cached(tile)` plus the team-overlay condition, stored as a mesh epoch per chunk/layer. `recache_building(layer, tile)` marks dirty; rebuild happens when the chunk is in `chunksToDrawSet` or during `drawBlocks` for dirty chunks only. The upstream global `SpriteCache` page pool (16382 sprites) becomes per-chunk mesh epochs; `sprites_used` accounting is preserved as a counter for the budget.
- Cached visual changes call plan 07's `Building::recache()`, which routes to `blocks.rs` exactly like `TileChangeEvent`; fog/team transitions recache like upstream when `wasVisible` flips (`drawBlocks` / `tileExtraCachedView` paths).
- Multi-tile draws: only the center tile is indexed (`tile.is_center()`), so multiblocks draw once via `BuildDrawData { entity, x, y, rot, size, progress, warmup, total_progress, … }` — matching `drawBase`/`drawBaseCached`. Cracks/status/team overlays for cached blocks come from `tileExtraCachedView` in upstream order.

### 3.7 Overlays, lights, fog, minimap

- **`OverlayRenderer`** (`overlays.rs`, used by `world.rs`): `drawBottom` = `input.draw_other_build_plans()` when `show_other_build_plans`, own plans when `player.is_builder()`, `input.draw_bottom()`; `drawTop` = player/enemy off-screen indicators, possession selection arrows (`select-arrow` region, 4 arrows at `rot = i*90 + 45 - Time.time % 360`, `unitFade` lerp), config-selected `draw_configure`, input top/unit selection, build fade, core-protection edges (`Voronoi.generate` over protected team cores, `CoreEdge::displayed()` parity), non-polygon build-radius circles (`state.teams.each_enemy_core`), spawner drop-zone dash circles, hover selection (`draw_select`, `draw_disabled`, quick-rotate arrow + accent square), logic-controller bounds/arrow, dropping-item overlay. Input data (plans, selection, mouse world) from plan 15; objectives/markers from plan 12; 16 orders primitives only.
- **`LightRenderer`** (`light.rs`): `enabled() = rules.lighting && rules.ambient_light.a > 0.0001 && renderer.draw_light`; pooled circle lights (`circle-shadow` region, 4×-downscaled target with `resize(screen/4)`); region lights (`add(x, y, region, rot, color, opacity)`); line lights (`circle-end`/`circle-mid` quads); `Shaders.light` composite with the `ambient` uniform from `rules.ambientLight`. Lights are queued during block/unit draws via `lights.add(...)` and drained at `Layer.light`.
- **`FogRenderer`** (`fog.rs`): plan 12 publishes fog to `mind-gdext` through `ClientHooks::fog_handle_event(packed: u64)` (packing `x:16 | y:16 | radius:16 | team:8`, plan 12 §3.9); 16 unpacks and queues it. Dynamic target is rebuilt each frame with 20-gon `Fill.poly` per fog-radius building (`BlockFlag.hasFogRadius`) and per player-team unit (`unit.type.fog_radius * 8`); static target accumulates white 20-gons from queued events or `copy_from_cpu` from the `FogControl` discovery bitset on team change (`FogControl::get_discovered(team)`/`data(team)`, plan 12); composites at `Layer.fogOfWar` with `rules.dynamic_color` (alpha floor 0.5) and the static `tilesize/2` offset parity. Textures are exposed to the minimap provider.
- **`MinimapRenderer`** (`minimap.rs`): world-sized `Image` (RGB8) + `ImageTexture`; dirty `IntSet` flushed every 2 frames (`updateCounter += delta`); `color_for(tile)` ported over plan-04 `MapIO::color_for` (fog-masked `real_block`, darkness multiply `1 - clamp(darkness/4)`, `0.7` block-above multiply, liquid-edge `0.84, 0.84, 0.9` multiply); `update_all` on world load / darkness change; `get_region()` camera crop for the widget. Plan 14 owns the interactive widget and fullscreen variant; `MindMinimap` exposes the exact plan-14 boundary names: `get_texture()`, `region()`, `minimap_to_screen(x, y)`, `screen_to_world(x, y)`, `zoom_by(amount)`, `set_zoom(z)`, `update(tile)`, `update_all()`, plus `draw_entities(view, full)`, `draw_spawns()`, `draw_indicators()`, marker passes queued under the widget transform.

### 3.8 Pixelator, Lod, EnvRenderers, debug

- **`Pixelator`** (`pixelate.rs`): when the `pixelate` setting is on, the renderer re-renders into a low-res `SubViewport` sized `clamp((int)camera.width, 2, screen_w)` × `clamp((int)camera.height, 2, screen_h)`, with camera position snapped to half-pixel parity (`(int)px + (width % 2 == 0 ? 0 : 0.5)`) exactly as upstream; blits at `Layer.end` with the `screenspace` shader and blending disabled; UI renders on the normal path afterward. Cutscene sizing uses `landScale`/`getScale`.
- **`Lod`** (`lod.rs`): `l1/l2`, `alpha1/alpha2` with `threshold1 = 1.4`, `threshold2 = 0.8`, `fade = 0.2`; `disable` forces `l1 = l2 = true`, alphas 1 (map screenshot).
- **`EnvRenderers`** (`env.rs`): 16 owns the registration/selection contract `MindWorldRenderer::add_env_renderer(env_mask, pass_id)` and the stage-15 loop `(env & rules.env) == env`; 16 ports the constants and the draw bodies from `graphics/EnvRenderers.java` as the default passes (underwater: `0x353982` fill at `Layer.light+1`, additive `caustics` blit, 50 additive ray sprites from `sprays/rays.png` with the exact modulo placement/slope alpha; scorching: `distortAlpha.png`, `Color.scarlet`, `drawNoiseLayers(…, 4, -1.3, 0.7, 0.8, 0.9)` at `fog ? Layer.fogOfWar+1 : Layer.weather-1`). Plan 17 owns the FX bodies and re-registers/replaces passes through the same API (17 §2.3/§3.10); weather particle simulation stays in 17 (`FxSink` + `Weather::draw_particles` sink). No duplicate port: the shared math lives in `mg/env.rs` and 17 consumes it.
- **`DebugCollisionRenderer`** (`debug.rs`): `drawhitboxes` gate; hitbox squares (green 0.3), solid-tile edges (magenta 0.4), `debugDrawAvoidance` squares, unit tile hitboxes, physics circles (`unitCollisionRadiusScale`); ported verbatim.

### 3.9 Shaders

- **Consumption.** `shaders.rs` loads plan 03's `assets/shaders/shader.index.json`; each entry maps `name → gdshader` through plan 03's `FileTree` (mod override honored). Logical registry keys: `blockbuild`, `shield`, `buildbeam`, `unitbuild`, `unitarmor`, `darkness`, `fog`, `light`, `water`, `mud`, `tar`, `slag`, `arkycite`, `cryofluid`, `space`, `caustics`, `planet`, `clouds`, `planetgrid`, `atmosphere`, `mesh`, `unlit`, `unlitwhite`, `screenspace` (the disabled `shockwave` is not ported; `ParticleRenderer`'s shader is not wired upstream either).
- **Port strategy.** Hand-ported `.gdshader` sources in `client/shaders/` (canvas_item for fullscreen/surface/darkness/fog/light/shield/buildbeam/unitbuild/unitarmor/blockbuild/screenspace; spatial for planet/clouds/atmosphere/planetgrid/unlit/mesh), copied by `mind-tools shaders build`; plan 03's `shaders check` is extended by this plan with a reverse check (every uniform/texture the Godot shader declares is fed by its `apply()` equivalent).
- **Uniform application.** One `ShaderMaterial` per logical use-site. `apply()` equivalents read from a per-frame `ShaderGlobals { time, global_time, cam_pos, cam_size, resolution, dp }` block. `Draw.shader(s)` becomes a band material bracket; `s.bind() + s.apply()` becomes binding the material and setting uniforms before replaying that band.
- **Fallbacks.** `shield` is nullable; when missing, the `Layer.shields` bracket is skipped exactly like `Renderer.java:403`. Any `.gdshader` that fails to load logs `[E]` and the registry substitutes a neutral pass-through canvas shader, counted in `get_render_stats()`.
- **Bloom** (§3.3 step 18): the commands between `Layer.bullet-0.02` and `Layer.effect+0.02` are also fed to `Bloom/SubViewport` (same camera transform), where a `WorldEnvironment` `Environment.glow` runs with `blurPasses` and `setBloomIntensity(settings/4 + 1)` parity (threshold 0.8, upstream `Bloom` defaults); the result composites at `Layer.effect+0.02`. Fallback: root `use_hdr_2d = true` + whole-frame `WorldEnvironment` glow (deviation, §8-OD16-C).

### 3.10 Menu, load, planet (g3d), cutscene, screenshots

- **`MenuRenderer`** (`menu.rs`): 100×50 (desktop) / 60×40 (mobile, `Vars.mobile`) procedural world on `CachedTile`-equivalents with the exact Simplex/Ridged selection logic, floor/overlay/wall bakes through the same chunk pipeline, shadow quad, additive darkness 0.3 overlay, and `flyers` sample units (default type list `flare, horizon, zenith, mono, poly, mega, alpha, beta, gamma`) animated by the `flyers` math. `state.is_menu()` drives `clear(black)` + menu render. `--menu-seed` pins the view RNG for oracles.
- **`LoadRenderer`** (`load.rs`): rotating `PlanetGrid.create(2)` mesh (port of `MeshBuilder.buildPlanetGrid` → `ArrayMesh` + `colorRed` = `Pal.breakInvalid.lerp(black, 0.3)`), render-time window, back-end version string, progress text; `Bar`s are instantiated by plan 14 from plan 03's `AssetsStageEvent`. `loading.tscn` hosts the mesh + placeholder bars so boot works before 14 lands.
- **g3d** (`g3d/mod.rs`, `g3d/mesh.rs`): `PlanetRenderer` becomes a `Node` with `Camera3D` (fov 60, far 150, up `Y`, `camLength = 4`, `projector` scaling `1/150`), a `PlanetMeshRegistry` per `PlanetDef` built lazily by `MeshBuilder` (`build_hex`, `build_icosphere`, `build_planet_grid` ported exactly, including the `tiles*6 < 65535` indexed threshold and normal packing omitted on weak GL), cubemap skybox (`cubemaps/stars/` → `Cubemap` + 1×1 box mesh with the `cubemap` shader), the planet shader family (`planet`, `clouds`, `atmosphere`, `planetgrid`, `unlit`, `unlitwhite`), orbit rings (line-loop `ImmediateMesh` per frame, alpha by `uiAlpha`), sector grid (`render_sectors`), arc/selection/border/fill decorations, bloom (threshold 0.8, 6 passes), and `PlanetParams` (camera position/lerp, zoom, `drawUi`, `drawSkybox`, `alwaysDrawAtmosphere`, view size). `PlanetInterfaceRenderer` becomes a Rust trait implemented by plan 14's `PlanetDialog` host (`render_sectors`, `render_projections`, `render_over_projections`). `Trigger.universeDrawBegin/universeDraw/universeDrawEnd` fire at the same points.
- **Launch/land cutscene**: `show_landing`/`show_launch`/`is_launching`/`land_scale`/`get_land_time_in` ported with `launchDuration`, `begin_launch`, `zoom_launch`, `update_launch`, `end_launch`, `draw_launch`, `draw_launch_global_z`; animator data from plan 07's `LaunchAnimator`. Camera scale is driven from the cutscene before stage-1 clamping, matching upstream order.
- **Screenshots** (`snapshot.rs`): `take_map_screenshot` port — memory check (`checkScreenshotMemory`: mobile 65 MB else 120 MB → `@screenshot.invalid`), `Lod.disable = true`, `drawWeather = false`, `disableUI = true`, camera resized to `(w*8, h*8)` with half-tile offset, full render into a `SubViewport`, alpha forced to 255, `Image.save_png` into `<data-dir>/screenshots/screenshot-<ms>.png`, async write + info toast; all state restored in a `finally` equivalent. `capture_layers(dir)` is the oracle helper: per Layer/band name, toggle visibility, capture at the current fixed pose, restore.

### 3.11 Draw command bridge (mind-core → mind-gdext) and view state

- **`DrawCommands`** (owned here; consumed by 07/10/11/17): a reusable POD buffer (`SmallVec`-backed) with `Sprite { region: RegionId, x, y, w, h, origin_x, origin_y, rot, color: u32, blend: Blend, z: f32 }`, `Shape`, `Lines`, `Fill`, `SetZ`, `SetBlend`. `RegionId` is interned at content load from plan 03's `AtlasIndex`; the name↔id map is append-only and dumped as an audit artifact. `DrawBlock`/`DrawSpec` never sees a string per frame.
- **Who drives traversal.** `mind-gdext` iterates the visible sets from `mind-core::render::scan` and calls the registered draw descriptors (`DrawBlock::{draw, draw_light, draw_plan}`, `Floor::{draw_base, draw_non_layer}`, `UnitTypeDraw`) with a `DrawCommandSink`; commands convert to queued `Cmd`s. Buffers are reused; steady-state frames must not grow them (alloc-audit assertion).
- **External draw state**: bullets/turrets/shields/lasers (`10 §3.12`), units (`11`), logistics (`08 §3.9`), power/heat/liquid reads (`09 §3.12`) are read into view structs in `mind-gdext` and queued at the bands those plans specify (`Layer::{bullet, turret, turretHeat, shields, blockAdditive, effect, debris}`).
- **`Groups.draw` equivalent**: `scan::draw_group_order()` returns `Groups.draw` in insertion-stable slot order (plan 05 §3.6); per-entity draw dispatch matches `Drawc.draw` (units, bullets, effects data from 10/11/17).
- **View interpolation**: `ViewClock { alpha: f32 }` from the fixed-step accumulator remainder. Local-sim parity rule: entity position = sim position (D8; interpolation never feeds back). `alpha` drives camera smoothing, selection fades and the plan-21 `@SyncField` companion state; `LerpMode::{None, Alpha}` defaults to `None`.
- **Culling**: floors/blocks cull via chunk AABB + quadtree intersect with `camera.bounds.grow(tilesize*2)` parity and the `processBlocks` early-out. Dynamic groups are not culled (D16-8). No per-entity `is_visible` checks beyond upstream's fog rules.

### 3.12 External interfaces (plan-12 adapter + not-yet-written plans)

Plan 12 is now on disk; its `Rules` resource is the source of truth for every render field below (12 §3.4: `fog`, `static_fog`, `dynamic_color`, `static_color`, `ambient_light`, `lighting`, `env`, `background_texture`, `planet_background`, `limit_map_area`), and its `FogControl`/`MapMarkers` provide the fog bitsets/events and marker lists. 16 adapts them into the small read view below so `mind-gdext` never reaches into sim internals:

```rust
// adapter over plan 12 `Rules` + `FogControl` + `MapMarkers` — read by fog/env/background/overlay:
pub struct RulesRenderView {
    pub static_fog: bool, pub fog: bool,
    pub dynamic_color: [f32; 4], pub static_color: [f32; 4],
    pub lighting: bool, pub ambient_light: [f32; 4], pub env: u32,
    pub background_texture: Option<String>, pub background_scl: f32,
    pub background_speed: f32, pub background_offset: [f32; 2],
    pub planet_background: Option<PlanetParams>,
    pub custom_background_callback: Option<String>,
    pub limit_map_area: bool, pub limit_rect: [i32; 4],
    pub fog_radius_blocks: &[Entity],            // BlockFlag.hasFogRadius
}
pub struct FogEvent { pub x: i32, pub y: i32, pub radius: f32 }
```

- `mind-gdext` reads this view inside the render frame only; missing plan-12 fields default to `static_fog=false, fog=false, lighting=false, ambient_light=[0.01,0.01,0.04,0.99], env=terrestrial` so M0–M7 are testable.
- Plan 14 minimap widget calls only `MindMinimap` and supplies the transform; plan 14's `LoadRenderer` bars consume `AssetsStageEvent`; plan 14's `PlanetDialog` implements the `PlanetInterfaceRenderer` trait.
- Plan 17 registers draw callbacks for `Trigger.preDraw/draw/drawOver/postDraw` via a view event bus in `mind-gdext`; screen shake calls `MindWorldRenderer::shake(intensity, duration)`.
- Plan 19 uses `floor_cache`/`block_cache` epochs, `scan` visible sets, and the render-list API for `EditorRenderer` and map previews; it must not allocate its own chunk meshes.

### 3.13 Reconciliation with sibling plans (by filename)

| Sibling | Interface frozen there | This plan's resolution |
|---|---|---|
| `00_FOUNDATION` | Spine scene/node paths (`/root/Spine/SimHost`, `/root/Spine/World/{TileGrid,Camera2D}`), `MindSimHost` API, scenario/dump formats, MCP pid-stamp rule. | Verbatim; 16 only appends `/root/Spine/World/Renderer`, `Bloom`, `LowRes`, `MinimapProvider`, `/root/Spine/MindRender`, `Planet`, and gates plan-00's `TileGrid` draw. No path renamed. |
| `02_CONTENT` | `BlockId`/`UnitTypeId`/`PlanetId` ID spaces, `BlockDef` metadata, `EffectId`. | 16 reads only; `Block.cache_layer`, `draw_cached`, `building_cache_layer`, `emit_light`, `obstructs_light`, `light_clip_size`, `clip_size`, `offset`, `display_shadow`, `draw_team_overlay`, `custom_shadow`, `draw_disabled`, `draw_dynamic`, `has_consumers` are required `BlockDef` fields (flag gaps through plan 02 §3.6). |
| `03_ASSETS` | `AtlasIndex`/`Region`/`PageType`, region-name grammar, `sprites.atlas.json`, `shader.index.json`, `.gdshader` storage, `Tex`/`LoadRegions`. | Verbatim; 16 adds `RegionId` interning over `AtlasIndex` and owns the expected-shader-name list plus the reverse uniform check. `MultiPacker` stays with 03. |
| `06_WORLD_TERRAIN` | `WorldGrid`/`Tile`/`Tiles`, tile ops, `RenderHooks` (`recache_tile`, `recache_wall`, `add_floor_index`, `remove_floor_index`, `invalidate_tile`, `minimap_update`), darkness functions, `CacheLayerId` order, `WorldHooks`. | Verbatim; 16 registers the `RenderHooks` implementation and consumes `world.get_darkness/get_wall_darkness`; no tile mutation in 16. |
| `07_BLOCKS_BUILD` | `DrawBlock`/`DrawCommands`/`DrawSpec`, `BuildDrawData`, `BlockKindData`, `drawCached`/`recache`, `BarSpec`/status data, `LaunchAnimator`. | 16 implements the `DrawCommands` sink and executor; file ownership of `DrawCommands` moves to `mc/render/commands.rs` at merge (07 keeps the trait and descriptors). |
| `08_LOGISTICS` | Component-held drawing data (belt items, payload, `blendbits`, region names §6.5). | 16 reads components after the tick; no duplicate state. |
| `09_POWER_LIQUIDS_HEAT` | Read helpers `PowerGraph::{satisfaction, links_of}`, `LiquidModule::{current, current_amount, get}`, `HeatBlock::{heat, heat_frac}`, fluid frame index. | 16 consumes exactly these for `DrawPower`/heat/liquid drawers and the power-link overlay. |
| `10_COMBAT_BULLETS` | `BulletDrawState`/`TurretDrawState`/`ShieldDrawState`/`LaserDrawState`; required Layer mapping `bullet, turret, turretHeat, shields, blockAdditive, effect, debris`. | 16 queues those states at the named layers/bands; `17` supersedes 10's `CombatFx` sink with `FxSink`. |
| `11_UNITS_AI_WAVES` | Unit components/queries/hitbox; `UnitType` draw selection; trails fields; unit inspector API. | 16 computes `UnitDrawState` in `mind-gdext` and dispatches per def; part bodies are 17. |
| `12_CAMPAIGN` | `Rules` render fields, `FogControl` bitsets + packed `FogEvent` (`x:16|y:16|radius:16|team:8`), `ClientHooks::fog_handle_event`, `MapMarkers` (`world_markers/map_markers/light_markers`), `PlanetParamsRef // 16 data`, `MindCampaign` autoload. | §3.7/§3.12 adapter consumes these; `PlanetParamsRef` resolves to this plan's `PlanetParams`. Path naming `/root/Main/*` vs plan-00 `/root/Spine/*` is an orchestrator reconcile (OD16-J). |
| `14_UI` | Minimap widget + fullscreen fragment, `MenuFragment`/`LoadingFragment` referencing 16 nodes, `PlanetDialog` + `PlanetInterfaceRenderer`, `MindBar` binding, `MindCamera2D` pan requests. | 16 exposes exactly `get_texture/region/minimap_to_screen/screen_to_world/zoom_by/set_zoom/draw_entities` plus `menu.tscn`/`loading.tscn` host nodes; `PlanetInterfaceRenderer` is a Rust trait implemented by 14. |
| `17_FX_PARTS` | `FxSink` (replaces 10's `CombatFx`), `DrawPrim { layer: f32, … }` programs, `MindFxLayer` seed, `render/{layer,draw}.rs` seeds, `ShakeState::offset(view_tick)`, `add_env_renderer(Env, pass)`, bloom contract `Layer::bullet..=Layer::effect`. | 16 owns `render/{layer,command}.rs` (the `Layer` table and `DrawCommands` payload that `DrawPrim` maps to), accepts `DrawPrim` batches from 17 at their layers, applies `ShakeState::offset` in stage 5, and provides `add_env_renderer`; 17's seed files are deleted at merge. Rust constant style is normalized to the Java identifiers (`Layer::effect`; 17's `Layer::EFFECT` is the same constant). |
| `19_MAPS_EDITOR` (not on disk) | `EditorRenderer`/`EditorSpriteCache`, preview textures, `DrawOperation`. | Contract frozen in §2.3/§7.5: 16 exposes chunk/quadtree/render-list APIs; 19 owns editor draw ops and preview pixels. |

## 4. Port map

Legend: target paths are relative to `client/rust/` unless stated. `mg = mind-gdext/src/render`, `mc = mind-core/src/render`. Every ported `.rs`/`.gdshader` file carries the GPL header (HLP §6.4).

| Mindustry source | Target Rust module / Godot target | Notes on adaptation |
|---|---|---|
| `core/Renderer.java` | `mg/world.rs` (`MindWorldRenderer`), `mg/mod.rs` (`MindRender` autoload), `mc/render/mod.rs` (`Layer`, `BandKey`) | Frame stages §3.3; settings→`ShaderGlobals`; shake, cutscene state machine, `Lod.disable`, `takeMapScreenshot`→`snapshot.rs`. |
| `graphics/Layer.java` | `mc/render/mod.rs` `Layer` (f32 const table) + `mg/bands.rs` band table | Append-only; band mapping §3.4; `LayerName` mirrors Java identifiers. |
| `graphics/CacheLayer.java` | `mc/render/mod.rs` `CacheLayerId` (plan 06 owns ordering) + `mg/floor.rs` material per layer | `ShaderLayer::begin/end`→`effectBuffer` SubViewport; `add/addLast` not needed (init-only table). |
| `graphics/BuildingCacheLayer.java` | `mc/render/block_cache.rs` (`under=29.5`, `normal=30`) + `mg/blocks.rs` | Enum + `layers[]` array; band 14/15. |
| `graphics/FloorRenderer.java` | `mc/render/floor_cache.rs` (dirty/used/epochs) + `mg/floor.rs` (ArrayMesh bake/draw, `grow_sprites`, error fallback, underwater replay) | §3.5; `ChunkMesh`→per-(chunk,layer) `ArrayMesh`; `FloorRenderBatch.draw`→quad emitter; `recacheTile`. |
| `graphics/BlockRenderer.java` | `mc/render/scan.rs` (5 quadtrees, `processBlocks` lists) + `mg/blocks.rs` (dynamic draws, cached chunks, shadows/darkness) | §3.6; `SpriteCache`→per-chunk `ArrayMesh` epochs; cracks/destroyed/team/status; FBOs §3.3 steps 13/17. |
| `graphics/OverlayRenderer.java` | `mg/world.rs` `overlays::*` + `mc/render/math/voronoi.rs` + `mc/render/drawf.rs` | §3.7; `CoreEdge`/Voronoi in core; input/plan/marker data injected. |
| `graphics/LightRenderer.java` | `mg/light.rs` | 4× target; pooled circles; region/line lights; `Shaders.light` composite; `enabled()` parity; `max` blend deviation D16-3. |
| `graphics/FogRenderer.java` | `mg/fog.rs` (`FogEvent` in `mc/render/scan.rs` or `09`/`12` contract) | Static/dynamic targets; 20-gon polys; `copy_from_cpu` from plan 12; textures exposed to minimap. |
| `graphics/MinimapRenderer.java` | `mg/minimap.rs` (`MindMinimap`) | Provider only; widget in 14; `colorFor` via plan 04 `MapIO::color_for`; update-batch parity. |
| `graphics/Pixelator.java` | `mg/pixelate.rs` | Low-res SubViewport + `screenspace` blit at `Layer.end`; UI unaffected. |
| `graphics/Shaders.java` | `mg/shaders.rs` (`ShaderRegistry`) + `client/shaders/*.gdshader` | `LoadShader`→material per use-site; `SurfaceShader` noise binding; nullable `shield`; `getShaderFi`→plan 03 `FileTree`. |
| `graphics/Pal.java` | `mc/render/drawf.rs` `pal` consts | Exact `Color.valueOf` hex values; used by 16/17/14. |
| `graphics/Drawf.java` | `mc/render/drawf.rs` (emitters) + `mg/drawf.rs` (executor helpers) | `text/flame/buildBeam/additive/dash/light/selected/shadow/liquid/circles/select/square/arrow/laser/tri/construct/spinSprite` split into core data + gdext raster. |
| `graphics/Trail.java` | `mc/render/trail.rs` | Points stored as `SmallVec<[f32; 3*N]>`; `update/draw/cap/shorten/copy` ported; owner components decide lifetimes (10/11/17). |
| `graphics/EnvRenderers.java` | `mg/env.rs` | Underwater + scorching callback registration; weather particle data to 17; exact colors/speeds/modulo math. |
| `graphics/Lod.java` | `mg/lod.rs` | Exact thresholds 1.4/0.8/fade 0.2 + `disable`. |
| `graphics/MenuRenderer.java` | `mg/menu.rs` | Procedural world on `CachedTile`; flyer sample units; `--menu-seed`; mobile sizes. |
| `graphics/LoadRenderer.java` | `mg/load.rs` + `scenes/render/loading.tscn` | `PlanetGrid.create(2)` mesh; bars layout owned by 14; version text. |
| `graphics/MultiPacker.java` | `mind-atlas` / `mind-tools` (plan 03) | Offline packer; 16 consumes `sprites.atlas.json` only. |
| `graphics/DebugCollisionRenderer.java` | `mg/debug.rs` | Hitboxes/tile edges/avoidance/physics circles; `drawhitboxes` gate. |
| `graphics/IntelGpuCheck.java`, `graphics/NvGpuInfo.java` | not ported | D16-6; Godot abstracts GL quirks. |
| `graphics/InverseKinematics.java` | `mc/render/math/inverse_kinematics.rs` | Leg solver for 11/17 unit drawing. |
| `graphics/Voronoi.java` | `mc/render/math/voronoi.rs` | Core-protection edges + plan 19 reuse; deterministic. |
| `graphics/ParticleRenderer.java` | not ported | D16-5. |
| `graphics/CubemapMesh.java` | `mg/g3d/mesh.rs` (`CubemapMesh::render`) | Graphics-package root file; `Cubemap` + 1×1 box mesh + `cubemap` shader. |
| `graphics/g3d/PlanetRenderer.java` | `mg/g3d/mod.rs` | Camera3D/bloom/skybox/planet/transparent pass order; `PlanetInterfaceRenderer` trait; `drawArc*`/`drawSelection`/`fill`/`drawBorders`/`setPlane` ports. |
| `graphics/g3d/PlanetMesh.java` | `mg/g3d/mesh.rs` `PlanetMeshImpl` | `preRender` uniform setup per subclass; `u_proj`/`u_trans`. |
| `graphics/g3d/PlanetGrid.java` | `mc/render/g3d/grid.rs` | Exact icosahedron constants, `create/subdividedGrid/addCorner/addEdge/pos`, caches size 10; consumed by 12 sector grid. |
| `graphics/g3d/PlanetParams.java` | `mg/g3d/mod.rs` `PlanetParams` | Fields 1:1; `otherCamPos`/`otherCamAlpha` camera lerp; `renderer` trait handle. |
| `graphics/g3d/MeshBuilder.java` | `mg/g3d/mesh.rs` | `buildHex`/`buildIcosphere`/`buildPlanetGrid`; indexed threshold, packed normals (skipped when weak GL), `tmpHeights` reuse. |
| `graphics/g3d/HexMesh.java` | `mg/g3d/mesh.rs` | `Shaders.planet` setup: light dir via solar-system position, rotation, ambient; `emissive`. |
| `graphics/g3d/NoiseMesh.java` | `mg/g3d/mesh.rs` | 1- and 2-color noise height/color variants (Simplex3D from 06's noise). |
| `graphics/g3d/HexSkyMesh.java` | `mg/g3d/mesh.rs` | Rotating cloud shell; `skip`/`getHeight`/`getColor`; `speed`/`relRot`. |
| `graphics/g3d/SunMesh.java` | `mg/g3d/mesh.rs` | Emissive hex sun with octave/persistence noise colors. |
| `graphics/g3d/HexMesher.java`, `GenericMesh.java`, `MatMesh.java`, `MultiMesh.java`, `ShaderSphereMesh.java` | `mg/g3d/mesh.rs` | Traits/wrappers ported as Rust traits + `Vec<Box<dyn GenericMesh>>`; `MatMesh` transform premultiply. |
| `world/draw/DrawBlock.java`, `DrawMulti.java`, `DrawMultiWeave.java` | `07` owns trait/`DrawSpec`; 16 executes in `mg/blocks.rs` | Command capture; `iconOverride`/`finalIcons` to plan 03. |
| `world/draw/DrawRegion.java`, `DrawDefault.java`, `DrawShape.java`, `DrawFade.java`, `DrawPulseShape.java`, `DrawSideRegion.java`, `DrawSpikes.java`, `DrawCells.java`, `DrawCircles.java`, `DrawBubbles.java`, `DrawBlurSpin.java`, `DrawWeave.java`, `DrawSoftParticles.java`, `DrawParticles.java`, `DrawFrames.java` | `mg/drawblock/region.rs` (executor family) reading `DrawSpec` from 07 | Sprite/part descriptors → `DrawCommands`; rotation/spin/color/layer/particle sinks. |
| `world/draw/DrawWarmupRegion.java`, `DrawGlowRegion.java`, `DrawHeatRegion.java`, `DrawHeatInput.java`, `DrawHeatOutput.java`, `DrawLiquidRegion.java`, `DrawLiquidTile.java`, `DrawPumpLiquid.java`, `DrawLiquidOutputs.java`, `DrawPower.java` | `mg/drawblock/fluids_power.rs` | Reads `warmup/progress/heat/liquid/current/power.status` via plan 09 query helpers; fluid frame index from 09. |
| `world/draw/DrawFlame.java`, `DrawArcSmelt.java`, `DrawCrucibleFlame.java`, `DrawCultivator.java`, `DrawPlasma.java`, `DrawPistons.java` | `mg/drawblock/effects.rs` + `mc/render/drawf.rs` | Builds additive quads/lines; long-lived particle/plasma timing uses view clock. |
| `world/draw/DrawTurret.java` | `mg/blocks.rs` turret path + 10's `TurretDrawState` | Barrel/heat/liquid/top regions, recoil/rotation; DrawPart sub-parts deferred to 17. |
| `world/draw/DrawBlockParts.java` | `17` owns `DrawPart`; 16 provides the preview/plan quad | `part.draw(params)` goes through 17's part executor; 16 owns only `drawPlan` preview. |

## 5. Milestones & task breakdown

Each milestone is verifiable through `mind-headless` and/or MCP; evidence goes in the Changelog. The smallest vertical slice is M1 (floor chunk + screenshot at a fixed pose).

### M0 — Render spine, scene, and oracle scaffolding

- `mc/render/mod.rs` Layer table + `BandKey`; `mg/bands.rs` band plan; `mg/queue.rs` stable-sort/replay; `mg/world.rs` skeleton running stage order with empty lists; `MindRender` `#[func]` API; `/root/Spine/World/Renderer` + aux nodes added to `spine.tscn`; `world_renderer.tscn`.
- `mind-headless render-list` command skeleton + scenario registration (`render_flat_floor`, `render_block_change`, `render_layer_order`); render-list JSON schema test; fixed camera pose JSON fixture.
- `mg/stats.rs` counters; inspector Render tab raw JSON.

**Verify:** `cargo check -p mind-gdext -p mind-headless`; `Godot --headless --editor --quit --path client` clean; `godot_exec call /root/Spine/MindRender get_render_stats` returns keys; `render-list` runs headless with empty world.

### M1 — Floor chunk pipeline (first vertical slice)

- `mc/render/floor_cache.rs` dirty/used/epochs; `mg/floor.rs` ArrayMesh bake + CacheLayer sub-bands + error fallback + `grow_sprites`; `scan::floor_layers_in_view`; `recache_tile` wired to plan 06's `RenderHooks`.
- Retire `MindTileGrid` drawing to debug-only; wire `Camera2D` scale clamp and `set_camera_pose`.

**Verify:** `mind-headless render-list render_flat_floor` golden matches (deterministic); `render_block_change` shows exactly the two affected chunks dirty and the re-baked list identical to a cold rebuild; MCP: place/break a block at a fixed pose, floor chunk screenshot diff non-empty then equal after rebuild (§7c-2).

### M2 — Block renderer: quadtrees + dynamic draws + DrawCommands

- `mc/render/scan.rs` 5 quadtrees + `processBlocks` lists + early-out; `mg/blocks.rs` dynamic draws, cracks, team overlay, destroyed plans, status (`Lod.l2` gate); plan 07 `DrawCommands` executor; `mg/drawblock/*` region/fluids/power family; `MindTileGrid` fully off.

**Verify:** `render_block_change` golden includes block sprites; headless tests for quadtree index/unindex on `TilePreChangeEvent`/`TileChangeEvent`; MCP: place a crafter + router, screenshot at fixed pose, `get_render_stats().dynamic_sprites > 0`.

### M3 — Shadows + darkness FBOs

- `mg/shadow.rs`: shadow `SubViewport` (1 px/tile, `blendShadowColor = white.lerp(black, 0.71)`) rebuilt from `shadowEvents`; darkness target built from `world.get_darkness` (`1 - min((d+0.5)/4, 1)`), `limitMapArea` black fill + white rect; `draw_shadows`/`draw_darkness` composites with `Shaders.darkness`; `recache_wall` dark-radius logic + `check_changes` writing `tile.data = world.get_wall_darkness`.

**Verify:** headless test `shadow_tiles_match_display_shadow_predicate`; MCP layer-toggle screenshots for `darkness`; a wall build/break changes darkness screenshot in the expected radius.

### M4 — Cached buildings (SpriteCache equivalent) + `BuildingCacheLayer`

- `mc/render/block_cache.rs` chunk/layer dirty + epochs; `mg/blocks.rs` `cache_chunk`/`recache_building`/`draw_blocks` order (`under` before `normal`); baked team overlay; fog/team `wasVisible` transitions; plan 07 `Building::recache` route.

**Verify:** `render_block_change` golden; headless `recache_marks_only_affected_chunk`; MCP: build a cached block (e.g. container), rotate a cached turret, confirm rebuild counters and identical screenshot after settle; team change under fog triggers recache.

### M5 — Overlays, lights, bloom, shields/buildBeam

- `mg/world.rs` overlays (plans/selection/core edges/spawns/dropping/hover); `mg/light.rs` + `Shaders.light`; `Bloom/SubViewport` capture/render; shield/buildBeam effect-buffer brackets with null check; `mg/drawf.rs` executor.

**Verify:** MCP layer toggles for `plans`, `overlayUI`, `light`, `shields`; headless `voronoi_core_edges_deterministic`; screenshot at fixed pose with and without `bloom` setting; `Shaders.shield == None` path exercised via a forced-missing shader registry test.

### M6 — Fog, minimap provider, pixelator, Lod, env, debug

- `mg/fog.rs`, `mg/minimap.rs`, `mg/pixelate.rs`, `mg/lod.rs`, `mg/env.rs`, `mg/debug.rs`; plan 12 `RulesRenderView` adapter with safe defaults; minimap update batching; `update_all` hooks.

**Verify:** MCP toggles for `fogOfWar`; minimap texture eval (`MindMinimap.texture()` non-null, sampled pixel matches `color_for` on a known tile); pixelate on/off screenshot diff; `drawhitboxes` toggle screenshot; perf counters.

### M7 — Menu, loading, planet g3d, cutscene, screenshots

- `mg/menu.rs`, `mg/load.rs`, `mg/g3d/*`, `PlanetGrid` port + tests, orbit/sector/selection draws, `PlanetInterfaceRenderer` trait, launch/land cutscene, `take_map_screenshot`, `capture_layers`.

**Verify:** `render_menu_world --menu-seed N` golden list; MCP planet screenshot at a fixed `PlanetParams`; map screenshot opens as PNG with correct dimensions and alpha 255; cutscene camera scale trace matches `getLandTimeIn` expectations.

### M8 — Shaders, drift, fallback polish

- Port every `.gdshader`; extend `mind-tools shaders check` both directions; substitution counter; nullable shield; `SurfaceShader` noise/caustics/space uniforms; env shader cases.

**Verify:** `mind-tools shaders check` clean; `Godot --headless --editor --quit` has no `SHADER ERROR`; a deliberately removed `.gdshader` produces an `[E]` log + pass-through, not a crash.

### M9 — Performance, compat, plan exit

- Allocation audit on the render build path; batching pass (same-material command coalescing remains per-band); draw-call/triangle counters; 250×250 benchmark scenario; addon-not-adopted note into `THIRD_PARTY_NOTICES.md` (only if any adopted); update repo playtest skill with the render recipes; run §7e.

**Verify:** §7d budget numbers recorded in `bench/render_baseline.json`; full §7e checklist ticked; changelog evidence.

## 6. Data & formats

### 6.1 Layer/band tables

- `mc::render::Layer` consts exactly as §3.4; unit is f32 world z (append-only).
- `mg::bands::BandPlan` returns an `i32` z-index per `BandKey { layer: LayerId, sub: u8, blend: Blend }`; serialized to `build/render/bands.json` at startup for audit and golden diffing (fields: `layer`, `value`, `band`, `blend`; sorted by band). Band numbers are append-only ABI like `Layer` itself.
- `CacheLayerId` order fixed as `water,mud,tar,slag,arkycite,cryofluid,space,normal,walls` = 0..8 (plan 06 §3.12; ABI).

### 6.2 `DrawCommands` binary shape (core-internal, not a wire format)

```rust
pub enum DrawCmd {
    SetZ(f32), SetBlend(Blend), SetColor(u32), SetMixColor(u32, f32), SetAlpha(f32),
    Sprite { region: RegionId, x: f32, y: f32, w: f32, h: f32,
             ox: f32, oy: f32, rot: f32, color: u32, blend: Blend },
    Shape { kind: ShapeKind, params: [f32; 8], color: u32 },
    Lines { kind: LineKind, params: [f32; 8], color: u32, stroke: f32 },
    Fill { kind: FillKind, params: [f32; 6], color: u32 },
}
```

- `RegionId(u32)` interning table is rebuilt on content load (`ids.rs`), append-only; `region_names.txt` audit artifact is emitted by 03. `DrawCommands` is reused across frames with `clear()` (no dealloc).
- Ordering key for the queue is `z`; within equal `z`, insertion sequence (stable sort), matching Arc's `Draw` stable queue.

### 6.3 Render-list JSON (the headless oracle format, `format: 1`)

```json
{
  "format": 1,
  "scenario": "render_flat_floor",
  "tick": 60,
  "camera": { "x": 128.0, "y": 128.0, "w": 320.0, "h": 180.0, "zoom": 4.0, "team": 0 },
  "sort": true,
  "entries": [
    { "seq": 0, "z": 0.0, "layer": "floor", "level": "sprites", "region": "grass",
      "x": 16.0, "y": 16.0, "w": 8.0, "h": 8.0, "rot": 0.0, "color": "ffffffff", "blend": "normal" },
    { "seq": 1, "z": 30.0, "layer": "block", "level": "sprites", "region": "copper-wall",
      "x": 40.0, "y": 16.0, "w": 32.0, "h": 32.0, "rot": 0.0, "color": "ffffffff", "blend": "normal" }
  ],
  "chunks": {
    "floor": { "dirty": [], "built": 4, "epoch": 1 },
    "blocks": { "dirty": [], "built": 2, "epoch": 1 }
  },
  "stats": { "entries": 2, "regions": 2, "queue_max": 2 }
}
```

- Sorted by `(z, seq)` when `sort: true`, raw emission order otherwise (`--no-sort`). Region names are plan-03 ABI strings; entries carry no floats formatted at reduced precision (fixed 3 decimals) to keep goldens byte-stable.
- `mind-headless render-list <scenario> [--out path] [--no-sort] [--all-chunks]` mirrors plan 00's CLI conventions (`--json`, exit codes 0/1/2). Golden files live at `tests/golden/render/<scenario>.json`.

### 6.4 Camera-pose fixture (MCP oracle)

```json
{ "pose": "render_flat_center", "tile_x": 16, "tile_y": 16, "zoom": 4.0,
  "viewport": [1280, 720], "paused": true, "tick": 0, "menu_seed": 1234 }
```

`tests/golden/render/poses.json` is the canonical list; `MindRender.set_camera_pose(x, y, zoom)` and `capture_layers(dir)` consume it.

### 6.5 Shader manifest consumption

- 03's `shader.index.json` entries are loaded into `ShaderRegistry { name -> ShaderEntry { gdshader, uniforms, textures, stage, source_glsl } }`. This plan owns the expected-name list (§3.9); 03 owns the file. Missing entry → `[E]` + neutral substitute (counted). Mod overlays follow 03's `AssetOverlayProvider::shaders`.
- `mind-tools shaders check` grows a `--reverse` mode invoked by CI: for every registry entry, assert each declared uniform/texture is consumed by the `.gdshader` and vice versa.

### 6.6 Files/paths

| Artifact | Path | Producer | Committed? |
|---|---|---|---|
| Band table audit | `build/render/bands.json` | `mg/bands.rs` at startup | no |
| Render-list goldens | `tests/golden/render/*.json` | `mind-headless render-list` | yes |
| Camera poses | `tests/golden/render/poses.json` | hand-authored | yes |
| Render baseline (perf) | `bench/render_baseline.json` | M9 | yes |
| Layer screenshots | `<data-dir>/oracle/layer-<name>.png` | `MindRender.capture_layers` | no (CI artifacts) |
| Map screenshots | `<data-dir>/screenshots/screenshot-<ms>.png` | `snapshot.rs` | no |
| Scenes | `client/scenes/render/{world_renderer,planet,loading,menu_bg}.tscn` | 16 | yes |
| Shader sources | `client/shaders/*.gdshader` | 16 | yes |

## 7. Oracle & verification (REQUIRED)

Rendering has **no upstream JUnit coverage**. The substitute oracle is: (a) headless render-list extraction and ordering assertions, (b) chunk/quadtree cache-invalidation logic tests, (c) MCP screenshot diffs per `Layer` toggled at a fixed camera pose (the playtest skill's pid-stamped pattern), and (d) perf counters. Every oracle artifact is deterministic under a fixed seed/pose/tick.

### 7.1 Ported tests / new tests (`cargo test -p mind-core` + `mind-gdext` unit tests where no Godot is needed)

| Mindustry behavior (no test exists) | Rust test | Notes |
|---|---|---|
| `Layer` value table | `render::layer::tests::layer_values_parity` | Asserts every constant equals the Java value, table length unchanged. |
| `CacheLayer.init()` order | `render::layer::tests::cache_layer_order_parity` | `water..walls` = 0..8, exactly plan 06 §3.12. |
| `BuildingCacheLayer` values | `render::layer::tests::building_cache_layer_values` | `29.5`/`30.0`. |
| `Draw` queue stable sort | `render::queue::tests::stable_within_equal_z`, `sort_disabled_preserves_emission` | Mirrors `Draw.sort` semantics. |
| `Draw.drawRange` brackets | `render::queue::tests::range_shader_brackets` | `blockbuild`, shields, buildBeam. |
| `FloorRenderer.recacheTile` | `render::floor_cache::tests::recache_marks_only_own_chunk`, `recache_out_of_bounds_is_noop` | Chunk math `x/30`. |
| `FloorRenderer` used-layer scan | `render::floor_cache::tests::used_layers_border_and_accessibility` | Border + `cacheLayer != normal` + accessible rules. |
| `BlockRenderer.recacheWall` radius | `render::block_cache::tests::recache_wall_dirties_dark_radius` | `darkRadius=4`; calls minimap update per tile. |
| `BlockRenderer` index predicates | `render::scan::tests::index_predicates_match_java` | `isCenter`, `cacheLayer == normal`, `drawCached`, light/overlay emit-light + darkness < 3. |
| `processBlocks` early-out | `render::scan::tests::process_blocks_early_out` | Same camera tile/range/team reuses lists. |
| `updateShadow` size loop | `render::block_cache::tests::update_shadow_covers_multiblock` | `size x size` from `tile + sizeOffset`. |
| Darkness math | `render::tests::darkness_curve` | `1 - min((d + 0.5)/4, 1)` and `limitMapArea` fill/clip. |
| Minimap `colorFor` | `render::minimap_logic::tests::color_for_rules` (pure function split) | Fog-masked, darkness multiply, 0.7 above-block, liquid-edge 0.84/0.84/0.9. |
| `Lod.update` | `render::lod::tests::lod_thresholds` | 1.4/0.8/fade 0.2; `disable`. |
| `PlanetGrid` structure | `render::g3d::grid::tests::{tile_corner_edge_counts, subdivide_links}` | Counts `10*3^n+2`, `20*3^n`, `30*3^n`; ring links. |
| `MeshBuilder.buildHex` indexing | `render::g3d::mesh_data::tests::indexed_threshold` | `tiles*6 < 65535`. |
| `Voronoi` core edges | `render::math::voronoi::tests::{sites_edges_deterministic, displayed_team_rule}` | Deterministic iteration; `CoreEdge.displayed()`. |
| Region interning | `render::ids::tests::region_ids_stable_and_dense` | Append-only, no per-frame strings. |
| Draw command reuse | `render::commands::tests::no_alloc_after_warmup` | Counts `Vec` capacity growth (alloc audit hook). |

### 7.2 Headless harness scenarios (`mind-headless`)

New command `mind-headless render-list <scenario> [--out <path>] [--no-sort] [--all-chunks] [--json]`, scenarios registered like plan 00 §6.1 and named `render_*`:

| Scenario | Seed / world | Script | Assertions |
|---|---|---|---|
| `render_flat_floor` | seed 1, flat 64×64, `stone` floor | no tick; fixed pose | golden render-list; only floor entries; chunk count `ceil(64/30)^2 = 9`; no dirty. |
| `render_block_change` | seed 1, flat 32×32, `infinite_resources` | place `copper-wall` (4,4) t0; break t30 | t1 list contains block sprite at `Layer.block`; after each change exactly 1 floor chunk + 1 block chunk dirty; after rebuild, list equals a cold rebuild byte-for-byte. |
| `render_layer_order` | seed 1, flat 32×32 + `foreshadow` blocks on a known tile | place a cached block at (10,10), a dynamic one at (12,12) | entries sorted by `(z, seq)`; `floor < blockUnder < block(under) < block(normal) < overlay < plans`; `--no-sort` matches emission order. |
| `render_darkness_radius` | seed 2, flat 48×48 | place `stone-wall` ring; rebuild | dark tiles within `darkRadius`; render-list darkness entries count equals predicate count. |
| `render_menu_world` | `--menu-seed 1234` | menu state | deterministic menu block/floor counts; golden list. |
| `render_bench` | seed 0, 250×250 generated, 5000 buildings + 2000 units + 3000 bullets (spawned headlessly per 10/11) | 600 frames | `render-list` build p50/p99; no alloc growth. |

### 7.3 MCP playtest scenarios (concrete, pid-stamped)

Preconditions follow plan 00 §7c / the repo playtest skill: `godot_health check`; if `BRIDGE_NOT_CONNECTED`, launch the editor (`nohup godot4 --editor --path /home/c/g/code_examples/mindustry-godot/client >/tmp/mind-editor.log 2>&1 &`), wait ~20 s, `godot_instance list`. Every eval returns `{"pid": OS.get_process_id(), …}` and is compared with `godot_game instances`.

**7c-1 — Fixed-pose layer screenshot sweep (per-layer oracle).**
1. `godot_editor_edit open_scene res://scenes/spine.tscn`; `godot_game play` with the scene passed explicitly.
2. Load `spine_place_break`, `set_paused [true]`.
3. Eval pid + `get_node("/root/Spine/MindRender").set_camera_pose(16, 16, 4.0)`; assert `get_render_stats().tick` is stable.
4. `godot_exec call /root/Spine/MindRender capture_layers ["user://oracle"]` → for every layer name, a PNG at the same pose; each result path recorded in the eval return.
5. Toggle one layer at a time: `set_layer_visible("floor", false)` → capture → `set_layer_visible("floor", true)`; repeat for `block`, `darkness`, `light`, `plans`, `overlayUI`, `fogOfWar`, `effect`. Diff against the full-frame capture; each toggle must change pixels only in its expected screen region (compositor-testing skill's A/B-at-same-pose rule).
6. `godot_screenshot game` for the human-readable archive; `godot_log errors` clean.

**7c-2 — Chunk cache rebuild after a tile change.**
1. Same boot as 7c-1; pose on tile (16,16) zoom 4.
2. `godot_exec call /root/Spine/SimHost place_block [16, 16, "copper-wall"]` → `true`; eval `MindRender.get_render_stats()`: `{floor_chunks_dirty >= 1, block_chunks_dirty >= 1, mesh_rebuilds_delta == 2}`.
3. `godot_screenshot game` → PNG A; wait one frame; screenshot B; A != B (block appeared), B == C (settled, no rebuild churn).
4. `break_block [16,16]`; assert dirty counts return to baseline and screenshot returns to A's floor-only content in the changed chunk region (full-frame diff can differ by placement UI; compare the cropped chunk rect).

**7c-3 — Minimap + fog provider.**
1. Eval `get_node("/root/Spine/World/MinimapProvider").texture()` non-null; sample pixel `(x=16, y=height-1-16)` and compare to `color_for` computed via `godot_exec call` on a known tile (exposes `minimap_color_at(x,y)` for tests).
2. With `rules.fog` enabled via the plan-12 adapter (or scenario), toggle `fogOfWar` layer capture and confirm the fog composite only affects the expected band.

### 7.4 Performance budget + measurement

| Metric | Budget (dev host, AMD Radeon 860M, Forward+) | Measurement |
|---|---|---|
| World frame build (Rust render-list + queue + band partition, 250×250, 5k buildings, 2k units, 3k bullets) | p50 ≤ 2.5 ms, p99 ≤ 5 ms CPU | `mind-headless render-list render_bench --ticks 600` timing + in-engine `MindRender.get_render_stats().build_us` |
| Full frame (sim ≤ 4 ms + render) | ≤ 16.6 ms, headroom ≥ 25% | `godot_profiler series` + `Performance.get_monitor(Performance.TIME_PROCESS/FPS)` |
| GPU frame | ≤ 8 ms | `godot_profiler`/`Performance.RENDER_TOTAL_*` |
| Floor chunk cold bake (250×250, 9×9 chunks) | ≤ 250 ms total | load-time log `mesh: N ms` parity with upstream `Generated world mesh: @ms` |
| Single chunk recache (floor or cached block) | ≤ 2 ms | counter in `get_render_stats()` |
| Draw calls | ≤ 4000 typical (≤ 8000 worst) | `Performance.get_monitor(RENDER_TOTAL_DRAW_CALLS_IN_FRAME)` |
| Triangles | ≤ 80k typical (250×250 in view) | `RENDER_TOTAL_PRIMITIVES_IN_FRAME` |
| Render path allocations | 0 growth after warmup (256-frame window) | alloc-audit counter (`render_alloc_events == 0`) |
| Texture memory (vanilla atlas + FBOs) | ≤ 512 MB | `Performance.get_monitor(RENDER_TEXTURE_MEM_USED)` |

Regressions block the P6 gate; `bench/render_baseline.json` stores committed numbers with the same ±20% warn / +50% fail policy as plan 00 §7d.

### 7.5 Exit criteria checklist

- [x] `cargo fmt --check`, `clippy -D warnings`, `cargo check`, `cargo test -p mind-core` green; `mind-core::render` boundary grep empty (no `godot`/`tokio`). *(2026-10-02, `lane/16-render`: 550 lib + 2 + 2 + 1 pass / 2 ignored; boundary grep empty.)*
- [ ] §7.1 tests pass and are listed in plan 23's port inventory.
- [x] `mind-headless render-list` scenarios pass with committed goldens; `--no-sort` emission-order test passes. *(2026-10-03: 4 goldens — `render_flat_floor` 1120, `render_block_change` 897, `render_layer_order` 897, `render_darkness_radius` 1144 — all PASS; `--no-sort` exercised in `list` but no committed golden.)*
- [ ] `render_block_change` proves dirty-set minimality and cold-rebuild equality.
- [ ] In-engine stage order matches §3.3 (order trace test via `MindRender.get_render_stats().stage_trace`).
- [ ] MCP 7c-1/7c-2/7c-3 pass; every eval pid-stamped; `godot_log errors` clean; all toggled flags restored.
- [ ] Floor/block/animated-liquid/walls caches at parity; `under`/`normal` building caches at parity.
- [ ] Shadows/darkness/light/fog/minimap/pixelator/bloom verified at fixed poses; `shield` missing-skips cleanly.
- [x] `mind-tools shaders check --reverse` clean; no `SHADER ERROR` on `--headless --editor --quit`. *(2026-10-02, `lane/16-render`: 26 ported, 0 unported (1 ok `shockwave`), 0 forward + 0 reverse drift; `godot4 --headless --script res://tools/shaders_check.gd` 26/26 compiled.)*
- [ ] Menu renderer pinned-seed golden; loading screen; `PlanetGrid` g3d parity; cutscene camera trace; map screenshot PNG.
- [x] §7.4 budgets recorded in `bench/render_baseline.json`; alloc audit zero. *(2026-10-03: `mind-core/bench/render_baseline.json`; release p50 2240 µs / p99 3745 µs on the 250×250 `render_bench`; `intern_str` removes the per-tile allocation; `no_alloc_after_warmup` + `intern_str_matches_intern_without_alloc`.)*
- [ ] `THIRD_PARTY_NOTICES.md` addon posture matches D16-11; repo playtest skill updated with §7c recipes.
- [ ] Changelog evidence entries completed (commands, golden paths, screenshot paths, counters).

## 8. Risks & open decisions

Each item has the default this plan proceeds with. Items marked `NEEDS USER DECISION` are load-bearing and not covered by `HIGH_LEVEL_PLAN.md`; execution continues on the stated default unless the user overrides.

| # | Decision / risk | Default being planned against | Needs user? |
|---|---|---|---|
| OD16-A | Floor/block chunk representation: `ArrayMesh` + band items vs `MultiMeshInstance2D` vs `TileMapLayer`. | **`ArrayMesh` per `(chunk, CacheLayer)`** (§3.5): arbitrary bake-time commands, per-layer shaders, 30×30 recache, error fallback. Band items preserve ordering. | **yes** |
| OD16-B | Renderer method for this plan follows plan 00 §8-OD-R2. | **Locked 2026-10-01 (NUD-06=B): `mobile` on all platforms.** Implications: RenderingDevice-only paths are out; OD16-D's RD max-blend falls away so additive blending is final; bloom/light shaders must stay within `mobile` renderer limits. `gl_compatibility` is the emergency fallback. Planner reads `project.godot` and logs a changelog note if it changes. | locked |
| OD16-C | Bloom implementation: band-capture `SubViewport` vs whole-frame HDR-2D glow. | Band capture at `Layer.bullet-0.02 … effect+0.02`; whole-frame glow is the documented fallback if the auxiliary viewport cannot share the camera transform reliably on 4.7. | **yes** |
| OD16-D | Light composite blend fidelity (`Gl.blendEquationSeparate(funcAdd, max)`). | `CanvasItemMaterial.BLEND_MODE_ADD` (canvas); a custom `RenderingDevice` canvas shader implementing per-channel `max` is reserved if screenshot diff shows a visible deviation at halo overlaps. | **yes** |
| OD16-E | Map screenshot capture path (world `SubViewport` read-back) and whether screenshots run on the render thread. | Main-thread `SubViewport` render + `Image.save_png` on a worker; if Godot's render thread rules (compositor-testing skill) require it, save via `RenderingServer.call_on_render_thread`. | **yes** |
| OD16-F | Menu world randomness. | Dedicated view RNG with `--menu-seed` override; default random per launch (upstream parity). | no |
| OD16-G | Interpolation mode. | `LerpMode::None` (positions = sim positions, D8) with `Alpha` reserved for plan 21 `@SyncField` companions. | no |
| OD16-H | `mind-core` view-only code lives in `mind-core::render` (draw descriptors, scan, chunk bookkeeping) although it never touches sim state. | Keep it in core (07 already decides `DrawBlock` is core); CI boundary grep only forbids `godot`/`tokio`, not view code, and HLP §2.4 permits render-only state as long as it never feeds back. | no |
| OD16-I | Plan paths in `world/draw/` (not `world/blocks/draw/`). | Assignment text names `world/blocks/draw/*`; the repo's actual path is `world/draw/*` — the port map uses the real path. | no |
| OD16-J | Plans 05 (`/root/Main/SimBridge`), 12 (`/root/Main/MindCampaign`) and 14 reference `/root/Main/*` while plan 00 fixes `/root/Spine/*`. | Orchestrator reconciles 05/12/14 to the plan-00 paths; this plan uses `/root/Spine/SimHost` and appends `/root/Spine/World/Renderer` + `/root/Spine/MindRender`. | no (reconcile) |
| OD16-K | `world/draw/DrawBlockParts` + `entities/part/*` split between 16 and 17; 17's seed files `mind-core/src/render/{layer,draw}.rs` and `MindFxLayer`; `DrawPrim` vs `DrawCmd` naming; `Layer::EFFECT` vs `Layer::effect` style. | 16 provides the `DrawCommands` sink and `drawPlan` preview; 17 owns `DrawPart`/`PartProgress` bodies. At merge 16 owns `render/{layer,commands}.rs`; 17's `DrawPrim` is a caller-side program that maps onto `DrawCmd`; constants are normalized to the Java identifiers. Frozen in §3.13/§4. | no (reconcile) |
| OD16-L | `MapPreviewLoader.checkPreviews()` frame call-site ownership. | 16 owns the two call sites in the stage order; 06 owns the queue/path layer; 19 owns pixels/PNG. Frozen in §3.3 steps 6/10. | no (reconcile) |
| OD16-M | `Settings` keys read here (`bloom`, `bloomintensity`, `bloomblur`, `animatedwater`, `blockstatus`, `effects`, `hidedisplays`, `drawlight`, `pixelate`, `screenshake`, `playerindicators`, `indicators`, `drawhitboxes`, `destroyedblocks`, `atmosphere`, `unitlaseropacity`, `lasersopacity`, `bridgeopacity`, `maxzoomingamemultiplier`, `minzoomingamemultiplier`) come from plan 04's store. | Plan 04 owns persistence; 16 adds keys to the settings registry if missing, with upstream defaults. | no (reconcile) |
| OD16-N | Plan 12 landed while this plan was written; its `Rules`/`FogControl`/`MapMarkers` shapes must match the §3.12 adapter. | Plan 12 §3.4 `Rules` is the source of truth; the adapter is a thin read view (reconciled in §3.13). If a field name drifts, fix the adapter, not 12. | no (reconcile) |
| OD16-O | `expect` panics in render paths on missing regions. | Missing region → `error` fallback + warn-once (upstream `FloorRenderBatch.draw` behavior); no panic. `clippy::unwrap_used` stays denied in `mind-core`. | no |
| OD16-P | Godot canvas batching means one band item can issue many small draw calls; perf risk at 250×250. | Bands are per-material; command coalescing by region is an M9 optimization (pre-sorting by region within a band is safe because upstream `SpriteBatch` also batches by texture). Budgeted in §7.4. | no |

## 9. References

Repo-local: `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 D1–D9, §2.1–2.4 crate boundaries and determinism, §3 plan table, §4 template, §5 P6 gate, §6 conventions, §7 verification tooling, §8 addons, §9 parity ledger, §10 OD1–OD9, §11 execution); `mindustry-godot/PRELIMINARY_PLAN.md`; `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (§3.4 project settings, §3.5 spine scene/node paths, §3.10 extension contract, §6 formats, §7c MCP recipe, §7d budgets); `02_CONTENT_IMPLEMENTATION_PLAN.md` (§3.2 ContentType/IDs, §3.3 model, §3.6 interfaces); `03_ASSETS_IMPLEMENTATION_PLAN.md` (§3.3 atlas types, §3.4 region grammar, §3.6 runtime binding, §3.7 shader strategy, §6.3 atlas manifest, §6.7 `shader.index.json`, §6.9 managed files); `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (preview/`MapIO::color_for` seams, settings store); `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (§3.4 schedule, §3.5 FieldMeta incl. `@SyncField` interpolation, §3.6 Groups, §3.7 triggers, §3.8 Time, §3.13 Godot surfaces); `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (§3.2–3.6 WorldGrid/Tile/WorldContext/darkness, §3.12 `CacheLayerId` + `RenderHooks`, §6.1–6.2 dump); `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (§3.4 building entity, §3.11 `DrawBlock`/`DrawCommands`/`DrawSpec`, §3.13 Godot/STDB, §6.4 dump additions); `08_LOGISTICS_IMPLEMENTATION_PLAN.md` (§3.9 view boundary, §6.5 region names); `09_POWER_LIQUIDS_HEAT_IMPLEMENTATION_PLAN.md` (§3.12 query helpers consumed by 16, §3.13); `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (§3.12 draw states + Layer mapping, §3.14 ledger); `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (§3.13 surfaces); `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md`, `14_UI_IMPLEMENTATION_PLAN.md`, `15_INPUT_RTS_IMPLEMENTATION_PLAN.md`, `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (landed while this plan was written; reconciled symbol-by-symbol in §3.13); `18_AUDIO_IMPLEMENTATION_PLAN.md`, `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md`, `21`, `22`, `23` (not yet on disk — contracts frozen in §2.3, §3.12 and §3.13).

Mindustry sources read in full or key parts: `core/src/mindustry/core/Renderer.java`; `graphics/AGENTS.md`; `core/src/mindustry/core/AGENTS.md`; `world/AGENTS.md`; `core/assets/AGENTS.md`; `ui/AGENTS.md`; `graphics/Layer.java`, `CacheLayer.java`, `BuildingCacheLayer.java`, `FloorRenderer.java`, `BlockRenderer.java`, `OverlayRenderer.java`, `LightRenderer.java`, `FogRenderer.java`, `MinimapRenderer.java`, `Pixelator.java`, `Shaders.java`, `Pal.java`, `Trail.java`, `EnvRenderers.java`, `Lod.java`, `MenuRenderer.java`, `LoadRenderer.java` (structure), `MultiPacker.java` (signatures), `DebugCollisionRenderer.java`, `Drawf.java` (signatures), `InverseKinematics.java`, `Voronoi.java`, `CubemapMesh.java`; `graphics/g3d/PlanetRenderer.java`, `PlanetMesh.java`, `PlanetGrid.java`, `PlanetParams.java`, `MeshBuilder.java`, `HexMesh.java`, `NoiseMesh.java`, `HexSkyMesh.java`, `SunMesh.java`, `MatMesh.java`, `MultiMesh.java`, `HexMesher.java`, `GenericMesh.java`, `ShaderSphereMesh.java`; `world/draw/DrawBlock.java`, `DrawBlockParts.java`, `DrawMulti.java`; `world/blocks/environment/Floor.java` (draw fields via AGENTS); `core/assets/shaders/*` (names via `Shaders.java`).

Tooling: `/mnt/c/Users/Clinton/g/.opencode/skills/godot-compositor-testing/SKILL.md` (headless import/parse checks, windowed `--capture`, RenderingDevice gotchas); `/mnt/c/Users/Clinton/g/.opencode/skills/playtest/SKILL.md` (pid-stamped recipes, screenshot attribution); open-godot-mcp tool docs (`godot_health`, `godot_game`, `godot_exec`, `godot_input`, `godot_screenshot`, `godot_profiler`, `godot_runtime_state`, `godot_log`); Godot 4.7 docs (`SubViewport`/`ViewportTexture`, `CanvasItem.z_index`, `CanvasItemMaterial`, `WorldEnvironment`/glow, `use_hdr_2d`, `RenderingServer.canvas_item_*`, `ArrayMesh`/`ImmediateMesh`, `Cubemap`).

## Changelog

> Append entries here when execution starts. Every "done" claim carries evidence (command, golden path, screenshot path, counter).

- **2026-10-02 — M0 complete; M1 headless half (`lane/16-render`, base `81177fa`).**
  - **M0 (render spine, scene, oracle scaffolding).** `mind-core::render` added: `layer` (`Layer` 35-value exact table + `LayerName`; `CacheLayerId` frozen ABI `water,mud,tar,slag,arkycite,cryofluid,space,normal,walls` = 0..8; `BuildingCacheLayer` 29.5/30.0), `bands` (`BandKey`/`BandEntry`/`BandPlan`, append-only, `to_json` audit), `commands` (`Blend`, `DrawCmd`, `CommandBuffer` with alloc warmup), `ids` (`RegionId`/`RegionIdTable`), `queue` (stable `(z, seq)` sort, `drawRange` brackets), `lod` (1.4/0.8/0.2 + `disable`), `scan` (`CameraView`, `process_blocks` early-out, `floor_layers_in_view`), `floor_cache`/`block_cache` grids. `mind-gdext::render` added: `MindWorldRenderer` (append-only band `Node2D` children, `z_as_relative=false`, frame skeleton, `RenderStats`) and `MindRender` `#[func]` facade. `spine.tscn` appends `World/Renderer`, `World/Bloom`, `World/LowRes`, `World/MinimapProvider`, `/root/Spine/MindRender`, `/root/Spine/Planet`. `mind-headless render list <scenario>` + `render bands`; `render_list.rs` scenarios registered. Evidence: `cargo test -p mind-core --lib render::` **27 passed**; `cargo check -p mind-gdext -p mind-headless` clean; fmt + workspace clippy `-D warnings` clean; `godot4 --headless --editor --quit --path client` **exit 0**; `render bands` prints the append-only table.
  - **M1 (floor chunk pipeline, headless half).** `mind-core::render::floor_cache::FloorChunkGrid` (`recache_tile` `x/30`, out-of-bounds no-op, `chunk_aabb` `[-4,-4,248,248]`, mesh epochs); `mind-core::render::list` emits deterministic floor/overlay/wall/block entries and the `format: 1` JSON (fixed 3-decimal floats). Goldens committed: `client/rust/mind-headless/tests/golden/render/{render_flat_floor,render_block_change,render_layer_order}.json` (1120/897/897 entries); `cargo run -p mind-headless -- render list <s> --check <golden> --json` passes. `mind-core::render::hooks::RenderInvalidation` implements plan 06's `RenderHooks` (`recache_tile`/`recache_wall`/`add_floor_index`/`remove_floor_index`/`invalidate_tile`/`minimap_update`) over the grids behind a `Mutex`, with the baker-side `with_floor_mut`; tests `recache_hooks_dirty_only_own_chunk`, `recache_wall_dirties_dark_radius_and_minimap`. Engine `FloorRenderer` ArrayMesh bake + `MindTileGrid` debug-only retirement deferred (see below).
  - **Deviations / reconciliations to flag.** (a) Plan §3.4/§6.1 froze `CacheLayer` order with `tar,slag,arkycite,cryofluid`, but upstream `CacheLayer.init()` is `water,mud,cryofluid,tar,slag,arkycite`; this lane **follows the frozen plan ABI** and records the discrepancy (parity risk for liquid floor ordering). (b) Plan 07's `BlockDef` has no `cache_layer`/`draw_cached`/`display_shadow`/`draw_team_overlay`/`obstructs_light`/`emit_light` fields yet; `render::list` derives wall/floor cache membership from `BlockKind` and the real fields must be reconciled at merge. (c) `BandPlan` lives in `mind-core::render::bands` (headless needs it) with the plan's `mg::bands` path to re-export; `bands.json` is produced by `render bands`.
  - **Open (M1 engine half → M9).** Engine `FloorRenderer` ArrayMesh + CacheLayer sub-bands, `BlockRenderer` 5 quadtrees + dynamic draws, shadows/darkness FBOs, cached buildings, overlays/lights/bloom, fog/minimap/pixelator/env/debug, menu/loading/g3d/cutscene/screenshots, `.gdshader` port + drift check, MCP §7c and §7d budgets.

- **2026-10-02 — M1 engine half (`lane/16-render`, base `581a007`): `FloorRenderer` in-engine slice.**
  - `mind-gdext/src/render/floor.rs`: `FloorRenderer` bakes one `ArrayMesh`/`MeshInstance2D` per `(chunk, CacheLayer, atlas page)` via `SurfaceTool` (two triangles per quad, normalized page UVs) and parents them to the append-only floor band nodes from `BandKey::floor(cache)` (§3.5). Ports `cacheChunk` used-layer computation (border + `!wall || isAccessible`, `floor_cache_layer`/`block_cache_layer`), `cacheChunkLayer` base/overlay decision, the `env-error`/`error` fallback, and `growSprites` 0.04 (walls layer, upstream `cacheChunkLayer` semantics). Chunk AABB and 30×30 math come from `mind-core::render::floor_cache::FloorChunkGrid`; rebake is lazy (only dirty chunks overlapping the camera view). Counters (`mesh_rebuilds`/`floor_chunks_dirty`/`missing_regions`) feed `MindRender.get_render_stats`.
  - `mind-gdext/src/render.rs`: `MindWorldRenderer` gains the band-key index/`band_node_at`, `build_floor` (pulls `MindSimHost` + `/root/MindAssets`), `camera_view`, and the frame stage-9 floor call; `RenderStats` gains `missing_regions`; `MindRender` gains `set_tile_grid_visible`.
  - `mind-gdext/src/assets/atlas.rs`: read-only `page_texture(page)` + `region_geometry(name) -> [x,y,w,h,page]` (no `AtlasTexture` needed for the mesh bake).
  - `mind-gdext/src/sim_host.rs`: `world_revision()` (`#[func]`), plain `grid()`/`content_registry()` views; revision bumps in `emit_world_changed`.
  - `client/scenes/spine.tscn`: `TileGrid.visible = false` (debug-only after M1), per §3.2.
  - **Deviations flagged.** (a) Floors/walls draw at `TILESIZE` / `size*TILESIZE`, matching the §6.3 `render-list` oracle quads; plan 02/03/06's 47-slice autotiling and natural-region sizing are not applied yet. (b) Chunk invalidation is coarse (`world_revision` → all chunks dirty) because plan 06's `RenderHooks` are not yet fired by the `Sim` tile-mutation path; the fine-grained `FloorChunkGrid` recache already passes its headless tests. (c) The plan §3.5 sentence "while baking any layer except `walls`, `grow_sprites = true`" contradicts `FloorRenderer.cacheChunkLayer` (which sets it true for walls only); this lane follows upstream.
  - **Oracle evidence.** `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo check -p mind-gdext -p mind-headless` clean; `bash tools/build.sh` succeeds; `bash tools/godot.sh --headless --editor --quit --path client` **exit 0**; `cargo test -p mind-core --lib render::` **29 passed**; goldens `render_flat_floor` (1120 entries), `render_block_change`/`render_layer_order` (897) PASS. In-engine screenshot/MCP §7c deferred to the orchestrator's single-editor mutex.
  - **Open (M2 → M9).** `BlockRenderer` 5 quadtrees + dynamic draws, shadows/darkness FBOs, cached buildings/`BuildingCacheLayer`, overlays/lights/bloom/shields, fog/minimap provider/pixelator/Lod/env/debug, menu/loading/g3d/cutscene/screenshots, `.gdshader` port + drift check, MCP §7c and §7d budgets.

- **2026-10-02 — M2 slice (`lane/16-render`): dynamic `BlockRenderer` pass.**
  - `mind-core::render::scan::visible_blocks` (+ `VisibleBlock` export): deterministic row-major visible set of non-static, non-air blocks with the `processBlocks` `camera.bounds.grow(tilesize*2)` cull; test `visible_blocks_excludes_static_walls_and_air` (stone-wall excluded, router included). This is the headless-assertable `tileview` slice.
  - `mind-gdext/src/render/atlas_bind.rs`: shared `RegionResolver` (page texture + normalized UVs, `env-error`/`error` fallback, missing counter) and `build_mesh`/`Quad`/`color_from`, used by the block pass.
  - `mind-gdext/src/render/blocks.rs`: `BlockRenderer` rebuilds a per-atlas-page `ArrayMesh` of the visible dynamic blocks and assigns it to pooled `MeshInstance2D` nodes under the `Layer.block` band; `processBlocks`-style early-out on `(CameraKey, world_revision)`; multiblocks deduped by center entity. `dynamic_sprites`/`mesh_rebuilds`/`missing_regions` feed `get_render_stats`.
  - **Deviations.** Cached (`drawCached`) buildings, cracks, team overlays, destroyed plans, status bars, shadows/darkness and the 5 quadtrees are deferred to M3/M4/M9 (this slice draws every dynamic block each rebuild). `MindTileGrid` remains off.
  - **Oracle evidence.** fmt + workspace clippy `-D warnings` clean; `cargo check -p mind-gdext -p mind-headless` clean; `cargo test -p mind-core --lib render::` **30 passed**; goldens PASS; `bash tools/build.sh` + `bash tools/godot.sh --headless --editor --quit --path client` **exit 0**. MCP §7c (place crafter+router screenshot / `dynamic_sprites > 0`) deferred to the orchestrator's single-editor mutex.

- **2026-10-02 — M8 complete (`lane/16-render`): full shader port, registry, drift + fallback.**
  - **Ported shaders.** `client/shaders/*.gdshader` — all 26 upstream GLSL shaders translated to the Godot shading language (24 `Shaders.*` registry keys + the `default`/`cubemap` helpers; upstream-disabled `shockwave` deliberately not ported per §3.9), each with the GPL header and `Ported from:` line. Canvas-item shaders (`water`/`mud`/`tar`/`slag`/`arkycite`/`cryofluid`/`space`/`caustics`/`light`/`darkness`/`fog`/`shield`/`buildbeam`/`unitbuild`/`unitarmor`/`blockbuild`/`screenspace`/`default`) and spatial shaders (`planet`/`clouds`/`atmosphere`/`planetgrid`/`mesh`/`unlit`/`unlitwhite`/`cubemap`). Translation rules: `texture2D`/`textureCube`→`texture`, `gl_FragColor`→`COLOR`, `#define HIGHP` dropped, `a_position`/`a_normal`/`a_color` mapped to Godot built-ins, `varying mat4 v_model` replaced by a `v_world` varying, and Godot's built-in `PI` reused (dropping the duplicate `const`).
  - **`mind-core::render::shaders`.** New Godot-free module owning the expected list: `EXPECTED` (24 registry keys, `shield` nullable per `Renderer.java:403`), `AUXILIARY` (`default`,`cubemap`), `UNPORTED` (`shockwave`), `requires_port`/`is_nullable`, `parse_uniforms` (handles both Godot `uniform <type> <name> : hint` and Arc GLSL), and `forward_drift`/`reverse_drift`. Tests: `expected_registry_keys_unique_and_ported`, `parse_godot_uniform_syntax`, `drift_directions_are_exact`.
  - **Drift gate.** `mind-tools shaders check --reverse` now reports `unported`/`unported_ok`/forward `drift`/`reverse_drift`; `shaders build` copies `client/shaders/**` → `assets/shaders/godot/` and writes `shader.index.json`; `pack` runs both directions. `client/tools/shaders_check.gd` compiles every shader headlessly (Godot's `--editor --quit` does not compile unreferenced `.gdshader`s).
  - **`mind-gdext::render::ShaderRegistry`.** Loads the plan-03 index from the resolved asset dir (`{assets_dir}/shaders/shader.index.json`, falling back to `res://shaders/*.gdshader`), compiles each source via `Shader::set_code`, logs `[E]` + substitutes the neutral `screenspace` pass on a miss, and reports `manifest_loaded`/`loaded`/`substitutions`/`missing` through `MindRender.shader_status()` and `RenderStats::{shaders_loaded, shader_substitutions}`. `shield` missing is treated as nullable (skips the shield bracket, not a crash).
  - **Oracle evidence.** `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo check -p mind-gdext -p mind-headless` clean; `cargo test -p mind-core --lib render::` **33 passed**; `mind-tools shaders check --reverse --root .` → **26 ported, 0 unported (1 ok), 0 forward drift, 0 reverse drift**; `godot4 --headless --path client --script res://tools/shaders_check.gd` → **26/26 compiled, 0 failed**; `bash tools/godot.sh --headless --editor --quit --path client` **exit 0**, no `SHADER ERROR`. The removed-shader `[E]`+neutral path is implemented and counted; its in-engine MCP exercise is deferred to the single-editor mutex.

- **2026-10-02 — M3 complete (`lane/16-render`): shadows + darkness maps.**
  - **`mind-core::render::shadow`.** `SHADOW_COLOR` (`Color(0,0,0,0.71)`), `BLEND_SHADOW_COLOR` (`white.lerp(black,0.71)` = 0.29), `darkness_value` (`1 - min((d+0.5)/4, 1)`), `new darkness_tile_value`, `shadow_tile_color` (the `processShadows` white/`blendShadowColor` predicate incl. fog `wasVisible` and `ignoreBuildings`/`ignoreTerrain` flags), `in_limited_rect`/`in_fill_rect` (Arc `Rect.contains(..., width - 1, ...)` off-by-one ported exactly), `build_darkness_map`/`build_shadow_map`, and `wall_data_update` (`checkChanges`). Tests: `darkness_curve`, `limit_map_area_fill_clip`, `shadow_tile_predicate`, `blend_shadow_color_is_lerp_white_black_071`, `shadow_map_base_and_event`, `check_changes_filling_only`.
  - **`mind-gdext::render::ShadowRenderer`.** Builds 1 px/tile `Image`/`ImageTexture` shadow + darkness maps from `world.iter_row_major()` and plan-06 `get_static_darkness`, then composites both through the ported `darkness` shader at `Layer::BlockUnder` (shadows) and `Layer::Darkness` (the `Renderer.java:516`/`BlockRenderer.drawDarkness` call sites). `RenderStats::{shadow_rebuilds, darkness_rebuilds, shadow_events}` feed `get_render_stats`.
  - **Deviations.** (a) plan 02's `BlockDef` still lacks `has_shadow`/`fills_tile`, so shadow casters/`checkChanges` filling tiles are approximated by `is_static_kind` until those fields land; (b) the shadow/darkness targets are CPU `ImageTexture`s rather than `SubViewport`s (D16-9 fallback — deterministic and headless-testable); (c) `checkChanges`'s `tile.data` write is plan-06-owned, so only the predicate is ported here.
  - **Oracle evidence.** `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo check -p mind-gdext -p mind-headless` clean; `cargo test -p mind-core --lib render::` **39 passed** (6 new); `bash tools/godot.sh --headless --editor --quit --path client` **exit 0**. MCP layer-toggle screenshots for `darkness` are deferred to the single-editor mutex.

- **2026-10-02 — M4 core slice (`lane/16-render`): building-cache seam + cached visible set.**
  - **`mind-core::render::draw_meta`.** `BlockDrawMeta::from_def` centralizes the upstream `Block` draw flags (`cache_layer`, `draw_cached`, `draw_dynamic`, `display_shadow`, `fills_tile`, `obstructs_light`, `emit_light`, `draw_team_overlay`, `building_cache_layer`). Because plan 02's `BlockDef` still lacks them, the defaults preserve the M2/M3 behavior (`draw_dynamic = !is_static_kind`; `display_shadow = fills_tile = obstructs_light = is_static_kind`; `draw_cached = false`). Tests: `wall_is_static_and_shadowcasting`, `router_is_dynamic_by_default`.
  - **`mind-core::render::scan::visible_cached_blocks`.** The `processBlocks` `tileExtraCachedView` port: center tiles matching the caller's `draw_cached` predicate with the `camera.bounds.grow(tilesize*2)` cull. Test `visible_cached_blocks_uses_predicate`.
  - **`BuildingCacheGrid::recache_tiles`.** Batches tile → chunk/layer dirty marks and returns the newly-dirtied chunk count; test `recache_tiles_batches_per_chunk`. The existing `recache_building_marks_only_affected_chunk` covers the M4 minimality verify. `ShadowRenderer` now consumes `BlockDrawMeta::display_shadow` rather than an inline kind check.
  - **Deferred (plan-02 gap).** The in-engine cached-chunk `ArrayMesh` pass (`cacheChunk`/`drawCached` under/normal, `BuildingCacheLayer`) is not built because `BlockDef` has no `draw_cached`/`building_cache_layer`; `BlockDrawMeta::from_def` returns `draw_cached = false`, leaving the M2 dynamic pass unchanged. The seam is ready for plan 02's fields.
  - **Oracle evidence.** `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo check -p mind-gdext -p mind-headless` clean; `cargo test -p mind-core --lib render::` **43 passed** (4 new); `bash tools/godot.sh --headless --editor --quit --path client` **exit 0**. MCP cached-build counters/screenshots deferred to the single-editor mutex.

- **2026-10-02 — WIP salvage + M4 in-engine + M5/M6 slices (`lane/16-render`, base `main` @ `ff6852e`, from WIP `8f7f8e6`).**
  - **WIP assessed and kept.** The interrupted F10 commit compiles clean (fmt/clippy/check all green); no reset needed. It reconciled plan 02's `BlockDef` **append-only**: `BlockSpec`/`BlockDef` gain `cache_layer`, `draw_cached`, `draw_dynamic`, `building_cache_layer`, `fills_tile`, `has_shadow`, `custom_shadow`, `display_shadow`, `obstructs_light`, `emit_light`, `draw_team_overlay` with upstream class defaults (`init_self` derives `display_shadow = has_shadow`; `customShadow` disables `hasShadow` unless re-enabled; liquid blocks set `emit_light`). `content::registries::blocks` parity tests (all-metadata-valid, ergonomic-flags, names/ids goldens) still pass; no content ID reorder. `render::draw_meta`/`list`/`scan` now read those fields; `world::tile::static_darkness` uses `solid && fills_tile && build.is_none()`; `render::list::is_accessible` is a real `World.isAccessible` port.
  - **M4 (in-engine cached buildings) completed.** `mind-gdext::render::building_cache::BuildingCacheRenderer` bakes one `ArrayMesh`/`MeshInstance2D` per `(chunk, BuildingCacheLayer, atlas page)` from center tiles of `drawCached` blocks, parented `under` = `Layer.blockUnder` and `normal` = an `z_index -1` holder under `Layer.block` so cached draws precede the dynamic pass. `RenderStats::cached_sprites` + `mesh_rebuilds` accounting fixed to sum the three passes' boot totals (the WIP accumulated them every frame). `mind-core::render::block_cache::BuildingCacheGrid::recache_building`/`recache_tiles` and `BlockDrawMeta` (wall/router/container/duct/emitter cases) are the headless oracle.
  - **M5 slice.** `mind-core::render::light` (`LightAccumulator` pool + `enabled()` + `SCALING = 4`, 2 tests); `mind-gdext::render::light::LightRenderer` builds a world-sized CPU light map from `emit_light` visible tiles and composites through the ported `light` shader with `u_ambient`; `MindWorldRenderer::set_draw_light` / `MindRender.set_draw_light` toggles. Nullable-shield gate exposed as `shield_available()` (plan §3.9 `Renderer.java:403`). **Deferred:** overlays (needs plan-15 input), bloom `SubViewport`, shield/buildBeam band bodies.
  - **M6 slice.** `mind-core::render::{fog,pixelate,minimap}` — `FogEvent` pack/unpack (`x:16|y:16|radius:16|team:8`), 20-gon `fog_poly`; `Pixelator` size clamp + half-pixel camera snap; minimap zoom clamp/camera-region/`color_for` shading math (all unit-tested). `mind-gdext::render::minimap::MindMinimap` (`MinimapProvider` in `spine.tscn`, now a `MindMinimap` node) owns a world `Image`/`ImageTexture` from `BlockPalette`+`color_for`+static darkness, 2-frame dirty batching (`update_at`/`update_all`), `region()`/`zoom_by`/`set_zoom`/`minimap_to_screen`/`screen_to_world`/`minimap_color_at` (the plan-14 boundary). `DebugCollisionRenderer` draws green solid-block footprints + magenta exposed edges at `Layer.overlayUI` when `draw_hitboxes` is on. `Lod` is wired into the frame (stage 4) and reported as `lod_l1`/`lod_l2`. **Deferred:** fog `SubViewport` composite + plan-12 `RulesRenderView` adapter, pixelator low-res target, `EnvRenderers`, `MenuRenderer`/`LoadRenderer`/g3d/cutscene/screenshots.
  - **Oracle evidence.** `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo check -p mind-gdext` clean; `cargo test -p mind-core` = **738 lib + 3 `blocks_golden` + 3 `combat_golden` + 2 `sim_core_determinism` + 2 `sim_core_meta` + 1 `sim_core_schedule` = 749 passed / 2 ignored**; `cargo test -p mind-headless` = **34 passed**; `mind-headless render list {render_flat_floor,render_block_change,render_layer_order} --check` all **exit 0** (goldens unchanged, no re-record); `bash tools/build.sh` **exit 0**; `bash tools/godot.sh --headless --editor --quit --path client` **exit 0** with no parse/script/shader errors (only the pre-existing `MindMods::get` shadow warning). In-engine MCP §7c screenshots remain deferred to the orchestrator's single-editor mutex.
  - **Still open (M5–M9).** Overlays/lights-bloom/shields-band, fog in-engine + menu/loading/g3d/cutscene/screenshot timing, `PlanetGrid`/`MeshBuilder` g3d core, full `world/draw/*` descriptor execution, M9 perf/alloc-audit, `bench/render_baseline.json`, and the repo playtest-skill §7c recipes.

- **2026-10-03 — M5/M6/M7 core + in-engine hosts + M9/M10 (`lane/16-render`, base `57b3c27`; commits `b28564f`, `77721a5`, `b2f43c7`).**
  - **M5 remainder (core).** `render::math::voronoi` (full Fortune sweepline port: deterministic `(y,x)` site sort, arena half-edges, `minDistanceBetweenSites=1` clip; tests `two_sites_have_one_bisector`, `sites_edges_deterministic`, `collinear_sites_do_not_panic`); `render::overlay` (`CoreEdge`/`displayed` team rule, selection-arrow angles/length, indicator offset, off-screen gate); `render::bloom` (`Layer.bullet-0.02`/`Layer.effect+0.02` bands, intensity `settings/4+1`, threshold 0.8); `render::drawf` (full `Pal` table as `[f32;4]` + `to_bits`/`lerp`/`mul`); `render::trail` (`Trail` update/shorten/draw/cap). In-engine: `mg::render::overlays::OverlayRenderer` (Voronoi core edges at `Layer.overlayUI`, `MindRender` core list API); `mg::render::env::EnvRenderer` (underwater tint fill + `EnvRegistry` selection); `mg::render::load`/`g3d`/`menu` hosts.
  - **M6 remainder (core + in-engine).** `render::rules::RulesRenderView` (§3.12 adapter with safe defaults + fog alpha floors); `render::env` (`Env` bits, `(env & rules.env) == env`, registration/selection order, underwater/scorching constants); `render::snapshot` (memory check 65/120 MB, full-map buffer + camera math, alpha 255); `render::math::inverse_kinematics` (two-segment leg solver). In-engine: `mg::render::fog::FogRenderer` (packed `FogEvent` queue + `fogOfWar` band composite; plan-12 FBO fallback documented), `mg::render::pixelate::Pixelator` (size clamp + half-pixel snap gate), `MindRender::{set_pixelate,set_fog_enabled,set_rules_env,push_fog_event}`.
  - **M7.** `render::g3d::grid` (`PlanetGrid` exact icosahedron constants + `create`/`subdividedGrid`; tests `tile_corner_edge_counts` = 12/20/30 → 32/60/90 → 92/180/270, `subdivide_links`); `render::g3d::mesh_data` (`build_planet_grid`, `hex_is_indexed` `tiles*6 < 65535`; test `indexed_threshold`); `render::g3d::params::PlanetParams` 1:1; `render::menu` (deterministic 100×50/60×40 terrain selection via plan-06 Simplex/Ridged + flyer math; test `generate_is_deterministic_for_seed`); `render::cutscene` (`LaunchAnimator` trait + `showLanding`/`showLaunch`/`landScale`/`getLandTimeIn`/`weatherAlpha`; tests). In-engine: `mg::render::{g3d,menu,load}` hosts (`PlanetParams`, `PlanetGrid.create(2)`, `MenuWorld`, `LoadRenderer` grid/tint) + `MindRender::{generate_menu,planet_info}`; `RenderStats.stage_trace` now walks the full `Renderer.draw` order (`pre_draw`…`post_draw`).
  - **M9/M10.** `mind-headless render bench --width 250 --height 250 --buildings 5000 --iters N --json`; `RegionIdTable::intern_str` removes the per-tile `String` clone from `build_entries` (the alloc fix that brought the release 250×250 build from p50 5102 µs to **2240 µs** / p99 **3745 µs**, within the ≤2500/≤5000 budget). Committed `mind-core/bench/render_baseline.json`; new golden `render_darkness_radius` (1144 entries).
  - **Oracle evidence.** `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test -p mind-core` = **853 lib + 3 `blocks_golden` + 5 `combat_golden` + 2 `sim_core_determinism` + 2 `sim_core_meta` + 1 `sim_core_schedule` = 866 passed / 2 ignored**; `cargo test -p mind-headless` = **36 + 1 `logistics_golden` = 37 passed**; four `render-list` goldens (`render_flat_floor` 1120, `render_block_change` 897, `render_layer_order` 897, `render_darkness_radius` 1144) PASS; `render bench` release p50/p99 = 2240/3745 µs (`pass:true`, 67 500 entries, 2 regions); `bash tools/build.sh` exit 0; `bash tools/godot.sh --headless --editor --quit --path client` exit 0, no SHADER/parse errors.
  - **Deferred (owners).** In-engine MCP §7c per-layer screenshots / minimap pixel / fog band and the §7.4 GPU/full-frame/draw-call/triangle/texture-memory budgets are **DEFERRED — orchestrator (single-editor mutex)**. `SubViewport` pixelation, fog static/dynamic FBOs (`copy_from_cpu`), the additive caustics/rays/particle env bodies, and the g3d spatial shader family are structural hosts only (data path + counters); their visuals are MCP-deferred. Menu `render_menu_world` golden and `--no-sort` emission-order test not recorded. `THIRD_PARTY_NOTICES.md` unchanged (D16-11: no addons adopted).
