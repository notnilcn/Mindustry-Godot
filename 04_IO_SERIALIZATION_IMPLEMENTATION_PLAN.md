# 04 — IO & Serialization Implementation Plan

> Inherits `HIGH_LEVEL_PLAN.md` §0 (locked decisions), §2 (architecture), §4 (template), §6–§9 (conventions).
> This plan is the persistence layer for `mind-core`: saves/maps, `TypeIO`, entity revisions, JSON config, settings, save slots and map headers.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | ✅ **Merged to `main` 2026-10-02 — M0–M6 + M8 budgets complete. M7 (legacy `.msav` importer) explicitly declined per OD2/NUD-02; the M8 `MindIo` autoload + `save_current`/`load_current` dev actions and the §7c in-engine MCP run are deferred to plan 05 (live world/`IoSet`) + the single-editor mutex.** |
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

#### 2.3.1 OD2 record (save/map byte compatibility)

**Decision status: RESOLVED 2026-10-01** — 23 §8.1.1 NUD-02=A: native `MGRS` first, opt-in one-way map importer. The default below is what this plan implements; **the importer build itself is explicitly declined for this execution** (plan 04 M7, 2026-10-02 Changelog) — `msav-import` stays a declared, compiling, no-op feature so a later opt-in can land `legacy::java` without touching the engine.

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

- **Godot**: no new scenes. `mind-gdext` registers autoload class `MindIo` with methods `save_game(slot: String) -> bool`, `load_game(slot: String) -> bool`, `list_saves() -> Array[Dictionary]`, `list_maps() -> Array[Dictionary]`, `settings_get(key, default) -> Variant`, `settings_set(key, value)`, `preview_for_slot(slot) -> Image` (plan 19 converts to texture). Plan 00 owns the scene; plan 14 owns dialogs. **M8 deferral (2026-10-02):** the autoload itself plus `save_current`/`load_current` dev actions is **not implemented yet** — `save_game`/`load_game` must capture/apply a live world at plan 05's `IoSet::Capture/Apply` boundary, and plan 05 is unstarted. The `mind-core` file/`SaveSlot` API it wraps is complete and bench-verified; the thin gdext layer lands with plan 05 (and the §7c MCP run follows it).
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
- **M3 — Entity codec + revisions + class IDs.** ✅ **COMPLETE 2026-10-02**
  `mind-derive` crate, `io/entity/**`, revision manifests for all existing entity defs (43 revision dirs upstream), `entity_class_ids.toml`, `mind-headless io check-revisions [--update]`.
  *Verify*: revision check test green; synthetic component add/remove/rename yields correct read-skip and fails strict name check until bumped; class-ID append-only test.
- **M4 — Full native v1 map/entities/markers/custom.** ✅ **COMPLETE 2026-10-02**
  `map` region RLE + tile data + nested tile-entity chunks against `WorldContext` stubs; `content` header + temp mapper; team plans; entity ID mapping; `after_read_all`; `markers` via plan-12 trait stub; `custom` registry; `patches` via plan-20 trait stub (empty OK).
  *Verify*: `mind-headless io roundtrip --map serpulo/groundZero --ticks 600` checksum equality (world placeholder from plan 06; until then use a synthetic 64×64 map fixture).
- **M5 — Slots + MapIO + previews hooks.** ✅ **COMPLETE 2026-10-02**
  `save/slot.rs`, `map/**`; parallel listing; meta-only reads; import/export; `write_image/read_image` using a plan-02 `ColorMapper` stub; `PreviewImage` generation delegates to plan 19.
  *Verify*: `mind-headless io map-list` prints correct meta for maps+saves; slot name/autosave KV round-trip; corrupted primary falls back to backup.
- **M6 — JsonIO.** ✅ **COMPLETE 2026-10-02**
  `json/**`; content serializers + fallbacks; `Rules`/`GameStats`/`MapLocales`/`SectorInfo` derives; class tags; objectives (incl. parents fixup + legacy lowercase rule).
  *Verify*: ported `writeRules2`; golden JSON fixture from upstream `JsonIO` for a rules blob; forward/backward field tolerance.
- **M7 — Legacy importer (gated by OD2).** ⛔ **DECLINED 2026-10-02 (OD2 default; recorded in Changelog §7e)**
  `io/legacy/java/**` under `msav-import`; fixtures + expected JSON. **Not built**: the native `MGRS` path ships and the feature stays a compiling no-op. Re-open only on an explicit user opt-in — the reader structure/fixture plan is fixed in §2.3.1 so the shim lands without touching the engine.
  *Verify (decline path)*: `cargo check/test -p mind-core --features msav-import` compiles and is green (stub unchanged).
- **M8 — MCP integration + budgets + docs.** 🔶 **PARTIAL 2026-10-02** (done: bench command + budgets + docs; deferred: `MindIo` gdext hooks and §7c in-engine run, both blocked by plan 05's live world/`IoSet`)
  `mind-headless io bench-save` (P50/P95 over `NativeFs`) and `cargo bench -p mind-core --bench io` (criterion) landed and recorded in §7d.1; final budget docs in §7d.1. **Deferred (precisely marked)**: `MindIo` GDExtension autoload (`save_game`/`load_game`/`settings_get`/`settings_set`/`list_saves`/`list_maps`) and dev actions `save_current`/`load_current` need plan 05's live world + `IoSet::Capture/Apply`; §7c's in-engine MCP scenario is queued behind the repo's single-editor mutex and is handed to the orchestrator (run after plan 05 lands).
  *Verify*: `io bench-save --json` pass + recorded timings; criterion numbers recorded; in-engine §7c deferred (documented in Changelog).

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

> **M8 deferral (2026-10-02):** this scenario is **not run yet**. It needs (a) plan 05's live world/`IoSet::Capture`/`Apply` and the `MindIo` autoload hooks, which do not exist while plan 05 is unstarted, and (b) the repo's single running editor (shared-resource mutex), so the orchestrator queues it after plan 05 lands. Steps 1–4/8–12 (health, play, probe, watch, log, screenshot) are reusable as written; the digest assertions stay the acceptance gate.

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

#### 7d.1 M8 measured results (2026-10-02, `lane/04-io`)

The §7d baseline world `serpulo/groundZero` needs plan 06 (world/generators), so M8 measured the synthetic `FixtureWorld` (64×64, 600 ticks, 5 597-byte save) and records the groundZero rows as **deferred to plan 06** when the real map can feed the same commands.

`mind-headless io bench-save --iters 50 --json` (NativeFs, dev/debug build; each save is a fresh file so backup rotation is excluded):

| Phase | P50 | P95 | min | max |
|---|---|---|---|---|
| Save (serialize + deflate + write) | 3.105 ms | 4.356 ms | 2.932 ms | 10.072 ms |
| Load (read + inflate + apply) | 3.758 ms | 6.548 ms | 3.407 ms | 11.514 ms |
| Meta-only read | 0.865 ms | 1.267 ms | 0.722 ms | 1.452 ms |

`cargo bench -p mind-core --bench io` (criterion, release `bench` profile, `MockFs` so the numbers isolate the codec):

| Benchmark | Estimate [lower, upper] |
|---|---|
| `io/save_synthetic_64x64_600t` | 248.33 µs [244.37, 252.68] |
| `io/load_synthetic_64x64_600t` | 231.98 µs [227.21, 236.81] |
| `io/meta_read` | 63.31 µs [62.03, 64.69] |
| `io/slot_listing_100` | 3.0356 ms [3.0028, 3.0698] |
| `io/settings_flush` | 3.0023 µs [2.9420, 3.0632] |
| `io/typeio_plan_encode` | 20.203 ns [19.999, 20.417] |

Verdicts against §7d (synthetic unless noted): save ≤ 50 ms P95 ✅ (4.36 ms debug / 0.25 ms release); load ≤ 100 ms P95 ✅ (6.55 ms debug / 0.23 ms release); meta ≤ 5 ms P95 ✅ (1.27 ms debug / 0.06 ms release); slot listing 100 ≤ 150 ms P95 ✅ (3.04 ms); settings flush ≤ 2 ms P95 ✅ (3.0 µs); TypeIO plan/config ≤ 1 µs ✅ (20 ns, scratch-buffer reuse); peak extra memory ≤ 1 region payload — structurally enforced (region scratch cap 128 MiB, §3.2) with a ≤ 5.6 KB payload here; **native save size groundZero ≤ 500 KB ⏳ deferred to plan 06** (synthetic 5 597 B; upstream `152.msav` = 230 KB reference).

### 7e. Exit criteria checklist

- [ ] `cargo test -p mind-core` green (all §7a native tests).
- [ ] `mind-headless io roundtrip` checksum equality on the campaign fixture.
- [ ] Meta-only map/save listing verified (incl. corrupt-file skip).
- [ ] Backup write/restore + fallback-load verified.
- [ ] Entity revision check + class-ID append-only tests green; manifests committed.
- [ ] TypeIO caps + safe-read paths covered by negative tests.
- [x] JsonIO `Rules` round-trip + forward/backward tolerance verified; class tags registered. (M6: `io::json::rules::tests::rules_json_roundtrip` ported from `writeRules2`, `rules_forward_backward_tolerance`, `io::json::objectives::tests::class_tag_registry_lists_both_forms`.)
- [ ] Settings atomic flush + corruption fallback verified; save-slot keys match upstream names.
- [ ] MCP scenario §7c passes with digest equality and a screenshot attached. → **DEFERRED (M8, 2026-10-02)**: blocked on plan 05 (`MindIo`/live world) and the single-editor mutex; handed to the orchestrator.
- [x] Budgets from §7d measured and recorded. → M8 §7d.1: `io bench-save` P50/P95 (NativeFs) + criterion numbers recorded; the `groundZero`-specific rows (real map, save size baseline) are deferred to plan 06 with the synthetic equivalents recorded.
- [x] `cargo tree -p mind-core` shows no `godot`/`tokio`; GPL headers on every ported file. (M8: `cargo tree -p mind-core` contains no `godot`/`tokio` lines; every new M6 file carries the GPL ported-source header.)
- [x] OD2 default documented; `msav-import` feature compiles and its tests pass **or** the opt-in is explicitly declined (recorded in Changelog). → **declined per OD2 default (M7, 2026-10-02)**: native `MGRS` only; `msav-import` compiles as a no-op stub and `cargo test -p mind-core --features msav-import` is green (171 passed / 1 ignored).

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
- 2026-10-02 — **M3 complete (`lane/04-io`).** Entity codec + revision/class-ID discipline. New workspace crate **`mind-derive`** (R1 acknowledged: pure `syn`/`quote` proc-macro, mirrors `annotations/entity/EntityIO.java`; no runtime dep, no Godot). `#[derive(EntityIo)]` emits the `EntityCodec` impl: `write` = newest revision `u16` + save fields in declaration order; `read` = `match revision` chain from `#[entity(since = N)]`/`removed_in = N` tombstones; `UnknownRevision` error names the def. Struct/field attrs map 1:1 to upstream (`no_serialize`, `no_sync`, `sync_local`, `transient`, `sync_field(interp, clamped)`); `@SyncField` metadata is a pure data export (`sync_fields()`, R11 — `interpolate` is a default no-op until 16/21). `io/entity/`: `field.rs` (`IoField` wire impls incl. TilePos/TeamId/content ids/Rgba), `registry.rs` (`define_entity_defs!` → `entity_defs()`/`def_by_name`/`def_by_class_id`; R2 reconciliation list — starts with `ecs::BuildingComp`, plans 05/11 append), `revisions.rs` (strict name+type compare per deviation 3 — a rename fails until bump+aliases; dense-chain validation; `--update` appends `<N+1>.json`, old files byte-identical), `idfile.rs` (append-only `entity_class_ids.toml` tool: max+1 in sorted-name order, duplicate name/id rejection, TOML↔`class_ids.rs` drift gate), `idmap.rs` (`EntityIdMap`, `DuplicateIdTracker`), `class_ids.rs` (generated, committed; full 50-entry upstream ID reservation with bare def names — **plan-text fix**: paths are `client/rust/mind-core/{revisions,entity_class_ids.toml}`, not `crates/mind-core/`). `mind-headless io check-revisions [--update]` + `io check-class-ids [--update]` (drift exits 1). `ecs.rs` BuildingComp gained the derive (additive; class id 6 = upstream). Evidence: `cargo test -p mind-core` 133 passed (14 entity: committed-manifest match, rename/add-field/--update cycle, duplicate-id reassign, unknown class skip, derive roundtrip + unknown revision), clippy/fmt clean across mind-core/mind-derive/mind-headless; `io check-revisions`/`io check-class-ids` both `pass:true`; drift simulation (`position` rename in a temp copy) exits 1 with the deviation-3 message; `--update` appends `1.json` leaving `0.json` byte-identical (md5 verified); `spine_place_break` golden `375c68a53e861948` unchanged. Manifests committed: `revisions/BuildingComp/0.json` (4 fields), `entity_class_ids.toml` (50 entries).
- 2026-10-02 — **M4 complete (`lane/04-io`).** Full native v1 regions landed. IO trait surface in `save/state.rs`: `MapSource`/`WorldContext` (tile seam, C10), `EntitySource`/`EntitySink` (entity chunks; `supports_class(class_id, custom_name)`), `CustomChunk` (plan-20 registry: `should_write`/`write_net` filters, unknown names skipped by length), `MarkersIo`/`MarkersSink` (plan-12 payload seam), `PatchSetIo` (plan-20 seam), `TeamPlan` (`Teams.BlockPlan` wire shape). `SaveV1.write/read`: real `write_map`/`read_map` (floor/overlay RLE pass with air→stone correction, block pass with packed entity/data byte, 7-byte tile data, center flag + nested tile-entity chunk skipped by length when `has_building()==false`, block RLE runs with over-run guards), `write_entities`/`read_entities` (custom ID map, dedup'd team plans via `Point2.pack` set, entity chunks skipped for unknown classes, `after_read_all` pass), markers/custom wired to registries. Temp-mapper cleanup in `SaveIo::load_bytes` (upstream `finally` parity). `EntityCodec::TILE_VERSION` added (`entity.version()` byte; derive `version = N`). Events added to `event.rs` (`SaveWriteEvent`, `SaveLoadEvent{is_map}`, `RulesLoadEvent{from_save}`; firing points land with plan 05's `IoSet`/MindIo). **Upstream-fidelity corrections from testing**: buildings are excluded from the entities region (`BuildingComp @EntityDef excludeGroups={"all"}, serialize=false`) — they travel via the map region only; tile team/rotation delegate to the building (mirrored back in `read_building`); `BuildingComp.version()` is 0. `save/fixture.rs`: `FixtureWorld` synthetic 64×64 world implementing all traits (+ RefCell `FixtureContext`/`FixtureSink` adapters for the read side), canonical xxh3 checksum; tick mutates only `save_data` tiles (non-save-data tile bytes are not persisted, upstream semantics). `mind-headless io roundtrip` (`--map synthetic` only; real maps error with a plan-06 gate message — **plan-text note**: `serpulo/groundZero` needs plan 06). Evidence: `cargo test -p mind-core` 134 passed (incl. `fixture_save_load_roundtrip`: 600 ticks → save → load → checksum equality + temp-mapper escape check), clippy/fmt clean; `io roundtrip --ticks 600 --out /tmp/m4-rt.msav` → C0=C1=`5cc6fedc4ff0beb9` (5 597-byte save, 9 buildings); `io dump-meta` reads it back (wave 13, 64×64); `spine_place_break` golden `375c68a53e861948` unchanged.
- 2026-10-02 — **M5 complete (`lane/04-io`).** `save/slot.rs`: `SaveSlot` (file + meta + preview path + name/autosave via upstream settings keys `save-<n>-name`/`save-<n>-autosave`), `save`/`load`/`delete`/`import_file`/`export_file` (embed-assets path), `queue_preview_rerender` (delete-and-requeue resilience at the path layer, deviation 7 — plan 19 renders pixels), `is_sector` via raw rules JSON until plan 12's `Rules` (noted), `list_files_meta`/`list_save_slots` (parallel meta reads via `std::thread::scope` + `available_parallelism`, corrupt skipped with warning, sorted output). `io/map/`: `MapIo` (`create_map` meta-only, `write_map` = save with `extra_tags`/`embed_assets`, `load_map`, `is_image` PNG sniff, `generate_preview` through a `PreviewContext` `WorldContext` — no events/world alloc), `MapHeader` (+ `is_map_header`), `preview.rs` (`PreviewImage`, `color_for` ported over a `BlockPalette` snapshot — the palette is owned so the registry can be mutably borrowed for the temp mapper; `write_image`/`read_image` with `ColorMapper`/`ImageTileSink` stubs, deterministic R9 fallbacks). `mind-headless io map-list --dir` (parallel listing, corrupt-skip count). **Plan-text fix**: §7b's `--dir client/rust/mind-core/tests/fixtures/maps` is runtime-generated (no binary fixtures committed while native v1 may still gain versions). Evidence: `cargo test -p mind-core` 146 passed (9 map/slot: `slot_meta_listing_parallel` (100 slots sorted, corrupt skipped), `backup_fallback`, `preview_paths` + KV round-trip, `create_map_reads_meta_only`, preview pixels, PNG sniff), clippy/fmt clean; `io map-list --dir /tmp/m5-map-list` → 3 listed (2 maps + 1 save) / 1 skipped (corrupt).
- 2026-10-02 — **M6 complete (`lane/04-io`).** `io/json/**` landed. (a) `json/mod.rs`: `JsonIo::{write,read,read_into,copy,pretty}` — `io.anuke.` prefix strip on read, recursive field-overlay for `read_into` (`json.readFields`), compact write matching Arc's default writer; `StringMap` re-export. (b) `json/content_serde.rs`: content **name** serializers + upstream fallback table — item→copper, liquid→water, block→exact then `SaveFileReader.fallback` (`water`→`shallow-water`) then air, planet→serpulo (null stays null), unit→dagger slot (unit registry is plan 02 M5; de-registered builds warn + id 0), weather/status/sector-preset pointered, unlockable via `content.byName` kind filter, `Attribute` name lookup (4-variant enum in this build; `Attributes` round-trips unknown names), `Team.get` byte mask, `SectorKey` (`"<planet>-<id>"`), `ColorHex` (`Arc Color.toString` 8-digit RRGGBBAA), `JsonItemStack`, `MusicContainer`. (c) `json/rules.rs`: **full upstream `Rules` field set** (103 fields incl. `TeamRules`/`TeamRule`, `Attributes` zero-skipping custom serde, `SpawnGroup` default-omitting custom write incl. upstream `scaling`/`amount` keys + never=i32::MAX, `WeatherEntry`, `PlanetParams`, music lists, per-team maps and content-name sets) + `GameStats`, `MapLocales` (`get_property`/`get_formatted` fallbacks), `SectorInfo` (non-transient fields + `ExportStat`); the runtime types stay plan-12-owned — these are the persisted shapes and plan 12 aliases them. (d) `json/objectives.rs`: 13 objective variants with shared `ObjectiveCommon`, plain class tags written (`"class":"DestroyUnits"`) with camelized aliases accepted, `parents` index fix-up with remap across skipped elements (deviation 6), legacy lowercase-class rule → empty executor, `editorPos` packed default, `ClassTagRegistry` (`objectives()` builds all 26 tags); all 8 `ObjectiveMarker` types (index-bool fields accept bool or int) + `TextureHolder`. (e) `typeio/codecs.rs`: `write/read_objectives` (60 000 cap, exact upstream message `Objectives bytes too long: N`) and `write/read_objective_marker` (40 000 cap, `Objective marker too long`) — the codecs.rs M2 deferral is closed. (f) `save/state.rs` + `save/versions/v1.rs`: `read_meta` now parses `stats`/`locales` eagerly and `read_rules` parses the stashed JSON after all regions (upstream v13+ order); new `SaveReadState::{rules,stats,locales}`; `SaveSlot::is_sector` uses typed `Rules.sector` with raw-JSON fallback. Golden fixture `client/rust/mind-core/tests/fixtures/json/rules_golden.json` (writeRules2 blob: `attackMode=true`, `buildSpeedMultiplier=99.1`, `tags={blah:bleh}`) is **source-derived — no JVM is available in this environment**, and is compared byte-for-byte by `rules_golden_blob_matches_fixture`; a `#[ignore]`d `regenerate_rules_golden_fixture` maintainer test rewrites it on intentional shape changes. Evidence: `cargo test -p mind-core` **171 passed, 0 failed, 1 ignored** (fixture generator); `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` clean; `mind-headless io roundtrip --json` → `"checksum_before":"5cc6fedc4ff0beb9","checksum_after":"5cc6fedc4ff0beb9","pass":true`; `io settings --json` → `"pass":true`; `io map-list --dir /tmp/m6-map-list --json` → `listed:2, skipped:1`; `io check-revisions --json` → `"pass":true`; `io check-class-ids --json` → `entries:50, pass:true`. Reconciliation notes for plan 12: shapes are name-based and carry `common.parents` as indices (not object refs); `Rules.copy` remains `Clone` (deviation 8); `GameStats`/`SectorInfo` content keys are content names, list order = insertion order (`IndexMap`/`IndexSet`, determinism §3.12.6).
- 2026-10-02 — **M7 DECLINED (`lane/04-io`), per OD2 default + §7e opt-in-decline path.** The legacy upstream `MSAV` importer (`io/legacy/java/**`, readers for versions 1–13, modified-UTF-8, revision adapters, `77/85/108/114/152/152_be` fixture oracles) is **not built**: the resolved OD2/ NUD-02=A decision keeps native `MGRS` as the shipping format and the one-way importer as an optional, later opt-in. `io/legacy.rs` stays the sniff boundary: a legacy `MSAV` header decompresses the version and fails with the exact upstream message (`Unknown save version: N. Are you trying to load a save from a newer version?`), covered by `io::save::tests::legacy_msav_fails_with_unknown_version_message`; the `msav-import` Cargo feature remains declared, default-off and compiling as a no-op. No fixtures were copied and no `io/legacy/java` tree exists (fixture/reader plan stays recorded in §2.3.1 for a future opt-in). Evidence: `cargo check -p mind-core --features msav-import` → `Finished dev profile`; `cargo test -p mind-core --features msav-import` → 171 passed, 0 failed, 1 ignored (same as the default feature set); `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` clean. Exit criterion "OD2 default documented; `msav-import` feature compiles and its tests pass **or** the opt-in is explicitly declined" checked off as **declined per OD2 default**.
- 2026-10-02 — **M8 PARTIAL (`lane/04-io`)** — benches + budgets + docs landed; the plan-05-dependent MCP/gdext parts are deliberately deferred. (a) `mind-headless io bench-save --map synthetic --iters N --json`: new `IoCommand::BenchSave` + `IoBenchSaveReport`/`IoBenchStat`; measures save (serialize+deflate+atomic write, fresh file per run), load (read+inflate+apply into a fresh `FixtureWorld`), meta-only read with P50/P95/min/max and a load-checksum verification pass. `--iters 50` result: bytes 5 597, save P50 3.105/P95 4.356 ms, load P50 3.758/P95 6.548 ms, meta P50 0.865/P95 1.267 ms, `"pass":true`. (b) `client/rust/mind-core/benches/io.rs` + `[[bench]] name="io" harness=false` and workspace `criterion 0.5` (default-features off + `cargo_bench_support`; **additive workspace dep**, CI compile cost only): `cargo bench -p mind-core --bench io` (release bench profile, MockFs) → save 248.33 µs [244.37, 252.68], load 231.98 µs [227.21, 236.81], meta 63.31 µs [62.03, 64.69], slot_listing_100 3.0356 ms [3.0028, 3.0698], settings_flush 3.0023 µs [2.9420, 3.0632], typeio_plan_encode 20.203 ns [19.999, 20.417]. (c) §7d.1 added with the measured table: every synthetic/release budget row passes (save/load/meta/listing/flush/plan-encode); the `serpulo/groundZero` rows (real-map timings + ≤ 500 KB save size) are **deferred to plan 06** with the synthetic 5 597 B equivalent recorded. (d) **Deferred with precise markers** (also in HLP §13): the `MindIo` GDExtension autoload and `save_current`/`load_current` dev actions, because `save_game`/`load_game` need plan 05's live world + `IoSet::Capture/Apply` (plan 05 unstarted); §7c's in-engine MCP scenario is queued behind the single-editor mutex and handed to the orchestrator after plan 05. §3.11 and §7c carry the same note; M8 milestone and the two exit criteria are marked partial/deferred accordingly. Evidence: `cargo test -p mind-core` 171 passed / 1 ignored; `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean (bench target included); `cargo tree -p mind-core` has no `godot`/`tokio` (exit criterion checked); `io roundtrip --json` still C0=C1=`5cc6fedc4ff0beb9`.

- 2026-10-04 — **M4/M8 residual slice: real `WorldContext` round-trip + `MapSource` team seam (`lane/f28-world`, base `main` @ `31fa7ab`).** (a) **04 §5 M4 swap.** `mind-headless io roundtrip --map serpulo` now generates a real Serpulo sector (64×64, seed 42) into a `WorldGrid`, writes it through plan-06 `world::EcsMapSource` (`WriteContext.map`) and reads it back through plan-06 `world::Context` (`SaveReadState.context`, `entities: None`) — no `FixtureWorld`. A tile checksum over floor/overlay/block (row-major FNV-1a) must be equal before/after; the report reuses `IoRoundtripReport` (`ticks: 0`). Evidence: `target/debug/mind-headless io roundtrip --map serpulo --json` → `{"bytes":8655,"buildings":0,"checksum_before":"132df0de53fe48e6","checksum_after":"132df0de53fe48e6","pass":true}`. The synthetic fixture path is unchanged: `--map synthetic` → `27346688f2cabe76` ==, 9 buildings. (b) **`MapScan.teams` ECS scan (06 M4).** `MapSource` gains `fn core_team(index: usize) -> Option<u8>` with a `None` default (deviation 1: Rust `Tile` has no team); `maps::scan_map_source` now unions those into `MapScan.teams`. Tile-only sources (`editor::maps_glue::EditorMapSource`, `FixtureWorld`) keep the empty fallback, so `maps roundtrip` golden `57bfde109806f8ea` is unchanged. The real ECS projection lives in plan-06 `world::EcsMapSource` (`world/map_source.rs`), which also resolves exact multiblock centers from `Building.tile`; unit test `world::map_source::tests::ecs_source_collects_core_teams`. **Plan-text note:** the team scan is now supplied by the world source, not the `Maps` registry; `MapScan.teams` docs updated accordingly. No golden move. Evidence: `cargo test -p mind-core` 1666 lib passed / 3 ignored; `cargo test -p mind-headless` 130 lib + goldens passed; fmt + workspace clippy `-D warnings` clean.

- 2026-10-04 — **Plan-19 `ImageTileSink::set_block` + `MapIO.readImage` block path slice (`lane/f30-editor`, base `main` @ `0c310a3`).** `io/map/preview.rs`: [`ImageTileSink`] gains `set_block(x, y, block, team, rot)`; [`BlockPalette`] gains `block_size`/`is_multiblock`; `read_image` now ports `MapIO.readImage` in full instead of the floors/overlays-only stub — skip `hasBuilding`, overlay→`set_overlay`, floor→`set_floor`, else `set_block(block, Team.derelict, 0)` with multiblock footprint expansion. The editor's `GridImageSink` (`editor/maps_glue.rs`) implements the new method (writes `tile.block`, clears any build), so `MapEditor.begin_edit_image` can place walls/blocks from PNG imports. New test `io::map::preview::tests::read_image_places_blocks`; the existing PNG/image golden (`maps image-roundtrip` `4d1d3a32c6b05ea5`) is unchanged (its fixture draws no walls). No save-format or golden changes. Evidence: `cargo test -p mind-core` 1722 lib passed / 4 ignored; `cargo test -p mind-headless` 130 lib + all goldens passed; `cargo fmt`/workspace `clippy -D warnings` clean.
