# 06 — WORLD & TERRAIN IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Draft v1 — 2026-10-01, not started. 1 item flagged `NEEDS USER DECISION` in §8; it does not block M0–M4. |
| **Phase** | P3 — World & systems |
| **Depends on** | `02_CONTENT_IMPLEMENTATION_PLAN.md` (`BlockDef` metadata incl. environment fields, `Attribute` ids, `Floor` data fields, `Planet`/`SectorPreset` metadata, `ContentRegistry` lookups), `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (`WorldContext` trait, `SaveIO`/`MapIO`/`JsonIO`, `FileSystem`/`Paths`, `CachedTile`-equivalent preview contract), `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (`Sim`/schedule/EventBus, `EntityIds`, `Groups`/`EntityGroup`, `ChecksumPart`, `Tmp`/`SimRng`, `math` module). |
| **Blocks** | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (Tile/Build seams, `WorldHooks`), `08_LOGISTICS_IMPLEMENTATION_PLAN.md` (tile item buffers), `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (world raycast, puddles/fires slots), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (terrain pathfinding data, world indexers), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (`Rules` sector fields, `Sector`/`Universe` runtime, `Schematics` launch loadout, `BaseRegistry`), `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (tile read contract, `CacheLayer` hints, recache hooks), `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (previews, image maps, editor tile ops). |
| **Sources** | `core/src/mindustry/core/World.java`; `world/Tile.java`, `Tiles.java`, `WorldContext.java`, `WorldParams.java`, `TileGen.java`, `CachedTile.java`, `Edges.java`, `ColorMapper.java`, `ItemBuffer.java`, `DirectionalItemBuffer.java`; `world/blocks/Attributes.java`, `world/blocks/environment/*` (data fields); `world/meta/Attribute.java`; `maps/Maps.java`, `Map.java`, `MapException.java`, `MapPreviewLoader.java`, `SectorDamage.java`, `SectorSubmissions.java`; `maps/filters/*.java` (15 registered + `RandomItemFilter`, `FilterOption`, `GenerateFilter`); `maps/generators/{WorldGenerator,BasicGenerator,BaseGenerator,PlanetGenerator,BlankPlanetGenerator,FileMapGenerator}.java`; `maps/planet/{Serpulo,Erekir,Tantros,Asteroid}*.java`; `graphics/CacheLayer.java` (ordering contract); `Vars.java` constants (`tilesize=8`, `maxBlockSize=16`, `darkRadius=4`, `finalWorldBounds=250`); `tests/src/test/java/ApplicationTests.java` (`createMap`, `playMap`, `edges`, `multiblock`, `blockInventories`, `blockOverlapRemoved`, `save`, `saveLoad`, `load77...152Save`). |
| **AGENTS read** | `Mindustry/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/AGENTS.md`, `core/src/mindustry/core/AGENTS.md`, `core/src/mindustry/world/AGENTS.md`, `core/src/mindustry/maps/AGENTS.md`, `core/src/mindustry/io/AGENTS.md`, `core/src/mindustry/graphics/AGENTS.md`, `core/src/mindustry/world/blocks/AGENTS.md`. |
| **Extends spine** | Replaces plan 00's placeholder `WorldGrid` with the real `WorldGrid`/`Tiles`/`Tile`. `mind-headless` gains `world` and `maps` subcommands (`world gen`, `world tile-ops`, `world multiblock`, `world filters`, `maps list`, `bench world-gen`). Scenario format gains `world.generator` variants (`planet`, `file`) and a dump `world` section with tile/floor histograms. MCP: `SimHost.load_scenario` loads a generated planet sector; the spine `TileGrid` draws real floor/wall layers via the plan-16 read contract, and the inspector shows `tile_changes`/`floor_changes` and dimensions. |

---

## 2. Scope & parity definition

### 2.1 In scope

1. **Tile grid model.** `Tile` fields and semantics (packed `data/floorData/overlayData/extraData`, `x/y: i16`, `block/floor/overlay` content ids, `Option<Entity> build`, `changing`), all `Tile` methods (`pos/array/relativeTo/getFlammability/passable/solid/breakable/dangerous/legSolid/staticDarkness/drop/wallDrop/getLinkedTiles/getHitbox/...`), multiblock linkage (proxy tiles point at the center `Building` entity), tile event firing/suppression (`TilePreChangeEvent`, `TileChangeEvent`, `TileFloorChangeEvent`, `TileOverlayChangeEvent`), and the `@Remote` static helpers as plain world functions (`setTile`, `removeTile`, `setFloor`, `setOverlay`, `fillTile*`, `setTileBlocks`, `buildDestroyed`, `setTeams`) that plan 21 will wrap as reducers.
2. **`Tiles` container.** Width/height, flat `Vec<Tile>` indexed `x + y*width`, `get`/`getn`/`getc`/`geti`/`getp`/`in`/`set`/`seti`/`each`/`eachTile`/iterator, puddle/fire slot arrays, `tmpFloorState`/`tmpBlockState` scratch, `fill()`.
3. **`WorldGrid` resource (Java `core.World`).** `resize`, `clearBuildings`, `beginMapLoad`/`endMapLoad`, `setGenerating`, `loadGenerator`, `loadSector`, `loadMap`, `filterContext`/`context`/`makeSectorContext`, `addMapLoader`, tile/build accessors, `packArray`, `conv`/`unconv`/`toTile`, `isInMapArea`, `checkMapArea`, edge darkness (`addDarkness`, `getDarkness`, `getWallDarkness`), `tileChanges`/`floorChanges` counters, `invalidMap`, `getQuadBounds`, and the grid raycast helpers (`raycast*`) consumed by plans 10/11/15.
4. **`WorldContext` implementation.** Plan 04 defines `io::save::WorldContext`; this plan implements it for `world::context::Context` (default), `FilterContext` (applies map filters in `end()`), and a sector context; plus `onReadBuilding`/`onReadTileData` callbacks used by save-loading (plan 04) and the `Map`-loading seam.
5. **Support types.** `WorldParams`, `TileGen`, `CachedTile`/`CachedTiles` (event-free, build-stub tile view for previews per 04 §3.9), `Edges` (precomputed edge offsets + pixel polygons), `ColorMapper`, `Attributes` (per-floor attribute float array; `Attribute` ids owned by 02), `PackTileData`/`PackTile` packing.
6. **Terrain generation.** `WorldGenerator` trait, `BasicGenerator` helper library (`pass`, `median`, `ores`, `ore`, `oreAround`, `wallOre`, `cliffs`, `terrain`, `noise`, `overlay`, `tech`, `distort`, `scatter`, `cells`, `blend`, `decoration`, `brush`, `erase`, `pathfind`, `trimDark`, `inverseFloodFill`, `removeWall`, `nearWall`, `nearAir`, `near`), `PlanetGenerator` + `BlankPlanetGenerator` + `HexMesher` (height/color/emissive data for plan 16 g3d), `FileMapGenerator`, `BaseGenerator` (behind campaign hooks), `SectorDamage`, vanilla planet generators (Serpulo, Erekir, Tantros, Asteroid), and the `Simplex`/`Ridged` noise ports they need.
7. **Generation filters.** `GenerateFilter` base + `GenerateInput`, `FilterOption` data descriptors + block predicates, `Maps.allFilterTypes` registry (`FilterRegistry`) with JSON class tags, the 15 registered filters + `RandomItemFilter`, buffered vs unbuffered semantics, `isPost`, `randomize()`, default filter stack (`ScatterFilter` per decorative floor + `OreFilter` per `oreDefault` floor), and `Map.filters()` (`build < 83` rule).
8. **`Maps`/`Map` registry.** `Map` metadata/tags (`name`/`author`/`description`/`rules`/`genfilters`/`build`/`width`/`height`), custom/workshop/mod flags, `rules()` JSON overlay + planet fallbacks, `filters()`, preview/cache file paths, `compareTo` sort; `Maps::load` (built-ins from `maps/default/<name>.msav`, `data/maps/`, mod `maps/`, workshop stub), `saveMap`, `importMap`, `removeMap`, `reload`, `byName`, `loadInternalMap`, `getNextMap` + `ShuffleMode`/`MapProvider`, preview queue/cache read/write hooks, `MapException`.
9. **World data contract** consumed by 07/16: tile/floor/wall/prop data fields, attributes, `Block.cacheLayer`/`fillsTile`/`solid`/`isStatic`/`synthetic`/`hasBuilding` selection hints, `Floor` movement/liquid/attribute/drown data, `CacheLayer` id ordering, and recache/render-invalidation hooks (implementation in 16).

### 2.2 "Done" means

- `cargo test -p mind-core` passes the §7a ported and new tests headlessly with no Godot/network.
- `mind-headless world gen --planet serpulo --sector <n> --seed <s>` generates a structurally valid sector twice (in-process, cross-process) with the same committed golden checksum; same for Erekir/Tantros/Asteroid smoke cases.
- `mind-headless world tile-ops`, `world multiblock`, `world filters` pass with the assertions in §7b.
- A plan-04 `io roundtrip` uses the real `WorldContext` implementation with no stubs (04 §5 M4 swap completed).
- The MCP scenario §7c runs: the spine tile grid shows generated Serpulo terrain, camera pans, place/break works on generated tiles, counters update, logs clean.
- `mind-core` stays Godot-free/tokio-free (plan-00 boundary greps green); every ported file carries the GPL header.

### 2.3 Deliberate deviations

| # | Deviation | Reason |
|---|---|---|
| 1 | **`World` is a Bevy resource named `WorldGrid`** (05 reserved the name); `Tile` stores `Option<Entity>` instead of a `Building` pointer; all tile ops are exclusive-`bevy_ecs::World` functions using `resource_scope::<WorldGrid>` so hooks may spawn/despawn entities without `unsafe`/interior mutability. | D1/§2.2; Rust aliasing rules; identical semantics. |
| 2 | **No `Vars` statics.** `WorldGrid`, `Maps`, `SimRng`, `ContentRegistry` are resources/config passed explicitly; tile ops take a `&mut WorldCtx` bundle. | D1, plan 05 §3.2. |
| 3 | **Tile event payloads are by-value structs** `{ x: i16, y: i16 }` (plus prev/next ids where Java carries them), not reused static instances holding `Tile` refs. | plan 05 §3.7 "reused Java payloads become by-value structs". |
| 4 | **`GenerateInput.tile()` reads a local packed-state buffer**, not the live `Tiles` array. `apply_tiles` maintains this buffer with Java's exact read-visibility rules: unbuffered filters see earlier tiles' writes (write-through buffer); buffered filters see the pre-pass state. | Rust borrow rules; observationally identical; allocation equals Java's buffered `long[]` (one buffer per pass). |
| 5 | **Deterministic generation RNG streams instead of Arc's global `Mathf.rand`.** Generators keep their own seeded `Rand` (`PlanetGenerator.rand`, `BaseGenerator`'s `Mathf.rand.setSeed(sector.id)` re-seed becomes `SimRng::MapGen` re-seed), and `GenerateFilter::randomize` takes `&mut SimRng` seeded from `(world seed, map name)` instead of reading process-global state. | HLP §2.4 (seeded PRNG, no wall-clock, deterministic replays); Java's global `Mathf.rand` state at filter time is not replay-stable. **NEEDS USER DECISION** (OD6-A, default chosen). |
| 6 | **Structural terrain parity, not Java float parity.** `Simplex`/`Ridged` are reference-algorithm ports; noise is evaluated through `sector.rect.project` exactly as upstream, but bit-identical output vs the JVM is not claimed. | HLP §9; only Rust↔Rust determinism required. |
| 7 | **Preview pixels/textures stay out of `mind-core`.** 06 owns the queue, paths and cache-file read/write; `MapIO::generate_preview` pixel generation and PNG/`Texture2D` handling are plans 04/19 (04 §2.3.7). | D1 (Godot-free), 04 §3.9. |
| 8 | **`ItemBuffer`/`DirectionalItemBuffer` are NOT ported here** — they are logistics state used by bridges/junctions/sorters and belong to `08_LOGISTICS_IMPLEMENTATION_PLAN.md`. `Tiles` only owns puddle/fire slots (entities owned by 10). | Assignment boundary; avoids 06↔08 overlap. |
| 9 | **`SectorSubmissions`** (hidden-sector presets, Discord links) is deferred to plans 12/22; `SectorDamage` is ported here but its `Effect::rubble` call is a plan-17 hook. | Campaign/Discord ownership. |
| 10 | **`Astar` grid search used by generators/filters lives in 06** (`world::generation::astar`); plan 11 owns unit pathfinding/flowfields and may reuse this module but not replace its API. | 06 blocks 11; generators cannot depend on 11. Orchestrator reconcile (§8 R4). |

### 2.4 Deferred ownership

| Item | Owner |
|---|---|
| `SaveIO`/`SaveVersion`/`MapIO`/`TypeIO`/`JsonIO`, `WorldContext` **trait definition**, `FileSystem`/`Paths`, native `MGRS` format, preview pixel generation entry points | `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` |
| `Block` behavior (`update`, `newBuilding` bodies, proximity, consumers), `Build` placement/breaking, `ConstructBlock`, `Building` ECS runtime, multiblock entity lifecycle bodies | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` |
| `ItemBuffer`/`DirectionalItemBuffer`, `TileBitmask`/`Autotiler` usage (drawing in 16) | `08_LOGISTICS_IMPLEMENTATION_PLAN.md` |
| `Fires`/`Puddles` entities, world raycast consumers, bullet collision | `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` |
| `Astar`/`ControlPathfinder`/flowfields/`BlockIndexer`, unit controllers, `Waves`, `BaseRegistry` bodies | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| `Rules` full struct, `Gamemode`, `Teams`/`TeamData`, `Sector`/`Universe`, `Schematic`/`Schematics`, `Loadouts` bodies, planet JSON data | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` |
| `FloorRenderer`/`BlockRenderer`/`MinimapRenderer`, `CacheLayer` runtime, recache bodies, g3d `HexMesher` consumers | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| Map editor, `DrawOperation`, preview PNG/texture lifecycle, image-map import/export UI | `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` |
| Mod map discovery/patching, asset `maps/` packaging, `FileTree` | `20_MODS_IMPLEMENTATION_PLAN.md`, `03_ASSETS_IMPLEMENTATION_PLAN.md` |
| STDB tables/views/reducers for map/sector sharing; late-join world streaming | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |

---

## 3. Target design

All names below are final unless marked otherwise. `mind-core` is Godot-free and tokio-free (D1). No `HashMap` iteration on sim/generation paths: ordered `IndexMap`/`Vec`/`BTreeMap` only. Content is referenced by `BlockId`/`ItemId`/`LiquidId`/`PlanetId` (02), never by pointer.

### 3.1 Module layout

```
client/rust/mind-core/src/
  world/
    mod.rs             # WorldGrid resource + WorldPlugin; tile op entry points (exclusive-system fns)
    pos.rs             # TilePos(i16,i16), Point2 pack/unpack parity, array/pos helpers
    tile.rs            # Tile, PackTileData, tile methods (read + mutating ops)
    tiles.rs           # Tiles (grid, puddles/fires, tmp state, iterators)
    events.rs          # TileChangeEvent/TilePreChangeEvent/TileFloorChangeEvent/TileOverlayChangeEvent
                       #   WorldLoadBeginEvent/WorldLoadEndEvent/WorldLoadEvent payloads (registered in 05 bus)
    context.rs         # impl io::save::WorldContext for Context / FilterContext / sector context
    darkness.rs        # add_darkness, get_darkness, get_wall_darkness, is_darkened
    raycast.rs         # raycastEach* / raycast (Bresenham ports)
    edges.rs           # Edges tables + pixel polygons
    cached.rs          # CachedTile/CachedTiles view, TileGen
    color_mapper.rs    # ColorMapper
    attributes.rs      # Attributes (float[Attribute::COUNT])
    params.rs          # WorldParams
    hooks.rs           # WorldHooks, RenderHooks, MapGenHooks, SpawnerHooks (no-op defaults)
    checksum.rs        # impl ChecksumPart for WorldGrid
  maps/
    mod.rs             # Maps resource + MapsPlugin, registry, save/import/remove, ShuffleMode
    map.rs             # Map metadata/tags/rules()/filters()/compareTo
    error.rs           # MapException/MapError
    preview.rs         # preview/cache paths + cache file read/write + queue (pixels in 19)
    shuffle.rs         # ShuffleMode, MapProvider, pvp map list
    sector_damage.rs   # SectorDamage
    filters/
      mod.rs           # GenerateFilter trait, GenerateInput, PackTile, FilterRegistry (allFilterTypes)
      option.rs        # FilterOption descriptors + BlockPredicate helpers
      noise.rs scatter.rs terrain.rs distort.rs river_noise.rs ore.rs ore_median.rs
      median.rs blend.rs mirror.rs clear.rs core_spawn.rs enemy_spawn.rs spawn_path.rs
      logic.rs random_item.rs
    generators/
      mod.rs           # WorldGenerator trait, GenerationCtx
      basic.rs         # BasicGenerator
      base.rs          # BaseGenerator (BaseRegistry/Schematic hooks)
      planet.rs        # PlanetGenerator + HexMesher trait
      blank_planet.rs  # BlankPlanetGenerator
      file_map.rs      # FileMapGenerator
      astar.rs         # deterministic grid A* used by generators/filters (shared with 11)
    planet/
      serpulo.rs erekir.rs tantros.rs asteroid.rs
  math/
    noise.rs           # Simplex / Ridged ports (additive to plan 05's math/ — see §8 R6)
  util/
    strings.rs         # strip_colors + sanitize_filename (needed by Map compare/findFile)
```

### 3.2 `WorldGrid` resource

```rust
#[derive(Resource)]
pub struct WorldGrid {
    pub tiles: Tiles,
    pub generating: bool,
    pub invalid_map: bool,
    pub tile_changes: i32,                 // Java int; starts at 1, reset to -1 on WorldLoadEvent
    pub floor_changes: i32,
    pub custom_map_loaders: IndexMap<MapKey, Box<dyn FnMut(&mut bevy_ecs::World) + Send + Sync>>,
    pub generator_seed: u64,               // sim/map seed used by filter randomize (OD6-A default)
}
```

- `WorldGrid::new()` matches `new World()`: counters start at 1 and increment via tile events; the `WorldLoadEvent` listener resets them to `-1` and calls `hooks.check_allow_update()` for every building (07 body, no-op until then).
- Tile ops are free functions with `&mut bevy_ecs::World`, e.g. `world::set_block(w, x, y, block, team, rot)`. They use `w.resource_scope::<WorldGrid, _>` and then access `ContentRegistry`, `EventBus`, `Groups`, `EntityIds` and `WorldHooks` from the remaining ECS world — no aliasing, no `unsafe`. Hot read paths (`tile(x,y)`, `solid`, `passable`) take `&WorldGrid` directly.

### 3.3 `Tile` and `Tiles`

```rust
#[derive(Clone)]
pub struct Tile {
    pub x: i16, pub y: i16,
    pub data: i8, pub floor_data: i8, pub overlay_data: i8, pub extra_data: i32,
    pub block: BlockId,
    pub floor: BlockId,
    pub overlay: BlockId,
    pub build: Option<Entity>,   // center entity for multiblocks; proxies point at it
    pub changing: bool,
}
pub const PACK_DATA_LAYOUT: &str = "extra_data:i32 | data:u8 | floor_data:u8 | overlay_data:u8 (private to MGRS)";

pub struct Tiles {
    pub width: i32, pub height: i32,
    array: Vec<Tile>,
    puddles: Vec<Option<Entity>>,
    fires: Vec<Option<Entity>>,
    tmp_floor_state: Vec<i64>, tmp_block_state: Vec<i64>,
}
```

- `Tile::default()` = floor/overlay/block `Blocks::air` (02 ids), mirroring the Java constructor. `x/y` are `i16`; out-of-range construction is a debug assert.
- `Tile::array()` requires the width from `Tiles`; provide `Tiles::array_of(tile)` and `Tiles::pos_of(x,y)` returning packed `i32` (`(x & 0xffff) | ((y & 0xffff) << 16)`), matching Arc `Point2.pack` so `pos()` values are interchangeable with 04/21 payloads.
- `get_packed_data`/`set_packed_data` are internal (`PackTileData` byte layout is **not** a wire format; native format stores the four fields separately per 04 §6.1).
- `Tiles::fill()` resets all tiles to default in-bounds tiles; `resize` replaces the whole grid and clears buildings first (Java `World.resize`), allocating a new `Tiles` only when dimensions differ.
- Puddle/fire slots are opaque `Option<Entity>` here; plans 10/11 own the entity components. `tmp_floor_state`/`tmp_block_state` are lazily allocated `i64` scratch, reset on `resize`.
- Iteration order is fixed: `each()` = x outer, y inner (Java); flat array order = `x + y*width`; `each_tile()`/`iter()` = row-major `(y, x)`. This is a determinism contract used by checksums and event logs.

### 3.4 Tile operations and events

`world::tile` ports the exact `Tile` mutation order:

1. `set_block(type, team, rot, entity_prov)`:
   - `changing = true`; static→static recache hooks (`RenderHooks::recache_tile`/`recache_wall`, no-op in core).
   - `force_team` override; `pre_changed()` → `fire_pre_changed()` (event), `build.on_removed()` + `remove_from_proximity()` (07 hooks), and for multiblocks the manual proxy reset loop (each other tile gets `fire_pre_changed`, `build=None`, `block=air`, `fire_changed`).
   - `block = type`; `change_build(team, prov, rot % 4)` → if old build: `build.remove()` (07), collect edge buildings via `Edges::edges(size)`, `update_proximity`; then if `type.has_building()`: spawn via `WorldHooks::new_building(...)` (07; default: bare `BuildingComp` entity so 06 tests pass).
   - Multiblock two-pass loop over `size²` tiles from `offset = -(size-1)/2`: pass 0 sets overlap tiles to air (recursively triggering removals), pass 1 assigns `other.build = entity; other.block = block`; then restore center.
   - `changed()`: if not generating, `build.update_proximity()` or manual `on_proximity_update()` for the four neighbors when the build is None; `fire_changed()` (`TileChangeEvent`, suppressed while generating); static recache; finally `hooks.block_changed(block, x, y)` (07/02 body).
2. `set_floor(type)` / `set_overlay(type)`: early-out if unchanged; `RenderHooks::remove_floor_index`/`recache`; `build.on_proximity_update()`; `pathfinder.update_tile` hook (11, no-op until then); fire `TileFloorChangeEvent`/`TileOverlayChangeEvent` only when `!generating`; call `hooks.floor_changed(floor, x, y)` if it actually changed (Java `Floor.floorChanged`).
3. `set_overlay_quiet` (no recache), `set_air`, `remove` (set air).
4. `@Remote` helpers become `pub fn` world ops: `set_tile_blocks(block, &[i32])`, `set_tile_floors`, `set_tile_overlays`, `fill_tile_blocks/floors/overlays`, `set_floor_and_overlay`, `remove_tile`, `set_team`, `set_teams` (power-graph reflow delegated to 09 via hook), `build_destroyed`, `build_health_update`. Plan 21 wraps them as reducers; plan 15 calls them through `Placement`.

Event payloads (registered in plan 05's `EventBus`, append-only):

```rust
pub struct TileChangeEvent { pub x: i16, pub y: i16 }
pub struct TilePreChangeEvent { pub x: i16, pub y: i16 }
pub struct TileFloorChangeEvent { pub x: i16, pub y: i16, pub prev: BlockId, pub next: BlockId }
pub struct TileOverlayChangeEvent { pub x: i16, pub y: i16, pub prev: BlockId, pub next: BlockId }
pub struct WorldLoadBeginEvent; pub struct WorldLoadEndEvent; pub struct WorldLoadEvent;
```

- `WorldGrid` increments `tile_changes`/`floor_changes` in the same functions rather than through listeners (observably identical, no registration-order hazard); both are suppressed while `generating`.
- All tile mutation happens on the sim thread. Plans 21/23 rely on the event stream being deterministic.

### 3.5 `WorldContext` implementation (plan 04 seam)

Plan 04 defines the trait; 06 implements it in `world/context.rs`:

```rust
pub struct Context<'a> { world: &'a mut WorldGrid, sector: Option<SectorKey>, /* ecs handle */ }
pub struct FilterContext<'a> { inner: Context<'a>, map: MapKey }
```

- `tile(index) -> TileRef`, `resize(w,h)`, `create(x,y,floor,overlay,wall)` = `Tile` construction + `change_build(derelict, wall::newBuilding, 0)` + `changed()` (events suppressed while generating), `is_generating`, `begin` = `begin_map_load`, `end` = `end_map_load`, `get_sector`, `is_map` (true for `FilterContext`), `on_read_building`/`on_read_tile_data` callbacks for 04's nested chunks.
- `FilterContext::end()` = `apply_filters()` then `end_map_load()` (Java `FilterContext.end`), using the filter stack from `Map::filters()`.
- Sector context (`make_sector_context`) returns a `Context` carrying `sector: Some(key)` used by 12's `SectorInfo` write-back.
- 04's `SaveVersion` readers call `context.create` with content **ids**; unknown ids were already mapped to fallbacks by 04's temporary `ContentMapper`.

### 3.6 `beginMapLoad`/`endMapLoad` and darkness

- `begin_map_load`: `generating = true`; fire `WorldLoadBeginEvent`.
- `end_map_load`: fire `WorldLoadEndEvent`; per tile: `hooks.legacy_remove_self(block, x, y)` (removes `LegacyBlock`s, 07/02), then `build.update_proximity()`; then `add_darkness(tiles)`; then `Groups::resize(-250, -250, w*8+500, h*8+500)` (05); `generating = false`; fire `WorldLoadEvent` (resets counters to `-1`, calls `hooks.check_allow_update` per building — 07).
- `add_darkness` ports the Java BFS: seed `dark[i] = darkRadius (4)` where `tile.is_darkened()`; run `darkRadius` relaxation passes over 4-neighbours; then write `tile.data = dark[idx]` for darkened tiles, and `darkRadius + 1` when fully enclosed. `get_darkness` returns the max of the `rules.borderDarkness` edge falloff, the sector-polygon falloff via the `SectorShape` hook (12), and `tile.data` when the tile is darkened. `get_wall_darkness` = Manhattan scan within `darkRadius`. `tile.data` is overloaded (block save data vs darkness) exactly as upstream; this is documented and asserted at the seam with 07.
- Constants live in `mind_core::constants` (plan 05/00): `TILESIZE = 8`, `MAX_BLOCK_SIZE = 16`, `DARK_RADIUS = 4`, `FINAL_WORLD_BOUNDS = 250.0`.

### 3.7 `Edges`, `ColorMapper`, `Attributes`, `CachedTile`, `TileGen`

- `Edges`: `static EDGES: OnceLock<[Vec<Point2i>; 16]>` built once (normal radius 12, block sizes 1..=16); port the exact `bot/top` formulas and the `Mathf.angle` sort; expose `edges(size)`, `inside_edges(size)`, `facing_edge(block, tx, ty, other) -> TileRef`, `pixel_polygon(radius) -> &'static [Vec2]` (`radius 1..=12`, `Geometry.pixelCircle((i+1)/2)`). `ApplicationTests.edges` is the oracle for the sort order.
- `ColorMapper`: `IndexMap<u32 /*rgba8888*/, BlockId>` rebuilt by `load(&ContentRegistry)`; `get(color) -> BlockId` defaulting to air; the `0,0,0,1` → air entry preserved. Invoked by 02's `ContentLoader::load_colors` equivalent (02 §3.4 lifecycle) and by 19's image import.
- `Attributes`: `[f32; Attribute::COUNT]` container with `clear/get/set/add(other)/add_scaled`, JSON serde by attribute **name** (04/12), resized to `Attribute::all.len()` on access. The `Attribute` registry itself (heat/spores/water/oil/light/sand/steam + `add(name)` for mods) is owned by 02 (`world/meta/Attribute`).
- `CachedTile`: event-free, module-stubbed tile used by `MapIO::generate_preview` (04 §3.9). Rust shape: `CachedTiles` owns `Vec<CachedTile { floor: BlockId, overlay: BlockId, block: BlockId, build: bool, team: u8 }>` plus a single reusable `CachedBuild` stub; `create(x,y,floor,overlay,wall)` never fires events and never touches proximity. 04/19 consume this; no `Entity` is spawned.
- `TileGen`: `{ floor: BlockId = stone, block: BlockId = air, overlay: BlockId = air }` with `reset()`; mutated by `PlanetGenerator::gen_tile` and the editor generator (19).

### 3.8 Generation filters

```rust
pub struct GenerateInput { pub x: i32, pub y: i32, pub width: i32, pub height: i32,
    pub floor: BlockId, pub block: BlockId, pub overlay: BlockId, pub packed_data: i64,
    pass: FilterPass }

pub trait GenerateFilter: Send + Sync {
    fn options(&self) -> Vec<FilterOption>;                 // data descriptors; UI in plan 14
    fn apply(&mut self, input: &mut GenerateInput) {}
    fn is_buffered(&self) -> bool { false }
    fn is_post(&self) -> bool { false }
    fn icon(&self) -> char { '\0' }
    fn seed(&self) -> i32; fn set_seed(&mut self, s: i32);
    fn simple_name(&self) -> &'static str;                  // class name minus "Filter", lowercased
    fn apply_tiles(&mut self, tiles: &mut Tiles, input: &mut GenerateInput, rng: &mut SimRng);
}

pub struct FilterRegistry(IndexMap<&'static str, fn() -> Box<dyn GenerateFilter>>);
```

- **`FilterRegistry` mirrors `Maps.allFilterTypes` in order** and defines JSON class tags as `Strings.camelize(class name minus "Filter")`: `noise, scatter, terrain, distort, riverNoise, ore, oreMedian, median, blend, mirror, clear, coreSpawn, enemySpawn, spawnPath, logic`. Tags are registered with 04's `ClassTagRegistry`; `RandomItemFilter` is ported and constructible but **not** registered (matches upstream).
- **`FilterOption`** becomes a data enum (`Slider{name,min,max,step,display}`, `BlockPicker{name,predicate}`, `Toggle{name}`, `Custom{name, action_key}`); plan 14 renders widgets and plan 19 supplies the editor flow. Block predicates (`floors_only`, `walls_only`, `floors_optional`, `walls_optional`, `walls_ores_optional`, `ores_only`, `ores_floors_optional`, `any_optional`) preserve the `!headless && atlas.isFound(icon) && inEditor` shape, with the atlas check behind a plan-03 `IconLookup` trait (headless: `in_editor` + kind checks only).
- **Serialization**: each filter serializes its public fields plus `seed`; content fields as names through 04's content serializers (OD9 camelCase field names). `Maps::read_filters(str)` returns the parsed stack, or the default stack on any error (Java behavior).
- **Default stack**: for every `BlockDef` that is a floor, `in_editor`, and `decoration != air` → `ScatterFilter { flooronto: floor, block: decoration }`; then `add_default_ores` for every overlay with `ore_default` → `OreFilter { ore, threshold: ore_threshold, scl: ore_scale }`. Iteration over content ID order, not hash order.
- **`is_post`**: applied during generation like any other filter; plan 19 skips them for editor undo (Java `isPost` semantics).

#### 3.8.1 `apply_tiles` (buffered/unbuffered) — deviation §2.3.4

- Build `states: Vec<PackedState { block, floor, overlay, packed_data }>` from `tiles` (one allocation per pass).
- Unbuffered: for each index in flat order: `input.set_from(states[i])`; `filter.apply(input)`; write `input` back into `states[i]` **and** `tiles`; then apply Java's post-steps: if floor changed and `!floor.has_surface() && overlay.needs_surface && overlay is OreBlock` → overlay = air; if neither old nor new block `synthetic()` → `set_block`; `tile.set_packed_data(input.packed_data)`. Because `states` is written through, later `input.tile(x,y)` calls observe earlier writes — the exact Java read-visibility rule.
- Buffered: same loop but only `states` is read (never updated) and results go to `buffer[i]`; after the pass, write `buffer` to `tiles` with the same overlay-surface rule and the `!tile.block().synthetic() && !new.synthetic()` guard; `packed_data` is not transferred (matches Java, which packs only block/floor/overlay in `PackTile`).
- `GenerateInput::tile(fx, fy)` clamps to `[0,width-1]×[0,height-1]` and reads `states`.
- `Maps::apply_filters(tiles, stack, rng)`: for each filter: `filter.randomize(rng)`; `input.begin(width,height)`; `filter.apply_tiles(...)`. `logic` filters call the plan-13 `LogicScriptRunner` hook (default: log `unimplemented` and no-op; plan 13 compiles/executes).

### 3.9 `Maps` registry and `Map`

```rust
pub struct Map {
    pub custom: bool,
    pub tags: IndexMap<String, String>,      // name/author/description/rules/genfilters/build/width/height/steamid/...
    pub file: MapKey,                        // resolved through 04 FileTree/FileSystem
    pub version: i32, pub build: i32,
    pub workshop: bool, pub mod_id: Option<ModId>,
    pub width: i32, pub height: i32,
    pub teams: BTreeSet<u8>, pub spawns: i32,   // from cache file, filled by 06/19
}
#[derive(Resource)] pub struct Maps { maps: Vec<Map>, shuffle_mode: ShuffleMode,
    provider: Option<Box<dyn MapProvider>>, next_override: Option<MapKey>,
    preview_list: Vec<MapKey>, pvp: &'static [&'static str] }
```

- `Maps::load()`: built-ins `maps/default/<name>.msav` for the 18 `DEFAULT_MAP_NAMES` (fallback `maps/<name>` when `use_default_folder=false`), then `Paths::custom_maps()` walk (`.msav` case-insensitive), then mod maps via `ModMapProvider` (20), then workshop via `Platform::workshop_maps` (22 stub) — each via 04 `MapIO::create_map` (meta only), name-empty → error, then `maps.sort()`.
- `Map::rules(base)`: `JsonIO::read` of `tags["rules"]`; planet fallback (`serpulo`→`erekir` when `has_env(Env::scorching)`; null→`serpulo`); empty spawns → `Waves::get()` (11 hook; default empty); errors → `new Rules()` (Java behavior).
- `Map::filters()`: if `tags.build` in `1..83` (and `!= -1`) and `genfilters` empty → empty stack; else `maps.read_filters(tags["genfilters"])`. `build == -1` (unknown/custom) always reads.
- `Maps::save_map(tags, embed_assets)` → 04 `MapIO::write_map`; headless skips preview work; on client it scans tiles for `CoreBlock` teams and `spawn` overlays, queues preview generation (19 renders), writes the cache file, and re-sorts. `import_map` copies then re-previews; `remove_map` deletes file + preview cache. `find_file` uses `sanitize_filename` with `_<n>` suffixing.
- `ShuffleMode::{None, All, Custom, Builtin}` + `MapProvider::next(mode, prev)` = shuffle candidate lists with the Java `Rand`, pick the first valid non-prev map. Validity (`Gamemode`) is supplied by plan 12 through `MapGenHooks::valid_for_mode`; the PvP list (`veins`, `glacier`, `passage`) and `is_pvp(map)` are owned here. `next_map_override` honored first.
- Preview naming: `<name>_v2.png` / `<name>-cache_v2.dat` (workshop: parent-folder name / `-workshop-cache.dat`). Cache file = `u8 version=0`, `i32 spawns`, `i8 team count`, `u8 team ids`; read errors re-queue a preview and leave defaults.
- `MapPreviewLoader` semantics preserved at the path layer: missing/stale/foreign preview → delete + `queue_new_preview`; `create_all_previews` runs after `ClientLoadEvent` via the 05 `post` queue. Actual pixels/PNG/`Texture2D` are 19.

### 3.10 Generators

- `WorldGenerator` trait: `generate(&mut self, tiles: &mut Tiles, params: &WorldParams)` + `post_generate(&mut self, tiles: &mut Tiles) {}`.
- `BasicGenerator` struct: `width/height/tiles` scratch + `rand: SimRng` + `floor/block/ore: Option<BlockId>` (Java's mutable draw fields) + `default_loadout: LoadoutId`; all helpers ported 1:1 (`pass` reads the three fields out, calls the closure, writes floor via `set_floor`, block via `set_block` when changed, overlay via `set_overlay`). `noise(x,y,octaves,falloff,scl,mag)` is abstract; `PlanetGenerator` implements it via `sector.rect.project` + `Simplex::noise3d`; a flat `Simplex2D` implementation is used by tests/filters.
- `PlanetGenerator`: `base_seed`, `seed`, `sector` view, `generate_sector`, campaign callbacks (`on_sector_captured/lost`, `before_save_write`, `allow_landing`, `find_launch_candidate`, `add_weather`, `get_locked_text`), `get_size_scl`/`get_sector_size`, `gen_tile(Vec3, &mut TileGen)`, `generate(tiles, sector, params)` (seed = `params.seed_offset + base_seed`, `rand.set_seed(sector.id + seed_offset + base_seed)`, per-tile `sector.rect.project(x/w, y/h)`, construct tiles), then dispatch to the generator body. `HexMesher` trait (`get_height`, `get_color`, `get_emissive_color`, `is_emissive`) is a data-only trait consumed by 16's g3d renderer.
- Campaign/runtime dependencies are trait hooks with safe defaults so 06 runs before 11/12:
  - `SectorView` (id, planet, preset, rect, threat, enemy-base flags, allow-launch-loadout, size) implemented by 12;
  - `SchematicHooks::place_launch_loadout/place_loadout/rotate` (12; default no-op + debug counter);
  - `BaseRegistryView` (cores/parts/for_resource/ores/ore_floors) supplied by 11; default empty makes `BaseGenerator::generate` early-return exactly like Java's "no loaded schematics" path;
  - `Waves::generate` hook (11); default empty spawn list;
  - `RulesWriter` for the fields planet generators mutate (`waves`, `waveSpacing`, `attackMode`, `enemyCoreBuildRadius`, `env`, `placeRangeCheck`, `dragMultiplier`, `borderDarkness`, `hideSpawns`, `planetBackground`, `spawns`) — plan 05 provisions the subset; the rest are recorded as pending until 12 and are excluded from golden checksums until then (§8 R3).
- `FileMapGenerator`: resolves candidate names (`<planet>/<map>`, `<map>`, mod-stripped variants) through the 03 `FileTree`, loads via 04 `SaveIO` with a `FilterContext` whose `end()` applies filters without double-firing `WorldLoadEvent`, then resolves cores/loadout (`corePositionOverride`, `allowLaunchLoadout`, `addStartingItems`), throws `All maps must have a core.` when none.
- `SectorDamage`: full port (frontier from enemy cores/spawn overlays, A* to the player core, path damage by health fraction, radial propagation with `falloff`, rubble hook to 17, core health floor). Uses `world::generation::astar` and `Groups`.
- `world::generation::astar`: deterministic A* (`BinaryHeap` by `(cost, insertion seq)`, 4/8-neighbour, `Astar::manhattan` heuristic, `passable` closure), used by generators/filters and shared with 11 (§8 R4).
- `math::noise`: `Simplex::noise2d/noise3d(seed, octaves, falloff, scl, x/y/z)` and `Ridged::noise2d/noise3d` ports with Arc's permutation-table construction, plus `Mathf` helpers generators need (`pow`, `clamp`, `lerp`, `dst`, `within`, `rand_range`, `chance`) sourced from 05 where they already exist.

### 3.11 Vanilla planet generators

Full ports of `SerpuloPlanetGenerator` (755 lines: terrain matrix, `rawHeight`, `getBlock`, room graph, rivers, shorelines, ores, ruins via `BaseGenerator`, spawn/enemy placement, wave rules), `ErekirPlanetGenerator` (434 lines: height/temp fields, `getBlock`, regolith walls, arkycite/slag, vents with `SteamVent.offsets`, ores/crystals), `TantrosPlanetGenerator` (100 lines: redmat/bluemat field), `AsteroidGenerator` (160 lines: asteroid stamping, craters, ores, `oreAround`/`wallOre`, spawn at map edge, planet background params). Their `rand` is the generator's `SimRng`; every `Mathf.rand` use maps to `self.rand` (deviation §2.3.5). Structurally they must reproduce floor/wall/ore distributions; goldens are per-seed checksums recorded once by the Rust build (not Java).

### 3.12 Schedule, Godot and STDB touchpoints

- **Schedule (05).** `WorldPlugin` registers: `WorldLoadApply` at `IoSet::Apply` (04's queue: `SaveIo` load requests → `WorldContext` end); no per-tick world systems other than the tile-change consumers already owned by other plans. `load_sector`/`load_map`/`load_generator` are called by 12/19/22 outside the tick, or by tests directly.
- **Godot (16).** No scenes added here. 06 exposes read APIs only: `WorldGrid::tile`, `Tiles` iteration, `CacheLayerId` ordering (init order: `water=0, mud=1, tar=2, slag=3, arkycite=4, cryofluid=5, space=6, normal=7, walls=8` — **not** the field-declaration order), `Floor`/`BlockDef` data accessors, and `RenderHooks` (`recache_tile`, `recache_wall`, `add_floor_index`, `remove_floor_index`, `invalidate_tile`, `minimap_update`) with no-op defaults. Plan 16 implements the trait and owns all rendering.
- **STDB.** No tables/reducers/views. Map tags/`genfilters` JSON and `WorldParams` serde shapes are shared with plans 12/21; 06 adds round-trip tests to keep them stable. D2 unchanged.

### 3.13 Boundaries & invariants

1. All tile mutation is centralised in `world/tile.rs`; no other module writes `Tile` fields directly (fields are `pub` for 07/16 but access is by convention + clippy review).
2. `generating == true` suppresses all tile events, recache and proximity side effects; `SaveIo` sets it via `begin()/end()`.
3. `Tile.x/y: i16` and packed `pos()` are ABI; never widen.
4. `Tiles` never allocates per access; `Vec` growth only on `resize`.
5. `mind-core` stays Godot-free; preview pixels never enter core (04 §2.3.7).
6. No `HashMap` iteration in generation/checksum paths.
7. Every ported file carries the GPL header; content names/keys are ABI.

---

## 4. Port map

| Mindustry source | Rust target | Notes |
|---|---|---|
| `core/World.java` | `world/mod.rs`, `world/context.rs`, `world/darkness.rs`, `world/raycast.rs` | `WorldGrid` resource; contexts implement 04's trait; `addDarkness`/`getDarkness`/`getWallDarkness`; Bresenham raycasts shared with 10/11. |
| `world/Tile.java` | `world/tile.rs` | Packed fields, multiblock link loop, event suppression, `@Remote` helpers become plain ops (21 wraps). |
| `world/Tiles.java` | `world/tiles.rs` | Flat grid + puddle/fire slots + tmp states; iteration order contract. |
| `world/WorldContext.java` | trait in 04; impl in `world/context.rs` (`Context`, `FilterContext`) | 04 owns the trait (04 §3.1); no duplicate. |
| `world/WorldParams.java` | `world/params.rs` | `seed_offset`, `save_info`, `core_position_override`. |
| `world/TileGen.java` | `world/cached.rs` (`TileGen`) | Brush used by planet gen and editor. |
| `world/CachedTile.java` | `world/cached.rs` (`CachedTile`/`CachedTiles`) | Event-free preview view; consumed by 04 §3.9 / 19. |
| `world/Edges.java` | `world/edges.rs` | Sort-order parity asserted by `ApplicationTests.edges`. |
| `world/ColorMapper.java` | `world/color_mapper.rs` | Rebuilt by 02's loader; used by 19 image import. |
| `world/ItemBuffer.java`, `world/DirectionalItemBuffer.java` | **not here** — plan 08 | Explicit deferral (assignment + §2.3.8). |
| `world/blocks/Attributes.java` | `world/attributes.rs` | `Attribute` ids stay in 02 (`world/meta/Attribute`). |
| `world/blocks/environment/*` movement/liquid/attribute data | consumed from 02's `BlockDef` (`FloorDef` fields) | 06 defines the accessor contract only; behavior in 07, draw in 16. |
| `graphics/CacheLayer.java` (ordering only) | `world::CacheLayerId` hint enum | Ids follow `CacheLayer.init()` order, not field order; renderer in 16. |
| `maps/Maps.java` | `maps/mod.rs`, `maps/shuffle.rs` | Registry, load/save/import/remove, filter registry host, `ShuffleMode`. |
| `maps/Map.java` | `maps/map.rs` | Tags, `rules()`, `filters()`, preview/cache paths, `compareTo`. |
| `maps/MapException.java` | `maps/error.rs` | Carries the offending map. |
| `maps/MapPreviewLoader.java` | `maps/preview.rs` (paths/queue/cache) + plan 19 (texture) | Delete-and-requeue resilience kept at the path layer. |
| `maps/SectorDamage.java` | `maps/sector_damage.rs` | `Effect::rubble` behind a plan-17 hook. |
| `maps/SectorSubmissions.java` | plan 12/22 | Out of scope. |
| `maps/filters/GenerateFilter.java` | `maps/filters/mod.rs` | Trait + `GenerateInput` + `PackTile`; buffered/unbuffered adaptation §2.3.4. |
| `maps/filters/FilterOption.java` | `maps/filters/option.rs` | Data descriptors + predicates; widgets in 14. |
| `maps/filters/*Filter.java` | `maps/filters/*.rs` | 15 registered + `random_item.rs`; class tags frozen. |
| `maps/generators/WorldGenerator.java` | `maps/generators/mod.rs` | Trait + `GenerationCtx`. |
| `maps/generators/BasicGenerator.java` | `maps/generators/basic.rs` | All helpers; `pass` field-mutation semantics preserved. |
| `maps/generators/BaseGenerator.java` | `maps/generators/base.rs` | `BaseRegistryView`/`SchematicHooks` defaults; early-return on empty bases. |
| `maps/generators/PlanetGenerator.java` | `maps/generators/planet.rs` | + `HexMesher` data trait (16 consumes). |
| `maps/generators/BlankPlanetGenerator.java` | `maps/generators/blank_planet.rs` | `fill()` + `generate()`. |
| `maps/generators/FileMapGenerator.java` | `maps/generators/file_map.rs` | Core/loadout resolution via 12 hooks. |
| `maps/planet/SerpuloPlanetGenerator.java` | `maps/planet/serpulo.rs` | Full port incl. rooms/rivers/ruins. |
| `maps/planet/ErekirPlanetGenerator.java` | `maps/planet/erekir.rs` | Full port incl. vents/crystals. |
| `maps/planet/TantrosPlanetGenerator.java` | `maps/planet/tantros.rs` | Full port. |
| `maps/planet/AsteroidGenerator.java` | `maps/planet/asteroid.rs` | + `state.rules.planetBackground` params (12). |
| `ai/Astar.java` (grid-search subset) | `maps/generators/astar.rs` | Shared with 11 (reconcile §8 R4). |
| `arc.util.noise.Simplex`, `Ridged`, `Geometry`, relevant `Mathf` | `math/noise.rs` (+ 05 `math/`) | Additive to 05's module; reconcile §8 R6. |

---

## 5. Milestones & task breakdown

Each milestone ends with evidence (test output / dump path / checksum / screenshot) appended to the Changelog. Order is strict.

**M0 — Tiles + Tile foundation (smallest vertical slice).**
- `world/pos.rs`, `tile.rs`, `tiles.rs`, `cached.rs`, `params.rs`, `world/events.rs` payloads, `world/mod.rs` with `WorldGrid` + tile ops for floor/overlay/block **without** buildings (build hook returns `None`), `WorldPlugin`.
- Tests: `world::tests::{create_map_resize_fill, tile_get_getn_getc_geti_in, tile_packed_data_roundtrip, fill_resets_grid}`; scenario `world_tile_ops`.
- Verify: `cargo test -p mind-core world::`; `mind-headless world tile-ops --seed 1 --dump out/tiles.json`.

**M1 — Edges, Attributes, ColorMapper, counters.**
- `edges.rs`, `attributes.rs`, `color_mapper.rs`, counter increments, `util::strings::strip_colors/sanitize_filename`.
- Tests: `world::edges::tests::edge_order` (ported `ApplicationTests.edges`), `attributes::tests::json_roundtrip_names`, `color_mapper::tests::rgba_lookup`.
- Verify: `cargo test -p mind-core world::edges world::color_mapper`.

**M2 — World methods + contexts (04 seam).**
- `resize/clear_buildings/accessors/conv/unconv/to_tile/pack_array/is_in_map_area`, `begin_map_load/end_map_load`, `load_generator`, raycasts, `world/context.rs` implementing 04's `WorldContext` for `Context`/`FilterContext`/sector context.
- Coordinate the 04 §5 M4 stub→real swap; `mind-headless io roundtrip` uses the real context.
- Verify: `world::tests::{begin_end_map_load_events, raycast_dda, is_in_map_area}`; `mind-headless io roundtrip --map fixtures/flat64.msav` green (with 04).

**M3 — Buildings linkage, multiblock, darkness.**
- `WorldHooks`/`RenderHooks` traits, `new_building` fallback bare entity, `change_build`, multiblock two-pass, proximity calls, `legacy_remove_self`, `add_darkness/get_darkness/get_wall_darkness`, `load_map` skeleton, `check_map_area`.
- Tests: `world::tests::{multiblock_linkage, multiblock_overlap_removed, darkness_bfs_reference, tile_events_suppressed_while_generating}`; scenario `world_multiblock`.
- Verify: `mind-headless world multiblock --block test-core-shard --size 3 --dump out/mb.json`; assertions listed in §7b.

**M4 — Maps registry + Map metadata + shuffle + preview hooks.**
- `maps/mod.rs`, `map.rs`, `error.rs`, `shuffle.rs`, `preview.rs`; built-in list resolution through 04 `FileTree`; save/import/remove; cache file IO; `pvp` list.
- Tests: `maps::tests::{map_tags_rules_fallback, build_lt_83_filters_empty, shuffle_none_all_custom_builtin, preview_cache_roundtrip, corrupt_map_skipped}`; scenario `maps_list`.
- Verify: `mind-headless maps list --dir tests/fixtures/maps` prints sorted meta; corrupt file skipped with warning.

**M5 — Generation filters.**
- `filters/mod.rs` (trait + `GenerateInput` + buffered engine), `option.rs`, all 16 filter files, `FilterRegistry`, class tags with 04, default stack.
- Tests: `maps::filters::tests::{class_tags_roundtrip, buffered_reads_pre_state, unbuffered_reads_write_through, default_stack_order, filter_json_roundtrip}`; scenario `world_filters_order`.
- Verify: `mind-headless world filters --map fixtures/flat64.msav --stack scatter,ore,median --order forward --dump fwd.json` == golden; reverse differs.

**M6 — BasicGenerator + noise + WorldGenerator + FileMapGenerator + BaseGenerator.**
- `math/noise.rs`, `generators/{mod,basic,file_map,base,astar}.rs`; hooks for schematics/bases/waves.
- Tests: `maps::generators::tests::{noise_seeded_stable, basic_pass_writes_fields, file_map_requires_core, base_generator_empty_registry_returns}`; scenario `world_gen_simplex`.
- Verify: `mind-headless world gen --generator simplex --seed 7 --width 128 --height 128 --dump out/simplex.json` reproduces checksum across processes.

**M7 — PlanetGenerator/Blank + planet smoke.**
- `generators/planet.rs`, `blank_planet.rs`, `HexMesher`; `load_sector` with `SectorView`/`RulesWriter` hooks; scenario `world_gen_planet`.
- Tests: `planet::tests::{seed_formula, sector_size_even, blank_fills, project_positions}`.
- Verify: `mind-headless world gen --planet tantros --sector 0 --seed 11 --dump out/tantros.json`.

**M8 — Vanilla planet generators.**
- `planet/serpulo.rs`, `erekir.rs`, `tantros.rs`, `asteroid.rs`; per-seed goldens recorded.
- Tests: `planet::serpulo::tests::{generation_checksum, has_core_region, ore_present}`, equivalent Erekir/Tantros/Asteroid; `sector_damage::tests::{frontier_damage, full_fraction_kills_cores}` (with a test `SchematicHooks`/empty effects).
- Verify: `mind-headless world gen --planet serpulo --sector 0 --seed 42 --iters 1 --json` under §7d budgets; goldens committed.

**M9 — Save/import polish, MCP, budgets, docs.**
- `save_map`/`import_map`/`remove_map` end-to-end with 04; `MapPreviewLoader` queue semantics; MCP scenario; `bench world-gen`; docs.
- Verify: §7c recorded; `cargo bench -p mind-core --bench world` numbers in Changelog.

---

## 6. Data & formats

### 6.1 Tile/grid invariants

- `tiles[y*width + x]`; `Tile.x/y: i16` in `0..width-1`/`0..height-1` for live tiles.
- `Tile.build`: `Some(center_entity)` on every tile covered by an `hasBuilding()` multiblock; only the center tile's entity has `tile == self`; `is_center()` is `build.is_none() || center == self`.
- `data`: block save data **or** edge darkness for static solid walls (`fillsTile && !synthetic()`), exactly upstream; `floor_data`/`overlay_data` per the block's `saveData` flag; `extra_data: i32`.
- Counters: `tile_changes`/`floor_changes: i32`, reset to `-1` on `WorldLoadEvent`, incremented only when events fire.

### 6.2 Scenario/dump extension (append-only, plan 00 §3.10)

```json
"world": { "generator": "planet", "planet": "serpulo", "sector": 0, "seed_offset": 42,
           "width": 250, "height": 250, "params": { "save_info": true, "core_position_override": 0 } }
```

- `generator` variants: `flat` (plan 00) · `simplex` · `planet` · `file` (`"map": "serpulo/groundZero"`) — append-only.
- Dump `world` section adds: `"generator"`, `"tile_changes"`, `"floor_changes"`, optional sparse `"floors": [{"x","y","floor"}]` and `"counts": { "floors": {...}, "blocks": {...}, "overlays": {...} }` when `--histogram` is passed. Existing keys unchanged.

### 6.3 Preview cache file (`<map>-cache_v2.dat`, parity ABI)

```
u8  version (0)
i32 spawns (big-endian, matches 04 wire convention)
i8  team count
u8* team ids (scan order: rows then columns of the map scan)
```

Read: any error → `queue_new_preview` + defaults (`spawns=0`, empty teams). Write: after `save_map` on client hosts only (headless skips).

### 6.4 `genfilters` JSON

```json
[ { "class": "scatter", "seed": 123456, "chance": 0.013, "flooronto": "moss", "floor": "air", "block": "spore-cluster" } ]
```

- `class` = camelized simple name minus `Filter` (frozen tags per §3.8). Content fields are name strings (04 content serializers with fallback renames). `seed` always serialized. Unknown class tag → 04 `ClassTagRegistry` warn + skip element (map still loads); unknown field ignored; `read_filters("")` or parse error → default stack (upstream behavior).

### 6.5 IDs/keys

- `BlockId`/`ItemId`/`LiquidId`/`PlanetId` are plan-02 append-only `u16` spaces — tile save data and `genfilters` serialization depend on them.
- Built-in map names (18) and PvP names (3) are string ABI lists: `maze, fortress, labyrinth, islands, tendrils, caldera, wasteland, shattered, fork, triad, mudFlats, moltenLake, archipelago, debrisField, domain, veins, glacier, passage`; PvP = `veins, glacier, passage`.
- Attribute names: `heat, spores, water, oil, light, sand, steam` (+ mod-added, append-only).

### 6.6 Files/dirs

- Built-ins: `<assets>/maps/default/<name>.msav`, campaign `<assets>/maps/{serpulo,erekir}/...`, hidden `<assets>/maps/serpulo/hidden/<id>.msav` (asset pipeline plan 03).
- Custom: `<data-root>/maps/*.msav`; previews: `<data-root>/previews/<map>_v2.png`, `<map>-cache_v2.dat`; paths resolved by 04 `Paths`.
- Test fixtures: `client/rust/mind-core/tests/fixtures/maps/` (tiny generated `.msav`s written by plan 04 + copies for parity), `tests/golden/world_gen_*.checksum`.

---

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests (`Mindustry/tests/src/test/java/**` → `cargo test -p mind-core`)

| Upstream test | Rust test | Notes/owner |
|---|---|---|
| `ApplicationTests.createMap` | `world::tests::create_map_resize_fill` | 8×8 resize + fill + begin/end load; ported exactly. |
| `ApplicationTests.playMap` | `world::tests::load_internal_map_ground_zero` | Needs 02/04 fixtures; uses a `maps/` fixture until 03 assets land (`#[ignore = "plan 03 assets"]` for the real file, un-ignored at M9). |
| `ApplicationTests.edges` | `world::edges::tests::edge_order` | Exact order + `edges(2).len() == 8`. |
| `ApplicationTests.blockOverlapRemoved` | `world::tests::multiblock_overlap_removed` | Multiblock overlap clear semantics (entities via `WorldHooks`). |
| `ApplicationTests.multiblock` | `world::tests::multiblock_linkage` | 3×3 center + proxies share one entity; team propagation. |
| `ApplicationTests.blockInventories` | `world::tests::multiblock_shared_modules` | World half only; module behavior owned by 07/08 (`#[ignore = "plan 08"]` marker noted). |
| `ApplicationTests.save` (world half) | `world::tests::save_then_resize_clears_buildings` | Actual save I/O in 04. |
| `ApplicationTests.saveLoad` (world half) | `world::tests::save_load_preserves_world_size` | 04 owns the save path; asserts `width/height` + tile spot-check through `WorldContext`. |
| `ApplicationTests.load77...152Save` (world half) | `world::tests::legacy_size_<n>` gated by 04 `msav-import` | World sizes per upstream; full fixtures in 04 §7a. |
| `ApplicationTests.buildingOverlap` (linkage half) | `world::tests::edge_tile_proxy_cleanup` | Placement rules themselves are 07. |
| `ApplicationTests.testSectorValidity` (world half) | `world::tests::sector_rules_hooks_called` | Full validity in 12. |
| `ApplicationTests.arrayIterators` (world iterator half) | `world::tiles::tests::iteration_order_is_row_major` | Order contract. |
| (new, ported from map-generation behavior) | `maps::tests::{default_filter_stack_matches_ore_defaults, filters_for_old_build_empty, shuffle_modes}` | No upstream unit test exists; asserts documented behavior. |
| (new, from `Map`/`Maps`) | `maps::tests::{map_tags_roundtrip, preview_paths, corrupt_cache_falls_back}` | |

### 7b. Headless harness scenarios (`mind-headless`)

| Command | Script | Assertions |
|---|---|---|
| `mind-headless world gen --planet serpulo --sector 0 --seed 42 --dump out/gen1.json` | generate once; run twice in-process + once in a fresh process; `--checksum-every 0` | all dump `checksum` fields equal the committed golden; floor/block histogram golden; no panic on 250×250 |
| `mind-headless world gen --planet {erekir,tantros,asteroid} --sector 0 --seed 7 --dump` | smoke | identical across two processes; `erekir` has ≥1 vent or crystal overlay; `asteroid` has background + ≥1 asteroid floor; `tantros` has redmat/bluemat |
| `mind-headless world tile-ops --seed 1 --width 64 --height 64 --ops 10000 --dump` | 10k deterministic interleaved `set_floor`/`set_overlay`/`set_block`/`set_air` ops from a seeded generator | tile histogram matches expected counts; `tile_changes`/`floor_changes` equal event counts (asserted via dump); completes < 1 s release |
| `mind-headless world multiblock --size 3 --block core-shard --dump` | place center (4,4), assert 9 tiles share one build; place a second overlapping at (2,2) → first fully cleared; break center → 9 air + one entity removed | matches `blockOverlapRemoved`/`multiblock` semantics |
| `mind-headless world filters --map fixtures/flat64.msav --stack scatter,ore,median,blend --order forward --dump fwd.json` | apply stack; then `--order reverse`; then buffered-only and unbuffered-only stacks | forward == committed golden; forward != reverse; buffered output independent of write-through state; `packed_data` preserved on unbuffered and dropped on buffered |
| `mind-headless maps list --dir tests/fixtures/maps --json` | listing incl. one corrupt file | sorted per `compare_to`; corrupt skipped with warning; tags/width/height correct |
| `mind-headless world load-roundtrip --map fixtures/flat64.msav` (with 04) | save → reset → load | tile checksum equal (04's `io roundtrip` scenario extended) |

### 7c. MCP playtest scenario (open-godot-mcp)

Preconditions: plan-00 spine built (`tools/build.sh`); `godot_health check`; if `BRIDGE_NOT_CONNECTED` launch the editor per the repo skill; node paths per plan 00 §3.5 (`/root/Spine/SimHost`, `/root/Spine/World/TileGrid`, `/root/Spine/World/Camera2D`, `/root/Spine/Ui/StateInspector/Label`). Plan 05 assumes `/root/Main/SimBridge`; plan 00 is authoritative — adapt per §8 R5.

1. `godot_game play` with `scene: "res://scenes/spine.tscn"`.
2. Pid-stamp: `godot_exec eval {"code": "return {\"pid\": OS.get_process_id(), \"w\": get_node(\"/root/Spine/SimHost\").world_width()}"}`; compare with `godot_game instances`.
3. Load a generated world: `godot_exec call /root/Spine/SimHost load_scenario ["res://scenarios/world_gen_serpulo.json"]` → `true`; assert `world_width() > 32` and the inspector `world` section shows non-zero floor/wall counts (spine grid "grows into generated terrain").
4. Determinism: set `set_paused(true)` so generation happens once, then `godot_exec eval` compares `get_checksum()` with the golden checksum recorded in the scenario file.
5. Tile probe: pick a generated air tile via a new append-only test API `get_node("/root/Spine/SimHost").find_tile("air")`, then `place_block [x, y, "stone-wall"]` → `true`; eval the tile JSON shows the block and `tile_changes` increased; `break_block` → tile `air` and `tile_changes` increased again.
6. Camera pan: `godot_exec call /root/Spine/World/Camera2D center_on_tile [128,128]`; then `godot_input mouse_motion` drag across the viewport (or WASD via `godot_input key`); eval `screen_to_tile` center changed and `tile_to_screen` round-trips; `godot_screenshot game` records the panned view.
7. Visual: screenshot shows floor colors/terrain detail distinct from the P0 flat grid (non-blank PNG read back).
8. Logs: `godot_log errors` empty; no "unimplemented hook" errors beyond the allow-list (generation hooks).
9. Teardown: `godot_game stop`; record screenshot + checksums in the Changelog.

`tools/mcp-smoke.sh` gains steps 1–5 as a non-visual smoke variant.

### 7d. Performance budget + measurement

Benchmarks: `mind-headless bench world-gen --planet X --sector N --seed S --iters 20 --json` (p50/p95) and `cargo bench -p mind-core --bench world` (criterion; per-phase attribution: noise, `pass`, filters, darkness). Baseline 256×256 (Tantros/Asteroid 500×500 variants). Dev-machine release numbers; CI records, plan 23 gates.

| Metric | Budget (release, dev machine) |
|---|---|
| `PlanetGenerator` full `generate()`, 256×256 (Serpulo/Erekir) | p50 ≤ 120 ms, p95 ≤ 250 ms |
| `BlankPlanetGenerator` + simple noise, 500×500 (Asteroid) | p50 ≤ 80 ms, p95 ≤ 180 ms |
| One unbuffered filter pass, 256×256 | p95 ≤ 3 ms |
| Buffered `MedianFilter` radius 2, 256×256 | p95 ≤ 20 ms |
| `add_darkness`, 256×256 | p95 ≤ 5 ms |
| `Tiles::set_floor` per tile | ≤ 100 ns |
| `Tiles::set_block` (no building) per tile | ≤ 250 ns |
| Full-grid write loop (65 536 tiles) through `pass` | ≤ 15 ms |
| `size_of::<Tile>()` | ≤ 40 bytes; 256×256 grid ≤ 2.7 MiB |
| `maps list` of 100 fixtures (meta-only, parallel) | p95 ≤ 150 ms (04 parallel listing) |

Regression policy: +50% over committed baseline blocks the milestone; +20% warns (plan 00 §7d / plan 23 owns the gate).

### 7e. Exit criteria checklist

- [ ] `cargo test -p mind-core` green; §7a rows implemented or explicitly `#[ignore = "plan NN"]` with an owner.
- [ ] `cargo fmt --check` + `cargo clippy -p mind-core -- -D warnings` clean; no `HashMap` iteration in `world/`/`maps/` generation paths.
- [ ] `mind-headless world gen` golden checksums reproduce in-process, cross-process, and with `--workers 1` vs default.
- [ ] `world_tile_ops` counters match event counts; `world_multiblock` matches `blockOverlapRemoved`/`multiblock`.
- [ ] `world_filters_order` forward golden + order sensitivity + buffered/unbuffered visibility asserted.
- [ ] Real `WorldContext` implementation used by 04 `io roundtrip`; `begin/endMapLoad` events and `generating` suppression verified.
- [ ] Edge-darkness BFS output matches the reference grid; `tile.data` overload documented and asserted with 07.
- [ ] `maps list` sorted correctly incl. corrupt-file skip; preview cache round-trips; save/import/remove delete preview files.
- [ ] MCP scenario §7c executed with logs/screenshot/checksum evidence; generated terrain visible and camera pans.
- [ ] §7d budgets measured and recorded; no regression > 50% vs baseline.
- [ ] `mind-core` Godot/tokio-free; GPL headers on every ported file; `math/noise.rs` merge with 05 recorded.
- [ ] Reconciliation notes for 04/05/07/11/12/16/19 recorded in this file's Changelog.

---

## 8. Risks & open decisions

Each item has the default this plan proceeds with. Items marked **NEEDS USER DECISION** are load-bearing and not covered by `HIGH_LEVEL_PLAN.md`; execution continues on the stated default unless the user overrides.

| # | Decision / risk | Default being planned against | Needs user? |
|---|---|---|---|
| OD6-A | **Generation RNG determinism.** Java's filter `randomize()` and several generators read Arc's process-global `Mathf.rand`, whose state depends on prior activity — it cannot be replayed deterministically. | Explicit `SimRng::MapGen` stream seeded from `(world seed, map name hash)`; `BaseGenerator`'s `Mathf.rand.setSeed(sector.id)` re-seed maps to a named stream; every generator gets its own seeded `Rand`. Replay-stable across processes; structural output may differ from Java (see OD6-B). | **NEEDS USER DECISION** |
| OD6-B | **Exact Java terrain parity.** | Structural parity + Rust↔Rust determinism only (HLP §9). `Simplex`/`Ridged` are faithful algorithm ports; goldens are Rust-recorded, not Java-compared. | No (covered by HLP §9) |
| R1 | **`WorldContext` trait shape (04 vs 06).** 04 defines `io::save::WorldContext`; if 04's M4 stub differs from §3.5 (`create(x,y,floor,overlay,wall)` id-based), the swap breaks. | Treat 04 §3.1/§5 M4 as authoritative; 06 adapts to the shipped trait and records the exact signature in the Changelog. No duplicate trait. | Orchestrator reconcile with 04 |
| R2 | **07 seam.** 06 needs `newBuilding`, `onRemoved`, proximity, `blockChanged`, `floorChanged`, `legacy remove` before 07 exists. | `WorldHooks`/`RenderHooks` traits with no-op defaults; `new_building` falls back to a bare `BuildingComp` entity so multiblock scenarios pass; 07 replaces the implementation without changing Tile call sites. | Orchestrator reconcile with 07 |
| R3 | **12/11 dependencies inside planet generators.** `state.rules` mutations, `Schematics.placeLaunchLoadout`, `BaseRegistry`, `Waves.generate`, `Sector` views, `Gamemode` validation. | `SectorView`/`SchematicHooks`/`BaseRegistryView`/`Waves`/`MapGenHooks::valid_for_mode` traits with safe defaults (empty bases → early return, no-op schematics); fields not yet on plan 05's `Rules` boundary are excluded from golden checksums until 12 lands. | Orchestrator reconcile with 11/12 |
| R4 | **Astar ownership (06 generators vs 11 pathfinding).** | 06 ships `world::generation::astar` as a deterministic grid search for generation/filters only; 11 may reuse or wrap it for unit pathing but must not break its API. | Orchestrator reconcile with 11 |
| R5 | **MCP node paths differ between 05 and 00.** Plan 05 assumes `/root/Main/SimBridge`; plan 00 ships `/root/Spine/SimHost`. | Plan 00 §3.5 paths are authoritative; §7c updates when plan 00 lands (same note as 05 §8 R10). | No |
| R6 | **Noise/math module ownership.** 05 owns `mind_core::math`; 06 adds `noise.rs`. | Additive file in `math/`; if 05 lands a conflicting `noise`, 06 re-exports rather than duplicates; CI merge note in Changelog. | Orchestrator reconcile with 05 |
| R7 | **`CacheLayer` id ordering confusion.** Field declaration order differs from `CacheLayer.init()` `addLast` order. | Export the init order (`water,mud,tar,slag,arkycite,cryofluid,space,normal,walls`) as `CacheLayerId`; plan 16 maps its render layers to these ids. | Orchestrator reconcile with 16 |
| R8 | **Preview pipeline split 04/06/19.** 04 §3.9 defines `generate_preview`; 19 owns pixels/PNG/texture; 06 owns paths/queue/cache. | Interfaces: 06 exports `preview_file`/`cache_file`/`queue_new_preview`/`read_cache`/`write_cache`; 19 consumes. No pixel code in core. | Orchestrator reconcile with 04/19 |
| R9 | **`tile.data` overload (block save data vs static darkness).** | Preserved 1:1; `add_darkness` only writes `data` on `is_darkened()` static walls; 07 must route block `saveData` through the same field only when `tile.should_save_data()`; asserted at the seam. | Orchestrator reconcile with 07 |
| R10 | **Checksum compatibility.** 05's `CHECKSUM_VERSION = 1` includes a placeholder `WorldGrid::checksum_part()`; 06 replaces it. | Replace in the same commit that updates plan-05/23 goldens (plan 00 §6.4 invariant: changing the stream is a breaking change updating all goldens together). | Orchestrator reconcile with 05/23 |
| R11 | **Built-in `.msav` assets are not available until plan 03.** | Tests use generated fixtures; `Maps::load` resolves through the 03 `FileTree` when present; `playMap` test ignored with the plan-03 marker. | No |
| R12 | **Traversal determinism of `Map.tags`.** Java `StringMap` is an `ObjectMap` (hash order); serialized tag order could vary. | `IndexMap` preserving insertion order (file/meta read order + editor writes); no hash iteration; serialized output is deterministic. | No |

---

## 9. References

### Mindustry sources read

- `core/src/mindustry/world/AGENTS.md`, `core/src/mindustry/maps/AGENTS.md`, `core/src/mindustry/io/AGENTS.md` (map/save context), `core/src/mindustry/graphics/AGENTS.md`, `core/src/mindustry/world/blocks/AGENTS.md`.
- `core/src/mindustry/core/World.java`, `core/World` AGENTS (`core/AGENTS.md`).
- `core/src/mindustry/world/Tile.java`, `Tiles.java`, `WorldContext.java`, `WorldParams.java`, `TileGen.java`, `CachedTile.java`, `Edges.java`, `ColorMapper.java`, `ItemBuffer.java`, `DirectionalItemBuffer.java`.
- `core/src/mindustry/world/blocks/Attributes.java`, `world/blocks/environment/{Floor,OverlayFloor,OreBlock,StaticWall,Prop,Cliff,ShallowLiquid,SteamVent,SpawnBlock}.java`, `world/meta/Attribute.java`.
- `core/src/mindustry/maps/Maps.java`, `Map.java`, `MapException.java`, `MapPreviewLoader.java`, `SectorDamage.java`, `SectorSubmissions.java`.
- `core/src/mindustry/maps/filters/{GenerateFilter,FilterOption,NoiseFilter,ScatterFilter,TerrainFilter,DistortFilter,RiverNoiseFilter,OreFilter,OreMedianFilter,MedianFilter,BlendFilter,MirrorFilter,ClearFilter,CoreSpawnFilter,EnemySpawnFilter,SpawnPathFilter,LogicFilter,RandomItemFilter}.java`.
- `core/src/mindustry/maps/generators/{WorldGenerator,BasicGenerator,BaseGenerator,PlanetGenerator,BlankPlanetGenerator,FileMapGenerator}.java`.
- `core/src/mindustry/maps/planet/{SerpuloPlanetGenerator,ErekirPlanetGenerator,TantrosPlanetGenerator,AsteroidGenerator}.java`.
- `core/src/mindustry/graphics/CacheLayer.java` (ordering), `Vars.java` (constants).
- `tests/src/test/java/ApplicationTests.java` (`createMap`, `playMap`, `edges`, `multiblock`, `blockInventories`, `blockOverlapRemoved`, `save`, `saveLoad`, `load77...152Save`), `tests/AGENTS.md`.

### Plan set

- `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 D1–D9, §2.1 layout, §2.4 determinism, §4 template, §5 P3 gate, §6 conventions, §7 verification, §9 parity ledger, §10 OD2).
- `mindustry-godot/PRELIMINARY_PLAN.md` (history).
- `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (spine, scenario/dump/command formats, MCP rig).
- `02_CONTENT_IMPLEMENTATION_PLAN.md` (`BlockDef`/`FloorDef`/`Attribute`/`PlanetDef` metadata, lifecycle, `get_by` lookups).
- `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (`WorldContext` trait, `MGRS` save/map format, `MapIO`/`JsonIO`/`FileSystem`/`Paths`, preview contract, revision discipline).
- `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (`Sim`, schedule slots/`IoSet::Apply`, `EventBus`/`SimEvent`, `Groups`, `EntityIds`, `SimRng`, `ChecksumPart`, `Tmp`, `math/`).
- Sibling plans referenced by name: `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md`, `08_LOGISTICS_IMPLEMENTATION_PLAN.md`, `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md`, `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md`, `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md`, `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md`, `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md`, `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md`.

## Changelog

> Append entries here when execution starts. Every "done" claim carries evidence (command, dump path, screenshot path, checksum).

- 2026-10-01 — Draft v1 written. No implementation started. OD6-A (generation RNG determinism) requires user input before M5; R1–R4, R6–R10 require orchestrator reconciliation with plans 04/05/07/11/12/16/19/23 as noted.

- 2026-10-02 — **M0 COMPLETE (`lane/06-world`, commit `4e83423`).** Real `Tile`/`Tiles`/`WorldGrid` replaces the P0 placeholder (`world/{pos,tile,tiles,events,hooks,params,cached,ops,attributes,color_mapper,edges,darkness,raycast,context,checksum}.rs`). `WorldGrid` owns `Tiles` and the `tile_changes`/`floor_changes` counters; tile ops are centralised in `world::ops::WorldCtx` (floor/overlay/block/air) with upstream counter semantics (`tileChanges` only on `TileChangeEvent`, `floorChanges` only on `TileFloorChangeEvent`). `Sim`/dump/gdext were migrated to the new storage API and the P0 canonical stream is byte-identical: `spine_place_break` → `a1a7b96167c9718d` (goldens preserved, R10 not triggered). New `mind-headless world tile-ops` (counters == event counts, `tile-ops` seed 1: `tile_changes=4165`, `tile_events=4164`) and `world multiblock` (`pass:true`). Tests: `world::tests::{create_map_resize_fill,tile_get_getn_getc_geti_in,fill_resets_grid}`, `world::tiles`, `world::tile`, `world::ops`, `world::darkness`, `world::raycast`, `world::context`, `world::checksum`. Evidence: `cargo test -p mind-core` 287 lib + 2 + 1 passed / 1 ignored; `cargo fmt --all -- --check` + `cargo clippy -p mind-core -p mind-headless --all-targets -- -D warnings` clean.

- 2026-10-02 — **M1 COMPLETE (`lane/06-world`).** `world/edges.rs` ports `Edges` (bot/top formulas, `Mathf.angle` sort; `edge_order` == `ApplicationTests.edges`, `edges(2).len()==8`), `world/attributes.rs` (`Attributes` float vector + name-keyed JSON), `world/color_mapper.rs` (`rgba8888` + `load`/`get` with the `0,0,0,1`→air seed), counter increments (M0 ops), and `util/strings.rs` (`strip_colors`, `sanitize_filename` ported from `arc.util.Strings`). Tests: `world::edges::tests::edge_order`, `world::attributes::tests::json_roundtrip_names`, `world::color_mapper::tests::rgba_lookup_after_load`, `util::strings::tests::{strip_colors_removes_tags,sanitize_filename_ports_arc}`. Deviation recorded: named color tags are accepted structurally (Arc's full `Colors` table is a view concern); `ATTRIBUTE_COUNT = 4` matches plan 02's current `Attribute` set (heat/spores/water/light) until mod attributes append.
