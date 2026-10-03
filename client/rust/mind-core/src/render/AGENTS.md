# AGENTS.md — mind-core/src/render (Godot-free render data)

`mind-core/src/render` is the Godot-free half of the world renderer. It owns the render `Layer`
constant table, `CacheLayerId` ordering, the append-only `BandPlan` band allocator, the stable
`RenderQueue`, region-id interning, floor/block chunk bookkeeping, draw descriptors/metadata,
bloom/cutscene data and the visible-set scan. It is view-only: its output never feeds simulation
state, saves, sync or the checksum. The Godot-facing half — band replay, `SubViewport` bakes,
materials, shaders and screenshots — lives in `mind-gdext::render`. Read the root
[`AGENTS.md`](../../../../../AGENTS.md) first. The crate map is
[`mind-core/AGENTS.md`](../../AGENTS.md).

## Layout

| File / subfolder | Responsibility |
|---|---|
| `mod.rs` | Module root: declares submodules and the re-export surface. |
| `layer.rs` | `Layer` (35 z values), `CacheLayerId` (frozen 9-value order) and `BuildingCacheLayer`. |
| `bands.rs` | Append-only `BandPlan`/`BandKey`/`BandEntry`: `(Layer, sub, Blend)` → integer `z_index`, `to_json` audit. |
| `queue.rs` | Stable `RenderQueue` sorted by `(z, seq)`; `QueueItem::Cmd`/`RangeBegin`/`RangeEnd` brackets. |
| `commands.rs` | POD `DrawCmd`, reusable `CommandBuffer`, `Blend`/`ShapeKind`/`LineKind`/`FillKind`, `SpriteBatch`. |
| `ids.rs` | `RegionId`/`RegionIdTable`: dense, append-only region-name interner. |
| `list.rs` | Sprite render-list extraction (`RenderList`, `RenderEntry`, `build_entries`, `sort_entries`, `is_accessible`). |
| `layers.rs` | Full-layer list (`LayerRenderList`, `build_layers`, `count_entries`) plus `DrawCounters`. |
| `scan.rs` | Visible-set scan: `CameraView`, `ChunkSet`, `process_blocks`, `visible_blocks`, `visible_cached_blocks`, `floor_layers_in_view`. |
| `floor_cache.rs` | `FloorChunkGrid` dirty/used-layer/epoch bookkeeping; `CHUNK_SIZE`, `CHUNK_UNITS`, `chunk_aabb`. |
| `block_cache.rs` | `BuildingCacheGrid` per-chunk/per-layer dirty/epoch state; `recache_wall_tiles`, `update_shadow_tiles`. |
| `hooks.rs` | `RenderInvalidation` implements `world::hooks::RenderHooks` over both grids and the minimap pixel queue. |
| `draw.rs` | FX geometry contract: `DrawPrim`/`DrawProgram`/`PrimKind`, `RegionKey`/`TextureKey`/`ShaderKey`, deterministic hashing. |
| `draw_desc.rs` | Ported `world/draw/*` bodies: `DrawDesc`, `DrawState`, `execute`, `execute_extras`, `vanilla_chain`, `global_layer_markers`. |
| `draw_meta.rs` | `BlockDrawMeta`: derived `cache_layer`/`draw_cached`/`draw_dynamic`/shadow/light flags from `BlockDef`. |
| `drawf.rs` | `pal` color constants and helpers (`to_bits`, `with_alpha`, `lerp`, `mul`). |
| `bloom.rs` | Bloom capture/render z offsets, `intensity` and `blur_passes` math. |
| `light.rs` | `LightAccumulator`/`CircleLight` pool, `enabled` gate and `SCALING`. |
| `fog.rs` | Fog-event pack/unpack (`fog_event` and accessors) and the 20-gon `poly_vertices`/`fog_poly`. |
| `shadow.rs` | Shadow/darkness tile predicates, falloff and `build_darkness_map`/`build_shadow_map`. |
| `overlay.rs` | `CoreEdge`/`build_core_edges`, off-screen indicators and selection-arrow geometry. |
| `env.rs` | `EnvRegistry`/`EnvRendererSpec`, `matches` bitmask and underwater/scorching constants. |
| `lod.rs` | `Lod` level-of-detail thresholds and alpha fades. |
| `minimap.rs` | Minimap zoom clamp, camera region crop and per-tile shade multipliers. |
| `pixelate.rs` | Low-res target size and half-pixel camera snap. |
| `cutscene.rs` | `Cutscene` + `LaunchAnimator` launch/land state machine and weather-alpha handoff. |
| `snapshot.rs` | Map-screenshot memory check, buffer size and `ScreenshotPlan`. |
| `menu.rs` | Procedural `MenuTile`/`MenuWorld`/`MenuSummary` generation and flyer sampling. |
| `rules.rs` | `RulesRenderView`: the render-relevant slice of `Rules`/`FogControl`/`MapMarkers`. |
| `shaders.rs` | `EXPECTED`/`AUXILIARY`/`UNPORTED` shader keys, `Uniforms` parsing and drift helpers. |
| `trail.rs` | `Trail` ribbon state machine and quad emitter. |
| `g3d/` | `PlanetGrid` icosphere (`grid.rs`), `PlanetParams` (`params.rs`) and `MeshData` builder data (`mesh_data.rs`). |
| `math/` | `voronoi` (Fortune sweepline core edges) and `inverse_kinematics` (two-segment joint solver). |

## Key types

- Layers/bands: `Layer`, `CacheLayerId`, `BuildingCacheLayer`, `BandPlan`/`BandKey`/`BandEntry`.
- Ordering/queue: `RenderQueue`/`QueueEntry`/`QueueItem`, `RegionId`/`RegionIdTable`.
- Draw payload: `DrawCmd`, `CommandBuffer`, `Blend`, `DrawPrim`/`DrawProgram`/`PrimKind`, `DrawDesc`/`DrawState`.
- Terrain bookkeeping: `FloorChunkGrid`, `BuildingCacheGrid`, `RenderInvalidation`, `BlockDrawMeta`.
- Scans/lists: `CameraView`, `ProcessBlocksState`/`ProcessBlocksOut`, `VisibleBlock`, `RenderList`/`LayerRenderList`.
- View data: `Lod`, `EnvRegistry`, `Cutscene`, `Trail`, `RulesRenderView`, `ScreenshotPlan`, `PlanetGrid`/`PlanetParams`/`MeshData`.

## Invariants

- **Godot-free.** No source here may reference `godot`; the Godot `CanvasItem`/mesh/shader work is in
  `mind-gdext::render`.
- **View never feeds sim.** These values are excluded from saves, sync and the canonical checksum.
- **Append-only ABI.** `Layer::ALL`, `CacheLayerId::ALL` (id `0..=8`, frozen order) and interned
  `RegionId`s never renumber; `BandPlan` band numbers only append.
- **Stable ordering.** `RenderQueue` is stable-sorted by `z` using its monotonic `seq`; band entries
  strictly increase in layer order; region ids are dense in first-call order.
- **Capacity reuse.** `CommandBuffer::clear`/`warm`, `LightAccumulator::clear` and `DrawProgram::clear`
  retain capacity for the steady-state alloc audit.
- **Minimal chunk dirt.** Mutating one tile dirties only the containing floor/building chunk.

## Rules

- Port upstream Mindustry and cite it in file headers; files are UTF-8/LF, GPL-3.0-only.
- Keep rendering data and math here; add anything that touches Godot on the `mind-gdext` side.
- Add crates through `[workspace.dependencies]` and opt into `[lints] workspace = true`; no
  `unwrap`/`expect` on runtime data.
- Determinism is behavioral: keep iteration, sort and hashing orders exactly as the goldens expect.

## Verification

Run from `client/rust/`.

```bash
cargo test -p mind-core render
cargo clippy -p mind-core
cargo fmt --all -- --check
```
