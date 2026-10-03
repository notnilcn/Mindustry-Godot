# 19 — MAPS & EDITOR IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections + Changelog, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.
> License: project is GPL-3.0 (D6). Ported assets/data keep original names, keys and credit entries.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | **M0–M8 landed 2026-10-03** (`lane/f19-19` M0, `lane/f20-19` M1–M2, `lane/f21-19` M3, `lane/f22-19` M4–M8); **M7 playtest state machine + map save/import e2e landed 2026-10-03 (`lane/f25-maps`, base `main` @ `e3c5174`): `editor::playtest::EditorPlayState` (`edit_in_game`/`playtest`/`resume_editing`/`resume_after_playtest`/`try_exit`) over plan-12 `PlaySession`, `MindEditor` facade delegating, `save_map_e2e`/`import_map_e2e` preview pixels, harness `editor playtest` `10ed5a26be6add3c` + `maps roundtrip` `57bfde109806f8ea`; in-engine MCP §7c + plan-16 chunk-mesh draw in the editor SubViewport + `Gd<ImageTexture>` binding remain deferred to the single-editor mutex.** OD19-A/OD19-B locked 2026-10-01. |
| **Phase** | P7 — Editor, mods, export |
| **Depends on** | `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (`SaveIo`/`MapIo`, `WorldContext` trait, `JsonIo`, `FileSystem`/`Paths`, `PreviewImage`, `MapHeader`, slot/preview paths), `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (`WorldGrid`/`Tiles`/`Tile`, `CachedTile`/`TileGen`, `Maps` registry + `Map` + `MapException` + preview queue/cache + `ShuffleMode`, `FilterRegistry`/`GenerateFilter`/`FilterOption`, `ColorMapper`, `WorldGrid::load_sector`), `14_UI_IMPLEMENTATION_PLAN.md` (`MindDialog`/`MindTable`/`FileChooser`/`ColorPicker`/`PaletteDialog`/`MapListDialog`/`NodeName`/`spawner` widgets, `MapPlayDialog` shell, `CustomRulesDialog`, `IconSelectDialog`), `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` (`FocusState`/`InputLocks`, gesture plumbing, camera rig, keybinds, `Placement` helpers), `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (`floor_cache`/`block_cache` epochs, `scan` visible sets, band renderer, `checkPreviews` call site, `MindRender` facade). |
| **Blocks** | `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (export includes maps + file associations), `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (editor scenarios, map-lifecycle goldens, MCP catalog). Consumed (not blocked) by `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (map metadata/rules tags, `MapObjectives`/`MapMarkers` mutation via editor, `MapLocales`). |
| **Sources** | Mindustry: `core/src/mindustry/editor/*.java` in full (`MapEditor`, `EditorTile`, `DrawOperation`, `OperationStack`, `EditorTool`, `EditorRenderer`, `EditorSpriteCache`, `MapView`, `MapEditorDialog`, `MapInfoDialog`, `MapGenerateDialog`, `MapResizeDialog`, `MapLoadDialog`, `MapObjectivesDialog`, `MapObjectivesCanvas`, `WaveInfoDialog`, `WaveGraph`, `MapProcessorsDialog`, `MapLocalesDialog`, `BannedContentDialog`, `SectorGenerateDialog`, `data/MapAssetsDialog` + `data/{AssetView,MapPatchesView,MapContentView,MapBundlesView,MapImagesView,MapAudioView}.java`); `core/src/mindustry/io/MapIO.java`; `core/src/mindustry/maps/{Maps,Map,MapException,MapPreviewLoader}.java`; `core/src/mindustry/maps/filters/*`; `core/src/mindustry/type/MapLocales.java`; `core/src/mindustry/ui/dialogs/{MapListDialog,EditorMapsDialog,MapPlayDialog,FileChooser}.java`; `core/src/mindustry/game/{MapObjectives,MapMarkers}.java` (data halves); `tools/src/mindustry/tools/MapFixer.java`; `tests/src/test/java/ApplicationTests.java` (`createMap`, `playMap`, `multiblock`, `blockInventories`, `blockOverlapRemoved`, `save`, `saveLoad`, `edges`, legacy save loads, `testSectorValidity`); `tests/src/test/java/DataAssetTests.java`. |
| **AGENTS read** | `Mindustry/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/AGENTS.md`, **`editor/AGENTS.md`**, **`maps/AGENTS.md`**, **`io/AGENTS.md`**, **`ui/AGENTS.md`**, **`world/AGENTS.md`**, `tools/AGENTS.md`, `tests/AGENTS.md`. |
| **Extends spine** | (a) `mind-headless` gains `editor` and `maps` subcommands (`editor ops|roundtrip|bench`, `maps list|preview|image|save-load-save|fix`), scenarios `editor_*`/`maps_*`, fixture maps under `client/rust/mind-core/tests/fixtures/maps/`; (b) `MindEditor` autoload facade appended at `/root/Spine/MindEditor` (same add-only pattern as `/root/Spine/MindRender`) exposing the editor state/command API for GDScript and MCP; (c) `client/scenes/editor/**` editor scenes instantiated under `/root/Spine/Ui/EditorDialog`; (d) state inspector gains an **Editor** tab (`tool`, brush, selection, op count, canUndo/canRedo, saved, preview queue); (e) `MindPreview` texture provider for map previews; (f) `MindRender` gains `mount_editor_view(viewport)` / `unmount_editor_view()` so the editor viewport reuses the plan-16 chunk meshes (no second mesh owner). |

---

## 2. Scope & parity definition

### 2.1 In scope

1. **Map lifecycle (client-visible half).** `Maps.load`/`loadPreviews`/`createAllPreviews`/`queueNewPreview`/`createNewPreview`, `Map.previewFile`/`cacheFile`, `MapPreviewLoader` behavior (missing/stale/foreign preview → delete + re-queue; `error.png` fallback), preview generation `MapIO.generatePreview(Map)` and `MapIO.generatePreview(Tiles)` pixels, `Maps.saveMap`/`importMap`/`removeMap` UI plumbing, `MapException`/`tryCatchMapError` error mapping, `Maps.getNextMap`/`ShuffleMode` (06 owns the registry; 19 owns the client preview sink and the editor-facing wrappers and verifies the upstream method surface), PNG/image map import/export invocation (`MapIO.readImage`/`writeImage`), `tools:fixMaps` equivalent (optional, M8).
2. **`MapEditor` model.** Singleton state (`drawBlock`, `drawTeam`, `rotation`, `brushSize`, `showTerrain`/`showFloor`/`showBuildings`), `beginEdit(int,int)` / `beginEdit(Map)` / `beginEdit(Pixmap)` / `updateRenderer()`, tile resize + shift, draw primitives (`drawBlocks`, `drawBlocksReplace`, `drawCircle`, `drawSquare`, brush sizes), `addCliffs`, undo/redo API, `WorldContext` implementation, `EditorTile` op recording, `flushOp`/`clearOp`, `currentOp` accumulation.
3. **Op packing.** `DrawOperation` (`opFloor..opDataExtra`), generated `TileOp`/`TileOpData` `@Struct` equivalents defined as a fixed Rust bit layout, `OperationStack` (max 30, redo index semantics ported bug-for-bug).
4. **Tools.** `EditorTool` enum (`zoom/pick/line/pencil/eraser/fill/spray`), `touched`/`touchedLine`, per-tool alternate modes, key bindings, `fill` flood-fill (scanline stack), `spray` chance, cliff fill, team fill, under-liquid fill.
5. **`MapView` interaction.** Canvas element with pan/zoom (`0.2..20`), brush outline previews, grid (`ctrl+g`), right-click temporary eraser, middle-click temporary zoom, shift/alt temporary pick, gesture detector semantics, project/unproject math.
6. **Editor rendering.** `EditorRenderer`/`EditorSpriteCache` semantics reconstructed on plan 16 (60×60 grouping, synthetic-last sort, wall/floor layers, shadow recache, `updateStatic`/`updateBlock`), with `showTerrain`/`showFloor`/`showBuildings` toggles.
7. **Editor UI.** `MapEditorDialog` (toolbar/palette/menu/save/playtest), `MapInfoDialog` + sub-dialogs: `CustomRulesDialog` (shared, 14), `WaveInfoDialog` + `WaveGraph`, `MapObjectivesDialog` + `MapObjectivesCanvas`, `MapGenerateDialog` (filter list + async preview), `MapResizeDialog`, `MapLoadDialog`, `MapProcessorsDialog` (13), `MapLocalesDialog`, `BannedContentDialog`, `data/MapAssetsDialog` + all `*View`s (14/20 interfaces), `SectorGenerateDialog`, `EditorMapsDialog` (subclass of 14's `MapListDialog`).
8. **Save/playtest flows.** `editor.tags` ownership (`name`/`description`/`author`/`rules`/`genfilters`/`locales`/`steamid`), rules serialization, genfilters/locales serialization, export with embedded data assets, `tryExit` dirty tracking + confirm, editor playtesting rules (`Gamemode.editor.apply`), `resumeEditing`/`resumeAfterPlaytest`.
9. **Map data models for dialogs.** Objective field descriptors (replaces Java reflection), `MapLocales` type + editing, wave JSON read/write (06) + `SpawnGroup` editing (12), banned content sets (12 `Rules`), processor list (13), map asset views (20).
10. **Oracle & verification** for the above (§7).

### 2.2 “Done” means

- `mind-headless editor ops` replays a recorded op log: apply → undo-all → redo-all produces the **same world checksum** as the golden at each of the three checkpoints (`editor_oplog_*` fixtures).
- `mind-headless maps save-load-save` on an editor map is **idempotent** (second save byte-equal to the first after tag ordering is normalized); tags `rules`/`genfilters`/`locales` round-trip through 04 JSON.
- `mind-headless maps preview` on a hand-built 8×8 tile set produces a deterministic `PreviewImage`; PNG encode/decode round-trips (`write_image` → `read_image` checksum equal).
- MCP scenarios §7c pass: open editor, draw a block line, undo, redo, save map, reload it; screenshots show brush outline, block palette and the saved map.
- Editor chunk recache/op/preview budgets in §7d are recorded and within budget.
- `mind-core` remains Godot-free/tokio-free; every ported file carries the GPL header; no `HashMap` iteration on serialized/sim-visible paths.

### 2.3 Deliberate deviations

| # | Deviation | Reason |
|---|---|---|
| 1 | **No `EditorTile` subclass.** Tile-op recording is an optional `TileOpSink` on `WorldGrid` (`None` in normal play); 06's tile mutation functions notify it when the editor is active. Observer-visible behavior identical (op values, suppression rules, recache calls). | Rust has no subclassing; 06 §3.13 centralizes tile mutation — a hook preserves that boundary without per-tile vtable. |
| 2 | **Objective editor fields are declarative, not reflective.** A `#[derive(ObjectiveFields)]` proc-macro emits `&'static [ObjectiveField]` per objective/marker type; the dialog renders from descriptors. Java annotations (`@Second`, `@TilePos`, `@Multiline`, `@LogicCode`, `@Researchable`, `@Synthetic`) become `FieldFlags`. | D1/§6.2: no Java reflection; keeps field names/camelCase keys as the JSON ABI. |
| 3 | **`EditorRenderer`/`EditorSpriteCache` are adapted onto plan 16, not ported as a second mesh owner.** 16's 30×30 `BuildingCacheChunk`/`FloorChunkGrid` epochs are reused; the editor supplies an `EditorRenderSpec` (60×60 grouping is draw-batch ordering only, synthetic-last sort, team border, wall/floor layers). No new `ArrayMesh` allocation for the editor. | 16 §2.3/§7.5 froze this contract ("19 … must not allocate its own chunk meshes"); Godot mesh lifecycle belongs to 16. Fallback if the user demands literal `EditorSpriteCache`: a private 60×60 mesh layer (OD19-A). |
| 4 | **Async generation preview uses an owned snapshot + cloned filters on a worker.** Java's `MapGenerateDialog.update()` reads `editor.tile()` from `mainExecutor` with `world.setGenerating(true)`; the port copies packed tile state into `EditorSnapshot` under an exclusive world lock, filters are `clone_box()`ed, results return over a channel and are applied on the main thread. No worker ever touches the live `WorldGrid`. | Determinism/aliasing (D8, plan 05 `IoSet`); removes the upstream race; preview may lag one frame. |
| 5 | **PNG codec lives in plan 04's `io::map` (`png` crate) behind `PreviewImage`; Godot `Image` is display-only.** | `mind-core` must stay Godot-free (D1); headless preview/image tests need a real codec. Reconcile with 04 (OD19-D). |
| 6 | **`MapLocales` runtime bundle override is applied by plans 03/14 (`BundleStack`/`Vars.locales` equivalent); 19 only owns the type, JSON and editor.** 12 reads objective text through a `LocaleView` trait whose default is bundle keys and whose map-locales impl is supplied here. | 03 owns bundles; 12's R9 already asks for a `LocaleView`; avoids UI-core reaching into locale loading. |
| 7 | **`WaveGraph` computes a `WaveGraphData` series in `mind-core`; GDScript draws it.** `WaveInfoDialog` remains a GDScript list editor; `SpawnGroup` stays 12's type. | Presentation stays in Godot UI; numeric series is testable headlessly. |
| 8 | **`Maps.saveMap`/`importMap`/`removeMap` orchestration stays in 06; 19 supplies the client `PreviewSink` + UI wrappers and escalates missing upstream methods to 06 (OD19-I).** | 06 landed first and owns `maps/mod.rs`; a single writer avoids duplicated registry state. |
| 9 | **`tryExit` always shows the unsaved confirm** (`saved` remains an informational flag, as upstream where it "is never read"). | Bug-for-bug parity (`MapEditorDialog.java:50`, `:789`). |
| 10 | **`tools:fixMaps` becomes `mind-headless maps fix --dir <path> [--dry-run]`**, same checks as `MapFixer`, optional milestone M8. | Pure-Rust tooling (D1); Gradle/Java tools do not exist in the port. |
| 11 | **PNG image maps are limited to 800×800 at import** (`MapResizeDialog.maxSize`), matching upstream; no bypass except mods/console. | Upstream `editor.errorimage`/too-large error paths preserved. |
| 12 | **Editor maps are capped at 16383×16383 indirectly by the 14-bit `TileOp` x/y fields; only the UI limits (50..800) are practical.** | `@StructField(14)` is upstream ABI; keep the packing widths verbatim. |

### 2.4 Deferred ownership & reconciliation by filename

| Plan | Interface used here | Notes |
|---|---|---|
| `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` | `MapIo::{create_map, write_map, load_map, is_image, write_image, read_image, generate_preview, generate_preview_from_tiles, color_for}`, `PreviewImage`, `MapHeader`, `JsonIo::{read,write}`, `WorldContext`, `Paths::{maps,previews,saves}`, `SettingsStore`, `MindIo` autoload | Pixels called by 19; PNG encode/decode addition in 04 (OD19-D). `preview_for_slot` returns `Image` — 19 converts via `MindPreview`. |
| `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` | `WorldGrid` (tile ops, `resize`, `begin_map_load`/`end_map_load`, `set_generating`, `load_sector`, `load_map`, `raw_tile`), `Tile`/`Tiles`, `TileGen`, `CachedTile`, `ColorMapper`, `Maps` registry (`load`, `save_map`, `import_map`, `remove_map`, `read_filters`, `add_default_ores`, `write_waves`/`read_waves`, `load_internal_map`, `by_name`, `ShuffleMode`, preview queue/paths/cache), `FilterRegistry`/`GenerateFilter`/`GenerateInput`/`FilterOption`, `RenderHooks` | 06 is the registry owner; 19 verifies the full upstream `Maps` method surface and files gaps. `GenerateFilter` needs `clone_box`/`name`/`icon`/`copy` additions (OD19-M). |
| `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` | `Block::in_editor`/`editor_configurable`, `Block::build_editor_config`, `Block::editor_picked`, `Block::place_ended`, `Block::ui_icon`/`full_icon`, `Build::valid_place`, block flags (`synthetic`, `rotate`, `save_data`, `save_config`, `is_floor`, `is_overlay`, `is_multiblock`, `cache_layer`, `has_color`, `has_building`), `CoreBlock` | Palette filter (`in_editor`, `build_visibility != debug_only`), `draw_blocks` behavior, pick hook, config collapser. |
| `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` | `Waves::generate`, `SpawnGroup` runtime use (owner 12) | Wave randomize. |
| `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` | `Rules` (full, incl. `editor`, `allow_edit_rules`, `objective_flags`, `objectives`, `banned_blocks`/`banned_units`, `spawns`, `revealed_blocks`, `infinite_resources`, `instant_build`), `Gamemode` + `Gamemode::editor.apply`, `Team`/`Teams`, `MapObjectives`/`MapObjective` (`editor_x`/`editor_y`, `validate`, `reset`), `MapMarkers`, `SpawnGroup` (`write`/`read`/`get_spawned`/`copy`), `GameStats`, `TickSet` gating (`!editor`) | Editor mutates rules/objectives via 12 data; `GameState::map_locales` set by locale dialog; `LocaleView` trait (12 R9) implemented by 19. |
| `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` | `LogicBuild` (`tag`, `icon_tag`, `show_edit_dialog`), `LogicBlock::max_name_length`, `Blocks::world_processor`, config display hook | `MapProcessorsDialog` content only. |
| `14_UI_IMPLEMENTATION_PLAN.md` | `MindDialog`, `MindTable`, `MindUi` (`show_info`/`show_confirm`/`show_text_input`/`show_error`/`show_info_fade`), `FileChooser`, `ColorPicker` (`ui.picker`), `PaletteDialog`, `MapListDialog` base + `MapPlayDialog` + `CustomRulesDialog` + `IconSelectDialog`, `BlockConfigFragment` host hook, `MindHud`/toasts | 14 owns the shells and shared widgets; 19 owns editor dialog content and subclasses `MapListDialog`. |
| `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` | `FocusState` (`has_keyboard`, `has_field`, `has_dialog`, `chat_shown`), `InputLocks`, `MindCamera2D`, gesture/scroll plumbing, `KeyCode`/binding registry, `Placement` helpers | `MapView` handler registration mirrors `Core.input.getInputProcessors().insert(0, …)`; camera restored after `editInGame`. |
| `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` | `MindWorldRenderer` bands + `MindRender` facade (`set_layer_visible`, `rebuild_chunks`, `get_render_stats`), `mc::render::{floor_cache, block_cache, scan, Layer}`, `BuildingCacheChunk` epochs, `checkPreviews` frame call sites, shadow/darkness FBOs, `RenderHooks::recache_tile`/`recache_building`/`update_wall_darkness` | 19 supplies `EditorRenderSpec` + draw ordering and mounts an editor view; no new meshes. |
| `20_MODS_IMPLEMENTATION_PLAN.md` (**not on disk at authorship**) | `DataManager`/`DataAsset`/`ContentAsset`/`PatchAsset`/`BundleAsset`/`ImageAsset`/`SoundAsset`/`MusicAsset`/`DataAssetType` (`patches|content|bundles|sprites|sounds|music` folders; extensions `json|hjson|json5`, `properties`, `png`, `mp3|ogg`), `DataPatcher::fix_content_arrays`, asset cache/zip read+write | 19 ships `MapAssetsDialog` + views against a `DataManagerApi` trait with these exact names; adapter fixes drift. |
| `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (**not on disk**) | export presets include `data/maps/`; file associations for `.msav`/`.png`; Workshop/Steam publish (`MapPublishEvent`) | 19 emits `MapPublishEvent` and leaves the platform path to 22. |
| `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (**not on disk**) | scenario/golden registration, MCP catalog, bench harness | 19 registers `editor_*`/`maps_*` scenarios and the §7 goldens. |

---

## 3. Target design

All names below are final unless marked otherwise. `mind-core` is Godot-free and tokio-free (D1). No `HashMap` iteration on editor/op/serialization paths (`IndexMap`/`Vec`/`BTreeMap` only). Content is referenced by `BlockId`/`ItemId`/`UnitTypeId`/`TeamId` (02), never by pointer.

### 3.1 Module layout

```
client/rust/mind-core/src/
  editor/
    mod.rs            # MapEditor resource, EditorPlugin, begin_edit/adopt/resize, undo API
    tile_op.rs        # TileOp/TileOpData bit packing + accessors (the @Struct equivalent)
    draw_op.rs        # DrawOperation, OpKind, OP_* constants
    stack.rs          # OperationStack (max 30)
    tool.rs           # EditorTool, BRUSH_SIZES, alt modes, touched/touched_line dispatch
    draw.rs           # draw_blocks/draw_blocks_replace/draw_circle/draw_square/fill/add_cliffs
    context.rs        # EditorContext (impl io::save::WorldContext), recorder hook
    render_spec.rs    # EditorRenderSpec + editor draw-entry ordering (consumed by 16)
    gen.rs            # EditorSnapshot, async filter preview job, apply_filters_to_editor
    objectives.rs     # objective field descriptors, derive support, canvas validation
    wave_graph.rs     # WaveGraphData series (counts/health)
  maps/
    locales.rs        # MapLocales type, property statuses, bundle text parse/write
    fix.rs            # optional MapFixer port (`maps fix`)
client/rust/mind-gdext/src/
  editor/
    mod.rs            # EditorPlugin, MindEditor autoload facade (#[func] API)
    render.rs         # EditorRenderer adapter over plan 16 (mount/unmount editor view)
    map_view.rs       # MapViewDriver: project/unproject, input routing, brush preview
    preview.rs        # MapPreviewLoader/PreviewPipeline (Texture2D + PNG write jobs)
    ui.rs             # UI bridge state/signals for the GDScript dialogs
```

```
client/scenes/editor/
  map_editor_dialog.tscn / .gd       # toolbar, palette, menu sheet
  map_view.tscn / .gd                # Control + SubViewportContainer + world view mount
  map_info_dialog.tscn / .gd
  map_generate_dialog.tscn / .gd
  map_resize_dialog.tscn / .gd
  map_load_dialog.tscn / .gd
  map_objectives_dialog.tscn / .gd
  map_objectives_canvas.tscn / .gd
  wave_info_dialog.tscn / .gd
  wave_graph.tscn / .gd
  map_processors_dialog.tscn / .gd
  map_locales_dialog.tscn / .gd
  banned_content_dialog.tscn / .gd
  sector_generate_dialog.tscn / .gd
  data/map_assets_dialog.tscn / .gd
  data/patches_view.gd  content_view.gd  bundles_view.gd  images_view.gd  audio_view.gd
```

Scene tree added (plan-00 paths preserved; 16's renderer reused):

```
Spine (Node)
├── MindEditor (MindEditor)                      # NEW appended: Rust facade
├── World/…                                      # unchanged; renderer suspendable
└── Ui
    ├── StateInspector (…)                       # + Editor tab
    └── EditorDialog (map_editor_dialog.tscn)    # hidden until shown
        ├── LeftTools (VBoxContainer)
        ├── MapView (Control, map_view.gd)
        │   └── SubViewportContainer → SubViewport → EditorWorldView (16 band host)
        └── RightPalette (VBoxContainer, search + grid + config)
```

`MindEditor` `#[func]` API (append-only test API, plan-00 §3.10 rule 4) — state: `tool()`, `set_tool(name)`, `tool_mode()`, `set_tool_mode(i)`, `brush_size()`, `set_brush_size(v)`, `draw_block()`, `set_draw_block(name)`, `draw_team()`, `set_draw_team(name)`, `rotation()`, `set_rotation(i)`, `show_terrain()/show_floor()/show_buildings()` + setters, `grid()/set_grid(b)`, `can_undo()/can_redo()`, `undo()/redo()`, `tags()`, `set_tag(k,v)`, `status() -> Dictionary`; lifecycle: `begin_new(w,h)`, `begin_edit_map(file)`, `begin_edit_image(file)`, `save() -> Dictionary`, `playtest()`, `edit_in_game()`, `resume_editing()`, `try_exit()`, `last_error() -> Dictionary`; dialog data: `map_info()/set_map_info(name,desc,author)`, `filters_json()/set_filters_json(s)/apply_filters()`, `resize_map(w,h,sx,sy)`, `generate_sector(planet,sector,seed)`, `objectives_json()/set_objectives_json(s)`, `waves_json()/set_waves_json(s)`, `locales_json()/set_locales_json(s)`, `processors() -> Array`, `assets() -> Dictionary` (20); dev/MCP: `dev_draw_line/draw_rect/draw_circle`, `dev_op_log() -> Array`, `dev_apply_op_log(arr)`, `dev_undo_all()`, `dev_redo_all()`, `dev_state_digest() -> int`, `dev_save_as(name)`, `dev_open(name)`, `dev_preview_png(name)`, `check_invariants() -> Dictionary`.

### 3.2 `MapEditor` resource

```rust
pub const BRUSH_SIZES: [f32; 9] = [1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 9.0, 15.0, 20.0];

#[derive(Resource)]
pub struct MapEditor {
    pub tags: IndexMap<String, String>,          // name/description/author/rules/genfilters/locales/steamid
    pub brush_size: f32,                          // always one of BRUSH_SIZES; reset -> 1.0
    pub rotation: i32,                            // 0..3, `Mathf.mod(rot±1, 4)`
    pub draw_block: BlockId,                      // default Blocks::stone
    pub draw_team: TeamId,                        // default Team::sharded
    pub show_terrain: bool, pub show_floor: bool, pub show_buildings: bool, // all true
    pub tool: EditorTool,                         // default Zoom
    pub tool_modes: [i32; EditorTool::COUNT],     // per-tool mode, -1 = standard
    pub loading: bool,                            // suppresses recording (isLoading)
    pub saved: bool,                              // informational (upstream never reads)
    pub shown_with_map: bool,                     // show() does not reset the world
    pub last_saved_rules: Option<Rules>,          // editInGame/resumeEditing round-trip
    stack: OperationStack,
    current_op: Option<DrawOperation>,
    fill_stack: Vec<i32>,                         // scanline flood-fill scratch (upstream `fill.stack`)
}

impl MapEditor {
    pub fn is_loading(&self) -> bool;
    pub fn begin_edit_size(&mut self, ctx: &mut WorldCtx, w: i32, h: i32);   // reset + createTiles + renderer.resize
    pub fn begin_edit_map(&mut self, ctx: &mut WorldCtx, map: &Map) -> Result<(), MapError>; // tags.copy_from + load(MapIO.load_map)
    pub fn begin_edit_image(&mut self, ctx: &mut WorldCtx, img: &PreviewImage) -> Result<(), MapError>; // createTiles + MapIO.read_image
    pub fn adopt_world(&mut self, ctx: &mut WorldCtx);                       // Java updateRenderer()
    pub fn runtime_load<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R;  // `load(Runnable)`: loading=true; f; false
    pub fn create_map(&self, file: MapKey) -> Map;                           // Map(file, width, height, tags.clone(), custom=true)
    pub fn resize(&mut self, ctx: &mut WorldCtx, w: i32, h: i32, shift_x: i32, shift_y: i32);
    pub fn add_cliffs(&mut self, ctx: &mut WorldCtx);
    pub fn clear_op(&mut self);              // stack.clear()  (upstream name)
    pub fn undo(&mut self); pub fn redo(&mut self);
    pub fn can_undo(&self) -> bool; pub fn can_redo(&self) -> bool;
    pub fn flush_op(&mut self);
    pub fn add_tile_op(&mut self, ctx: &mut WorldCtx, op: u64);
    pub fn ops(&self) -> usize;
    pub fn remove_last_ops(&mut self, amount: usize);
    pub fn reset(&mut self);                 // brush 1.0, stone, tags empty, stack cleared
}
```

Exact ports to preserve:

- `reset()` clears the op stack, `brushSize = 1`, `drawBlock = stone`, `tags = new StringMap()` (note: team is *not* reset).
- `begin_edit_map` copies tags, applies the Steam parent-folder `steamid` hack only when `steam` is true (22), calls `load(|| MapIO.load_map(map, EditorContext))`, then `renderer.resize(width, height)` when not headless.
- `begin_edit(Pixmap)` calls `createTiles(pixmap.width, pixmap.height)` first (world resize), then `read_image`; no name check.
- `adopt_world` captures center builds first, replaces every tile with an editor-flagged tile preserving `floor_data`/`extra_data`, then reattaches each building (`set_block(block, team, rotation, entity)`); `renderer.resize`.
- `resize` clears the op stack first, clears buildings, builds the new `Tiles`, copies in-bounds tiles, captures each center build's config *before* remapping `x/y`, shifts `build.x/y`, calls `BuildPlan::point_config` shifting unless `ignore_resize_config`, then `configure_any(out)` with `state.rules.editor` forced true; out-of-bounds becomes `EditorTile(x,y,stone,air,air)`.
- `add_cliffs` bitmask (8-neighbour) then `flushOp`.

### 3.3 Tile ops: packing, `DrawOperation`, `OperationStack`

The generated `TileOp`/`TileOpData` `@Struct` layout is ABI; port bit-for-bit:

```rust
pub const OP_FLOOR: u8 = 0;       // value = previous floor content id
pub const OP_BLOCK: u8 = 1;       // value = previous block content id
pub const OP_ROTATION: u8 = 2;    // value = previous build rotation
pub const OP_TEAM: u8 = 3;        // value = previous team id
pub const OP_OVERLAY: u8 = 4;     // value = previous overlay content id
pub const OP_DATA: u8 = 5;        // value = TileOpData::get(data, floor_data, overlay_data)
pub const OP_DATA_EXTRA: u8 = 6;  // value = previous extra_data

// TileOp: x@0..13 (14b), y@14..27 (14b), type@28..30 (3b), value@31..63 (33b)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TileOp(pub u64);
impl TileOp {
    pub const X_MASK: u64 = (1 << 14) - 1;
    pub fn get(x: i32, y: i32, ty: u8, value: i32) -> u64 {
        ((x as u64) & Self::X_MASK)
            | (((y as u64) & Self::X_MASK) << 14)
            | (((ty as u64) & 0x7) << 28)
            | ((value as u64) << 31)
    }
    pub fn x(op: u64) -> i32 { (op & Self::X_MASK) as i32 }
    pub fn y(op: u64) -> i32 { ((op >> 14) & Self::X_MASK) as i32 }
    pub fn ty(op: u64) -> u8 { ((op >> 28) & 0x7) as u8 }
    pub fn value(op: u64) -> i32 { (op >> 31) as i32 }   // 33 bits truncated to i32 as upstream
}

// TileOpData: three i8 fields packed little-endian into the low 24 bits of an i32
pub struct TileOpData;
impl TileOpData {
    pub fn get(data: i8, floor_data: i8, overlay_data: i8) -> i32 {
        (data as u8 as i32) | ((floor_data as u8 as i32) << 8) | ((overlay_data as u8 as i32) << 16)
    }
    pub fn data(v: i32) -> i8 { (v & 0xFF) as u8 as i8 }
    pub fn floor_data(v: i32) -> i8 { ((v >> 8) & 0xFF) as u8 as i8 }
    pub fn overlay_data(v: i32) -> i8 { ((v >> 16) & 0xFF) as u8 as i8 }
}
```

`DrawOperation` is `Vec<u64>` with upstream semantics:

```rust
pub struct DrawOperation { ops: Vec<u64> }
impl DrawOperation {
    pub fn is_empty(&self) -> bool; pub fn size(&self) -> usize;
    pub fn remove(&mut self, amount: usize);                 // truncate to max(0, len-amount)
    pub fn add(&mut self, op: u64);
    pub fn undo(&mut self, ctx: &mut WorldCtx, editor: &mut MapEditor);   // reverse order
    pub fn redo(&mut self, ctx: &mut WorldCtx, editor: &mut MapEditor);   // forward order
    fn update_tile(&mut self, i: usize, ctx, editor) -> Result<()>;       // swap-in-previous + set_tile
}
```

`update_tile(i)` is the exact Java swap: read the *current* value for `ty` from the live tile (`get_tile`), write it back into `ops[i]`, then `set_tile(.., old_value)`. `get_tile` cases:
`opFloor → tile.floor_id()`, `opOverlay → tile.overlay_id()`, `opBlock → tile.block_id()`, `opRotation → tile.build.map_or(0, |b| b.rotation)`, `opTeam → tile.team_id()`, `opData → TileOpData::get(tile.data, tile.floor_data, tile.overlay_data)`, `opDataExtra → tile.extra_data`.

`set_tile(tile, ty, to)`:

- block/team/rotation first recache every linked tile (`renderer.update_block` + `update_static`), then mutate under `editor.runtime_load(...)`:
  - `opFloor`: if `content.block(to)` is a floor → `tile.set_floor(floor)`
  - `opOverlay`: if floor → `tile.set_overlay(floor)`
  - `opBlock`: `tile.set_block(content.block(to), tile.team(), tile.build.map_or(0, |b| b.rotation))`; if a build exists → `build.enabled = true`
  - `opRotation`: set `build.rotation = to`
  - `opTeam`: `tile.set_team(Team::get(to))`
  - `opData`: set `tile.data/floor_data/overlay_data`
  - `opDataExtra`: set `tile.extra_data`
- then recache linked tiles again.

`OperationStack` (bug-for-bug):

```rust
pub const MAX_SIZE: usize = 30;   // upstream `OperationStack.maxSize`
pub struct OperationStack { stack: Vec<DrawOperation>, index: i32 }  // index <= 0
impl OperationStack {
    pub fn clear(&mut self) { self.stack.clear(); self.index = 0; }
    pub fn add(&mut self, op: DrawOperation) {
        self.stack.truncate((self.stack.len() as i32 + self.index).max(0) as usize);
        self.index = 0; self.stack.push(op);
        if self.stack.len() > MAX_SIZE { self.stack.remove(0); }
    }
    pub fn can_undo(&self) -> bool { !(self.stack.len() as i32 - 1 + self.index < 0) }
    pub fn can_redo(&self) -> bool { !(self.index > -1 || self.stack.len() as i32 + self.index < 0) }
    pub fn undo(&mut self) { if self.can_undo() { let i = self.stack.len() as i32 - 1 + self.index; self.stack[i as usize].undo(..); self.index -= 1; } }
    pub fn redo(&mut self) { if self.can_redo() { self.index += 1; let i = self.stack.len() as i32 - 1 + self.index; self.stack[i as usize].redo(..); } }
}
```

Unit tests assert the trace `A,B,C → undo,C → add D → [A,B,D]`, full-undo `can_redo == true`, and the 30-cap eviction order.

### 3.4 Editing a live world: recorder hook and `EditorContext`

06 owns `world/tile.rs`; 19 adds a single optional sink:

```rust
pub trait TileOpSink {
    /// Called with the *previous* value around the mutation. Center fan-out is the sink's job.
    fn floor_changed(&mut self, x: i32, y: i32, prev: BlockId);
    fn overlay_changed(&mut self, x: i32, y: i32, prev: BlockId);
    fn block_changed(&mut self, x: i32, y: i32, prev_block: BlockId, prev_rot: i32, prev_team: TeamId, was_center: bool);
    fn team_changed(&mut self, x: i32, y: i32, prev_team: TeamId);
}
```

`WorldGrid.tile_op_sink: Option<Box<dyn TileOpSink>>` is `None` in normal play. The recorder is active only when `editor.is_loading() == false && !world.is_generating() && !state.is_game()` — the Rust form of `EditorTile.skip()`. The editor installs its sink on `begin_edit`/`adopt_world` and removes it on hide/reset. `Tile` has no subclass; `is_editor_tile()` reports whether the sink is installed (used by 06 selection/render hints only).

`EditorTile` override semantics mapped 1:1:

| Upstream | Rust |
|---|---|
| `setFloor`: OverlayFloor routing, `floor == type` early-out, `op(opFloor, floor.id)`, `world.floorChanges++`, `floor.floorChanged(this)` | recorder emits `opFloor(prev)`, calls `world::floor_changed`, bumps `floor_changes`; recache hook |
| `setBlock`: non-center records `opRotation`/`opTeam`/`opBlock` on the center; center records same; after mutation recaches linked tiles or `updateShadowTile`; `build.wasVisible=true`; `world.tileChanges++`; `type.blockChanged` | recorder fans out to center; 16 hook `update_shadow`; plan-07 `block_changed` |
| `setTeam`: only records when `synthetic()` and team differs | recorder guard `tile.synthetic()` |
| `setOverlay`: surface/needsSurface guard; records `opOverlay` | recorder emits `opOverlay(prev)` |
| `fireChanged`/`firePreChanged`: game → events, editor → `updateStatic` | 16 `RenderHooks`; events suppressed in editor (05 run conditions already do this) |
| `changeBuild`: no update ticks (`init(this, team, false, rotation)`), modules allocated without ticking | plan 07 `Building::init_editor(...)` equivalent (07 §2 editor-only views) |
| `isDarkened`: only when `skip()` | editor forces `false` while recording is suppressed |

`EditorContext` implements 04's `WorldContext` for map loading/creation:

```rust
pub struct EditorContext<'a> { ... }
impl WorldContext for EditorContext<'_> {
    fn tile(&mut self, index: i32) -> &mut Tile;
    fn resize(&mut self, w: i32, h: i32);
    fn create(&mut self, x: i32, y: i32, floor: BlockId, overlay: BlockId, wall: BlockId) -> &mut Tile; // editor-flagged
    fn is_generating(&self) -> bool;
    fn begin(&mut self);   // world.begin_map_load()
    fn end(&mut self);     // world.end_map_load()
}
```

### 3.5 Drawing primitives and tools

`draw.rs` ports `MapEditor.java:139-306` exactly:

- `draw_blocks_replace(x, y)`: tester `tile.block() != air || draw_block.is_floor()`.
- `draw_blocks(x, y, square, force_overlay, tester)`:
  - multiblock: clamp `x` to `(size-1)/2 .. width - size/2 - 1`; if `!has_overlap`: `set_block`, then explicit `TileOp(opTeam, draw_team)`.
  - single: `is_floor = draw_block.is_floor() && draw_block != air`; per tile in circle/square:
    - data pre-ops when `draw_block.save_data || tile.should_save_data()`: record `opData` (old packed) and `opDataExtra` (old extra), remember them;
    - floor path: `force_overlay → set_overlay`; else `wall_ore && !tile.block().solid` skip, else `set_floor` + clear overlay when `!(overlay is OverlayFloor) && !floor.supports_overlay`;
    - block path: skip when `tile.block().is_multiblock() && !draw_block.is_multiblock()`; if `draw_block.rotate && tile.build.rotation != rotation` record `opRotation(rotation)`; `set_block`; `changed = !draw_block.synthetic()`; for synthetic record `opTeam(draw_team)`;
    - when `changed && draw_block.save_config` → `draw_block.place_ended(tile, None, rotation, draw_block.last_config)` + `renderer.update_static(x, y)`;
    - rollback: if data ops were added but nothing changed and packed/extra equal, `remove_last_ops(2)`.
- `draw_circle`: `within(rx, ry, brush - 0.5 + 0.0001)` inclusive; bounds skip.
- `draw_square`: full `[-b..b]²`.
- `has_overlap`: same-size direct replacement allowed; else any multiblock in the size footprint blocks placement.
- `add_cliffs`: static blocks not cliff → 8-neighbour bitmask → `set_block(cliff)`/`set_block(air)`; final pass; `flush_op()`.

`tool.rs` ports `EditorTool.java`:

```rust
pub enum EditorTool { Zoom, Pick, Line, Pencil, Eraser, Fill, Spray }
impl EditorTool {
    pub const COUNT: usize = 7;
    pub const ALL: [EditorTool; 7];
    pub fn key(self) -> KeyCode;                 // v,i,l,b,e,g,r
    pub fn alt_modes(self) -> &'static [&'static str]; // line: replace/orthogonal; pencil: replace/square/drawteams/underliquid; eraser: eraseores; fill: replaceall/fillteams/fillerase/fillcliffs/fillunderliquid; spray: replace
    pub fn edit(self) -> bool;                   // pencil/eraser/fill/spray
    pub fn draggable(self) -> bool;              // pencil/eraser/spray
}
```

`touched(x, y, editor, ctx)` dispatch: `Pick` (bounds check; choose block/overlay/floor; call `draw_block.editor_picked(tile)`), `Line` stores start (line drawing happens in `touched_line`), `Pencil` (modes −1 normal, 0 replace, 1 square, 2 draw teams via `draw_circle(set_team)`, 3 under-liquid overlay fill), `Eraser` (−1 `tile.remove()`, 0 `clear_overlay`), `Fill` (all 6 modes; multiblock guard → `pencil.touched`; unbuffered scanline flood fill with `fill_stack`, including the `stack.clear()` recovery path), `Spray` (chance `0.012` per tile; floor spray; replace-only skips air). `touched_line(x1,y1,x2,y2)` uses `Bresenham::line` (06/15 port); line mode 1 snaps to the dominant axis.

`MapEditorDialog.doInput()` mapping: `ctrl` selects alt mode with number keys (`tool_modes[..] = i`), otherwise first tapped tool key wins; `escape` opens the menu sheet; `r`/`e` rotate (`+1`/`-1 mod 4`); `ctrl+z` undo, `ctrl+shift+z`/`ctrl+y` redo, `ctrl+s` save, `ctrl+g` grid; text fields suppress shortcuts via 15 `FocusState.has_field`.

`MapView` interaction (ported verbatim in `map_view.rs`, input routed from the GDScript Control):

- Left press: `tool.touched(x, y)`; pointer 0 only; `tool.edit` → `ui.editor.reset_saved()`; `drawing = true`; stores `last`/`start`/`first_touch`.
- Right press: remember the current tool, switch to `EditorTool::Eraser` for the duration; middle press: switch to `EditorTool::Zoom`. On release both restore `last_tool` (set to `None` after).
- Drag: when `drawing && tool.draggable` and the projected tile changed → `reset_saved()` + Bresenham between `last` and current, calling `tool.touched` per tile; line tool keeps the endpoint axis-snapped in orthogonal mode.
- Release: if `tool == Line` → `reset_saved()` + `tool.touched_line(start, end)`; always `editor.flush_op()`.
- `Shift`/`Alt` tap → temporary `EditorTool::Pick`; release restores. Pick chooses block (if `block.in_editor`), else overlay, else floor, then `block.editor_picked(tile)`.
- Scroll over the view (when it is the scroll focus and console hidden): `zoom += axis * 0.1 * zoom`, clamp `0.2..20`; middle-drag and touch pinch pan (`offset += delta / zoom`); gesture config mirrors `GestureDetector(20, 0.5, 2, 0.15)`.
- Movement keys pan (`Binding.moveX/moveY`, `-15 * delta / zoom`) unless a text field or Ctrl is held; `ctrl+g` (or the toolbar button) toggles the grid.
- Projection: fixed-aspect letterbox math from `MapView.project/unproject`, including the even-block-size offset `(int)(x - 0.5)` when `draw_block.size % 2 == 0 && tool != Eraser`; brush outline uses `Edges::pixel_polygon(brush_index)` and the multiblock square outline; the canvas border uses `Pal.remove`, grid uses `Pal.bulletYellowBack` quarter lines + `Pal.accent` center lines.

### 3.6 Lifecycle details

- `updateRenderer` (`adopt_world`): capture builds center-first, convert all tiles to editor tiles preserving `floor_data`/`extra_data`, reattach blocks with `set_block(..., build_prov)`, `renderer.resize`. In Rust no tile replacement is needed: install the sink, flag the grid editor-active, clear darkness (`StaticWall.data = 0` loop from `EditorRenderer.resize`), rebuild chunks via 16.
- `EditorRenderer.resize` also clears darkness for `StaticWall` tiles and zeroes `recacheChunks`; 19 calls 16's `RenderHooks::clear_editor_darkness()` (new hook; reconcile with 06/16) or iterates tiles itself (cheap, once per edit session).
- `beginEdit` is always wrapped in `ui.loadAnd` equivalent (`MindUi::load_and`) so loading UI is shown; headless uses `runtime_load` directly.

### 3.7 Rendering boundary: `EditorRenderer` on plan 16

Chosen approach (OD19-A default): **no second mesh owner.** `mind-core::editor::render_spec` defines the editor draw order; 16 bakes and draws.

```rust
pub struct EditorRenderSpec {
    pub show_terrain: bool, pub show_floor: bool, pub show_buildings: bool,
    pub synthetic_last: bool, pub team_border: bool, pub chunk_grouping: u32, // 60 parity, draw-batch ordering only
}
pub struct EditorDrawEntry { pub tile: TilePos, pub block: BlockId, pub rotation: f32, pub team: TeamId, pub is_build: bool }
pub fn editor_draw_entries(tiles: &Tiles, spec: &EditorRenderSpec, cx: i32, cy: i32) -> Vec<EditorDrawEntry>;
```

- Entries per 60×60 group, blocks only where `block != air && block.cache_layer == normal && tile.is_center()`; `synthetic_last` sort (`comparingBool(b -> !b.block().synthetic())`); team border quad drawn for tiles with a build (`block-border` region tinted `team.color`).
- Floor/walls drawn by 16's `FloorChunkGrid` with the same `show_terrain`/`show_floor` flags and `animate_water = false` for the editor view; shadows drawn by 16's FBO with `editor.show_buildings`/`show_terrain` arguments.
- `update_static(x, y)`: 16 hook = `recache_tile` for `(x,y)` and 4 neighbours (upstream exact bounds checks).
- `update_block(tile)`: mark chunk dirty + `update_shadow_tile`; recache chunks processed on even frame ids (`frame_id % 2 == 0`) — 16 owns the throttle.
- `recache`/`recache_terrain`/`recache_shadows` call 16's `reload_floor`, `reload_blocks`, `update_shadows` with the editor flags.
- In-engine view: `MindEditor` calls `MindRender.mount_editor_view(viewport_rid, spec)`; 16 suspends the main world bands while the editor dialog is visible and draws the same baked meshes through the editor camera transform inside the `SubViewport`. `unmount_editor_view()` restores. `MapView` maps its pan/zoom to a camera rect (`project`/`unproject` formulas ported verbatim, `Tmp.p1` scratch), and the UI border/scissor is the `SubViewportContainer` clip rect.
- `EditorSpriteCache` is **not ported**; its per-texture draw ranges become 16's per-chunk texture-run batching.

### 3.8 Map lifecycle client: previews, PNG, registry glue

Ownership split (see §2.4): 06 keeps registry + paths + cache read/write + `save_map`/`import_map`/`remove_map`; 04 keeps `MapIo`; 19 implements the client sink and dialog wrappers.

`mind-gdext::editor::preview`:

```rust
pub struct PreviewPipeline {
    queue: Vec<MapKey>,                              // `Maps.preview_list`
    textures: IndexMap<MapKey, Gd<ImageTexture>>,    // `Map.texture` equivalent
    error: Gd<Texture2D>,                            // sprites/error.png
    png_jobs: Vec<JoinHandle<Result<(), IoError>>>,  // mainExecutor.submit
}
impl PreviewPipeline {
    pub fn load_previews(&mut self, maps: &Maps);                 // Maps.loadPreviews
    pub fn queue_new_preview(&mut self, key: MapKey);             // Maps.queueNewPreview
    pub fn create_all_previews(&mut self);                        // after ClientLoadEvent (05 post queue)
    pub fn create_new_preview(&mut self, map: &Map) -> Result<(), MapError>; // generate -> texture -> async PNG+cache
    pub fn texture_for(&self, map: &Map) -> Gd<Texture2D>;        // safeTexture()
    pub fn check_previews(&mut self);                             // 16 frame stage 6 call site (OD16-L)
}
```

Rules preserved:

- Missing preview file, or present but texture load fails → delete preview file/asset entry and `queue_new_preview` (upstream `MapPreviewLoader.loadAsync` catch); sync fallback `new Texture(file)` then `sprites/error.png`.
- `create_new_preview` runs `04::generate_preview(map)` on the **main thread** (it may read temp content-mapper state), assigns the texture, then submits PNG write + `06::write_cache(map)` to a worker (`Maps.saveMap` client branch does the same); failures log and keep `error.png`.
- `Maps.loadPreviews` cache path: `previewFile` exists → load texture + read cache (`spawns`/`teams`); cache read error → `queue_new_preview`.
- `checkPreviews` keep-alive reflection hack (`Rules.fog`/`staticFog` forcing preview loads for campaign clients) is **not ported**; 12's `FogControl` marks previews retained directly.

Save/import/remove wrappers in `MindEditor`/GDScript:

- `save()` (§3.11) → `06::Maps::save_map(tags, embed_assets=false)` for custom maps; built-in/workshop name collision → `@editor.save.overwrite`.
- `EditorMapsDialog.try_import_map`: reject images (`MapIO::is_image`), `MapIO::create_map` to sniff name (generate `unknownN` when absent), conflict handling (`editor.import.exists`/`editor.overwrite.confirm`), `06::Maps::import_map` then preview.
- `remove_map`: dispose texture + `06::Maps::remove_map`.
- `load_internal_map("serpulo/groundZero")` used by tests.
- `MapException`/`tryCatchMapError` map to `MindUi.show_error` keys: `Outdated legacy map format` → `@editor.errornot`; message containing `Incorrect header!` → `@editor.errorheader`; else `@editor.errorload` with exception popup.

PNG image maps:

- `is_image(file)`: 8-byte PNG signature through `FileSystem` (04).
- export image: `MapIO::write_image(tiles)` → `PreviewImage` → PNG bytes in 04 → `FileChooser.export`; `y` flipped (`height-1-y`); building blocks with `has_color && !has_building` use block color else floor color.
- import image: decode via 04, enforce `<= 800`, `editor.begin_edit_image(img)`, `MapIO::read_image(img, tiles, color_mapper)`: skip blocks with buildings, overlay→`set_overlay`, floor→`set_floor`, multiblock→`set_block(derelict, 0)`, else `set_block`; final pass `floor == air → stone`.

### 3.9 Dialog data models

**Objective field descriptors** (replaces reflection; consumed by `MapObjectivesDialog`):

```rust
bitflags! { pub struct FieldFlags: u32 { const SECOND=1; const TILE_POS=2; const MULTILINE=4; const LOGIC_CODE=8;
    const RESEARCHABLE=16; const SYNTHETIC=32; const HIDDEN=64; } }
pub enum FieldKind { String, Bool, Byte, Int, Float, Content(ContentKind), Team, Color, Vec2F, Vec2I,
    Objective(ObjectiveFilter), Seq(Box<FieldKind>), Map(Box<FieldKind>) }
pub struct ObjectiveField { pub name: &'static str, pub kind: FieldKind, pub flags: FieldFlags }
pub trait ObjectiveFields { fn fields(&self) -> &'static [ObjectiveField]; }
// #[derive(ObjectiveFields)] in mind-derive: emits OBJECTIVE_FIELDS from #[objective(...)] attrs
```

Default providers by type: `String → ""`, `bool → false`, `byte/i32 → 0`, `f32 → 0` (interpreters multiply `Second`×60, `TilePos`×8), `UnlockableContent → core-shard`, `Block → copper-wall`, `Item → copper`, `UnitType → dagger`, `Team → sharded`, `Color → Pal.accent`, `Vec2/Point2 → 0`. Objective insertion opens the type picker (`ALL_OBJECTIVE_TYPES` from 13/12 with localized names and `editorX=-999`/`editorY=-999` normalization for legacy programmatic objectives — port `MapObjectivesDialog.rebuildObjectives` including its rebuild branch). Canvas: `objWidth=5`, `objHeight=2`, `bounds=100`, connector graph (parent indices fixup via 04's objectives serializer `parents`), query placement, drag, right-click cancel, pan clamps, `placeQuery` on mobile via a separate button, clipboard copy/paste.

**Waves**: `SpawnGroup` (12) list; `WaveInfoDialog` features — index search, unit-type filter, sort modes (`Sort::{begin,type,health,shield,…}`) + reverse, add/duplicate/remove/expand rows, per-group fields (`begin`, `unit`, `effect`, `items`, `spawn`, `shield`, `health`, `amount`…), copy/load clipboard via 06 `write_waves`/`read_waves`, clear/reset, randomize `Waves::generate(1/10)` (11). `hidden(() => state.rules.spawns = groups)` on close. `WaveGraphData { mode: Counts|Health, from, to, series: Vec<Vec<f32>>, units: Vec<UnitTypeId>, max, max_total, max_health }` computed in `wave_graph.rs`; GDScript draws bars/legend and handles pan/zoom/hidden-unit toggles.

**Locales**: `pub type MapLocales = IndexMap<String /*locale*/, IndexMap<String,String>>` (Java `ObjectMap<String, StringMap>`). Dialog features: per-locale property editing, `locales.applytoall`, search by key/value, add-to-other-locales, view-property across locales, add-icon (`:icon:` insertion from 03's icon list), rollback to `lastSaved`, copy/paste single bundle and full locale (properties text parse/write), statuses (`unique`, `modified`, `conflict`, `missing`, `same`) used for card coloring, locale add/remove (default from `SettingsStore.locale`). Saves: `editor.tags["locales"] = JsonIo::write(&locales)`; `state.map_locales = locales` (12 consumes via `LocaleView`). Cancel/close prompts `@editor.savechanges` unless already saved.

**Banned content**: `BannedContentDialog<T: UnlockableContent>` — block and unit instantiations; `IndexSet<ContentId>`/`BTreeSet` backing `Rules.banned_blocks`/`banned_units` (12); `Category` filter buttons (02); search; selected/deselected panes with add/remove/add-all; portrait two-row branch.

**Generate dialog**: `MapGenerateDialog(boolean applied)`:

- `applied == true` (editor menu): Apply button runs `apply_to_editor`, hide; `applied == false` (map info): on hide writes filters back to `tags["genfilters"]` with `seed = 0` on each.
- Filter list: add (`FilterRegistry` entries minus `is_post` when applied), per-filter options (06's `FilterOption` rendered by 14 widgets), randomize, move up/down, duplicate (`clone_box` + `randomize`), remove; "default ores" button; edit menu (copy/load clipboard, clear, reset defaults when not applied).
- Preview: `pixmap` size `(width/scaling, height/scaling)`, `scaling = mobile ? 3 : 1`; `EditorSnapshot` packed tiles; cloned filters applied on a worker (deviation §2.3.4); double buffer (`buffer1`/`buffer2`) exactly like upstream; colors via `MapIO::color_for(..., Team::derelict)`; texture updated on the main thread; generation overlay while `generating`; queued re-run when edits land mid-generation.
- `apply_to_editor`: for each filter: `input.begin` + walk all tiles writing `PackTile::get(block,floor,overlay)` and `packed_data`; then `editor.runtime_load(|| for each tile: if !tile.synthetic() && !block.synthetic() set_block; set_packed_data; set_floor; set_overlay)`; then `renderer.recache()` + `editor.clear_op()`.
- `MirrorFilter` drag: hit-test closest axis line (pad `Scl.scl(16)`), normalized `axisX/axisY`, 33 ms throttle.

**Resize**: min 50, max 800, increment 50; digits-only width/height fields with positivity + bounds validators; shift fields parse any int; OK → `editor.resize`.

**Sector generate**: planet picker over accessible planets with `generator != null && sectors.size > 0`; sector index validated `< sectors.size`; seed int; size label `[ NxN ]`; Apply: `editor.clear_op` → `editor.runtime_load(|| { let preset = sector.preset.take(); logic.reset(); world.load_sector(sector, WorldParams{ seed_offset: seed, save_info: false, .. }); sector.preset = preset; })` → `editor.adopt_world()` → `state.rules.sector = None` → `tags["genfilters"] = "{}"`.

### 3.10 Godot editor UI

GDScript owns layout and presentation only; every game-rule decision calls `MindEditor`:

| Scene | Root | Key nodes / responsibilities |
|---|---|---|
| `map_editor_dialog.tscn` | `MindDialog` (14) | fullscreen, black background; menu sheet (`BaseDialog("@menu")`); tools column; `MapView`; palette column |
| `map_view.tscn` | `Control` | `SubViewportContainer` → `SubViewport` → `EditorWorldView` (16 mount); `_gui_input` forwarded to `MindEditor.map_view_event(...)`; `_draw` requests `MindEditor` primitives (border `Pal.remove`, brush outline polygons from `Edges::pixel_polygon`, grid `GridImage` equivalent, multi-tile square for multiblocks, line preview at both endpoints) |
| `map_info_dialog.tscn` | `MindDialog` | name field (50), description `TextArea` (1000), author (50), 7 route buttons (rules 14 / waves / objectives / generation / locales / processors / assets) |
| `map_generate_dialog.tscn` | `MindDialog` | left preview `BorderImage` + overlay, right filter-card `ScrollContainer`; bottom buttons Back/Apply/Randomize/Edit/Add |
| `map_resize_dialog.tscn` | `MindDialog` | 4 fields + Cancel/OK |
| `map_load_dialog.tscn` | `MindDialog` | map grid (250×90 buttons, `BorderImage(safe_texture)`), selection group, Load disabled until selected, `@maps.none` |
| `map_objectives_dialog.tscn` | `MindDialog` | canvas + toolbar (add/search/export/import/copy/paste), left fields panel |
| `map_objectives_canvas.tscn` | `Control` | pan, tiles (5×2 units at `Scl.scl(48)`), connector lines/dots, query cursor, grid bounds ±100 |
| `wave_info_dialog.tscn` | `MindDialog` | search + unit filter, group list, Add/Sort buttons, `WaveGraph` on the right, edit-menu sheet |
| `wave_graph.tscn` | `Control` | draws `WaveGraphData`; scroll = zoom, drag = pan; mode toggle; legend with hide toggles |
| `map_processors_dialog.tscn` | `MindDialog` | search, processor rows (icon/tag/pos/edit/eye/delete), Add (first non-synthetic tile → `world-processor`) |
| `map_locales_dialog.tscn` | `MindDialog` | locale list + add/edit/copy, property cards + search + statuses, "?" info |
| `banned_content_dialog.tscn` | `MindDialog` | search, category row (blocks), two panes (banned/unbanned) with Add-all |
| `sector_generate_dialog.tscn` | `MindDialog` | planet button + picker sheet, sector/seed fields, size label, Apply |
| `data/map_assets_dialog.tscn` | `MindDialog` | type tab strip + menu (import/export zip, clear all), search, per-type view body, close button built per view |
| `map_assets` views | `Control` | `patches_view`, `content_view`, `bundles_view`, `images_view`, `audio_view` implement the upstream `AssetView` contract (`build`, `build_buttons`) over 20's `DataManagerApi` |

`MapEditorDialog` toolbar (upstream `build()`): menu, grid, zoom, undo, redo, pick, line, pencil, eraser, fill, spray, rotate (image rotation bound to `editor.rotation * 90`), team color buttons (base teams, 3 per row), brush slider (indices into `BRUSH_SIZES`), `showblocks`/`showterrain`/`showfloor` checkboxes, center button (desktop); palette: search field (`maxNameLength`, name `editor/search`), selected block name label (wrap 200), config collapser when `draw_block.editor_configurable` (07 hook → 14 `BlockConfigFragment` host), scroll pane of 50×50 `ui_icon` buttons (6 desktop / 4 mobile columns) sorted by `(is_core desc, synthetic asc, is_overlay asc, id asc)`, filtered by `in_editor && build_visibility != debug_only` and search; `@none.found` empty state; `MapAssetsDialog.hidden` triggers `rebuild_block_selection`.

### 3.11 Save, export, playtest, exit

- `save() -> Option<Map>`:
  1. snapshot `is_editor = rules.editor`; set `rules.editor = false`, `allowEditRules = false`; clear `rules.objective_flags`; `rules.objectives.each(reset)`; `stats = GameStats::new()`.
  2. `tags["rules"] = JsonIo::write(&rules)`; remove `width`/`height`; `player.clear_unit()`; remove core-spawned unit.
  3. Empty name → show `MapInfoDialog` + post `@editor.save.noname`, return `None`.
  4. Existing non-custom, non-workshop map with the same name → `handle_save_builtin` → `@editor.save.overwrite` (protected hook subclasses override), return `None`.
  5. Preserve `steamid` when the found map had one (mark workshop); `06::Maps::save_map(tags, embed_assets=false)` → `@editor.saved` toast; return `Some(map)`.
  6. Restore `rules.editor = is_editor`; hide menu; `saved = true`.
- `export_map(file)`: `MapIo::write_map(file, editor.create_map(file), embed_assets=true)` (04) — embeds data assets via 04's custom chunks (20 bodies); `export_image(file)`: `write_image` → PNG.
- `edit_in_game()`: `last_saved_rules = rules.clone()`; hide; `state.teams = Teams::new()`; `player.reset()`; `rules = Gamemode::editor.apply(last_saved_rules.clone())`; `limit_map_area=false`; `sector=None`; `fog=false`; synthetic `state.map` (name "Editor Playtesting", width/height); `state = Playing`; `world.end_map_load()`; clear core-spawned units; `world.weather.clear()`; `logic.play()`; spawn a `CoreBlock` unit type (or `evoke`/`alpha` when scorching) at the view center, set `spawned_by_core`, possess, camera to unit.
- `playtest()`: `save()`; shift → hide + auto-pick gamemode (`survival.valid ? survival : attack.valid ? attack : sandbox`) + `control.play_map(map, map.apply_rules(mode), playtesting=true)`; else `MapPlayDialog` (14) with `play_listener = hide`, `show(map, true)`.
- `resume_editing()`: `state = Menu`; `shown_with_map = true`; `show()`; `rules = last_saved_rules.take().unwrap_or_default()`; `saved=false`; `renderer.recache()`.
- `resume_after_playtest(map)`: `begin_edit_map(map.file)`.
- `try_exit()`: `show_confirm("@confirm", "@editor.unsaved", || { logic.reset(); hide(); })` — always, regardless of `saved`.
- `begin_edit_map(file)` (from `EditorMapsDialog`): `shown_with_map = true`; `editor.begin_edit(MapIO::create_map(file, true))`; `show()`; errors → `@editor.errorload`.
- `show()` when `!shown_with_map`: `logic.reset(); rules = Rules::new(); editor.begin_edit_size(200, 200)`; reset `shown_with_map`.
- `hide()`: `editor.clear_op()`; platform landscape hooks (22); `logic.reset()` only on exit-confirm.

### 3.12 MapAssetsDialog and view adapters

`map_assets_dialog.gd` owns the tab strip (`patch/content/bundle/image/sound/music` via `DataAssetType.all` with icons), menu sheet (import zip, export zip, clear all), search, and delegates the body/buttons to the current view. The Rust side (`mind-gdext::editor::ui`) exposes `MindEditor.assets()` returning the current type's asset list `[{name,path,type,hash,always_embedded}]` and command methods (`assets_import_zip(path)`, `assets_export_zip(path)`, `assets_clear_all()`, `assets_read(path)`, `assets_write(path, data)`) implemented against 20's `DataManagerApi`. Zip import follows upstream: per type, content has sub-folders per `ContentType.folderName`; duplicate name/path → error list; `state.data.load(prev + results)`; regenerate content sprites once; result dialog (`asset.imported`, `asset.image.error`, `asset.import.none`); `DataPatcher::fix_content_arrays()` + `rebuild_block_selection()` on hide; export zip iterates `get_all_assets()` using `get_full_path()` and `get_data()`/cache-file bytes; clear all → `state.data.unload()` + confirm. Views: `MapPatchesView` (patch JSON editor), `MapContentView` (content-type tabs, JSON editor with add/remove), `MapBundlesView` (per-locale property editing), `MapImagesView` (image import + generated sprite handling), `MapAudioView` (sound/music import + preview), matching upstream button sets.

### 3.13 `maps fix` (optional)

`mind-headless maps fix --dir <data/maps> [--dry-run]` ports `MapFixer` checks: hidden-map banned blocks/units cleared, `infinite_resources`/`instant_build` off, unit cost/build-speed warnings, unlocalized/typo `TimerObjective` texts (hidden only), suspicious non-`@` prints in world processors (hidden only), `wave > 1` reset to 1, `revealed_blocks` cleared, name normalized to `SectorPreset.localizedName` when `require_unlock` (skip Serpulo 263), hidden maps forced `attack_mode = true` when `win_wave <= 1`; rewrites changed maps through `MapIo::write_map`. Uses `EditorContext` so warning paths share code. `--dry-run` prints the would-change list. No Gradle task.

### 3.14 Schedule, threading, STDB, boundaries

- **Schedule (05).** Editor adds **no sim tick systems**. 05's run conditions already gate every gameplay set on `!editor`/`!state.is_game()`. Editor mutation happens outside the tick (menu state), driven by UI events. During playtest, `state = Playing` and normal systems run; the editor holds no sim state.
- **Threading.** Preview PNG + cache writes on a worker `std::thread` (no tokio); generation-preview jobs on a worker with an owned `EditorSnapshot`; results applied on the main thread via the 05 event bus (`EditorPreviewReady { map, image }`) or a `mind-gdext` channel. No IO or filter run inside the sim tick (04 §3.10).
- **Godot.** `MindEditor` at `/root/Spine/MindEditor`; editor scenes under `/root/Spine/Ui/EditorDialog`; `MindPreview` texture provider (`texture(id) -> Texture2D`); `MindRender.mount_editor_view/unmount_editor_view`; inspector Editor tab. `MapPublishEvent` fired on `add_steam_id` (22 handles the platform).
- **STDB.** **No new tables/reducers/views.** Map files stay local; publishing/sharing is 22/21 scope. `MapException` and map tags never enter STDB payloads directly.
- **Boundaries & invariants.**
  1. `mind-core` stays Godot-free/tokio-free; the editor model is data + logic only.
  2. Dedicated-server safety: `MindEditor`/preview pipeline are client-only; every entry point checks `Platform::is_headless()` and returns a no-op/error (AGENTS: `Vars.editor`/`ui.editor` are null headless).
  3. Tile mutation stays centralized in `world/tile.rs`; the recorder is an optional sink, never a parallel writer.
  4. Editor op logs are deterministic: no `HashMap` iteration, no wall-clock, no content lookups by name (ids only) in replay.
  5. Op packing widths are ABI: x/y 14 bits, type 3 bits, value 33 bits; `TileOpData` low 24 bits.
  6. `editor.tags`/`MapLocales` are `IndexMap` (insertion order) so saves are deterministic (06 R12).
  7. All limits are upstream constants: resize 50..800/step 50, PNG import ≤800, brush sizes, `OperationStack::MAX_SIZE = 30`, spray chance 0.012, map-objective bounds 100.
  8. No `unwrap`/`expect` on runtime data; errors surface as `MapError`/`EditorError` with UI mapping.
  9. Every ported file carries the GPL header; bundle keys and JSON field names are ABI (OD9).

---

## 4. Port map

| Mindustry source | Target | Notes |
|---|---|---|
| `editor/MapEditor.java` | `mind-core/src/editor/mod.rs` (`MapEditor`) | State machine, begin_edit, resize, undo API, `EditorContext` host, `runtime_load`. |
| `editor/EditorTile.java` | `mind-core/src/editor/context.rs` + `world/tile.rs` sink | No subclass; `TileOpSink` recording + `skip()` gate + linked-tile recache. |
| `editor/DrawOperation.java` | `mind-core/src/editor/draw_op.rs`, `tile_op.rs` | `@Struct` packing as fixed bit layout; undo/redo swap semantics. |
| `editor/OperationStack.java` | `mind-core/src/editor/stack.rs` | Max 30; negative-index redo semantics preserved. |
| `editor/EditorTool.java` | `mind-core/src/editor/tool.rs`, `draw.rs` | Enum + dispatch; per-tool mode array; flood fill in `MapEditor.fill_stack`. |
| `editor/EditorRenderer.java` | `mind-core/src/editor/render_spec.rs` + `mind-gdext/src/editor/render.rs` | Editor draw ordering + spec; 16 bakes/draws; mount/unmount view. |
| `editor/EditorSpriteCache.java` | **not ported** — 16 `BuildingCacheChunk` | Deviation §2.3.3; texture-run batching via 16. |
| `editor/MapView.java` | `mind-gdext/src/editor/map_view.rs` + `scenes/editor/map_view.tscn/.gd` | project/unproject, pan/zoom clamp, brush polygons, grid, temp eraser/zoom/pick. |
| `editor/MapEditorDialog.java` | `scenes/editor/map_editor_dialog.tscn/.gd` + `mind-gdext/src/editor/ui.rs` | Toolbar/palette/menu/save/playtest; `doInput` shortcuts; block selection. |
| `editor/MapInfoDialog.java` | `scenes/editor/map_info_dialog.tscn/.gd` | Metadata fields + routes. |
| `editor/MapGenerateDialog.java` | `scenes/editor/map_generate_dialog.tscn/.gd` + `mind-core/src/editor/gen.rs` | Filter cards; async snapshot preview; `apply_to_editor`. |
| `editor/SectorGenerateDialog.java` | `scenes/editor/sector_generate_dialog.tscn/.gd` | planet/sector/seed; `load_sector(save_info=false)`. |
| `editor/MapResizeDialog.java` | `scenes/editor/map_resize_dialog.tscn/.gd` | 50..800, shift. |
| `editor/MapLoadDialog.java` | `scenes/editor/map_load_dialog.tscn/.gd` | Map grid picker. |
| `editor/MapObjectivesDialog.java` | `scenes/editor/map_objectives_dialog.tscn/.gd` + `mind-core/src/editor/objectives.rs` | Field descriptors replace reflection; providers/interpreters per kind. |
| `editor/MapObjectivesCanvas.java` | `scenes/editor/map_objectives_canvas.tscn/.gd` | Tilemap graph, connectors, query, parents fixup. |
| `editor/WaveInfoDialog.java` | `scenes/editor/wave_info_dialog.tscn/.gd` | Group editor; 06 JSON; 12 `SpawnGroup`. |
| `editor/WaveGraph.java` | `scenes/editor/wave_graph.tscn/.gd` + `mind-core/src/editor/wave_graph.rs` | Series in core, drawing in GDScript. |
| `editor/MapProcessorsDialog.java` | `scenes/editor/map_processors_dialog.tscn/.gd` | Uses 13 `LogicBuild` API. |
| `editor/MapLocalesDialog.java` | `scenes/editor/map_locales_dialog.tscn/.gd` + `mind-core/src/maps/locales.rs` | `MapLocales` type + statuses; `state.map_locales` for 12. |
| `editor/BannedContentDialog.java` | `scenes/editor/banned_content_dialog.tscn/.gd` | Blocks/units; hosts `Rules.banned_*` sets (12). |
| `editor/data/MapAssetsDialog.java` | `scenes/editor/data/map_assets_dialog.tscn/.gd` + `mind-gdext/src/editor/ui.rs` | Tab strip + zip import/export + clear; 20 API. |
| `editor/data/{AssetView,MapPatchesView,MapContentView,MapBundlesView,MapImagesView,MapAudioView}.java` | `scenes/editor/data/*.gd` | `AssetView` contract via 20 API. |
| `io/MapIO.java` | 04 `io/map/**` (owned) | 19 invokes `is_image`, `write_image`, `read_image`, `generate_preview*`, `color_for`; PNG codec addition in 04. |
| `maps/Maps.java` | 06 `maps/mod.rs` (owned) + 19 client sink/wrappers | 19 verifies upstream surface, adds `PreviewPipeline` client half. |
| `maps/Map.java` | 06 `maps/map.rs` (owned) | `previewFile`/`cacheFile`/`rules`/`filters` consumed; Steam publish hook → 22. |
| `maps/MapException.java` | 06 `maps/error.rs` (owned) | UI error mapping in 19. |
| `maps/MapPreviewLoader.java` | `mind-gdext/src/editor/preview.rs` | Texture load + delete-and-requeue; loader errors resilient. |
| `maps/filters/*` | 06 `maps/filters/*` (owned) | 19 consumes `FilterRegistry`/`FilterOption`; adds clone/name/icon needs. |
| `type/MapLocales.java` | `mind-core/src/maps/locales.rs` | Ownership reconcile (OD19-G). |
| `ui/dialogs/EditorMapsDialog.java` | `scenes/editor/editor_maps_dialog.gd` (subclass of 14 `MapListDialog`) | New/import/open/delete; `try_import_map`. |
| `ui/dialogs/MapListDialog.java` | 14 (owned) | Base filters/settings; 19 subclasses. |
| `ui/dialogs/MapPlayDialog.java` | 14 (owned) | 19 sets `play_listener` and calls `show(map, true)`. |
| `tools/src/mindustry/tools/MapFixer.java` | `mind-core/src/maps/fix.rs` + `mind-headless maps fix` | Optional; same checks, `--dry-run`. |

---

## 5. Milestones & task breakdown

Each milestone ends with evidence (command output, dump/PNG path, screenshot) appended to the Changelog.

- **M0 — Tile ops + stack + tools (smallest vertical slice).**
  `editor/{tile_op,draw_op,stack,tool,draw}.rs` core-only: packing, `DrawOperation` undo/redo swap, `OperationStack`, `EditorTool` dispatch, `draw_blocks*`/`draw_circle`/`draw_square`/`fill`/`add_cliffs`, recorder trait + `WorldGrid` sink (06 hook).
  *Verify*: `cargo test -p mind-core editor::` (packing round-trip incl. 14-bit boundary; stack trace; circle/square footprints; flood-fill tuning; data-op rollback). `mind-headless editor ops --fixture tests/fixtures/editor/basic_ops.json` applies, undoes, redoes and matches the three committed checksums.
- **M1 — Editor lifecycle.**
  `begin_edit_size`/`begin_edit_map`/`begin_edit_image`/`adopt_world`/`resize`/`create_map`/`EditorContext`; `MapEditor` resource registration; darkness-clear + recache calls behind `RenderHooks` no-ops.
  *Verify*: `mind-headless editor roundtrip --map serpulo/groundZero --seed 1` (adopt → draw → undo → save) checksum stable; `editor_resize_shift` scenario preserves tiles/configs; `cargo test` for the skip gate (`is_game`/loading/generating).
- **M2 — Map lifecycle client + PNG.**
  04 PNG codec addition (reconcile), `PreviewPipeline` (load/queue/create/texture/check), `Maps` glue, `EditorMapsDialog` subclass, image import/export calls, `tryCatchMapError` mapping.
  *Verify*: `mind-headless maps save-load-save --map editor_fixture` idempotent; `maps preview-tiles` deterministic pixels + PNG round-trip; `maps image-roundtrip` checksum; registry/shuffle scenario.
- **M3 — Godot editor shell.**
  `MindEditor` facade + `EditorPlugin`; `map_editor_dialog`/`map_view` scenes; 16 `mount_editor_view`; toolbar/team/brush/show toggles; palette rebuild/sort/search/config collapser; grid/brush preview; `doInput` shortcuts; inspector Editor tab.
  *Verify*: `cargo check -p mind-gdext`; Godot `--headless --editor --quit` clean; MCP §7c-1 executes; screenshot shows palette + brush.
- **M4 — Meta dialogs.**
  `MapInfoDialog`, `MapResizeDialog`, `MapLoadDialog`, `MapGenerateDialog` (async preview + `apply_to_editor`), `SectorGenerateDialog`.
  *Verify*: MCP §7c-2 (filters preview + resize + sector gen); headless `editor gen-preview` determinism.
- **M5 — Objectives + waves.**
  `mind-derive` `ObjectiveFields`; providers/interpreters; canvas + connector graph; `WaveInfoDialog` + `WaveGraphData`.
  *Verify*: objective JSON golden round-trip incl. `editorPos`/`parents`; wave graph series golden; MCP screenshot of canvas + graph.
- **M6 — Processors, locales, banned, assets.**
  13 processor list; `MapLocales` + dialog + `LocaleView`; banned dialog; `MapAssetsDialog` + views against the 20 `DataManagerApi` trait.
  *Verify*: locale apply/rollback scenario; banned set mutates `Rules` JSON; assets zip import/export round-trip against a fixture zip; MCP screenshots.
- **M7 — Save/playtest/export/exit + budgets.**
  `save`/`export_map`/`export_image`/`edit_in_game`/`playtest`/`resume_editing`/`resume_after_playtest`/`try_exit`; bench suite; alloc audit.
  *Verify*: MCP §7c-3 (edit → playtest → return); `mind-headless editor bench` numbers in §7d; zero steady-state allocations in MapView draw/op recording.
- **M8 — Verification sweep, `maps fix`, docs.**
  Full §7 pass; register scenarios with 23; `mind-headless maps fix --dry-run` on a fixture dir; repo playtest-skill editor recipes; changelog evidence.

Dependency-safe ordering: M0–M1 need only 04's `WorldContext` trait + 06's tile ops; M2 can land as soon as 04 M5 + 06 M5 are green; M3 needs 16 M1–M2; M4–M6 are UI-only on top of M3.

---

## 6. Data & formats

### 6.1 Tile op / op log

- Bit layout as §3.3; `TileOp` is `u64`, `TileOpData` is an `i32` with 3 signed bytes (little-endian byte order in the low 24 bits).
- Headless fixture op log (JSON, plan-00 scenario conventions):
  ```json
  { "format": 1, "width": 64, "height": 64,
    "ops": [[789696, 789697], [789698]],
    "checksum_after_apply": "0x...", "checksum_after_undo": "0x...", "checksum_after_redo": "0x..." }
  ```
  `ops` is an array of `DrawOperation`s, each an array of packed `u64` values (decimal strings for values above `2^53`; the harness parses both number and string forms). The golden file stores the three `mind-headless` 64-bit world checksums (05/06).
- `MindEditor.dev_op_log()` emits the same list; `dev_apply_op_log(arr)` feeds it.

### 6.2 `editor.tags`

| Key | Owner / format |
|---|---|
| `name`, `description`, `author` | 19; plain strings; max 50/1000/50 |
| `rules` | 12 `Rules` JSON (`JsonIo::write`), camelCase fields, `#[serde(default)]` |
| `genfilters` | 06 `Seq<GenerateFilter>` JSON (camelized class tags, content names); `"{}"` clears after sector gen |
| `locales` | `MapLocales` JSON (§6.3) |
| `steamid` | 22 publish flow; preserved on save |
| legacy `build`, `width`, `height` | read from `MapHeader`; removed on save (`editor.tags.remove`) |

### 6.3 `MapLocales`

```json
{ "en": { "foo.name": "Foo", "foo.desc": "A [accent]thing[]" },
  "ru": { "foo.name": "Фу" } }
```

`IndexMap<String, IndexMap<String,String>>`; written with insertion order. Property statuses computed against `Vars.locales` (03) and other locale bundles. `state.map_locales` is set on dialog save and consumed by 12's `LocaleView` + 14's text rendering. Bundle text copy/paste uses Java `.properties` escaping (port `readLocale`/`writeLocale`).

### 6.4 Waves and filters

- Waves: `SpawnGroup[]` JSON via 06 `Maps::write_waves`/`read_waves`; field names stay upstream camelCase (`begin`, `unit`, `effect`, `items`, `spawn`, `shield`, `health`, `amount`, `max`…). `WaveInfoDialog.hidden` writes back to `state.rules.spawns` (12).
- Genfilters: 06 §3.8; the dialog writes `tags["genfilters"]` only in non-applied mode and zeroes each filter's `seed`.

### 6.5 Preview / cache paths (06 owned; 19 written)

- `<map>_v2.png`, `<map>-cache_v2.dat` under `Paths::previews()`; workshop: parent folder name / `<name>-workshop-cache.dat`.
- Cache file: `u8 version=0`, `i32 spawns`, `i8 team_count`, `u8 team ids` (scan order: columns then rows, matching 06 §6).
- Preview PNG is RGBA8; editor/map previews flip y internally once (`pixmap.height-1-y`) so `write_image`/`generate_preview` are consistently y-up in tile space.

### 6.6 Image maps

- PNG signature sniff; decode size must be ≥1 and ≤800 per axis; import skips blocks with buildings; unknown color → air via `ColorMapper` (`0,0,0,1` → air); final air-floor → stone. Export uses block color only when `has_color && !has_building`, else floor color.

### 6.7 Data asset zip layout (20 API; 19 UI)

| Type | Folder | Extensions | Embedded |
|---|---|---|---|
| patch | `patches` | json/hjson/json5 | yes |
| content | `content/<ctype.folderName>` | json/hjson/json5 | yes |
| bundle | `bundles` | properties | no |
| image | `sprites` | png | no |
| sound | `sounds` | mp3/ogg | no |
| music | `music` | mp3/ogg | no |

Export zip path = `get_full_path()` (folder + relative path); embedded assets use `get_data()`, others the cache-file bytes.

### 6.8 Map lifecycle limits

`MapResizeDialog::{MIN_SIZE=50, MAX_SIZE=800, INCREMENT=50}`; PNG import ≤ 800; `MapObjectivesCanvas::{objWidth=5, objHeight=2, bounds=100}`; `OperationStack::MAX_SIZE=30`; `BRUSH_SIZES` 9 entries; spray chance `0.012`; `MapEditorDialog` search `maxNameLength` (Vars, 50).

### 6.9 Save region integration

Editor save writes through 04 `SaveIo` with `SaveOptions { extra_tags: tags, embed_assets }`; native magic `MGRS`; `meta` tags carry `name`/rules/genfilters/locales; no editor-only custom chunk exists (editor data is tile data + tags only). Playtest uses 12's `Saves` slot machinery (04 paths).

---

## 7. Oracle & verification

### 7a. Ported tests

Mindustry's editor has **no JUnit coverage** (upstream tests only exercise maps/saves). Port the map/save candidates and add editor unit tests:

| Mindustry test | Rust test | Notes |
|---|---|---|
| `ApplicationTests.createMap` | `world::tile::tests::create_map_8x8` | 06 owns; editor relies on it. |
| `ApplicationTests.playMap` | `maps::tests::play_map_ground_zero` | 04/06. |
| `ApplicationTests.multiblock` / `blockInventories` / `blockOverlapRemoved` | `world::tile::tests::*` (06/07) | Editor `resize`/`set_block` interplay. |
| `ApplicationTests.save` / `saveLoad` | `maps::tests::save_load_ground_zero` (04) | Editor fixture extends with tags. |
| `ApplicationTests.edges` | `world::edges::tests::edge_order` (06) | Brush polygon parity. |
| `ApplicationTests.load77…152Save` | 04 `io::legacy` (feature-gated) | Not editor-specific. |
| `ApplicationTests.testSectorValidity` | 12; 19 adds `maps::tests::sector_generate_dialog_flow` | Sector dialog uses the same generators. |
| — (new) | `editor::tests::tile_op_packing_roundtrip` | All 7 op kinds, x/y 16383 boundary, value sign. |
| — (new) | `editor::tests::tile_op_data_packing` | 3 signed bytes. |
| — (new) | `editor::tests::operation_stack_trace` | 30 cap, redo truncate, full-undo `can_redo`. |
| — (new) | `editor::tests::draw_blocks_multiblock_clamp_overlap` | Clamp range + same-size replacement. |
| — (new) | `editor::tests::draw_circle_square_footprints` | `1.5` brush inclusive math. |
| — (new) | `editor::tests::data_op_rollback` | Unchanged tile removes the 2 pre-ops. |
| — (new) | `editor::tests::fill_flood_tuning` / `fill_replace` / `fill_erase` | Scanline correctness on a painted 32×32. |
| — (new) | `editor::tests::undo_redo_swap_identity` | apply → undo → redo == apply per tile field. |
| — (new) | `editor::tests::recording_suppressed` | `loading`/`generating`/playing gates. |
| — (new) | `editor::tests::resize_shift_preserves_config` | `point_config` shift + `ignore_resize_config`. |
| — (new) | `editor::tests::objective_field_descriptors` | Every upstream `MapObjective` subclass has fields whose names match the JSON keys. |
| — (new) | `maps::locales::tests::roundtrip_and_statuses` | JSON + property parse/write. |
| — (new) | `editor::wave_graph::tests::series_golden` | Counts/health series vs committed fixture. |
| — (new) | `maps::fix::tests::fixer_checks` | Fixture maps for each warning/changed case. |

### 7b. Headless harness scenarios (`mind-headless <cmd>`)

| Scenario | Setup | Assertions |
|---|---|---|
| `editor ops` | fixture 64×64 editor map + `basic_ops.json` op log | after apply/undo/redo: `checksum` equals the three committed goldens; `ops()` counts match; no event fired outside the editor. |
| `editor roundtrip` | groundZero → `adopt_world` → draw 3 lines/1 fill → save → load | tile histograms + checksum equal pre/post in a second session. |
| `editor resize-shift` | 100×100 painted map, resize 80×80 shifts ±10 | in-bounds rectangle preserved; configs shifted by `(-offset_x,-offset_y)`; border default stone. |
| `editor image-roundtrip` | decoded PNG fixture (color-mapped) → `read_image` → `write_image` | pixel checksum equal; size ≤800 enforced. |
| `maps save-load-save` | editor map with rules/genfilters/locales tags | second save byte-equal after canonical tag ordering; reloaded `rules` equal. |
| `maps preview-tiles` | 8×8 hand-tile set with ores/spawn/teams | `PreviewImage` byte checksum golden; PNG encode/decode round-trip; spawn/team counts. |
| `maps registry-shuffle` | MockFs: 2 built-ins + 2 custom + 1 mod map; seeds 1..N | `Maps::load` order; `ShuffleMode::{None,All,Custom,Builtin}` selection never returns `prev` when >1; PvP filtering. |
| `editor gen-preview` | fixed filters on 64×64 | preview checksum deterministic across runs; queued re-run coalesced. |
| `maps fix --dry-run` | fixture map dir | reports exactly the expected changes; write mode rewrites and second run is a no-op. |

### 7c. MCP playtest scenarios

Preconditions: plan-00 spine running (`res://scenes/spine.tscn`; `/root/Spine/SimHost`, `/root/Spine/World/Camera2D`, `/root/Spine/Ui/StateInspector`), plan 16 renderer live, `MindEditor` present. All evals are pid-stamped.

**C1 — open editor, draw a line, undo/redo, save, reload.**

1. `godot_exec eval`: `var e = get_node("/root/Spine/MindEditor"); e.begin_new(64, 64); return e.status()` → `{tool:"zoom", width:64, height:64}`.
2. `godot_exec call /root/Spine/MindEditor set_draw_block ["copper-wall"]`, `set_tool ["line"]`, `set_brush_size [3.0]`.
3. `godot_exec eval`: `MindEditor.dev_draw_line(10, 10, 40, 10)` → `true`; `MindEditor.dev_state_digest()` printed; `godot_runtime_state inspect /root/Spine/SimHost` shows tile (25,10) block `copper-wall`; `godot_screenshot game` → `editor_line.png`.
4. `godot_exec eval`: `MindEditor.undo()`; assert tile (25,10) is `air` with `stone` floor; screenshot `editor_undo.png`; `MindEditor.redo()` → block back.
5. `godot_exec call /root/Spine/MindEditor set_tag ["name","mcp_editor_map"]`, `save` → returns `{ok:true, custom:true}`; assert the saved map exists under the data maps dir.
6. `godot_exec call /root/Spine/MindEditor begin_edit_map ["<path>"]`; assert `status().width == 64` and tile (25,10) is `copper-wall`.
7. `godot_screenshot game` → `editor_reload.png`; verify the palette is visible and the map view is non-blank (plan-00 evidence pattern).

**C2 — generate filters async preview + resize + palette config.**

1. `MindEditor.set_draw_block ["ore-copper"]`; open `map_generate_dialog`; `filters_json()` returns the default stack; `set_filters_json` adds `{"class":"riverNoise",...}`; assert the preview texture changes (screenshot `editor_gen_preview.png`; `status().preview_generation > 0` observed during generation).
2. Apply filters; assert the tile histogram changes and `editor.can_undo() == false` (op stack reset).
3. `resize_map(100,100,0,0)`; `status().width == 100`; screenshot.
4. Select a block with `editor_configurable` (e.g. `sorter`); assert the config collapsible region appears (`godot_runtime_state inspect /root/Spine/Ui/EditorDialog/RightPalette/Config`).

**C3 — save → playtest → return to editor.**

1. Build a tiny base in the editor (core + wall line) via `dev_draw_line` + `set_draw_block("core-shard")` touch.
2. `MindEditor.save()` then `playtest()`; assert `MindSimHost.get_state_json()` shows `state:"playing"`, a player unit exists, and `rules.editor == true`.
3. `MindEditor.resume_after_playtest(status().last_map)`; assert editor width/height and tiles preserved; the editor world checksum equals the pre-playtest value.
4. `godot_log errors` → empty.

Fallbacks if plan-07 placement APIs differ: use `godot_input` clicks at `Camera2D.tile_to_screen` coordinates and assert via `godot_runtime_state inspect`.

### 7d. Performance budget + measurement

Method: `mind-headless editor bench --suite <name> --json` (release, median of 20 runs) and `MindEditor.bench(name)` in-engine; counters from 05 `TickReport` where applicable. Baseline fixture: 200×200 editor map with ~2 000 blocks (10% density) and 500 tiles of floor variation.

| Metric | Budget | Scenario/command |
|---|---|---|
| Full editor recache (200×200, blocks) | p50 ≤ 12 ms, p99 ≤ 25 ms | `editor bench recache --size 200` |
| Single-chunk block recache | p50 ≤ 0.5 ms | `editor bench chunk` |
| `update_static` (1 tile + 4 neighbours) | p50 ≤ 20 µs | `editor bench static` |
| Draw line 100 tiles + flush | p50 ≤ 0.6 ms | `editor bench line` |
| Undo / redo one 100-tile op | p50 ≤ 0.7 ms each | `editor bench undo` |
| Fill 200×200 (single block type) | p50 ≤ 30 ms | `editor bench fill` |
| Preview generation 200×200 (filters) | p50 ≤ 25 ms | `maps preview --bench` |
| Preview PNG write | p50 ≤ 15 ms | worker-side counter |
| Map save 200×200 with tags | p50 ≤ 150 ms | `maps save --bench` |
| Op-log replay 10 000 ops | p50 ≤ 500 ms | `editor bench replay` |
| Steady-state allocations during MapView draw + op recording | 0 bytes/frame | alloc audit (plan-00 CI flag) |
| Undo stack memory | per-op ≤ `area × 2 ops × 8 B`; 30-op worst case recorded; warn > 64 MB | `editor bench undo-memory --size 800` |

### 7e. Exit criteria checklist

- [x] `cargo test -p mind-core editor::` and `maps::` green; op-pack/undo/fill/resize/fixer/objective-field tests pass. (1542 lib passed.)
- [x] `mind-headless editor ops` matches apply/undo/redo goldens; `editor roundtrip`, `editor resize-shift`, `editor image-roundtrip` pass.
- [x] `mind-headless maps save-load-save` idempotent; `maps preview-tiles` golden; `maps registry-shuffle`; `maps fix --dry-run` fixture.
- [ ] MCP C1/C2/C3 pass with screenshots (`editor_line.png`, `editor_undo.png`, `editor_reload.png`, `editor_gen_preview.png`) and clean logs. — **deferred (single-editor mutex)**
- [ ] All editor dialogs open/close without errors; palette/team/brush/tool state mirrored in the inspector Editor tab. — **scene parse-check only; in-engine open deferred**
- [x] `editor.tags` round-trip (`name/rules/genfilters/locales`) through save/load/export; export embeds data assets; built-in overwrite refused. — **tags/genfilters/locales + embedded export landed; F26 folds the objectives/waves dialog `objectives`/`spawns` tags into the saved `rules` JSON (`maps_glue::fold_dialog_tags`, called from `save_editor_map`/`save_map_e2e`; idempotent, no-op when absent).**
- [~] Playtest round-trip preserves the editor world; `tryExit` always confirms and `logic.reset()` runs on confirm. — **`editor::playtest::EditorPlayState` state machine + `MindEditor.edit_in_game`/`playtest`/`resume_editing`/`resume_after_playtest`/`try_exit` landed (`lane/f25-maps`); the in-engine `SubViewport` mount/unmount + actual world preservation and `logic.reset()`-on-confirm wiring stay deferred to the plan-07/12 sim host + single-editor mutex.**
- [~] §7d budgets recorded in `bench/editor_baseline.json`; alloc audit zero. — **F26 adds `bench/editor_baseline.json` (§7d budgets + the measured `mapview` suite) and a new `editor bench --suite mapview` draw+op-recording alloc-audit record. The MapView steady-state audit is recorded but NOT yet zero (`alloc_count 71520` / `alloc_bytes 1245440` over 40 runs, size 200); gap documented in the baseline (pre-reserved scratch buffers in `tool::touched_line`/`DrawOperation` are a plan-19 follow-up).**
- [x] `mind-core` Godot-free/tokio-free greps green; no `HashMap` iteration in editor/save paths; GPL headers on all files.
- [x] Plan-20/23 reconciliation notes resolved or re-assigned; changelog evidence entries completed. (`editor::assets::DataManagerApi`; plan-23 catalogue registration left to its owner.)

---

## 8. Risks & open decisions

Each item has the default this plan proceeds with. Items marked `NEEDS USER DECISION` are load-bearing and not covered by `HIGH_LEVEL_PLAN.md`; execution continues on the stated default unless the user overrides.

| # | Decision / risk | Default being planned against | Needs user? |
|---|---|---|---|
| OD19-A | **Editor rendering strategy.** 16 froze "no second mesh owner", but the editor needs a clipped UI-space viewport and the upstream `EditorRenderer`/`EditorSpriteCache` semantics (60×60 groups, synthetic-last, team border, wall/floor toggles). | Reuse 16's floor/block chunk meshes + bands via `MindRender.mount_editor_view` and a `SubViewport`; supply only `EditorRenderSpec` + draw ordering. Fallback (if rejected): a private 60×60 editor mesh layer owned by 19, accepting duplicated chunk state. | **locked 2026-10-01 (NUD-43=A)** |
| OD19-B | **Where the editor model lives.** `mind-core::editor` (Godot-free, headless-testable; D1 "sim, view drivers, input and net are Rust") vs `mind-gdext` (client-only per `editor/AGENTS.md` "Client-only"). | `mind-core::editor` as a client-sim module with zero gameplay coupling; `mind-gdext` is the view facade only. Required for the §7b op-log checksum oracle. | **locked 2026-10-01 (NUD-44=A)** |
| OD19-C | `EditorTile` port shape (subclass vs sink). | Optional `TileOpSink` on `WorldGrid` (deviation §2.3.1); 06's tile ops notify it; `None` in normal play. | no (reconcile 06) |
| OD19-D | PNG codec dependency (`png` crate) and where it lives. | 04's `io::map` behind `PreviewImage`; Godot `Image` is display-only. Reconcile: 04 M5 must add `decode_png`/`encode_png`. | no (reconcile 04) |
| OD19-E | Async generation-preview threading vs Java's `mainExecutor`/`world.setGenerating(true)` read. | Owned `EditorSnapshot` + cloned filters on a worker; results applied on the main thread; no worker world access. | no |
| OD19-F | Objective field reflection replacement. | `#[derive(ObjectiveFields)]` + declarative descriptors; JSON keys unchanged. | no |
| OD19-G | **`MapLocales` ownership** (02 says 19; 04 lists the type under 12; 12 R9 asks 19 to supply it via `LocaleView`). | 19 owns `maps/locales.rs` (type + JSON + dialog); 12 consumes `LocaleView`; 03/14 apply bundle overrides. | no (reconcile 02/04/12) |
| OD19-H | `MapAssetsDialog` ↔ plan 20 API (`DataManager`/`DataAssetType`). | 19 ships the dialog against a `DataManagerApi` trait with upstream names; 20 supplies the impl. 20 was not on disk at authorship. | no (reconcile 20) |
| OD19-I | `Maps` registry split (06 owns; HIGH_LEVEL §3 credits 19). | 06 is the writer; 19 verifies the upstream method surface and escalates gaps (`loadPreviews`, `createAllPreviews`, `tryCatchMapError`, `reload`, `findFile`) to 06. | no (reconcile 06) |
| OD19-J | `EditorSpriteCache` fidelity (per-texture draw ranges, packed vertices). | Superseded by 16 chunk meshes (deviation §2.3.3); sorting parity asserted by 16's render-list tests. | no (tied to OD19-A) |
| OD19-K | Worst-case undo memory (30 × full-map fill ≈ 300 MB at 800×800). | Upstream parity cap kept; benchmark records the worst case, logs a warning > 64 MB, and a dev console action can clear the stack. | no |
| OD19-L | `tools:fixMaps` as a Gradle task is impossible in the port. | Optional `mind-headless maps fix`; M8. | no |
| OD19-M | `GenerateFilter` lacks `clone_box`/localized `name`/`icon` in 06's trait as written. | 19 requires them; add via a `GenerateFilterClone` supertrait + registry-provided metadata (reconcile 06). | no (reconcile 06) |
| OD19-N | Plans 05/12/14 MCP paths use `/root/Main/*`; plan 00 ships `/root/Spine/*`. | `/root/Spine/*` is authoritative; `MindEditor` and editor scenes append under it. | no (reconcile) |
| OD19-O | `MapPlayDialog`/`CustomRulesDialog`/`EditorMapsDialog` shell ownership. | 14 owns shells/`MapListDialog`; 19 owns editor dialogs and subclasses `EditorMapsDialog`. | no (reconcile 14) |
| OD19-P | GDScript draws editor primitives (brush polygons, grid, wave graph, objective canvas) — is that "layout only" (D1)? | Yes: presentation-only drawing with data supplied by Rust; no game rules in GDScript. If the user wants Rust-driven canvas items, `MindEditor` can emit draw commands instead. | no |
| OD19-Q | `SectorGenerateDialog`'s preset-null hack + `logic.reset()` + `save_info=false`. | Ported verbatim; the preset swap is scoped to the `runtime_load` closure and restored before `adopt_world`. | no |
| OD19-R | Editor-on-dedicated-server leakage. | All `MindEditor`/preview entry points are client-only and check `Platform::is_headless()`; the headless harness uses `mind-headless` directly. | no |

---

## 9. References

Repo-local: `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 D1–D9, §2 architecture, §3 plan table, §4 template, §5 P7 gate, §6 conventions, §7 verification, §8 addons, §9 parity ledger, §10 OD1–OD9, §11 execution); `PRELIMINARY_PLAN.md`; `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (§3.5 spine/node paths, §3.10 extension contract, §6 scenario/dump formats, §7c MCP recipe); `02_CONTENT_IMPLEMENTATION_PLAN.md` (§2.3 `MapLocales` note, §3.2 `ContentType`/ids, §3.6 interfaces, `SectorPresetDef`); `03_ASSETS_IMPLEMENTATION_PLAN.md` (§3.3 `Region`, §3.6 runtime binding, `uiIcon`/`fullIcon` chains, bundle fallback); `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (§2 ownership + OD2, §3.1 module map, §3.5 `JsonIO`, §3.8 slots, §3.9 `MapIO`, §3.10 threading, §3.11 `MindIo`, §6.1 container, §7); `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (§3.4 schedule + editor run conditions, §3.7 events, §3.12 `Rules` boundary); `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (§2.3 dev 7, §2.4 deferred ownership, §3.2–3.7 tile/cache, §3.8 filters, §3.9 `Maps`/`Map`, §3.10–3.11 generators, §3.12–3.13 boundaries/recache, §6 cache format); `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (§2.3 boundaries, §3.4 building init, §3.6 `valid_place`, §3.11 `DrawBlock`, editor config hooks); `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (`Waves::generate`); `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (§2.4 map-editor deferral, §3 `Rules`/`MapObjectives`/`MapMarkers`/`SpawnGroup`/`Gamemode`, R9 `LocaleView`); `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` (`LogicBuild`/processor dialog boundary); `14_UI_IMPLEMENTATION_PLAN.md` (§2.3 catalogue incl. `MapEditorDialog` shell, §3.3 `MindDialog`/widgets, §3.5 fragments, §8 19-reconcile row); `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` (§3.4 `FocusState`/locks, §3.6 `Placement`); `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (§2.3/§3.3 `checkPreviews` call sites, §3.5 `FloorChunkGrid`, §3.6 `BuildingCacheChunk`/quadtrees, §7.5, OD16-L); `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (effect hooks only); plan-19-referencing notes in 20/22/23 (not on disk at authorship).

Mindustry sources read: all files in the §1 Sources row (editor package in full, `MapIO.java`, `Maps.java`, `Map.java`, `MapException.java`, `MapPreviewLoader.java`, `tools/MapFixer.java`, `ui/dialogs/{MapListDialog,EditorMapsDialog,MapPlayDialog}.java`, `game/MapObjectives.java` data half, `mod/data/DataAssetType.java`, `tests/src/test/java/{ApplicationTests,DataAssetTests}.java`).

Tooling: `/mnt/c/Users/Clinton/g/.opencode/skills/godot-compositor-testing/SKILL.md` (headless import/parse checks, windowed capture, process hygiene); `/mnt/c/Users/Clinton/g/.opencode/skills/playtest/SKILL.md` (pid-stamped recipes, screenshot attribution); open-godot-mcp tool docs (`godot_health`, `godot_game`, `godot_exec`, `godot_input`, `godot_screenshot`, `godot_runtime_state`, `godot_log`, `godot_editor_*`); Godot 4.7 docs (`SubViewportContainer`/`SubViewport`, `Control._gui_input`/`_draw`, `Image.load_png_from_buffer`/`save_png`, `FileAccess`/`DirAccess`, `RenderingServer.canvas_item_*`).

## Changelog

> Append entries here when execution starts. Every "done" claim carries evidence (command, golden path, screenshot path, counter).

### M0 — Tile ops + stack + tools (DONE 2026-10-03, `lane/f19-19`)

**Deliverables.** `mind-core/src/editor/{mod,tile_op,draw_op,stack,tool,grid,context,test_grid}.rs` (Godot-free/tokio-free):
`TileOp`/`TileOpData` bit packing (`x`/`y` 14 bits, `type` 3 bits, `value` 33 bits; 3 signed bytes little-endian), `DrawOperation` undo/redo swap, `OperationStack` (max 30, negative-index redo semantics), `EditorTool` (7 tools, `alt_modes`/`key`/`edit`/`draggable`), `filled` `touched` dispatch + `touched_line` (Bresenham via `world::raycast::raycast_each`), `draw_blocks`/`draw_blocks_replace`/`draw_circle`/`draw_square`/`fill` (all six modes)/`add_cliffs`. `EditorGrid` seam + `WorldEditorGrid` adapter over `WorldGrid`; `TileOpSink` recorder wired into `WorldEditorGrid` (the §3.4 hook adapted to the adapter, no plan-06 file edits). `EditorContext` implements plan-04 `WorldContext`.

**Verify (plan §7 M0).**
- `cargo test -p mind-core editor::` → **19 passed / 0 failed**. Covers `tile_op_packing_roundtrip` (all 7 kinds + 16383 boundary + negative value), `tile_op_data_packing`, `operation_stack_trace` + 30-cap eviction, `draw_blocks_multiblock_clamp_overlap`, `draw_circle_and_square_footprints`, `data_op_rollback_removes_unchanged_pre_ops`, `fill_flood_tuning_only_reaches_connected_region`, `fill_replace_all_reaches_disconnected_region`, `fill_erase_removes_connected_blocks_only`, `recording_suppressed_while_loading`, `undo_redo_swap_identity`, `touched_line_bresenham_flushes_one_operation`, `add_cliffs_autotiles_isolated_static_block`, `tile_op_sink_records_previous_values`.
- `cargo run -p mind-headless -- editor ops --json` (cwd `client/rust`, fixture `mind-core/tests/fixtures/editor/basic_ops.json`, 64×64, 3 operations / 20 packed ops) → apply `c7669b5845c31d21`, undo `5c8e5cb798cc412c`, redo `c7669b5845c31d21`; `round_trip_ok=true`, `checksums_ok=true`, exit 0. The three committed goldens are stored as `0xc7669b5845c31d21` / `0x5c8e5cb798cc412c` / `0xc7669b5845c31d21` in the fixture.
- Full gate on the lane: `cargo test -p mind-core` **1454 passed / 3 ignored**; `cargo fmt --check` clean; workspace `cargo clippy --all-targets -- -D warnings` clean.

**Files.** `mind-core/src/editor/{mod,tile_op,draw_op,stack,tool,grid,context,test_grid}.rs`, `mind-core/src/lib.rs` (`pub mod editor;`), `mind-core/tests/fixtures/editor/basic_ops.json`, `mind-headless/src/{cli,exec}.rs` (`EditorCommand::Ops`).

**Deferred to M1+.** The `WorldGrid` sink is installed by the lifecycle owner (M1) — the draw path records ops explicitly for M0. `EditorBlockInfo::rotate` is `false` until plan-07 `BlockInstance::rotate` is wired; `supports_overlay`/`needs_surface` are approximated. `fillcliffs`/`fillunderliquid`/`fillteams` are ported but only flood/replace/erase have unit coverage. In-engine MCP (M3+) waits on the single-editor mutex.

### M1 — Editor lifecycle (DONE 2026-10-03, `lane/f20-19`, base `43d2c31`)

**Deliverables.** `mind-core/src/editor/lifecycle.rs` (`MapEditor::begin_edit_map`/`begin_edit_image`/`adopt_world`/`resize`/`create_map`/`runtime_load`/`export_image`; `try_catch_map_error`), `editor/context.rs` (`EditorContext` rewritten to own the grid + a precomputed `Block.hasBuilding` table so `SaveReadState.content` can borrow `&mut ContentRegistry`; concrete `EditorRecorder` = `TileOpSink`), `editor/grid.rs` + `editor/test_grid.rs` (`EditorGrid::{clear_editor_darkness, recache_all, resize_shift}`), `editor/mod.rs` (`MapEditor` `Resource` derive, `install_recorder`/`remove_recorder`/`drain_recorded_ops`/`should_record_ops`). Darkness clear + recache go through plan-06 `RenderHooks` no-ops; `resize_shift` captures center builds first and reattaches them (live configs are plan 07's `BuildPlan::point_config`, deferred).

**Verify (plan §7 M1).**
- `cargo run -p mind-headless -- editor roundtrip --json` (synthetic 32×32: `begin_edit_size` → `adopt_world` → 3 line ops → fill/flush → undo-all → redo-all → `save_editor_map` → `begin_edit_map`): normalized world checksums draw/redo/loaded all `cf47382195a5b7bc`; `round_trip_ok=true`, exit 0.
- `cargo run -p mind-headless -- editor resize-shift --json`: 100×100 → 80×80 shift `(-10,-10)`; tile `(10,10)` = `copper-wall`, extra data `(20,20)` = `0x4321`, border `(0,0)` floor `stone`; `ok=true`, exit 0.
- `cargo test -p mind-core editor::` → **30 passed** incl. `skip_gate_suppresses_recording` (`is_game`/loading/generating), `runtime_load_suppresses_then_restores`, `resize_shift_preserves_tiles_and_data`, `adopt_world_clears_darkness`, `recorder_shared_buffer_receives_ops`.
- Lane gate: `cargo test -p mind-core` **1497 passed / 3 ignored**; `cargo fmt --all -- --check` clean; workspace `cargo clippy --all-targets -- -D warnings` clean.

**Files.** `mind-core/src/editor/{mod,context,grid,test_grid,lifecycle}.rs`; `mind-headless/src/{cli,exec}.rs` (`EditorCommand::{Roundtrip,ResizeShift}`).

**Deferred to M3+.** `begin_edit_image`'s tile import writes floors/overlays only (plan-04 `ImageTileSink` has no `set_block`); live building-config shift; `EditorRenderer`; Godot lifecycle. MCP §7c waits on the single-editor mutex.

### M2 — Map lifecycle client + PNG (DONE 2026-10-03, `lane/f20-19`)

**Deliverables.** Plan-04 `mind-core/src/io/map/preview.rs` + `mod.rs`: **PNG codec addition** `encode_png`/`decode_png` (RGB/RGBA 8-bit) behind `PreviewImage` (OD19-D; `png = 0.17.16` workspace pin; the only plan-04 `io` change, reported). `mind-core/src/editor/preview.rs`: `PreviewPipeline` (`load_previews`/`queue_new_preview`/`create_all_previews`/`create_new_preview`/`texture_for`/`cache_for`/`check_previews`) — Godot-free core half; the `Gd<ImageTexture>` binding is M3. `mind-core/src/editor/maps_glue.rs`: `EditorMapSource` (writes the `map` region from a live `WorldGrid`), `GridImageSink`/`import_image_into_grid`, `save_editor_map` (map tags win over the standard base tag set), `try_import_map` (image reject + free-name copy), `canonicalize_tags`. `MapEditor::export_image`/`begin_edit_image` wire image export/import; `try_catch_map_error` maps `MapError` → `@editor.errornot`/`@editor.errorheader`/`@editor.errorload`.

**Verify (plan §7 M2).**
- `cargo run -p mind-headless -- maps save-load-save --json`: 32×32 editor fixture with `rules`/`genfilters`/`locales`; save A (4607 B) → load → save B; **byte-equal**; reloaded `rules` = `{"editor":false}`; `ok=true`.
- `cargo run -p mind-headless -- maps preview-tiles --json`: 8×8 hand tiles + ores; pixel checksum `1a0374ab5994da45`, PNG 177 B, `png_round_trip_ok=true`.
- `cargo run -p mind-headless -- maps image-roundtrip --json`: `write_image` → PNG → `read_image` → `write_image`; checksum `4d1d3a32c6b05ea5` both sides.
- `cargo run -p mind-headless -- maps registry-shuffle --json`: registry order custom→builtin, 16 selections, `repeats=0`.
- `cargo test -p mind-core io::map editor::preview editor::maps_glue` green; lane fmt + workspace clippy clean.

**Files.** `mind-core/src/io/map/{preview,mod}.rs`, `mind-core/Cargo.toml` (+`png`), `mind-core/src/editor/{preview,maps_glue}.rs`; `mind-headless/src/{cli,exec}.rs` (`MapsCommand::{SaveLoadSave,PreviewTiles,ImageRoundtrip,RegistryShuffle}`).

**Deferred to M3+.** `EditorMapsDialog` (14's `MapListDialog` subclass) and `MindPreview` `Texture2D` binding; preview PNG + cache writes are synchronous (std thread deferred with the Godot shell); plan-04 `FogControl`-retained previews (06 reflection hack intentionally not ported).

### M3 — Godot editor shell (PARTIAL 2026-10-03, `lane/f21-19`, base `main` @ `d85e49f`)

**Deliverables.** `mind-gdext/src/editor/mod.rs` (`MindEditor` GodotClass at `/root/Spine/MindEditor`): an owned Godot-free editor world (`mind_core::editor::MapEditor` + `WorldGrid` + `ContentRegistry` + `ecs::MindWorld` + `NoopWorldHooks`/`NoopRenderHooks`) behind the §3.1 `#[func]` state API (`tool`/`set_tool`, `tool_mode`/`set_tool_mode`, `brush_size`/`set_brush_size`, `draw_block`/`set_draw_block`, `draw_team`/`set_draw_team` over `Team::base_teams()`, `rotation`, `show_terrain/floor/buildings` + setters, `grid`, `can_undo`/`can_redo`, `undo`/`redo`, `tags`/`set_tag`, `status`, `last_error`), **palette behaviour** (`palette_blocks(search)` filter `in_editor && build_visibility != debug_only` + upstream sort `(is_core desc, synthetic asc, is_overlay asc, id asc)`, `block_info`, `palette_teams`, `brush_sizes`), **lifecycle** (`begin_new`/`begin_edit_map`/`begin_edit_image` ≤800/`resize_map`/`save` via `maps_glue`, M7 refinements deferred), **map view** (`map_view.rs` `MapViewDriver`: pan/zoom clamp `0.2..20`, verbatim `project`/`unproject` incl. even-block `-0.5`, `touch`/`drag` tool dispatch + `flush`, brush outline from `Edges::pixel_polygon`) and the **dev/MCP op-log oracle** (`dev_draw_line/rect/circle`, `dev_op_log`/`dev_apply_op_log`/`dev_undo_all`/`dev_redo_all`, `dev_state_digest`, `dev_save_as`/`dev_open`, `check_invariants`). Additive `mind-core` accessors: `OperationStack::ops()` + `MapEditor::retained_ops()`/`op_log()`. Scenes (tscn-first): `client/scenes/editor/map_editor_dialog.tscn`/`.gd` (fullscreen toolbar, team row, brush slider, show toggles, palette search/list, config collapser, `doInput` shortcut wiring `v/i/l/b/e/g/r`, `r/e` rotate, ctrl+z/y/s/g, ctrl+1..5 alt modes) and `map_view.tscn`/`.gd` (SubViewport/`EditorWorldView` host, pointer→`MindEditor` routing, border/grid/brush `_draw`). Plan-16 handoff: `MindRender.mount_editor_view(spec)`/`unmount_editor_view()`/`editor_view_mounted()`/`editor_view_spec()` + `MindWorldRenderer.set_editor_view_active` (stores the `EditorRenderSpec` dict and suspends/restores the main-world band bracket). State inspector gains an `Editor` tab (`state_inspector.tscn` + `_refresh_editor`). Spine adds `/root/Spine/MindEditor` and `/root/Spine/Ui/EditorDialog` (instance of the dialog scene).

**Verify (plan §7 M3, headless/parse subset; §7c-1 deferred to the orchestrator's single-editor mutex).**
- `cargo check -p mind-gdext` clean.
- `bash tools/build.sh` → `Finished` + `sync_scenarios: synced 11 file(s)`.
- `bash tools/godot.sh --headless --editor --quit --path client` → **exit 0, no SCRIPT/Parse/ERROR lines**.
- `cargo test -p mind-core` **1517 lib + 3 blocks_golden + 5 combat_golden + 2 sim_core_determinism + 2 sim_core_meta + 1 sim_core_schedule = 1530 passed / 3 ignored** (baseline F20 unchanged).
- `cargo fmt --all -- --check` clean; workspace `cargo clippy --all-targets -- -D warnings` clean.

**Files.** `mind-gdext/src/editor/{mod,map_view}.rs`, `mind-gdext/src/{lib,render}.rs`; `mind-core/src/editor/{mod,stack}.rs` (additive accessors); `client/scenes/editor/{map_editor_dialog,map_view}.{tscn,gd}` (+ generated `.uid`); `client/scenes/spine.tscn`; `client/scenes/ui/state_inspector.tscn` + `client/ui/state_inspector.gd`.

**Deferred (named).** `EditorPlugin`/autoload not needed (MindEditor is a spine node, HLP §6.6). `MindPreview` `Texture2D` provider (maps-list/dialog data, M4/M6). In-engine MCP §7c-1 (palette/brush screenshot) waits on the single-editor mutex; the mount handoff suspends the main bands but does not yet draw the shared chunk meshes inside the editor `SubViewport` (plan-16 draw integration). Dialog-data APIs (`filters_json`/`objectives_json`/`waves_json`/`locales_json`/`processors`/`assets`) and `playtest`/`edit_in_game`/`resume_editing`/`try_exit` belong to M4–M7.

### M4 — Meta dialogs (DONE 2026-10-03, `lane/f22-19`, base `main` @ `0e3d368`)

**Deliverables.** `mind-core/src/editor/gen.rs` (exported as `editor::generate`): `EditorSnapshot` packed-tile capture, `TilesMapSource`, deterministic `generate_preview(snapshot, filters, content, seed)` and write-through `apply_filters_to_editor(editor, grid, content, filters, seed)` (clears the op stack), `filters_from_tag`. `mind-gdext::MindEditor` gains `filters_json`/`set_filters_json`/`apply_filters`/`sector_generate`. Scenes `client/scenes/editor/{map_info_dialog,map_resize_dialog,map_load_dialog,map_generate_dialog,sector_generate_dialog}.{tscn,gd}`.

**Verify.** `mind-headless editor gen-preview --json` → pixel checksum `21a5176d736430f2`, second run identical, PNG round-trip true, exit 0. MCP §7c-2 deferred (single-editor mutex).

### M5 — Objectives + waves (DONE 2026-10-03, `lane/f22-19`)

**Deliverables.** `mind-derive::ObjectiveFields` proc macro (`#[objective(name,kind,flags)]`); `editor/objectives.rs` (13-class declarative descriptors, `FieldFlags`, `FieldFilter`, `provider_default`, `interpret_number`, `ObjectiveNode`/`edges`/`fixup_parents`, `parse_objectives`/`write_objectives`); `editor/wave_graph.rs` (`WaveGraphMode`, `WaveGraphUnit`, `WaveGraphData::compute`, `next_step`). `MindEditor.objectives_json`/`set_objectives_json`/`waves_json`/`set_waves_json`/`wave_graph`. Scenes `map_objectives_dialog`/`map_objectives_canvas`/`wave_info_dialog`/`wave_graph`. Fixtures `tests/fixtures/editor/{objectives,wave_graph}.json`.

**Verify.** `mind-headless editor objectives --json` → `round_trip_ok`/`editor_pos_ok`/`parents_ok`/`descriptor_ok`/`all_classes_covered` all true; `mind-headless editor wave-graph --json` → checksum `a81e6c788a043d02` (`expected_checksum` equal). MCP canvas/graph screenshots deferred.

### M6 — Processors, locales, banned, assets (DONE 2026-10-03, `lane/f22-19`)

**Deliverables.** `maps/locales.rs` (`MapLocales` helpers, `PropertyStatus`, `read_locale`/`write_locale`, `MapLocaleView: ObjectiveLocale`, `apply_to_all`); `editor/processors.rs` (`ProcessorEntry`, `processor_entries`, `add_processor`, `remove_processor`); `editor/banned.rs` (`BanKind`, set edit ops, `filter_pane`, `rules_json`); `editor/assets.rs` (`DataManagerApi`, `export_zip`/`import_zip`, `AssetRecord`). `MindEditor.locales_json`/`set_locales_json`/`processors`/`assets`. Scenes `map_processors_dialog`/`map_locales_dialog`/`banned_content_dialog`/`data/map_assets_dialog`.

**Verify.** `mind-headless editor locales --json` (apply/rollback + view all true); `editor banned --json` (`Rules` JSON round-trip true); `editor assets --json` (zip round-trip true, `PK`). `data/{patches,content,bundles,images,audio}_view.gd` body views + `assets()` plan-20 mount deferred.

### M7 — Save/playtest/export/exit + budgets (PARTIAL 2026-10-03, `lane/f22-19`)

**Deliverables.** `MindEditor.export_map` (embedded assets) / `export_image` (color-mapped PNG); `editor bench --suite {recache,line,undo,fill,replay} [--size] [--runs]`; `DrawOperation::{capacity,reserve}` + `op_recording_capacity_is_stable_after_warmup`.

**Verify.** `editor bench` exits 0 with `p50_us`/`p99_us` (64×64 dev host: line ~0.1 ms, fill ~0.02 ms, undo ~0.43 ms); `cargo test -p mind-core editor::draw_op` green.

**Landed 2026-10-03 (`lane/f25-maps`, base `main` @ `e3c5174`; commit `15e1050`).** Playtest state machine + map round-trip. `mind-core/src/editor/playtest.rs` (`EditorPlayState` with `edit_in_game`/`playtest`/`resume_editing`/`resume_after_playtest`/`try_exit`, `PlaytestOutcome`) over plan-12 `PlaySession`/`RulesEpoch` + `Gamemode::Editor`; `MindEditor` gains `edit_in_game`/`playtest`/`resume_editing`/`resume_after_playtest`/`try_exit`/`playtest_status`. `maps/mod.rs::Maps::{save_map,import_map}` + `editor/maps_glue::{save_map_e2e,import_map_e2e}` complete the plan-06 M4 round-trip with preview pixels. Harness: `mind-headless editor playtest` (transition checksum `10ed5a26be6add3c`) + `mind-headless maps roundtrip` (preview checksum `57bfde109806f8ea` both sides); new `tests/maps_editor_golden.rs` + `tests/golden/maps_editor.json` (pinned in `parity/golden_manifest.json`); catalogued as `editor_playtest`/`maps_roundtrip`. **Still deferred (plan 07/12 sim host + single-editor mutex):** the in-engine `SubViewport` mount/unmount, synthetic-map/camera/core-spawn world steps, `Gd<ImageTexture>` preview binding, release `bench/editor_baseline.json`, in-engine MapView alloc audit.

### M8 — Verification sweep, `maps fix`, docs (DONE 2026-10-03, `lane/f22-19`)

**Deliverables.** `maps/fix.rs` MapFixer port (hidden banned blocks/units, infinite resources/instant build, unlocalized/typo TimerObjectives, `wave > 1`, revealed blocks, hidden attack-mode) + `mind-headless maps fix [--dir] [--dry-run]`; this changelog + HLP §3 row/§13 entry.

**Verify.** `mind-headless maps fix --json` → `dry_changes 6`, `first_changed 1`, `second_changed 0`, exit 0. Plan-23 scenario registration of `editor gen-preview`/`objectives`/`wave-graph`/`locales`/`banned`/`assets`/`bench`/`maps fix` is left to plan 23's catalogue owner.

### 2026-10-03 — plan 06/19 round-trip + playtest surfaces (`lane/f25-maps`, base `main` @ `e3c5174`; commit `15e1050`)

Maintained the plan-23 catalog/golden registries in the same commit: `scenario_catalog.json` gains embedded `editor_playtest` + `maps_roundtrip`; `golden_manifest.json` gains `harness_maps_editor_roundtrip` (sha256 `96f99ff7ff3d722ad7211025e30f5ed3ad77b2ef5d4b9d3af05e8e30e3a82bfa`). New golden test `client/rust/mind-headless/tests/maps_editor_golden.rs` enforces both. All P0 goldens (`a1a7b96167c9718d`/`57bf3097cbd84349`/`c82143205ece24ed`) and `CHECKSUM_VERSION` unchanged.

### 2026-10-03 — F26 oracle-completeness (`lane/f26-oracle`, base `main` @ `8d67661`): dialog-tag fold + editor baseline + MapView alloc-audit record

Closed the M7 sim-host follow-up and the §7d baseline residual headlessly. **(1) Fold dialog tags into `rules`.** `editor/maps_glue.rs` gains `fold_dialog_tags(tags)`: when the objectives/waves dialog `objectives` or `spawns` tag is present it re-serializes `tags["rules"]` as the canonical `Rules` JSON with `Rules.objectives`/`Rules.spawns` populated (upstream's dialogs mutate `state.rules` directly). Called from `save_editor_map` and `save_map_e2e`; idempotent and a no-op when neither tag exists, so existing maps keep byte-identical `rules`. Test `fold_dialog_tags_writes_objectives_and_spawns_into_rules` (fold + idempotence + no-op). **(2) `bench/editor_baseline.json`.** New §7d baseline file (budgets copied from this plan + the measured `mapview` numbers) registered as a plan-23 budget source in `parity/bench_budgets.json`. **(3) MapView `editor bench` alloc-audit.** New `editor bench --suite mapview` (drawn line + flush + undo + redo over a reused `WorldEditorGrid`; `alloc-audit` counters in the JSON) — recorded `alloc_count 71520` / `alloc_bytes 1245440` over 40 runs at size 200 with `alloc_ok:false`. The steady-state gap is documented in the baseline (`tool::touched_line` per-call scratch `Vec` + `DrawOperation` growth); pre-reserved scratch buffers are a plan-19 follow-up (the per-frame draw call site is plan-15 input). **Evidence:** `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test -p mind-core` 1626 lib + integration / 3 ignored (incl. the new `maps_glue` test); `cargo test -p mind-headless` 130 lib + all goldens; `parity check --tests` 6/6; `editor bench --suite mapview --json` prints the new `alloc_*` fields.
