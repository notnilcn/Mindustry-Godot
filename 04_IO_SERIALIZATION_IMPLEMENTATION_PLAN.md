# 04 — IO & Serialization Implementation Plan

> Inherits `HIGH_LEVEL_PLAN.md` §0 (locked decisions), §2 (architecture), §4 (template), §6–§9 (conventions).
> This plan is the persistence layer for `mind-core`: saves/maps, `TypeIO`, entity revisions, JSON config, settings, save slots and map headers.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | In progress — M0–M2 complete 2026-10-02 (`lane/04-io`) |
| **Phase** | P1 (Platform & content) |
| **Depends on** | `02_CONTENT_IMPLEMENTATION_PLAN.md` (content registry, `ContentType` ordering, `MappableContent`, content-name lookups). Interfaces are written now against a stub registry so most of this plan can land in parallel. |
| **Blocks** | `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (entity IO interface + `IoSet` schedule slots), `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (`WorldContext` impl, tile data hooks), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (`Saves` policy, `SectorInfo`/`Rules`/markers persistence), `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (map registry, previews, image maps), `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (TypeIO for `@Remote` params, custom-chunk net subset, `NetworkIO` world streaming, `@SyncField` interpolation metadata). |
| **Sources** | `core/src/mindustry/io/*.java` (`SaveIO`, `SaveVersion`, `TypeIO`, `JsonIO`, `SaveFileReader`, `MapIO`, `SaveMeta`, `SaveOptions`, `SaveReadState`, `SavePreviewLoader`, `versions/*`); `core/src/mindustry/game/Saves.java`; `core/src/mindustry/world/WorldContext.java` (consumer view); `core/src/mindustry/Vars.java` (dirs/settings keys); `annotations/src/main/java/mindustry/annotations/entity/EntityIO.java` + `annotations/src/main/resources/revisions/**` + `classids.properties`; `tests/src/test/java/ApplicationTests.java`; `tests/src/test/resources/*.msav`; AGENTS: `io/AGENTS.md`, `annotations/AGENTS.md`, `world/AGENTS.md`, `game/AGENTS.md`, `net/AGENTS.md`, `tests/AGENTS.md`. |
| **Extends spine** | Adds to the plan-00 rig: (a) `mind-headless` subcommands `io roundtrip│dump-meta│map-list│settings│check-revisions│bench-save`; (b) GDExtension autoload `MindIo` (`save_game`/`load_game`/`settings_get`/`settings_set`/`list_saves`/`list_maps`) so the MCP rig can save/reload without UI; (c) `FileSystem` root + `data/` layout resolution; (d) `./data` portable dir for tests/headless, mirroring Arc `Fi`/`MockFiles`; (e) a `cargo test -p mind-core` gate asserting `mind-core` has no Godot/tokio dependencies. |

**License**: all new files carry `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`; project is GPL-3.0 (D6). Ported assets/data keep original keys.

---

## 2. Scope & parity definition

### 2.1 In scope

1. **Save engine**: header + `u32` format version + named length-prefixed regions `meta`/`patches`/`content`/`map`/`entities`/`markers`/`custom`; a `SaveVersion` trait with an **append-only** version chain; write-to-temp + `<name>-backup.msav` behavior with restore/fallback on error; meta-only reads for save/map lists; `SaveOptions`/`SaveReadState`/`SaveMeta`; custom-chunk registry for mods.
2. **`TypeIO` equivalent**: byte-tagged object codec used by tile configs, build plans, logic vars, `@Remote` parameters and schematic configs; array caps; safe-read semantics for untrusted input; entity fallback via entity IDs; direct codecs for common types.
3. **Entity IO**: Rust equivalent of the generated revisioned read/write (`revisions/<def>/<N>.json` discipline) — a derive macro with explicit flags, a revision-manifest checker, class-ID registry, tile-entity nested chunks, entity ID mapping/remap, `after_read_all` pass; sync vs save method split and `@SyncField` interpolation metadata exported for plans 16/21.
4. **`JsonIO` equivalent**: `serde`/`serde_json` codecs for `Rules`, `GameStats`, `MapLocales`, `SectorInfo`, map tags, class tags for filters/objectives, content name serializers with upstream fallbacks.
5. **Settings persistence**: Arc-like KV store (`Core.settings` shape) with typed getters/`get_json`/`put_json`/`defaults`, atomic flush, and the save-slot keys used by `Saves.java`.
6. **Save slots & map headers**: `SaveMeta`, `SaveSlot` (file + meta + preview path + name/autosave via settings), parallel slot listing, import/export file ops; `MapHeader`/map meta-only read; preview-cache hooks (pixel generation itself is plan 19).
7. **Directories & FS**: `saves/`, `maps/`, `previews/`, `config/` layout behind a `FileSystem` trait (`NativeFs`, `MockFs`); `data/maps/` custom maps; `data/previews/` caches (`save_slot_<n>.png`, `<name>_v2.png`).
8. **`MapIO` equivalent**: map read/write delegating to the save engine; meta/tags; preview pixel generation hooks; PNG image import/export (color-mapped) entry points.

### 2.2 “Done” means

- A campaign-shaped world (groundZero: ~200 buildings, live units, waves) saves → process reset → loads with **byte-identical sim checksum** (plan-23 canonical dump) via `mind-headless io roundtrip`.
- Legacy Java `.msav` compatibility is **not** claimed by default; the reader is structured so an importer shim can be added without touching the engine (OD2, §2.3.1).
- All ported tests in §7a pass under `cargo test -p mind-core`, including legacy fixtures **when** the `msav-import` feature is enabled.
- MCP scenario in §7c passes against the running Godot client.
- `mind-core` remains Godot-free/tokio-free.

### 2.3 Deliberate deviations

| # | Deviation | Reason |
|---|---|---|
| 1 | **Native magic `MGRS`, not `MSAV`; native version chain starts at 1.** Upstream `.msav` (magic `MSAV`, versions 1–13) is import-only behind the default-off `msav-import` feature. | OD2: byte compatibility not required. A distinct magic prevents v1..v13 alias collisions and makes the importer an explicit, testable boundary. Framing (region names/order, u32 lengths, string maps, chunk nesting, zlib deflate) stays isomorphic so the shim is mechanical. |
| 2 | **Big-endian wire integers** throughout the native container. | Matches Java `DataOutput`/Arc `Writes`; makes the future `.msav` shim a name/field mapping rather than an endian port. |
| 3 | **Child-revision check compares field names** (+type), not just count+size. Renames are flagged and require an explicit revision bump + alias entry. | Upstream `Revision.equal` misses renames (documented gotcha); a silent rename load is a corruption risk we can cheaply eliminate. Data layout is unchanged. |
| 4 | **Entity codec is a Rust derive macro + committed manifests**, not source generation in a separate annotation module. | D1/§6.2: no build-time Java processing; Rust-native mechanism. Generated-like discipline and append-only revision history are preserved. |
| 5 | **`JsonIO.writeBytes`/`readBytes` (UBJson)** not ported — grepped the tree and found no production callers. | Dead code upstream; adding a UBJSON dependency is not justified. Revisit only if a caller appears. |
| 6 | **Unknown objective class tag** → warn + skip that objective (preserve the rest). Legacy lowercase class tag → empty `MapObjectives` (upstream behavior preserved). | Upstream would drop all objectives on a lowercase tag; partial-skip is strictly more resilient for forward-loaded saves. |
| 7 | **Preview textures are owned by plan 19** (Godot `Texture2D`); `mind-core` only computes `PreviewImage { width, height, rgba }` and cache paths. `SavePreviewLoader`’s “missing/foreign preview → delete and re-queue” resilience is kept at the path layer. | `mind-core` must stay Godot-free (D1); textures are render state. |
| 8 | **`Rules::copy` uses `Clone`**, not a JSON round-trip. | Same semantics, no alloc churn; JSON remains the persistence format. |
| 9 | **Save/load is not allowed mid-tick**; load applies at the `IoSet::Apply` boundary owned by plan 05. | Upstream mutates the world on the main thread; we make the boundary explicit for determinism and Bevy scheduling. |

#### 2.3.1 OD2 record (open decision — save/map byte compatibility)

**Decision status: NEEDS USER DECISION** (HIGH_LEVEL §10 OD2; the default below is what this plan implements unless the user opts in).

**Default being built (opt-in = off):**
- Read/write **native `MGRS` v1 only** (import of upstream files fails with the upstream-style message `Unknown save version: …`).
- `msav-import` is a Cargo feature on `mind-core`, **disabled by default**. When enabled it adds `mind_core::io::legacy::java` with readers for upstream save versions 1–13 (`LegacySaveVersion`, `LegacySaveVersion2`, `LegacyRegionSaveVersion`, `ShortChunkSaveVersion`, `Save1..Save13` equivalents), upstream content fallback renames, and legacy unit-name mapping.
- `SaveIo::load` sniffs the first 4 bytes: `MGRS` → native; `MSAV` → importer if built, else the standard error.
- Imported files are **read-only until re-saved**; any save writes native. Upstream maps imported into `data/maps/` are listed and opened, then re-saved native on modification.
- Legacy fixtures `77.msav`, `85.msav`, `108.msav`, `114.msav`, `152.msav`, `152_be.msav` are the oracle. They live in `client/rust/mind-core/tests/fixtures/msav/` (copied from `Mindustry/tests/src/test/resources/`) and are only compiled/run under the feature. Expected assertions are recorded once from the Java reference build into `tests/fixtures/msav/<name>.expected.json` (wave, width/height, building/unit counts, a tile/entity spot-check) and checked in.
- **Not planned**: writing upstream `.msav` (one-way import only) unless the user asks later.

**If the user later opts in (what changes):**
1. Enable `msav-import` for `mind-core`, `mind-headless`, `mind-gdext`; CI adds a fixture job.
2. The shim maps upstream **content IDs → names → our IDs** through the content header (plan 02 guarantees append-only names) and upstream **entity revisions → our revisions** via explicit per-revision adapters (`legacy::java::mappings::<Element>::read_rev`). Upstream revision JSONs are not consumed verbatim; each upstream revision number gets a small adapter until v13.
3. Values that upstream stores in JVM-modified-UTF get a real modified-UTF-8 decoder in `io::legacy::java::utf`.
4. Plan 19 surfaces “Import map/save…” in the UI; plan 22 adds file associations.
5. Maintenance cost: every upstream format change would need an adapter; that is the price of the feature and is why it stays out of the default build.

### 2.4 Deferred ownership

| Item | Owner |
|---|---|
| `Saves` autosave cadence, sector-save remap, `Universe` playtime, slot policy | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` |
| `Rules`, `GameStats`, `MapLocales`, `SectorInfo`, `MapMarkers`, `MapObjectives` **types** (this plan only serializes them) | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` |
| Tile/`Tiles`/`World` runtime structures, `WorldContext` implementation, tile-data semantics | `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` |
| ECS entity runtime, groups, ID allocation, pools, schedule (`IoSet` insertion) | `05_SIM_CORE_IMPLEMENTATION_PLAN.md` |
| Component composition (`@Import`/`@Replace` equivalent), `@SyncField` application | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| Map registry, editor, preview rendering/caching/UI, `ColorMapper` content load | `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` |
| Data patches/`DataAsset` bodies, mod custom chunks registration | `20_MODS_IMPLEMENTATION_PLAN.md` |
| `NetworkIO` world streaming, packet schemas, sync interpolation runtime | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |
| Canonical state dump/checksums, golden scenarios, CI matrix | `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` |

---

## 3. Target design

### 3.1 Modules (all in `client/rust/mind-core/src/io/`, package path parity with `mindustry.io`)

```
mind-core/src/io/
  mod.rs                # re-exports; IoPlugin registration entry point
  fs.rs                 # FileSystem trait, NativeFs, MockFs, Paths (dir layout)
  error.rs              # IoError (thiserror): Header, Version, Region, Chunk, Revision, Bounds, Json...
  wire.rs               # WireWriter/WireReader big-endian primitives (Arc Writes/Reads) — added at M0, not in the original sketch
  save/
    mod.rs              # SaveIo: write/save/load/get_meta/is_save_valid/file_for/backup_file_for
    version.rs          # SaveVersion trait; VERSIONS registry (OnceLock<Vec<Arc<dyn SaveVersion>>>)
    versions/v1.rs      # native v1 writer + reader (current)
    chunk.rs            # SaveFileReader: regions, chunks, string maps, CustomChunk registry, scratch
    meta.rs             # SaveMeta
    options.rs          # SaveOptions
    state.rs            # SaveReadState, WorldContext trait (impl'd by plan 06), TemporaryContentMapper
    slot.rs             # SaveSlot, SaveSlotIndex, preview paths, import/export/delete
  typeio/
    mod.rs              # write_object/read_object/read_object_safe + TypeValue
    tags.rs             # tag constants (identical values to upstream)
    codecs.rs           # direct codecs: string/int/bytes/item/status/rules/objectives/plans/...
    mapper.rs           # ContentMapper (temporary mapper guard)
  entity/
    mod.rs              # EntityCodec trait, EntityReader/EntityWriter, entity chunk IO
    registry.rs         # EntityDefs!{} macro: name, class id, serialize/sync flags, component list
    class_ids.rs        # generated(committed) constants from entity_class_ids.toml
    revisions.rs        # revision manifest load/check/write (.json)
    idmap.rs            # EntityMapping (custom id -> name), duplicate-id handling, after_read_all
  json/
    mod.rs              # JsonIo read/write/read_into; class tags registry
    content_serde.rs    # Content-type serializers with upstream fallbacks
    rules.rs            # serde derives + defaults for Rules/GameStats/MapLocales/SectorInfo/objectives
  map/
    mod.rs              # MapIo: create_map/write_map/load_map/is_image/write_image/read_image
    preview.rs          # PreviewImage, generate_preview(map|tiles), color_for
    header.rs            # MapHeader
  settings.rs           # SettingsStore (Arc-like KV)
  legacy/
    mod.rs              # feature-gated: importer dispatch
    java/               # `msav-import` only: Save1..13 readers, modified-UTF, fallbacks
  slots.rs              # (re-export) listing helpers used by plan 12
```

Proc-macro crate (new workspace member): `client/rust/mind-derive/` — `#[derive(EntityIo)]`, `#[derive(TypeIo)]`, `define_entity_defs!` helpers. Pure Rust (`syn`/`quote`); no Godot. **Orchestrator note**: this adds one crate beyond §2.1; it is required by D1 (“no build-time Java processing”) and mirrors `annotations/` scope (see §8 RD1).

### 3.2 Save engine

```rust
pub trait SaveVersion: Send + Sync {
    fn version(&self) -> u32;
    fn read(&self, s: &mut SaveReader, state: &mut SaveReadState) -> Result<()>;
    fn write(&self, s: &mut SaveWriter, options: &SaveOptions) -> Result<()>;
    fn get_meta(&self, s: &mut SaveReader) -> Result<SaveMeta>;      // reads header len + meta only
    // default region hooks (override per version exactly like Save11/Save12):
    fn read_map(&self, ..) / fn write_map(..)
    fn read_entities(&self, ..) / fn write_entities(..)
    fn read_data_patches(..) / fn write_data_patches(..)
    fn read_custom_chunks(..) / fn write_custom_chunks(.., net: bool)
}
```

- Registry: `static VERSIONS: OnceLock<Vec<Arc<dyn SaveVersion>>>` built from `version_array()`; writer = last element. `get_writer(version)` resolves; unknown version error message **must match upstream**: `Unknown save version: {n}. Are you trying to load a save from a newer version?`.
- Adding a native version: add `versions/vN.rs`, append to `version_array()`, override only changed region hooks; never renumber/remove old entries; old JSON fixtures keep loading.
- `SaveIo::save(file, options)`: ensure parent; if file exists → rename to `backup_file_for`; write to `file.tmp`; flush+sync; rename over target; on any error restore backup and delete tmp. `load`: try primary; on error try backup (log), else propagate `SaveIoError::with_cause` (upstream `SaveException` semantics).
- `SaveIo::get_meta(file)`: stream → header/version → `ver.get_meta`; no world/content access beyond name lookups; used for listing (parallel, `std::thread::scope`, `available_parallelism()`).
- **Region IO**: `read_region(name, len `u32`, closure)`: read payload into a reusable scratch `Vec<u8>` (cap `MAX_REGION_BYTES = 128 MiB`), wrap in a `SliceReader`, run closure, then require `consumed == len` → else `RegionLengthMismatch { name, expected, actual }` (upstream message text kept). `write_region`: serialize into scratch `SaveScratch` (two buffers, mirroring upstream `byteOutput`/`byteOutput2`), then write `u32 len` + bytes; **nesting depth ≤ 2** (region → chunk) enforced, matching upstream’s one-level-nested static buffers.
- Custom chunks: `add_custom_chunk(name, Arc<dyn CustomChunk>)` into an `IndexMap`; `should_write()`/`write_net()` filters; unknown names on read are skipped via their length prefix (never fatal). Registered by plan 20 for mods.
- Events (plan 00 bus): `SaveWriteEvent` before writing, `SaveLoadEvent(is_map)` after load, `RulesLoadEvent(from_save)` for non-map saves (upstream fires it only when `!context.is_map() && !state.is_campaign()`; plan 12 handles campaign).

### 3.3 Native format & versioning

- Magic `MGRS` (4 bytes) + `u32 format_version` (native starts at **1**) + regions in fixed order `meta`, `patches`, `content`, `map`, `entities`, `markers`, `custom` (native v1 always writes all; readers tolerate optional trailing regions for forward growth).
- Stream compression: raw zlib deflate (level 1/fast) via `flate2` **rust backend** (`miniz_oxide`); same framing as Java `InflaterInputStream`, so the importer shim needs no new decoder. `MAX_DECOMPRESSED_BYTES` guard while inflating.
- `SaveWriter`/`SaveReader` own the counter and scratch; wire primitives are big-endian (`i8/u8/u16/u32/i32/i64/f32/f64`, `bool` as `u8`, strings as `u16` byte-length + UTF-8; `string_map` as `i16` count + pairs).
- OD2 shim boundary: only `save/versions/native` and `legacy/java` know wire layouts; the engine sees regions/hooks.

### 3.4 `TypeIO` equivalent

- Tag byte constants preserved exactly (`null=0 … unit_command=23`, `io/AGENTS.md`).
- `pub enum TypeValue { Null, Int(i32), Long(i64), Float(f32), Str(Option<String>), Content(ContentType, u16), IntSeq(SmallVec<i32>), Point2(i32,i32), Point2Array(SmallVec<i32>), TechNode(ContentType,u16), Bool(bool), Double(f64), Building(EntityRef), LAccess(u16), ByteArray(Vec<u8>), BoolArray(Vec<bool>), Unit(EntityRef), Vec2Array(SmallVec<[f32;2]>), Vec2(f32,f32), Team(u8), IntArray(Vec<i32>), ObjectArray(Vec<TypeValue>), UnitCommand(u16) }`.
- Caps: `MAX_ARRAY=1000`, `MAX_BYTE_ARRAY=40_000`, `MAX_SYNCED_PLANS=20`, safe string `1200`; non-safe object arrays capped at `200` (upstream quirk kept); `read_object_safe` used for all untrusted network input (plan 21) and always for plan configs.
- `read_object` takes `(box: bool, mapper: Option<&ContentMapper>, safe: bool, allow_arrays: bool)`; nested arrays forbidden when `allow_arrays=false`; unknown tag → `IoError::UnknownTypeTag`.
- Content values resolve through the temporary `ContentMapper` during save load, else the global registry; missing content falls back (`Blocks::air`, `Items::copper`, `Liquids::water`, `UnitTypes::dagger`, `Planets::serpulo`) exactly as upstream `JsonIO`/`TypeIO` do.
- Entity fallback: `write_entity_ref(EntityRef)` writes `i32` ID; `read_entity_ref` resolves via plan-05 `EntityIndex`; refs to not-yet-loaded entities become `EntityRef::Pending(i32)` recorded in `SaveReadState` and resolved in the `after_read_all` pass.
- Direct (non-tagged) codecs mirror `TypeIO.java`: `write_string/read_string` (exists byte + UTF), `write_int*`, `write_bytes/shorts`, `write_items/item_stacks/liquid_stacks`, `write_status/read_status` (dynamic-field bitmask), `write_rules/read_rules` (JSON bytes, 100 000 cap), `write_objectives/read_objectives` (60 000 cap), `write_objective_marker`, `write_plans/write_plans_queue_net/write_client_plans` (config whitelist: `Number|Boolean|Content`), `write_ui_builder`/`write_menu_result` (plan 14 payloads; codec lives here, types there).
- **Invariant**: all writers/readers take `&mut` and hold no shared mutable state (satisfies upstream’s thread-safety note by construction); caps are `const`.

### 3.5 Entity IO & revisions

```rust
pub trait EntityCodec: Sized {
    const NAME: &'static str;                 // element name, e.g. "BuildingComp"
    const CLASS_ID: u8;                       // from entity_class_ids.toml (append-only)
    fn fields() -> &'static [FieldDesc];      // name, type, size, flags
    fn sync_fields() -> &'static [SyncFieldMeta]; // name, Interp::{Linear,Angle,None}, clamped
    fn write(&self, w: &mut EntityWriter) -> Result<()>;
    fn read(&mut self, r: &mut EntityReader, revision: u16) -> Result<()>;
    fn write_sync(&self, w: &mut EntityWriter) -> Result<()>;
    fn read_sync(&mut self, r: &mut EntityReader, revision: u16) -> Result<()>;
    fn interpolate(&self, target: &Self, alpha: f32);   // consumed by plans 16/21
}
```

- `#[derive(EntityIo)]` emits the trait impl: `write` emits the **newest revision** (`u16`) then fields in declaration order; `read` is a `match revision` chain assigning only fields that exist in that revision; unknown revision → `IoError::UnknownRevision` (debug log + continue skipping the chunk where recoverable, mirroring upstream’s reassign/skip behavior). `#[entity(no_serialize)]`, `#[entity(no_sync)]`, `#[entity(sync_local)]`, `#[entity(sync_field(interp="linear|angle", clamped))]`, `#[entity(transient)]` map 1:1 to upstream annotations.
- `EntityDefs! { Building => [PosComp, HealthComp, …], Unit => [...], … }` (explicit macro list in `registry.rs`) gives the entity def → component aggregation and class IDs. Plan 05 owns entity spawning/groups; plan 11 owns which components exist. **This list is the reconciliation point with 05/11** (§8 RD2).
- Revision manifests: `client/rust/mind-core/revisions/<NAME>/<N>.json`, schema `{"version":N,"fields":[{"name","type","size","flags"}]}`. Append-only; never edit/delete old files. `mind-headless io check-revisions` (check mode, CI) compares `EntityCodec::fields()` to the newest manifest; `--update` writes the next `N.json` (mirrors `EntityIO.java` behavior and warning).
- Class IDs: committed `client/rust/mind-core/entity_class_ids.toml` (mirrors `classids.properties`; existing names keep IDs; new defs get `max+1`; never renumber). `class_ids.rs` is generated by `check-class-ids --update` and must be committed; plan 20 registers mod entity defs at runtime.
- Entity chunks: `write_chunk(class_id u8, entity_id i32, payload)`; `entities` region order = custom ID mapping (`u16 id → UTF name`), team build plans (`team i32`, count, `x/y/rot/block u16`, `TypeIO` config), then entity chunks. `read_world_entities` skips unknown class IDs, warns + reassigns duplicate entity IDs (`EntityGroup::next_id`), then runs `after_read_all` on all entities (upstream `Groups.all/unit/allBuildings`).
- Tile entities: nested chunk at the multiblock center (`entity.version() u8` + fields); non-center tiles write only a presence bool. Removed blocks’ entity chunks are skipped by length when `block.has_building()==false`.
- `@SyncField` metadata is a pure data export (`sync_fields()`), consumed by plan 21 (server→client interpolation) and view interpolation (plan 16). Storage of `_TARGET_`/`_LAST_` companions and the sync schedule are plan 05/11/21, not here.

### 3.6 `JsonIO` equivalent

- `JsonIo::write<T: Serialize>(v) -> String`, `read<T: DeserializeOwned>(s) -> Result<T>`, `read_into(base, s)` (field overlay like upstream `read(T, String)`). On read, strip legacy `io.anuke.` prefixes.
- Content serializers (`content_serde.rs`): `Item`/`Liquid`/`Block`/`Planet`/`UnitType`/`Weather`/`UnlockableContent`/`SectorPreset`/`Attribute`/`StatusEffect` → content **name**; read with the upstream fallback table (`SaveFileReader::fallback` and `modContentNameMap`). `Team` → `u8` id. `Color` → `#rrggbb[aa]` string. `Sector` → `"<planet>-<id>"`. `ItemStack` → `{"item":name,"amount":n}`. `MusicContainer` → name string.
- Class tags: `ClassTagRegistry` mapping camelized tag → deserializer (`"damage"`, `"heat"`, … for filters; objectives). `MapObjectives` gets a hand-written `Serialize/Deserialize`: array of `{"class":tag, …fields, "parents":[indices], "editorPos":packed}`; lowercase legacy tags → empty executor (upstream behavior); unknown class → warn + skip element. `Rules` gets `#[serde(default)]` on every field so old/new JSON both load; field names stay **exactly** upstream camelCase (OD9).
- Rules/stats/settings/map tags are the only consumers; sim never reads JSON per-tick.

### 3.7 Settings persistence

- `SettingsStore` (Arc-like): `IndexMap<String, SettingValue>` with `SettingValue::{Str,Int,Long,Float,Bool,Bytes,Json}`; API `get_string/get_i32/get_i64/get_f32/get_bool/get_json<T>/get_bytes`, `put*`, `toggle`, `defaults(list)`, `force_save`, `flush_if_dirty`.
- Disk: `config/settings.bin` — magic `MGST`, `u16` version, `u16` count, then typed key/value entries; atomic write (tmp+rename); debounced flush (default 1 s after mutation) + `force_save` on app quit/StateChange(menu).
- Ported key names (parity ABI): `save-<n>-name`, `save-<n>-autosave`, `saveinterval`, `last-sector-save`, `uiscale`, `locale`, `name`, `color-0`, etc. Plan 12/14 read the same keys.
- Test backend: `MockFs` + in-memory store; no global statics — `SettingsStore` is owned by the plan-00 app state and referenced by plans via a handle.

### 3.8 FS, directories, slots, preview hooks

- `FileSystem` trait: `read/write/open/append/exists/delete/rename/copy/ls/walk/mkdirs/absolute`, `Fi`-like handles by path. `NativeFs` (std::fs) and `MockFs` (in-memory, test parity with Arc `MockFiles`). Sim/IO never touches `std::fs` directly.
- Layout (resolved once by plan 00):
  - `saves/` → slot files `<n>.msav`, sector saves `sector-<planet>-<id>.msav`, backups `<name>-backup.msav`.
  - `maps/` → custom maps; built-ins come from the asset tree (plan 03).
  - `previews/` → `save_slot_<n>.png`, `<map>_v2.png`, `<map>-cache_v2.dat` (paths only; plan 19 writes PNGs/caches).
  - `config/` → `settings.bin`.
- Root default: portable `./data` when writable (upstream parity), else Godot `user://` (see §8, **NEEDS USER DECISION**). `mind-headless --data-dir <path>` overrides.
- `SaveSlot`: `{ file, meta: SaveMeta, requested_preview }`; `load/load_with(context)/save()/delete()/import_file()/export_file(embed_assets)/preview_path()/name()/set_name()/autosave()/is_autosave()/is_sector()/mode()/get_wave()`. Slot listing walks `saves/`, skips `*backup*`, validates via meta-read in parallel. Save-name/autosave KV via `SettingsStore` (upstream key scheme). Preview reload resilience: if a preview file is missing/stale/foreign, delete + mark for re-render (plan 19 renders).
- `SaveSlot` file/meta API is here; **autosave policy, sector remap and `Universe` integration are plan 12** (it wraps these types).

### 3.9 `MapIO` equivalent

- `MapIo::create_map(file, custom) -> MapHeader` (meta-only; no world alloc) — used by map lists.
- `MapIo::write_map(file, map, embed_assets)` → `SaveIo::save` with `SaveOptions { extra_tags: map.tags, embed_assets }`; `load_map(file, context)` → `SaveIo::load`.
- `MapIo::is_image(file)` (PNG signature); `write_image(tiles) -> PreviewImage`; `read_image(image, tiles, color_mapper) -> Result<()>` (skips buildings, floors default to stone) — editor owns invocation (plan 19).
- `generate_preview(map) -> PreviewImage` reads meta + content + the `preview_map` region through a `CachedTile`-equivalent (no events, no world mutation); `generate_preview_from_tiles(tiles)` for the editor. `PreviewImage { width: u32, height: u32, rgba: Vec<u8> }`.
- `color_for(wall, floor, overlay, team)` + minimap-color paths ported; the actual `Block::minimap_color` lives in plans 02/07; missing colors default deterministically.
- `MapHeader { name, width, height, version, build, tags: StringMap, teams: BTreeSet<u8>, spawns: u32 }`.

### 3.10 Schedule, threading & boundaries

- No IO runs inside the sim tick. Plan 05 inserts `IoSet` into the Bevy schedule:
  - `IoSet::Apply` (pre-tick): drain queued load requests, apply region data to the world, run `after_read_all`.
  - `IoSet::Capture` (post-tick, after `LogicUpdate`): snapshot the world into owned region buffers; compression + file write on a worker `std::thread` (no tokio; `mind-core` stays async-runtime-free).
- Save completion/errors are delivered via the plan-00 event bus (`SaveCompleteEvent { ok, error }`); UI (plan 14) subscribes.
- Wall-clock inputs (`saved` timestamp, playtime) are injected by the platform clock trait and only land in `meta` tags — never in sim state (determinism, §2.4 HIGH_LEVEL).
- Content temporary mapper uses an RAII guard (upstream `finally { setTemporaryMapper(null) }` behavior); `world.set_generating(false)` restored on all exits.

### 3.11 Godot / STDB touchpoints

- **Godot**: no new scenes. `mind-gdext` registers autoload class `MindIo` with methods `save_game(slot: String) -> bool`, `load_game(slot: String) -> bool`, `list_saves() -> Array[Dictionary]`, `list_maps() -> Array[Dictionary]`, `settings_get(key, default) -> Variant`, `settings_set(key, value)`, `preview_for_slot(slot) -> Image` (plan 19 converts to texture). Plan 00 owns the scene; plan 14 owns dialogs.
- **STDB**: **no new tables/reducers/views.** `Rules`/`GameStats`/`SectorInfo` serialized shapes are shared with plan 12’s sector tables and plan 21’s payloads; this plan adds serde round-trip tests to keep those shapes stable.

### 3.12 Boundaries & invariants

1. `mind-core` (and `io`) must not depend on `godot`, `tokio`, or platform types — CI check in plan 00.
2. No `unwrap()`/`expect()` on runtime data; all IO returns `IoError`; panics only for debug invariants.
3. Append-only forever: format versions, content IDs (plan 02), entity class IDs, entity revisions, custom chunk names.
4. Unknown save version → exact upstream message; unknown content → documented fallbacks; unknown custom chunk → skipped; unknown entity revision → error at that entity only (continue where recoverable).
5. Region payloads are length-checked; untrusted reads are capped (`safe` TypeIO, region/chunk/string/array caps).
6. No `HashMap` iteration on serialized/sim-visible paths; ordered maps only (`IndexMap`, `BTreeMap`).
7. Hot-path rule: save/load may allocate; TypeIO in the net path reuses scratch buffers and never grows unbounded (plan 21 measures).
8. Every ported source file starts with the GPL header comment.

---

## 4. Port map

| Mindustry (source) | Rust target | Notes |
|---|---|---|
| `io/SaveIO.java` | `io/save/mod.rs` (`SaveIo`) | Header, registry, save/load/backup, `is_save_valid`, `file_for`, `backup_file_for`; native magic `MGRS`; keep error text. |
| `io/SaveVersion.java` | `io/save/version.rs` (`SaveVersion` trait) + `io/save/versions/v1.rs` | Regions `meta/patches/content/map/entities/markers/custom`; `read_rules` after patches; temp mapper guard. |
| `io/versions/Save1..Save13`, `LegacySaveVersion`, `LegacySaveVersion2`, `LegacyRegionSaveVersion`, `ShortChunkSaveVersion` | `io/legacy/java/*` (feature `msav-import`) | One reader per upstream version; short-chunk and legacy map/entity layouts preserved for fixtures. Default build: absent. |
| `io/versions/LegacyIO.java` | `io/legacy/legacy_io.rs` | Unit-name map; server-list bytes reader (settings). |
| `io/SaveFileReader.java` | `io/save/chunk.rs` | Regions, chunks, legacy short chunks, string maps, `CustomChunk`; fallback rename tables (`fallback`, `mod_content_name_map`). |
| `io/SaveMeta.java` | `io/save/meta.rs` (+ `map/header.rs`) | `is_map()` = tags contain `"name"`; rules parsed lazily. |
| `io/SaveOptions.java` | `io/save/options.rs` | `embed_assets`, `extra_tags`. |
| `io/SaveReadState.java` | `io/save/state.rs` | `preview`, `rule_string`, `all_buildings`; adds `pending_entity_refs`. |
| `io/SavePreviewLoader.java` | `io/save/slot.rs` (paths) + plan 19 loader | Godot texture loading deferred; keep delete-and-requeue resilience. |
| `io/TypeIO.java` | `io/typeio/{mod,tags,codecs,mapper}.rs` | Tag table, caps, safe reads, entity fallback, plans/configs, status/items/rules/objectives. |
| `io/JsonIO.java` | `io/json/{mod,content_serde,rules}.rs` | serde; `io.anuke.` strip; class tags; content fallbacks. |
| `io/MapIO.java` | `io/map/{mod,preview,header}.rs` | Meta-only create, write=save, preview pixels, PNG image maps. |
| `game/Saves.java` | `io/save/slot.rs` (`SaveSlot`, listing, import/export) + `12_CAMPAIGN...` (`Saves` policy) | Split: file/meta here, autosave/sector policy in 12. |
| `annotations/entity/EntityIO.java` + `revisions/<def>/<N>.json` | `mind-derive` (`#[derive(EntityIo)]`) + `io/entity/revisions.rs` + `crates/mind-core/revisions/` | Revision byte then fields; `match revision` read chain; append-only manifests; name-strict compare (deviation 3). |
| `annotations/entity/EntityProcess.java` (`Groups`, `EntityMapping`) | `io/entity/registry.rs` (`EntityDefs!`), `idmap.rs`; runtime groups in `05` | Class IDs + custom mapping + duplicate reassign + `after_read_all`. |
| `annotations/remote/TypeIOResolver.java` / `SerializerResolver.java` | explicit `TypeIo` impls/derive in `io/typeio` + plan 21 packet derive | No reflection scan; unknown param type is a compile error. |
| `annotations/src/main/resources/classids.properties` | `crates/mind-core/entity_class_ids.toml` + `io/entity/class_ids.rs` | Append-only; generated by `check-class-ids --update`; committed. |
| `Arc Settings` / `Core.settings` | `io/settings.rs` (`SettingsStore`) | Typed KV, JSON values, debounced atomic flush, upstream key names. |
| `Arc Fi` (used by IO) | `io/fs.rs` (`FileSystem`, `NativeFs`, `MockFs`) | Enables headless/mock tests; single root resolver. |
| `Vars.saveDirectory/mapPreviewDirectory` etc. | `io/fs.rs` `Paths` | `saves/`, `maps/`, `previews/`, `config/`. |
| `world/WorldContext.java` (IO consumer surface) | `io/save/state.rs` `WorldContext` trait, implemented in `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` | 04 defines the trait because 04 blocks 06; 06 implements/aliases it. |
| `game/SectorInfo.java` IO | `io/json/rules.rs` derives + plan 12 types | `write/read` via `JsonIo`/TypeIO as upstream. |
| `FogControl` custom chunk (`static-fog-data`) | plan 12 registers via `add_custom_chunk` | Container support here; payload in 12/21. |

---

## 5. Milestones & task breakdown

Each milestone ends with evidence (test output / dump path / screenshot) appended to the Changelog.

- **M0 — Chunk primitives + container (smallest vertical slice).** ✅ **COMPLETE 2026-10-02**
  `io/fs.rs`, `io/error.rs`, `io/wire.rs` (wire primitives; layout addition, see Changelog), `save/chunk.rs`, `save/version.rs`, `save/mod.rs`, `save/versions/v1.rs` writing **only `meta`** (empty world), `save/meta.rs`, `save/options.rs`, `save/state.rs` (`SaveReadState` + `WorldContext` trait, C10).
  *Verify*: `cargo test -p mind-core io::save::chunk` (region round-trip, length-mismatch error, nested chunk, string map, backup restore) + `mind-headless io dump-meta` on a file written by M0.
- **M1 — FS + settings + directories.** ✅ **COMPLETE 2026-10-02**
  `io/settings.rs`, `io/fs.rs` `Paths`, atomic flush, mock backend.
  *Verify*: `mind-headless io settings` set→flush→reload equality; corrupted-settings file falls back to defaults.
- **M2 — TypeIO.** ✅ **COMPLETE 2026-10-02**
  `typeio/{mod,tags,codecs,mapper}.rs`; ported `writeStringTest`, `writeRules`, plan/config codecs; caps tests.
  *Verify*: `cargo test -p mind-core io::typeio` (round-trips, oversize rejection, unknown tag, safe-mode read).
- **M3 — Entity codec + revisions + class IDs.**
  `mind-derive` crate, `io/entity/**`, revision manifests for all existing entity defs (43 revision dirs upstream), `entity_class_ids.toml`, `mind-headless io check-revisions [--update]`.
  *Verify*: revision check test green; synthetic component add/remove/rename yields correct read-skip and fails strict name check until bumped; class-ID append-only test.
- **M4 — Full native v1 map/entities/markers/custom.**
  `map` region RLE + tile data + nested tile-entity chunks against `WorldContext` stubs; `content` header + temp mapper; team plans; entity ID mapping; `after_read_all`; `markers` via plan-12 trait stub; `custom` registry; `patches` via plan-20 trait stub (empty OK).
  *Verify*: `mind-headless io roundtrip --map serpulo/groundZero --ticks 600` checksum equality (world placeholder from plan 06; until then use a synthetic 64×64 map fixture).
- **M5 — Slots + MapIO + previews hooks.**
  `save/slot.rs`, `map/**`; parallel listing; meta-only reads; import/export; `write_image/read_image` using a plan-02 `ColorMapper` stub; `PreviewImage` generation delegates to plan 19.
  *Verify*: `mind-headless io map-list` prints correct meta for maps+saves; slot name/autosave KV round-trip; corrupted primary falls back to backup.
- **M6 — JsonIO.**
  `json/**`; content serializers + fallbacks; `Rules`/`GameStats`/`MapLocales`/`SectorInfo` derives; class tags; objectives (incl. parents fixup + legacy lowercase rule).
  *Verify*: ported `writeRules2`; golden JSON fixture from upstream `JsonIO` for a rules blob; forward/backward field tolerance.
- **M7 — Legacy importer (gated by OD2).**
  `io/legacy/java/**` under `msav-import`; fixtures + expected JSON.
  *Verify*: `cargo test -p mind-core --features msav-import io::legacy` loads 77/85/108/114/152/152_be with expected counts; **skipped/not built by default**.
- **M8 — MCP integration + budgets + docs.**
  `MindIo` autoload hooks wired with plan 00/05; dev actions `save_current`/`load_current`; bench command; final budgets.
  *Verify*: §7c scenario; `cargo bench -p mind-core --bench io` numbers recorded.

Dependency-safe ordering: M0–M2 need only plan 02’s name registry and plan 00’s FS scaffold; M3 stubs the plan-05 entity index; M4 stubs plan-06 `WorldContext` then swaps in the real implementation when 06 lands (the trait fixes the seam).

---

## 6. Data & formats

### 6.1 Native container (`.msav` extension, magic `MGRS`)

```
offset  bytes
0       "MGRS"                          magic
4       u32 format_version              native = 1
8       region*
        region := name_len:u8, name:utf8, byte_len:u32, payload
        (deflate/zlib wraps the whole stream after the 4-byte magic)
payload meta     := i16 count, (key:utf16-byte-len+utf8, value:same)*
payload content  := u8 mapped_types, (type:u8, count:u16, name:utf8)*
payload map      := u16 width, u16 height,
                    floor_pass: (floor:u16, overlay:u16, run:u8)*,
                    block_pass: (block:u16, packed:u8 [bit0 entity, bit2 data],
                                 [data:u8, floor_data:u8, overlay_data:u8, extra:i32]?,
                                 [is_center:bool, chunk(entity)]?, run:u8?)*
payload entities := id_map: (u16 custom_id, name:utf8)*,
                    teams: (team:i32, count:i32, (x:u16,y:u16,rot:u16,block:u16,config:TypeIO)*)*,
                    entity*: chunk(class_id:u8, id:i32, revision:u16, fields)
payload markers  := plan-12 `MapMarkers::write`
payload custom   := i32 count, (name:utf8, chunk)*
```

Chunk := `u32 byte_len` + bytes; nesting depth ≤ 2; region bytes cap 128 MiB.

### 6.2 `meta` keys (parity ABI — keep names)

`version`, `saved`, `playtime`, `build`, `mapname`, `wave`, `tick`, `wavetime`, `stats`, `rules`, `sectorPreset` (empty string = null), `locales`, `mods`, `controlGroups`, `width`, `height`, `viewpos`, `controlledType`, `nocores`, `playerteam`, `hasExternalAssets`; arbitrary `extra_tags` (map tags) merged on top. `SaveMeta::is_map()` ⇔ `tags` contains `name`.

### 6.3 TypeIO tag table (frozen)

`0 null, 1 int, 2 long, 3 float, 4 string, 5 content, 6 int_seq, 7 point2, 8 point2_array, 9 tech_node, 10 bool, 11 double, 12 building, 13 l_access, 14 byte_array, 15 legacy, 16 bool_array, 17 unit, 18 vec2_array, 19 vec2, 20 team, 21 int_array, 22 object_array, 23 unit_command`. Caps: `MAX_ARRAY=1000`, `MAX_BYTE_ARRAY=40_000`, `MAX_SAFE_STRING=1200`, `MAX_SYNCED_PLANS=20`, unsafe object array `200`, `MAX_RULES_BYTES=100_000`, `MAX_OBJECTIVES_BYTES=60_000`.

### 6.4 Entity revision manifest

```json
{ "version": 2,
  "fields": [
    { "name": "health", "type": "f32", "size": 4, "flags": ["save"] },
    { "name": "items",  "type": "ItemModule", "size": -1, "flags": ["save"] },
    { "name": "rotation", "type": "f32", "size": 4, "flags": ["save","sync","interp:angle"] }
  ] }
```

Directories: `crates/mind-core/revisions/<ElementName>/<N>.json` (append-only). Flags: `save`, `sync`, `sync_local`, `interp:linear|angle`, `clamped`, `transient`. Renames require bump + `aliases` entry (`"old_name" -> "new_name"`) used only by the importer path.

`entity_class_ids.toml`:

```toml
[ids]
"BuildingComp" = 6
"UnitComp" = 14
```

Append-only; regenerated by tooling with `--update`; committed.

### 6.5 Settings file

`config/settings.bin`: magic `MGST`, `u16` version = 1, `u16` count, entries `(key:u16+utf8, type:u8, payload)`, types `0 str, 1 i32, 2 i64, 3 f32, 4 bool, 5 bytes, 6 json(utf8)`. Atomic tmp+rename; debounced flush. Key names as §3.7.

### 6.6 Directories

```
<data-root>/
  saves/      <n>.msav, sector-<planet>-<id>.msav, <name>-backup.msav
  maps/       *.msav (custom)
  previews/   save_slot_<n>.png, <map>_v2.png, <map>-cache_v2.dat
  config/     settings.bin
```

### 6.7 JSON

Field names exactly upstream camelCase (OD9). `Rules` uses `#[serde(default)]` per field; banned/spawns/loadout element types preserved; `Color` as hex string; content as name; objectives as class-tagged array with `parents` index array. Unknown fields ignored; `io.anuke.` prefix stripped on read.

---

## 7. Oracle & verification

### 7a. Ported tests (`Mindustry/tests/src/test/java/ApplicationTests.java` → `cargo test -p mind-core`)

| Upstream test | Rust test | Oracle |
|---|---|---|
| `writeStringTest` (null, ASCII, CJK, emoji) | `io::typeio::tests::write_string_roundtrip` (case table) | exact round-trip; null preserved; no panic on long strings |
| `writeRules` (TypeIO binary) | `io::typeio::tests::write_rules_binary` | `attackMode`/`buildSpeedMultiplier` equal after round-trip |
| `writeRules2` (JsonIO) | `io::json::tests::rules_json_roundtrip` | fields + `rules.tags` equal |
| `save` | `io::save::tests::save_writes_valid_file` | file validates; meta readable |
| `saveLoad` | `io::save::tests::save_then_load_preserves_unit_and_map` | dagger at (20,30) hp 30 persists; world size equals map; player cores present |
| `createMap`/`playMap` (map-list path) | `io::map::tests::create_map_reads_meta_only` | width/height/tags equal; no world mutation |
| `load77Save` … `load152Save` (`msav-import` only) | `io::legacy::tests::load_<n>` | expected fixture JSON (wave, size, building/unit counts, spot-check tile) |
| (new, from `Saves.java` behavior) | `io::save::slot::tests::{backup_fallback, slot_meta_listing_parallel, preview_paths}` | backup load on corrupt primary; 100-slot listing sorted; preview path scheme |
| (new) | `io::entity::tests::{revision_check, rename_requires_bump, duplicate_id_reassign, unknown_class_skip}` | append-only discipline enforced |

### 7b. Headless harness scenarios (`mind-headless`)

| Command | Scenario | Assertion |
|---|---|---|
| `mind-headless io roundtrip --map serpulo/groundZero --ticks 600 --out tmp/rt.msav` | load map, run 600 ticks, canonical dump checksum `C0`, save, reset, load, dump checksum `C1` | `C0 == C1` (plan-23 canonical dump: tiles, sorted entities, ids, configs) |
| `mind-headless io dump-meta tmp/rt.msav` | meta-only read | width/height/wave/build match save-time values |
| `mind-headless io map-list --dir client/rust/mind-core/tests/fixtures/maps` | parallel listing incl. one corrupt file | corrupt entries skipped with warning; others listed |
| `mind-headless io settings --data-dir tmp/io-data` | set/get/flush/reload + corrupt file | values persist; corrupt → defaults, no panic |
| `mind-headless io check-revisions [--update]` | all `EntityDefs!` vs manifests | check mode exits non-zero on drift; update appends `N.json` and leaves old files untouched |
| `mind-headless bench io-save --map serpulo/groundZero --iters 50` | save/load timings | see §7d |
| `cargo test -p mind-core --features msav-import` | legacy fixtures | 77/85/108/114/152/152_be load; expected JSON matches |

### 7c. MCP playtest scenario (open-godot-mcp)

Preconditions: plan-00 rig with autoload `MindIo` and dev actions; plan 06 world placeholder acceptable for M0–M4, real map for M8.

1. `godot_health` → `{ok:true}`.
2. `godot_game play` (scene from plan 00, e.g. `res://scenes/spine.tscn`).
3. `godot_exec eval`: place a determinism probe — e.g. `MindSim.dev_place("conveyor", 40, 40, 0)` and set wave: `MindSim.set_wave(5)`.
4. `godot_exec eval`: `var d0 = MindSim.state_digest(); print("MCP_DIGEST_BEFORE=", d0)` — digest from plan-23 canonical dump exposed as a GDExtension helper.
5. `godot_exec eval`: `MindIo.save_game("mcp_roundtrip")` → must print `true`.
6. `godot_exec eval`: `MindSim.reset_state()` then `MindIo.load_game("mcp_roundtrip")` → must print `true`.
7. `godot_exec eval`: `var d1 = MindSim.state_digest(); print("MCP_DIGEST_AFTER=", d1)`; assert `d0 == d1`.
8. `godot_runtime_state watch` node `/root/MindSim` property `wave` 500 ms → equals 5.
9. `godot_log errors` → no `IoError`/corruption lines.
10. `godot_screenshot` → save/load toast or HUD showing wave 5 (evidence attachment).
11. `godot_exec eval`: `print(FileAccess.file_exists("user://saves/mcp_roundtrip.msav"), FileAccess.file_exists("user://saves/mcp_roundtrip-backup.msav"))` → `true false` (backup only after second save).
12. `godot_game stop`.

Fallback if plan 00 uses a different root: read `MindIo.data_root()` and assert existence via that path. When plan 14 lands, repeat steps 5–6 through the real Save/Load UI with `godot_input` clicks.

### 7d. Performance & size budget

Benchmark: `cargo bench -p mind-core --bench io` (criterion) and `mind-headless bench io-save`. Measure P50/P95 on the dev machine; record in Changelog. Baseline world: `serpulo/groundZero` mid-game fixture (target ~200 buildings, ~50 units, 256×256).

| Metric | Budget |
|---|---|
| Save (serialize + deflate + write), groundZero | ≤ 50 ms P95 |
| Load (read + inflate + rebuild world) | ≤ 100 ms P95 |
| Meta-only read, one file | ≤ 5 ms P95 |
| Slot listing, 100 files | ≤ 150 ms P95 (parallel) |
| Settings flush | ≤ 2 ms P95 |
| Native save size, groundZero | ≤ 500 KB (upstream `152.msav` = 230 KB reference) |
| TypeIO plan/config encode | ≤ 1 µs each, no allocation in steady state (scratch reuse) |
| Peak extra memory during save/load | ≤ 1 region payload (≤ 128 MiB cap; expected ≤ 2 MiB) |

CI treats budgets as recording-only until plan 23 wires hard gates.

### 7e. Exit criteria checklist

- [ ] `cargo test -p mind-core` green (all §7a native tests).
- [ ] `mind-headless io roundtrip` checksum equality on the campaign fixture.
- [ ] Meta-only map/save listing verified (incl. corrupt-file skip).
- [ ] Backup write/restore + fallback-load verified.
- [ ] Entity revision check + class-ID append-only tests green; manifests committed.
- [ ] TypeIO caps + safe-read paths covered by negative tests.
- [ ] JsonIO `Rules` round-trip + forward/backward tolerance verified; class tags registered.
- [ ] Settings atomic flush + corruption fallback verified; save-slot keys match upstream names.
- [ ] MCP scenario §7c passes with digest equality and a screenshot attached.
- [ ] Budgets from §7d measured and recorded.
- [ ] `cargo tree -p mind-core` shows no `godot`/`tokio`; GPL headers on every ported file.
- [ ] OD2 default documented; `msav-import` feature compiles and its tests pass **or** the opt-in is explicitly declined (recorded in Changelog).

---

## 8. Risks & open decisions

| # | Item | Default taken | Flag |
|---|---|---|---|
| OD2 | Upstream `.msav` read/write compatibility | Native `MGRS` v1 only; importer behind default-off `msav-import`; one-way import if enabled; fixtures 77–152 become the oracle | **NEEDS USER DECISION** (per HIGH_LEVEL §10; default chosen, work continues) |
| R1 | `mind-derive` is a new workspace crate (beyond §2.1’s four crates) | Add it as a pure proc-macro crate (deps `syn`/`quote`), mirroring `annotations/` scope; no runtime dependency | Orchestrator acknowledge |
| R2 | Entity def → component aggregation differs from upstream’s single generated class (Bevy ECS splits state across components) | `EntityDefs!` list in `io/entity/registry.rs` defines the canonical save field order per def; plans 05/11 must register their components there and keep the order stable | Reconcile with `05_SIM_CORE_IMPLEMENTATION_PLAN.md` + `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| R3 | Data-root location: portable `./data` (upstream) vs Godot `user://` | `./data` when writable, else `user://`; `--data-dir` override for headless/tests | **NEEDS USER DECISION** (save discovery/export UX); plan 22 impacted |
| R4 | Save serialization threading | Capture region bytes on the main thread at `IoSet::Capture`; deflate/write on a worker `std::thread`; completion event | Reconcile with `05_SIM_CORE_IMPLEMENTATION_PLAN.md` schedule |
| R5 | Compression backend | `flate2` rust backend (miniz_oxide), zlib wrapper, fast level; switch to zstd only via a new native format version (allowed append-only change) | No user needed |
| R6 | `WorldContext` trait ownership (04 can’t depend on 06) | 04 defines `io::save::WorldContext`; 06 implements/aliases it; no duplicate trait | Reconcile with `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` |
| R7 | Upstream revision JSONs are not consumed by the native path | Native chain + native manifests; importer maps upstream revision numbers via explicit adapters | Follows from OD2 |
| R8 | `patches`/`markers` types not built yet | Plan-20 `DataAssets` and plan-12 `MapMarkers` traits declared here; regions write empty until those plans land; importer fixture tests for patch-bearing saves remain gated | Reconcile with `20_MODS_IMPLEMENTATION_PLAN.md` |
| R9 | Preview pixels need block/floor minimap colors (plans 02/06/16) | `PreviewImage` + `color_for` shipped now; real minimap colors land with 16/19; missing colors deterministic | No user needed |
| R10 | Java modified-UTF-8 strings in imported saves | Importer implements a real modified-UTF decoder; native format uses plain UTF-8 | Follows from OD2 |
| R11 | `@SyncField` storage location (`_TARGET_`/`_LAST_` companions vs `SyncInterpState` component) | Plan 04 exports metadata only; plans 05/16/21 choose storage; native save serializes target as the sync field value | Reconcile with `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |

---

## 9. References

### Mindustry sources read

- `core/src/mindustry/io/AGENTS.md`
- `core/src/mindustry/io/SaveIO.java`, `SaveVersion.java`, `SaveFileReader.java`, `SaveMeta.java`, `SaveOptions.java`, `SaveReadState.java`, `SavePreviewLoader.java`
- `core/src/mindustry/io/TypeIO.java` (tags, `writeObject`/`readObject`/`readObjectSafe`, plans/configs, status/items/strings/rules/objectives)
- `core/src/mindustry/io/JsonIO.java`
- `core/src/mindustry/io/MapIO.java`
- `core/src/mindustry/io/versions/` — `Save1..Save13.java`, `LegacySaveVersion.java`, `LegacySaveVersion2.java`, `LegacyRegionSaveVersion.java`, `ShortChunkSaveVersion.java`, `LegacyIO.java`
- `core/src/mindustry/game/Saves.java`
- `core/src/mindustry/Vars.java` (dirs, extensions, settings keys)
- `core/src/mindustry/world/AGENTS.md`, `core/src/mindustry/game/AGENTS.md`, `core/src/mindustry/net/AGENTS.md`
- `annotations/AGENTS.md`; `annotations/src/main/java/mindustry/annotations/Annotations.java` (`@SyncField`/`@NoSerialize`/`@NoSync`/`@SyncLocal`); `annotations/src/main/resources/revisions/**` (43 dirs; `BuildingComp/0.json`, `1.json`); `annotations/src/main/resources/classids.properties` (52 entries)
- `tests/AGENTS.md`; `tests/src/test/java/ApplicationTests.java` (`writeStringTest`, `writeRules`, `writeRules2`, `save`, `saveLoad`, `load77Save`…`load152Save`); `tests/src/test/resources/{77,85,108,114,152,152_be}.msav`

### Plan set

- `HIGH_LEVEL_PLAN.md` (§0 D1–D9, §2.1 layout, §2.4 determinism, §4 template, §6.2 revision rule, §9 parity ledger, §10 OD2)
- `PRELIMINARY_PLAN.md` (history)
- `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (spine, FS root, event bus, MCP rig)
- `02_CONTENT_IMPLEMENTATION_PLAN.md` (content IDs/names/fallbacks)
- `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (ECS runtime, `IoSet`)
- `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (`WorldContext`, tiles, map load)
- `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (`Saves`, `Rules`, `SectorInfo`, markers)
- `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (map registry, previews, image maps)
- `20_MODS_IMPLEMENTATION_PLAN.md` (data patches, custom chunks)
- `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (TypeIO packets, sync, `NetworkIO`)
- `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (canonical dumps, checksums, CI gates)

## Changelog

- 2026-10-01 — initial draft; OD2 default recorded (native `MGRS` v1, importer off); R1/R3 flagged.
- 2026-10-02 — **M0 complete (`lane/04-io`).** Native `MGRS` container + chunk primitives landed in `mind-core::io`: `error.rs` (upstream message text kept), `wire.rs` (big-endian Arc-style primitives; **layout addition** — the §3.1 sketch had no home for them), `fs.rs` (`FileSystem`/`NativeFs`/`MockFs`/`Paths`, atomic writes, read-only fault injection), `save/chunk.rs` (named regions, 2-deep chunk nesting, 128 MiB region cap, 512 MiB inflate cap, full `SaveFileReader.fallback` table), `save/version.rs` (append-only chain, exact unknown-version message), `save/versions/v1.rs` (all 7 regions written; empty world), `save/meta.rs`, `save/options.rs`, `save/state.rs` (`SaveReadState` + `WorldContext` trait per C10), `save/mod.rs` (`SaveIo`: tmp+sync+rename, backup rotation + restore-on-error, backup fallback on load/meta, legacy `MSAV` sniff → unknown-version error via `io/legacy.rs` stub). `msav-import` feature declared (default-off, declined stub). Workspace `Cargo.toml`: added `flate2` (rust_backend, R5) — additive, flagged for merge. `version.rs`: added `BUILD = 0` const (upstream dev-build default). Evidence: `cargo test -p mind-core` 100 passed (36 io), `cargo clippy -p mind-core -p mind-headless --all-targets -D warnings` clean, `cargo fmt --check` clean; `mind-headless io dump-meta /tmp/mind-io-fixtures/m0_empty.msav --json` reads back format 1 / wave 2 / 8×8 / all 21 §6.2 meta keys; `spine_place_break` golden still `375c68a53e861948`. Plan-text fixes: §3.1 gained `wire.rs`; M0 file list gained `save/state.rs` (trait needed by `SaveVersion` signature). Events (`SaveWriteEvent`/`SaveLoadEvent`) deferred to M4 when load applies to a world.
- 2026-10-02 — **M1 complete (`lane/04-io`).** `io/settings.rs`: `SettingsStore` (Arc `Settings` port) — typed `SettingValue::{Str,Int,Long,Float,Bool,Bytes,Json}`, string-coercion getters, `get_json`/`put_json` (serde), `defaults`, `toggle`, atomic `force_save` (tmp+sync+rename) and 1 s debounced `flush_if_dirty`; native `MGST` format per §6.5 with corrupt-file → defaults fallback; upstream key names (`save-<n>-name`, `save-<n>-autosave`, `saveinterval`, `last-sector-save`, `uiscale`, `color-<n>`) as helpers/consts. `mind-headless io settings` self-check scenario (set → flush → reload equality → corrupt → defaults → recover). Evidence: `cargo test -p mind-core` 105 passed (5 settings), clippy/fmt clean; `mind-headless --data-dir /tmp/m1-io-data io settings --json` → `{"persisted":true,"corrupt_fallback":true,"recovered":true,"pass":true}` (6 keys checked).
- 2026-10-02 — **M2 complete (`lane/04-io`).** `io/typeio/`: frozen 24-tag table (§6.3), `TypeValue`/`EntityRef`, `write_object`/`read_object_with` (+`read_object_safe` for untrusted input) with all upstream caps (`MAX_ARRAY=1000`, `MAX_BYTE_ARRAY=40_000`, `MAX_SAFE_STRING=1200`, non-safe object-array cap 200 quirk kept, nested-array rejection) and `UnknownTypeTag` errors; `codecs.rs` direct codecs (strings/bytes/ints/shorts, item/liquid/block/weather/content refs, item+liquid stacks, status dynamic-bitmask, rules JSON (100 KB cap), plans/plans-queue-net (20+500B cap)/client plans (config whitelist), team, color, command/stance, vec2s, effect/unit/bullet ids); `mapper.rs` (`ContentMapper`, `RegistryContentMapper`, `TemporaryMapperGuard` RAII for `finally setTemporaryMapper(null)`). **Pulled forward from M6**: minimal `io/json/{mod,rules}.rs` `Rules` (attackMode/buildSpeedMultiplier/tags; serde defaults, camelCase) because the ported `writeRules` test needs it — M6 expands the field set (backward compatible by construction). **Seams/deviations recorded**: `write_client_plans`/`read_client_plans` take a `rotate: &dyn Fn(BlockId) -> bool` predicate because `BlockDef` has no `rotate` field yet (plan 07 owns it); `StatusEntry`/`BuildPlan` are wire-shape structs here (plans 10/11 and 07 own the runtime types — reconcile); `write_objectives`/`write_objective_marker` deferred to M6 (needs `MapObjectives`), `write_ui_builder`/`write_menu_result` to plan 14, `write_payload`/`write_mounts`/`write_abilities`/`write_controller` to 11/21. `Rgba::to_rgba8888` added (color.rs, additive). Workspace `Cargo.toml`: indexmap `serde` feature (additive). Evidence: `cargo test -p mind-core` 119 passed (13 typeio incl. ported `write_string_roundtrip`, `write_rules_binary`, caps/negative tests), clippy/fmt clean.
