# 02 — Content Framework & Vanilla Content Registry Implementation Plan

> Inherits `HIGH_LEVEL_PLAN.md` §0 (locked decisions), §2 (architecture), §4 (template), §6–§9 (conventions). Where this file conflicts with `HIGH_LEVEL_PLAN.md`, the high-level plan wins.

---

## 1. Header block

| Field | Value |
|---|---|
| **Status** | In progress — M1–M2 complete (2026-10-01; lane 02) |
| **Phase** | P1 — Platform & content (HIGH_LEVEL_PLAN §5) |
| **Depends on** | `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (workspace, `mind-core`/`mind-headless` crates, headless harness, spine state inspector). |
| **Blocks** | `03_ASSETS_IMPLEMENTATION_PLAN.md`, `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md`, `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md`, `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md`, `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md`, `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `20_MODS_IMPLEMENTATION_PLAN.md`. |
| **Sources (read in full)** | `Mindustry/core/src/mindustry/content/AGENTS.md`, `ctype/AGENTS.md`, `type/AGENTS.md`, `world/AGENTS.md`, `entities/AGENTS.md` (for `@EntityDef`/`EntityMapping`), `annotations/AGENTS.md`; `core/src/mindustry/core/AGENTS.md`, `mod/AGENTS.md`, `io/AGENTS.md`, `ai/AGENTS.md` (cross-checked); registries: `content/{Blocks,UnitTypes,Items,Liquids,StatusEffects,Bullets,Fx,Weathers,Planets,SectorPresets,Loadouts,TeamEntries,TechTree,SerpuloTechTree,ErekirTechTree}.java`, `ai/{UnitCommand,UnitStance,ItemUnitStance}.java`, `ctype/{Content,ContentType,MappableContent,UnlockableContent}.java`, `core/ContentLoader.java`, `type/*.java` + `type/{unit,weapons,weather}/*.java`, `world/Block.java` (metadata half), `entities/bullet/BulletType.java` (metadata half), `mod/{ContentParser,DataPatcher}.java` (interface shape), `tests/src/test/java/{ApplicationTests,DataAssetTests,PatcherTests}.java`. |
| **Extends spine** | Adds a `Content` tab to the plan-00 state inspector (per-type counts + scrollable name list), a `MindCore.content_counts()` / `MindCore.content_list(type)` eval API, `mind-headless` subcommands `content dump|audit|bench`, and the parity ledger/CI gate described in §7. |

**Locked inputs treated as constants:** GPL-3.0 (D6) with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.` headers; pure Rust/GDExtension (D1); content IDs append-only per type (HIGH_LEVEL_PLAN §2.4, ctype/AGENTS.md); full parity (D3); content is plain data outside the ECS, `bevy_ecs` `Resource` handle only.

---

## 2. Scope & parity definition

### 2.1 In scope

- Rust framework replacing `ctype/*` + `core/ContentLoader.java`:
  - `ContentType` with the exact 18-variant Java enum order, `_UNUSED` slots preserved, `folderName` table, `contentClass` equivalent (`ContentKind`).
  - `Content` root semantics: per-type `short` ID assignment at construction, `minfo` (`ModContentInfo`), `removed`, lifecycle hooks, `compareTo` by ID, error flags.
  - `MappableContent`: globally unique `name`, `transformName` mod prefixing, per-type name map + global `byName` map, duplicate-name throw semantics.
  - `UnlockableContent`: localized name/description/details/credit from bundle at construction, `alwaysUnlocked`, `hideDatabase`, `databaseCategory`/`databaseTag`, `shownPlanets`/`databaseTabs`, icon lookup chain metadata, `unlocked()/unlockedHost()/unlockedNow()`/unlock persistence keys, `researchRequirements()`/`onUnlock()`/`getDependencies()`/`showUnlock()`.
  - Content registry + loader: fixed `createBaseContent()` order, `createModContent()` hook, `init()`/`postInit()`/`loadIcon()`/`load()`/`afterPatch()`, per-content error routing to mod handler, `logContent()` dense-ID validation, `copy()`-equivalent index snapshot, `remove`/`removeLast`/`setCurrentMod`/`setTemporaryMapper`, typed lookups (`get_by`, `get_by_id`, `get_by_name`, `by_name`).
- All content-type structs and their vanilla metadata: `Item`, `ItemStack`, `ItemSeq`, `Liquid`, `CellLiquid`, `LiquidStack`, `StatusEffect` (opposites/affinities/transition specs), `Weather` + `ParticleWeather`/`RainWeather`/`MagneticStorm`/`SolarFlare`, `Planet`/`Sector`/`SectorPreset`/`SectorDifficulty`, `TeamEntry`, `UnitCommand`, `UnitStance` + `ItemUnitStance`, `TechTree`/`TechNode`, `Category`, `ErrorContent`, `PayloadStack`/`PayloadSeq` value types.
- The **metadata half** of `Block`, `UnitType`, `Weapon`, `BulletType`: identity, names, class-kind tags, stats/config values, cross-references, derived `init()` values that are pure metadata. Behavior halves are out of scope (see §2.3).
- Full vanilla registry port: Items (22), Liquids (11), StatusEffects (23), Bullets (6), Weathers (6), Planets (7), SectorPresets (46), Loadouts (4), TeamEntries (0 by design), UnitCommands (10), UnitStances (8 + 22 item stances), Fx effect-name metadata, Blocks (~420 records; ~7 036 source lines), UnitTypes (65 records; ~4 673 source lines), both tech trees (Serpulo 202 `node` + 20 `nodeProduce`; Erekir 135 `node` + 20 `nodeProduce`).
- The mod/patch **registry-side interfaces** consumed by plan 20 (trait definitions, index snapshot/restore, content removal, `dp-` prefixing rules).

### 2.2 Definition of done

`mind-headless content audit` exits 0 against the committed golden dump (names, order, IDs, kinds, per-type counts, tech-node structure, selected metadata fields), all §7 tests pass, the MCP inspector scenario passes, and the content-load perf budget is met. “Done” means **all vanilla content is registered with byte-identical names/keys/order** — not that all content *behaves* (blocks don’t build, units don’t move; those are 07/10/11).

### 2.3 Explicit boundaries (who owns what)

| Area | Plan 02 owns | Deferred to |
|---|---|---|
| `Block` | `BlockDef` record: name/id, `kind` tag (`GenericCrafter`, `ItemTurret`, `Conveyor`, …), size, health/armor, `requirements` (`ItemStack`), category/group/flags/env/buildVisibility, `ConsumeSpec` list, research cost fields, `setStats`/`setBars` **data**, config/save flags, `@NoPatch` markers | Behavior (`update`, `acceptItem`, `onProximityUpdate`, `newBuilding`), building entities, placement, consumers execution, multiblocks: `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` |
| `BulletType` | `BulletDef`: motion/damage/collision/pierce/frag/status/lightning **values**, sprite region names, kind tag (`Basic`, `Artillery`, `Laser`, …) | Bullet entities, collision/raycast, `hit`/`update`/`draw`: `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` |
| `UnitType` | `UnitTypeDef`: stats, `WeaponDef` list, ability/part/engine **kind tags + config**, sprite region names, `entity_def` table entry, derived metadata (`range`/`maxRange`/`itemCapacity`/`fogRadius`/`aimDst`/`stepShake`, default commands/stances/research requirements) | Entity/component system, unit kinds, controllers, mirroring at runtime, pathfinding, AI: `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| `Fx` / `Effect` | Name-keyed `EffectId` table + minimal `EffectMeta` (lifetime, clip) needed by content definitions (`Bullets.load()` reads `Fx.lightning.lifetime`) | Effect system, renderers, all effect bodies: `17_FX_PARTS_IMPLEMENTATION_PLAN.md` |
| Planet | `PlanetDef` metadata + generator/mesh **kind tags**, rule-setter effect data, launch rules, sector grid construction, `preset()` wiring | Planet generators, g3d meshes, `getData()` JSON remap, campaign runtime: `06`, `12`, `16` |
| Tech tree | `TechNode` graph, requirements math, objective **data**, unlock graph | Research runtime, UI, save of finished requirements: `12`, `14` |
| Loadouts | Raw base64 strings + `loadout` content entries | `Schematic` decoding, validation, launch flows: `12` |
| JSON parsing / patching | Registry APIs the parser calls; `ModContentProvider`, `ContentParserHook`, `PatchHook`, `ResetAction` | `ContentParser`, `DataPatcher`, `DataManager`, assets: `20_MODS_IMPLEMENTATION_PLAN.md` |
| Assets | Region-name expectations per content, parity audit input | Atlas packing, bundle loading/fallback, `Icon`s, region lookup: `03_ASSETS_IMPLEMENTATION_PLAN.md` |
| Save/IO name mapping | `mod_content_name_map()` fallback table contents; name→ID resolution contract | Binary format, temporary mapper lifetime, revision I/O: `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` |

### 2.4 Deliberate deviations (reason stated)

1. **No `Rc`/object pointers.** Cross-content references are typed IDs (`ItemId`, `BlockId`, `UnitTypeId`, `StatusId`, `PlanetId`, `BulletId`, `SectorId`, `TechNodeRef`). Same observable behavior, no reference cycles, patch-friendly.
2. **Set iteration order.** Java `ObjectSet`/`ObjectMap` hash order is not reproducible in Rust. Databases/UI iterate insertion-ordered sets; where upstream sorts explicitly (`StatusEffect.setStats` sorts opposites/affinities), we sort identically. Any position-dependent UI list must re-sort locally (flagged as parity risk in §8).
3. **`WeatherState`/`UnitController`/`Prov<...>` factories** become kind tags dispatched by plans 05/11; plan 02 stores only the tag.
4. **`Fx` bodies** are stubs (plan 17); content load only needs names/lifetimes.
5. **Object graph mutation during lifecycle.** Java `UnitStance.init()` mutates other stances’ bit sets; Rust runs a dedicated `link()` pass after the `init()` sweep where cross-content mutation is legal (see §3.4). Behavior-preserving.
6. **`Publishable`** (Steam) is out of scope (plan 22); `MapLocales` is out of scope (plan 19); `Sector.getData()` JSON remap is identity until plan 12 supplies planet data.

---

## 3. Target design

### 3.1 Crates / module layout (`mind-core`, Godot-free)

```
mind-core/src/content/
  mod.rs                 # ContentType, ContentKind, ContentRef, registry error types
  color.rs               # Rgba value type (content metadata colors)
  ctype.rs               # Content/Mappable/Unlockable data model + traits + ModContentInfo
  id.rs                  # ContentId<T> newtype + aliases (ItemId, BlockId, ...)
  load.rs                # ContentRegistry, lifecycle sweeps, error routing, log_content
  names.rs               # transform_name, name map, duplicate handling, by_name
  bundle.rs              # BundleView trait (impl: plan 03), localized-name helper
  settings_store.rs      # UnlockStore trait (impl: plan 04), MemoryUnlockStore
  parser_hooks.rs        # ModContentProvider / ContentParserHook / PatchHook / ResetAction
  snapshot.rs            # RegistryIndexSnapshot (javadoc: ContentLoader.copy equivalent)
  category.rs            # Category enum (place-menu categories)
  stacks.rs              # ItemStack, LiquidStack, PayloadStack + seq containers helpers
  registries/
    items.rs             # Item + Items::load
    liquids.rs           # Liquid, CellLiquid + Liquids::load
    statuses.rs          # StatusEffect + transition specs + StatusEffects::load
    bullets.rs           # BulletDef kind hierarchy (metadata) + Bullets::load
    weathers.rs          # WeatherDef/ParticleWeatherDef/RainWeatherDef + Weathers::load
    planets.rs           # PlanetDef, Sector, SectorDifficulty + Planets::load
    sectors.rs           # SectorPresetDef + SectorPresets::load
    loadouts.rs          # LoadoutDef (raw base64) + Loadouts::load
    teams.rs             # TeamEntry + TeamEntries::load (stub)
    commands.rs          # UnitCommandDef + UnitCommands::load
    stances.rs           # UnitStanceDef + ItemUnitStance + UnitStances::load/load_after_mods
    fx_meta.rs           # EffectId, EffectMeta (names/lifetimes only; bodies in 17)
    blocks/
      mod.rs             # BlockDef, BlockKind, ConsumeSpec, StatSpec, BarSpec, Blocks::load
      environment.rs     # wave B1
      crafting.rs        # wave B2
      defense.rs         # wave B2
      distribution.rs    # wave B3
      liquid.rs          # wave B3
      power.rs           # wave B3
      production.rs      # wave B4
      storage.rs         # wave B4
      turrets.rs         # wave B5
      units.rs           # wave B5
      payloads.rs        # wave B5
      sandbox.rs         # wave B6
      legacy.rs          # wave B6
      campaign.rs        # wave B6
      logic.rs           # wave B6
    units/
      mod.rs             # UnitTypeDef, WeaponDef, EntityDefSpec, UnitTypes::load
      standard.rs        # wave U1–U3
      erekir.rs          # wave U4 + ErekirUnitType/TankUnitType/MissileUnitType/NeoplasmUnitType presets
      special.rs         # wave U5 (legacy/internal/block/dummy/tethered)
  tech/
    mod.rs               # TechTree, TechNode, builders, TechStore
    serpulo.rs           # SerpuloTechTree::load
    ekir.rs              # ErekirTechTree::load + rebalance()
```

`world/`, `game/`, `io/` etc. never live under `content/`; behavior crates read `ContentRegistry` through `&ContentRegistry` accessors only.

### 3.2 `ContentType` and IDs (ID-stability invariant)

```rust
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ContentType {
    Item = 0, Block = 1, MechUnused = 2, Bullet = 3, Liquid = 4, Status = 5,
    Unit = 6, Weather = 7, EffectUnused = 8, Sector = 9, LoadoutUnused = 10,
    TypeIdUnused = 11, Error = 12, Planet = 13, AmmoUnused = 14, Team = 15,
    UnitCommand = 16, UnitStance = 17,
}
pub const ALL: [ContentType; 18] = [ /* declaration order */ ];
```

- `name()` returns the exact Java enum identifier (`"mech_UNUSED"`, `"unitCommand"`, …) for `databaseCategory`/debug parity. `folder()` returns the JSON folder (`items`, `blocks`, `unused`, `bullets`, `liquids`, `statuses`, `units`, `weather`, `sectors`, `planets`, `teams`, `unitCommands`, `unitStances`), `_UNUSED`/`Error` → `unused`.
- `ContentId<T>` is a `#[repr(transparent)] u16` with `PhantomData<T>`; aliases `ItemId`, `BlockId`, `BulletId`, `LiquidId`, `StatusId`, `UnitTypeId`, `WeatherId`, `SectorId`, `PlanetId`, `TeamEntryId`, `UnitCommandId`, `UnitStanceId`. Debug assertions require `id.raw() == index` (dense, append-only).
- **ID-stability invariant (locked):** for every type, construction order defines IDs; the registry must be dense `0..len-1` in creation order (`log_content()` fails with the upstream message on a gap); new content is appended only; `_UNUSED` variants keep their ordinals forever; content names, bundle keys (`<type>.<name>.name|description|details|credit`) and sprite region names are ABI and are never renamed. A revert of this invariant requires a save/network version bump (plan 04) and is a locked-decision challenge (stop and ask, HIGH_LEVEL_PLAN §11.5).

### 3.3 Content data model

- `trait Content { const TYPE: ContentType; fn id(&self) -> u16; fn minfo(&self) -> &ModContentInfo; fn minfo_mut(&mut self) -> &mut ModContentInfo; ... }` plus lifecycle defaulted methods (`init`, `post_init`, `load_icon`, `load`, `after_patch`, `remove_content`). Implemented by every content struct.
- `trait Mappable: Content { fn name(&self) -> &str; }`.
- `trait Unlockable: Mappable` with the common `UnlockFields` sub-struct (localized name/description/details/credit, always-unlocked, hide flags, database fields, icon region cache, `tech_node: Option<TechNodeRef>`, `tech_nodes: SmallVec<TechNodeRef>`, `unlocked: bool`) to avoid duplicating ~40 fields per type.
- `ModContentInfo { mod_id: Option<ModId>, source_file: Option<String>, error: Option<ContentError>, base_error: Option<String>, asset: Option<AssetRef> }`; `is_vanilla/is_modded/is_patch_content/has_errored`.
- `ErrorContent` is a real `Content`-only record for `ContentType::Error` (fallback for failed parses, plan 04/20).
- Entity-def table (replaces `@EntityDef` codegen for units): `EntityDefSpec { components: SmallVec<ComponentKind>, legacy: bool }` keyed by unit name; plan 02 ports the 19 declaration groups verbatim (see port map); plan 11 consumes it and may register more.
- Behavior-facing kind tags: `BlockKind`, `BulletKind`, `UnitKind`, `WeatherKind`, `ControllerKind`, `AbilityKind`, `DrawPartKind`, `EffectId`; all are content ABI names + ordinals (serialized in plans 04/20), with bodies owned by the plans in §2.3.

### 3.4 `ContentRegistry` and lifecycle

```rust
pub struct ContentRegistry {
    items: Vec<Item>, blocks: Vec<BlockDef>, bullets: Vec<BulletDef>,
    liquids: Vec<Liquid>, statuses: Vec<StatusEffect>, units: Vec<UnitTypeDef>,
    weathers: Vec<WeatherDef>, sectors: Vec<SectorPresetDef>, planets: Vec<PlanetDef>,
    teams: Vec<TeamEntry>, commandss: Vec<UnitCommandDef>, stances: Vec<UnitStanceDef>,
    loadouts: Vec<LoadoutDef>, errors: Vec<ErrorContent>,
    names: NameMaps,              // per-type + global name -> ContentRef
    current_mod: Option<ModId>,
    temporary_mapper: Option<Box<TempMapper>>,   // save loading; plan 04 fills
    tech: TechStore,
    phases: LifecyclePhases,      // bitset preventing double-run (Java `initialization` set)
    arr_epoch: u32,               // bumped when content added; ItemSeq/PayloadSeq grow
}
```

- **Lifecycle order is upstream-exact:** `create_base_content` runs `UnitCommand.loadAll() → TeamEntries.load() → Items.load() → UnitStance.loadAll() (needs items) → StatusEffects.load() → Liquids.load() → Bullets.load() → UnitTypes.load() → Blocks.load() → Loadouts.load() → Weathers.load() → Planets.load() → SectorPresets.load() → SerpuloTechTree.load() → ErekirTechTree.load()`.
- `initialize(phase)` sweeps `ContentType::ALL` in order, then content order within a type, exactly like `ContentLoader.initialize`; per-content errors are caught; vanilla errors rethrow, mod errors route to `ModErrorSink` (plan 20).
- Rust borrow strategy (implementation note, no semantic change): per-type `Vec`s + `split_at_mut`/destructuring so a hook gets `&mut current` + read-only access to all other records; after the `init()` sweep a `link()` pass runs with full `&mut ContentRegistry` (used by `UnitStance` incompatible bits and any same-type cross-mutation).
- `create_mod_content()` calls `ModContentProvider::load_content(&mut registry, &mut mods)` (plan 20), then `UnitStances::load_after_mods()` and an equivalent of `ModContentLoadEvent`.
- `init()` also runs `logic_vars.init()` equivalent hook (interface reserved for plan 13) and fires `ContentInitEvent` (plan 05 event bus).
- `load()`/`load_icon()` are skipped when `headless` is set (plan 00 flag), preserving ContentLoader semantics.
- `copy()` equivalent = `RegistryIndexSnapshot`: clones the *index* (per-type ID membership + name maps + `current_mod` + temporary mapper handle), **not** content payloads (payload mutation rollback is plan 20’s `ResetAction` closures, matching upstream resetters). `restore_index` re-syncs membership; `remove`/`remove_last` mirror `ContentLoader.remove/removeLast` including `remove_content()`.
- `log_content()` port: identical out-of-order message (`Out-of-order IDs for content '<name>' (expected <i> but got <id>)`) plus per-type count log; called after `create_mod_content` in debug and always in `content dump`.

### 3.5 Lifetime / reference rules (Rust)

1. **Single owner.** `ContentRegistry` owns every content value. No `Rc`, `Arc`, `RefCell`, or self-referential structs in content data.
2. **References are typed IDs.** Content records store `ItemId`/`LiquidId`/`…`; value stacks (`ItemStack { item: ItemId, amount: i32 }`) mirror Java. Name strings are permitted only where upstream stores strings (sprite regions, bundle keys, map JSON keys, `fullOverride`).
3. **Cross-registry references resolve in `init()`/`link()`**, not at construction — preserving the upstream load-order dependency rules (`statuses/liquids/bullets` before `units`; `units/blocks` before `planets`; everything before tech trees). Debug builds assert that a referenced ID existed before the current phase (`RegistryEpoch` guard) to turn load-order bugs into immediate failures.
4. **No content borrow may escape a lifecycle call.** Behavior systems borrow the registry per-access (`registry.item(id)`) or cache a `Resource<Content>` handle for the match lifetime; they never store `&Item`.
5. **Runtime-only data stays out.** Building/unit/bullet runtimes live in the ECS (plans 07/10/11); they reference content by ID. Content never references ECS entities.
6. **Tech graph ownership.** `TechStore` owns `TechNode`s; `Unlockable` stores `TechNodeRef` (index) and `tech_nodes` list; parents/children are indices, no cycles at the Rust type level (semantic cycles impossible: a node’s parent precedes it).
7. **Patch lifetime.** After `create_mod_content`, the registry is logically frozen; only plan 20 patch APIs mutate it, always inside `begin_patch`/`ResetAction` brackets. `after_patch()` re-derives metadata exactly like `Block.afterPatch()`/`Unlockable.afterPatch()`.

### 3.6 Interfaces owned here, implemented by sibling plans

```rust
// plan 03 (assets/bundle)
pub trait BundleView {
    fn get(&self, key: &str) -> Option<&str>;
    fn get_or(&self, key: &str, default: &str) -> String;
}
// plan 04 (settings persistence)
pub trait UnlockStore {
    fn get_bool(&self, key: &str) -> bool;
    fn set_bool(&mut self, key: &str, value: bool);
    fn get_i32(&self, key: &str) -> i32;
    fn set_i32(&mut self, key: &str, value: i32);
}
// plan 20 (mod content + patches)
pub trait ModContentProvider {                                    // called by create_mod_content()
    // `ModSet` is plan 20's type and was deferred at M1 (reconcile at M6).
    fn load_content(&mut self, reg: &mut ContentRegistry) -> Result<(), ContentErrors>;
}
pub trait ContentParserHook {                                     // `ContentParser.parse`
    fn parse(&mut self, reg: &mut ContentRegistry, asset: &ContentAsset)
        -> Result<ContentRef, ContentParseError>;
}
pub trait PatchHook {                                             // `DataPatcher.apply`
    fn apply(&mut self, reg: &mut ContentRegistry, patches: &[PatchAsset], content: &[ContentAsset])
        -> Result<Vec<ResetAction>, ContentError>;
}
pub type ResetAction = Box<dyn FnOnce(&mut ContentRegistry) + 'static>; // field restores; index restore via snapshot
// failure surface for per-content lifecycle errors
pub trait ModErrorSink {
    fn handle_content_error(&mut self, content: ContentRef, err: &ContentError);
}
```

Plan 20 calls, in order: `create_mod_content` → per-mod `ContentParserHook::parse` with `current_mod = Some(mod_id)` (names get `<modname>-` via `transform_name`), `finish_parsing`, `UnitStances::load_after_mods`, `PatchHook::apply` after `content.init()`; `logic.reset()` → `unload`/`ResetAction`s + `restore_index`. Names must be looked up by name (never serialized mod IDs) exactly as upstream mod/AGENTS.md warns.

### 3.7 External integration points

- **Plan 00 spine:** register `Content` as a `bevy_ecs` `Resource` (`#[derive(Resource)] pub struct Content(pub ContentRegistry)`); Godot inspector tab `res://scenes/dev/state_inspector.tscn` gains `/root/Spine/StateInspector/ContentCounts` (Label) and `/root/Spine/StateInspector/ContentList` (ItemList, first N entries of a type); `MindCore.content_counts() -> Dictionary` and `MindCore.content_list(type_name) -> PackedStringArray` GDExtension methods (read-only).
- **Plan 05 sim:** the `World` owns `Content`; entity spawns resolve IDs at create time; no lifecycle hooks run during ticks.
- **Plan 03 assets:** bundle is loaded **before** `createBaseContent()` (localized names are captured at construction, matching upstream); `after_patch` does not relocalize. `asset_manifest.json` is the audit input for sprite-region parity.
- **Plan 04 IO:** `get_by_id` honors `TemporaryMapper` (unknown/removed content → ID 0 default; `-1` → `None`), and `get_by_name(block, ..)` applies `mod_content_name_map()` fallbacks (`craters→crater-stone`, `deepwater→deep-water`, `water→shallow-water`, `slag→molten-slag`). The content-header writer emits names in ID order; reader installs the mapper and restores `None` in a `finally`-equivalent.
- **Plan 12 campaign:** planet JSON `getData().presets` remap and `Sector` runtime state; interface `SectorRemapProvider::preset_remap(planet: PlanetId, name: &str, fallback: u16) -> u16` default identity until plan 12 lands.
- **STDB (plan 21):** no tables/reducers here. Cross-wire rules: vanilla IDs are stable and may be used within the same build; mod content travels by **name**; command payloads carry content names (plan 21 validates cheaply by name lookup).

### 3.8 Preserved gotchas (must-test list)

| Gotcha (source) | Rust obligation |
|---|---|
| `ErekirTechTree.rebalance()` (`ErekirTechTree.java:26-48`) | Runs first in `erekir::load()`, before node building. Multiplies `damage *= 0.75` on bullets used by (a) every `UnitType` whose kind tag is `ErekirUnitType`, across all authored weapons, and (b) turrets whose `requirements` include ≥1 item **not** in `Items.serpuloItems`; `ItemTurret`/`ContinuousLiquidTurret` ammo values and `ContinuousTurret.shootType` included. Guarded by a `bitset` per bullet ID so double references scale once. |
| `UnitStance` incompatible bits built in `init()`, not `load()` (`UnitStance.java:43-55`) | `link()` pass after the `init()` sweep; symmetric bit setting; `ItemUnitStance` created per item at load and for mod items in `load_after_mods`. |
| `StatusEffect` affinities symmetric, opposites double-registered, `handleOpposite` cancels at 0.5× rate (`StatusEffect.java:179-201`) | Transition tables stored as data: `TransitionSpec::{Opposite, Affinity(AffinityTransition)}`, where `AffinityTransition` carries damage/pierce/effect/extend{cap}/trigger (a lossless superset of the `ExtendAffinity`/`Damage`/`SetEffect` shapes; composite handlers like burning↔tarred need it). Symmetric insertion runs in the `link()` pass in id order. |
| `Liquid.init()` gas rules (`Liquid.java:82-99`) | `gas → boil_point=-1, color.a=0.6, gas_color=color, bar_color=color` if unset. |
| `Block.init()` derived values (`Block.java:1357+`) | `health = size²·(40+Σ item.healthScaling)` rounded to 5; `offset/size_offset`; `build_time = Σ amount·item.cost × buildCostMultiplier` (20 tick default); consumer array partitions (`consumers/optional/nonOptional/update`); `flags += hasFogRadius|synced`; `liquidCapacity` inference. |
| `Block.postInit()` auto `shownPlanets` from requirements and `databaseTag = category.name()` | Implemented in block metadata `post_init`. |
| `Block.researchRequirements()` formula (`Block.java:1284-1295`) | `round_to_10(60·mult + amount^1.11 · 20 · mult · perItemMult)`; `researchCost` override; `researchCostMultiplier <= 0 → empty`. |
| `UnitType` derived fields (`UnitType.java:915+`) | `itemCapacity`, `range/maxRange` from weapon ranges −4 margin, `fogRadius = max(174, hitSize·2)/8`, `mechStride`, `aimDst`, `stepShake`, `pathCostId` (tag table from plan 11), `deathSound`/`wreckSound`/`lightRadius` defaults, mirrored weapon `reload`/`recoilTime` doubling. |
| `UnitType.researchRequirements()` from factory/reconstructor/assembler plans × multiplier (50; Erekir 10) (`UnitType.java:1355-1427`) | Ported in metadata; cache invalidated by `after_patch`. |
| `TechNode` auto `SectorComplete` when parent is `SectorPreset`, `getDependencies` → implicit `Research`, cost multipliers inherited, `req-…` settings keys (`TechTree.java:39-139`) | Exact port; `save()` writes only changed values. |
| `Block`/`SectorPreset`/`UnitType` hidden rules (`buildVisibility`, `description == null`, `hidden`) | Preserved per-type `is_hidden()`. |
| `ContentLoader.handleMappableContent` duplicate throw pops half-registered content first (`ContentLoader.java:188-207`) | Ported exactly, including `last_added` rollback. |
| `ItemSeq.add` ignores items beyond array length (`ItemSeq.java:126-130`); `DataPatcher.fixContentArrayCapacity` | `ItemSeq`/`PayloadSeq` grow against `arr_epoch`; out-of-range drops preserved for the general path. |
| `Fx` initialized on first reference, not in a `load()` | `fx_meta` is a static name/lifetime table initialized on first read; ordering side effects stay out (plan 17 owns bodies). |
| `UnlockableContent.loadIcon()` chain and `<name>-unlocked` settings keys | Chain metadata + `UnlockStore` calls with identical keys. |

---

## 4. Port map (every `content/`, `ctype/`, `type/` file + metadata halves)

Legend: **owner** = plan that ships the code; 02 = this plan.

| Mindustry source | Target Rust | Notes |
|---|---|---|
| `ctype/Content.java` | `content/ctype.rs` — `Content` trait, `ModContentInfo`, `has_errored/is_vanilla/is_modded/is_patch_content`, `compare_to` by id, `Display` as `type#id` | `@NoPatch` marker becomes `PATCH_DENIED` const set per record. |
| `ctype/ContentType.java` | `content/mod.rs` — `ContentType` enum, `ALL`, `folder()`, `name()`, `kind()` | 18 variants exact order; `_UNUSED` preserved; test `content_type_ordinals`. |
| `ctype/MappableContent.java` | `content/ctype.rs` — `Mappable` + `names.rs` registration | `transform_name` mod prefix. |
| `ctype/UnlockableContent.java` | `content/ctype.rs` — `Unlockable`, `UnlockFields`; `bundle.rs` for name capture; `settings_store.rs` for unlock state | Icon lookup stored as region-name expectation; actual TextureRegion is plan 03/16. |
| `core/ContentLoader.java` | `content/load.rs` + `snapshot.rs` + `names.rs` | Full API surface incl. `copy`, `remove`, `remove_last`, `set_current_mod`, `set_temporary_mapper`, `by_name`, `get_by_id` default-0 mapper rule, `log_content`. |
| `content/Items.java` | `registries/items.rs` | 22 items; `serpulo_items`/`erekir_items`/`erekir_only_items` lists; exact `load()` order. |
| `content/Liquids.java` | `registries/liquids.rs` | 11 liquids incl. `CellLiquid` neoplasm; `canStayOn` insertion-ordered sets. |
| `content/StatusEffects.java` | `registries/statuses.rs` | 23 statuses; `init` closures → `TransitionSpec` data; `allDatabaseTabs = true`. |
| `content/Bullets.java` | `registries/bullets.rs` | 6 internal bullets; `damageLightning.copy()` variant flags preserved. |
| `content/Fx.java` | `registries/fx_meta.rs` (names/lifetimes only) + **17** for behavior | ~100+ effects under one declaration; only `lightning.lifetime` etc. needed at load. |
| `content/Weathers.java` | `registries/weathers.rs` | 6 weathers (`snowing` name for `snow`!); `Time.toMinutes` constant; all hidden from DB. |
| `content/Planets.java` | `registries/planets.rs` | 7 planets; `makeAsteroid` helper ported; generator/mesh/rule-setter kind tags; `sectorCaptureReplacements` map. |
| `content/SectorPresets.java` | `registries/sectors.rs` + `sectors/sector_submissions.rs` hook | 46 presets; `SectorSubmissions.registerSectors()` stubbed behind a hook (plan 19 map submissions). |
| `content/Loadouts.java` | `registries/loadouts.rs` | Raw base64 preserved byte-for-byte; decode/validation in **12**. |
| `content/TeamEntries.java` | `registries/teams.rs` | Empty `load()` by design; commented-out entries not ported (note in file). |
| `content/TechTree.java` | `tech/mod.rs` | `nodeRoot/node/nodeProduce`, context, `TechNode`, `each/addDatabaseTab/addPlanet/icon/localizedName/setupRequirements/reset/save/remove`. |
| `content/SerpuloTechTree.java` | `tech/serpulo.rs` | Port order verbatim; ≈202 `node` + 20 `nodeProduce`. |
| `content/ErekirTechTree.java` | `tech/erekir.rs` | `rebalance()` first; `costMultipliers`; exact node order. |
| `content/Blocks.java` | `registries/blocks/*.rs` (waves B1–B6) + `BlockDef` | ~420 records, ~7 036 lines; `//region` order = wave order; behavior in **07**. |
| `content/UnitTypes.java` | `registries/units/*.rs` (waves U1–U5) + `UnitTypeDef`/`WeaponDef` | 65 units, ~4 673 lines; `@EntityDef` groups → `EntityDefSpec`; behavior in **11**. |
| `type/Category.java` | `content/category.rs` | 10 variants; `prev()/next()`. |
| `type/Item.java` | `registries/items.rs` | `Item` fields + `is_hidden` + stats data; `getAllOres()` (needs **07** `OreBlock`) deferred as helper on plan 07. |
| `type/ItemStack.java` | `content/stacks.rs` | Value type; `with/list/mult/copy`; default item `Items.copper` for deserialization. |
| `type/ItemSeq.java` | `content/stacks.rs` | ID-indexed counts; `arr_epoch` growth; JSON keyed by item name (plan 04 uses). |
| `type/Liquid.java` | `registries/liquids.rs` | Fields + `init()` gas rules + `bar_color()`; `draw_puddle/update/react` in **09**. |
| `type/CellLiquid.java` | `registries/liquids.rs` | Fields only; spreading/update in **09**. |
| `type/LiquidStack.java` | `content/stacks.rs` | Value type; default `Liquids.water`. |
| `type/StatusEffect.java` | `registries/statuses.rs` | Fields + transitions + stats data; `update/applied` behavior in **05/11**. |
| `type/Weather.java` | `registries/weathers.rs` | Fields, `WeatherEntry`, `create()` metadata; `WeatherState` entity in **05**; draw helpers in **16**. |
| `type/weather/ParticleWeather.java` | `registries/weathers.rs` | Fields + region/noise names; draw in **16**. |
| `type/weather/RainWeather.java` | `registries/weathers.rs` | Fields + `splash-0..11` region expectations. |
| `type/weather/MagneticStorm.java` | `registries/weathers.rs` | Stub class (no fields). |
| `type/weather/SolarFlare.java` | `registries/weathers.rs` | Stub class (no fields). |
| `type/Planet.java` | `registries/planets.rs` `PlanetDef` | Metadata + `sectors` grid + `preset()` wiring; `getData()`/mesh/launch runtime in **12/16**. |
| `type/Sector.java` | `registries/planets.rs` `Sector` | Content-time fields (`id`, `tile`, `rect`, `plane`, `preset`, `shieldTarget`, `threat`); save/info runtime in **12**. |
| `type/SectorPreset.java` | `registries/sectors.rs` | `initialize()` incl. remap hook; icons `sector-<name>`; `is_hidden() = description.is_none()`. |
| `type/SectorDifficulty.java` | `registries/sectors.rs` | Constants 0/3/5/8/10/13. |
| `type/TeamEntry.java` | `registries/teams.rs` | `team` tag; `displayExtra` key `team.<name>.log` (UI plan 14). |
| `type/MapLocales.java` | **19** (map editor/locales) | Out of scope; noted to avoid accidental omission. |
| `type/PayloadStack.java` | `content/stacks.rs` | Value type; `UnlockableContent` ref (Block or UnitType) as `ContentRef`. |
| `type/PayloadSeq.java` | `content/stacks.rs` | ID-indexed; growth with `arr_epoch`; used by **08**. |
| `type/ErrorContent.java` | `content/ctype.rs` | `ErrorContent` record, `ContentType::Error`. |
| `type/Publishable.java` | **22** (Steam) | Out of scope. |
| `type/UnitType.java` | `registries/units/mod.rs` `UnitTypeDef` | Metadata half: all config + derived metadata; `constructor` → `EntityDefSpec`; behavior in **11**. |
| `type/Weapon.java` | `registries/units/weapon.rs` `WeaponDef` | Names/mount/regions/reload/pattern kind/bullet id; firing in **10/11**. |
| `type/unit/ErekirUnitType.java` | `registries/units/erekir.rs` | Defaults (`researchCostMultiplier = 10`, `flying = true`-style presets). |
| `type/unit/TankUnitType.java` | `registries/units/erekir.rs` | Tank presets. |
| `type/unit/MissileUnitType.java` | `registries/units/special.rs` | Missile presets (`missile` unit). |
| `type/unit/NeoplasmUnitType.java` | `registries/units/erekir.rs` | Crawl presets. |
| `type/weapons/BuildWeapon.java` | `registries/units/weapon.rs` kind tag + **11** | Visual/behavior in **11**. |
| `type/weapons/MineWeapon.java` | `registries/units/weapon.rs` kind tag + **11** | — |
| `type/weapons/PointDefenseWeapon.java` | `registries/units/weapon.rs` kind tag + **10/11** | Interception in **10**. |
| `type/weapons/PointDefenseBulletWeapon.java` | `registries/units/weapon.rs` kind tag + **10/11** | — |
| `type/weapons/RepairBeamWeapon.java` | `registries/units/weapon.rs` kind tag + **11** | maxRange bullet constraint checked in metadata. |
| `ai/UnitCommand.java` | `registries/commands.rs` | 10 commands; metadata + `controller` kind tag (behavior **11**); `switch_to_move`/`draw_target`/`snap_to_building` flags. |
| `ai/UnitStance.java` | `registries/stances.rs` | 8 stances; `link()` computes incompatible bits; `load_after_mods`. |
| `ai/ItemUnitStance.java` | `registries/stances.rs` | One stance per item at load and per mod item post-mods. |
| `world/Block.java` (metadata half) | `registries/blocks/mod.rs` `BlockDef` | Fields listed in §6.1; `init/postInit/researchRequirements/getDependencies` metadata ported; all behavior to **07**. |
| `entities/bullet/BulletType.java` (metadata half) | `registries/bullets.rs` `BulletDef` | Fields from `entities/AGENTS.md` list; behavior to **10**. |
| `io/SaveFileReader.java` (`modContentNameMap`) | `content/names.rs::mod_content_name_map()` | Table contents duplicated here so plan 04 can apply them during `get_by_name(block, ..)`; plan 04 owns the file. |
| `mod/ContentParser.java`, `mod/DataPatcher.java` | **20** implements `ContentParserHook`/`PatchHook`; plan 02 ships `parser_hooks.rs` + registry APIs | Interfaces in §3.6. |
| `annotations/**` (`@EntityDef`, `ClassMap`, `@Load`) | `EntityDefSpec` table (this plan) + `LoadedRegion` expectations (plan 03) + class tags (plans 20) | Codegen is not ported; its *inputs* are data (annotations/AGENTS.md). |

---

## 5. Milestones & task breakdown

Every milestone ends with: `cargo fmt`, `cargo clippy -p mind-core -- -D warnings`, `cargo test -p mind-core`, and the named harness command. Evidence goes in the Changelog. Each ported file carries the GPL header.

**M1 — Framework + smallest vertical slice (Items/Liquids/StatusEffects/Bullets).**
Deliver `ContentType`/`ContentId`/`Content` traits, `ContentRegistry`, `names.rs`, `bundle.rs`/`settings_store.rs` traits (`MemoryUnlockStore`), lifecycle sweeper + `link()` pass, `log_content`, `ErrorContent`. Port `Items` (22), `Liquids` (11), `StatusEffects` (23), `Bullets` (6), `fx_meta` names/lifetimes. Harness: `content_load`, `content_ids`, `content_bench` (first number). Tests: `content_framework::content_type_ordinals`, `items::matches_golden`, `liquids::gas_post_init`, `statuses::opposites_symmetric`, `statuses::affinity_transition_tables`, `bullets::damage_lightning_copy_flags`.

**M2 — Small registries + tech trees (Serpulo/Erekir).**
Port `UnitCommands` (10), `TeamEntries` (stub), `UnitStances` (8 + item stances), `Weathers` (6), `Planets` (7), `SectorPresets` (46), `Loadouts` (4), `TechTree`/`TechNode`, `SerpuloTechTree`, `ErekirTechTree` incl. `rebalance()`. Harness: `content_ids` diff vs golden for all 12 live types; `content_load_order_bad` negative scenario. Tests: `erekir::rebalance_applies_once`, `tech::sector_complete_insertion`, `tech::requirements_rounding`, `sectors::presets_resolve`, `stances::incompatible_bits`, `commands::count_and_ids`.

**M3 — Blocks waves B1–B2.** `environment` + `ore`, then `crafting` + `defense`; `BlockDef` record, `BlockKind`, `ConsumeSpec`, stat/bar data; `post_init`/`init` metadata derivations. Ledger `parity/ledgers/blocks.md` rows for B1–B2 all checked. Tests: `blocks::construct_block_present` (`build2`), `blocks::health_and_buildtime_derivation`, `blocks::research_requirements_formula`.

**M4 — Blocks waves B3–B6.** `distribution`+`liquid`+`power`, `production`+`storage`, `turrets`+`units`+`payloads`, `sandbox`+`legacy`+`campaign`+`logic`. Full ID/name audit for `block`. Tests: `blocks::all_metadata_valid` (metadata half of upstream `allBlockTest`), `blocks::ergonomic_flags_match_golden`.

**M5 — UnitTypes waves U1–U5.** All 65 units, `WeaponDef`, `EntityDefSpec` table (19 groups), `UnitKind` presets (`ErekirUnitType`/`TankUnitType`/`MissileUnitType`/`NeoplasmUnitType`), derived metadata. Tests: `units::entity_def_table_covers_all_units`, `units::mirrored_weapon_reload_doubled`, `units::research_requirements_derived`, `units::hidden_flags`.

**M6 — Mod/patch registry hooks.** `RegistryIndexSnapshot`, `remove`/`remove_last`/`set_current_mod`, `transform_name`, `parser_hooks.rs`, `arr_epoch` growth. Contract tests with a fake provider/parser replicating `DataAssetTests` + `PatcherTests` reset semantics (plan 20 replaces the fake). Deliver the written hand-off note in §3.6 to `20_MODS_IMPLEMENTATION_PLAN.md`.

**M7 — Parity audit tooling, MCP integration, perf gate.** `mind-headless content audit` with bundle + region-manifest checks; golden dump committed; CI job; MCP inspector `Content` tab; ledgers finalized with zero unported rows; §7 checklist green.

---

## 6. Data & formats

### 6.1 Core records (selected field sets; full fields = source-faithful)

- `Item { unlock: UnlockFields, color: Rgba, explosiveness, flammability, radioactivity, charge, hardness: i32, cost, health_scaling, low_priority, frames: i32, transition_frames: i32, frame_time, buildable, hidden }`.
- `Liquid { unlock, gas, color, gas_color, bar_color: Option<Rgba>, light_color, flammability, temperature, heat_capacity, viscosity, explosiveness, block_reactive, coolant, move_through_blocks, incinerable, effect: StatusId, particle_effect: EffectId, particle_spacing, boil_point, cap_puddles, vapor_effect: EffectId, hidden, can_stay_on: IndexSet<LiquidId> }`; `CellLiquid` adds `color_from/color_to/cells/spread_target/max_spread/spread_conversion/spread_damage/remove_scaling`.
- `StatusEffect { unlock, damage_multiplier, health_multiplier, speed_multiplier, reload_multiplier, build_speed_multiplier, drag_multiplier, transition_damage, disarm, damage, interval_damage_time, interval_damage, interval_damage_pierce, effect_chance, parentize_effect, permanent, reactive, dynamic, show, color, effect: EffectId, apply_effect: EffectId, apply_extend, apply_color, parentize_apply_effect, affinities: IndexSet<StatusId>, opposites: IndexSet<StatusId>, transitions: Vec<(StatusId, TransitionSpec)>, outline }`; `TransitionSpec` is data (see §3.8).
- `BulletDef { id, kind: BulletKind, speed, lifetime, drag, accel, keep_velocity, scale_life, weave_*, collides*, hit_size, draw_size, pierce*, damage, splash_damage/radius, scaled_splash_damage, building_damage_multiplier, status: StatusId, status_duration, pierce_armor, armor_multiplier, lifesteal, hit/despawn/shoot/smoke/trail effects (EffectId), trail_length, frag_bullet: Option<BulletId>, frag_bullets: i32, frag_on_hit/despawn, interval_bullet, lightning*, suppression*, puddles, spawn_unit: Option<UnitTypeId>, despawn_unit, sprite, back_sprite, width, height, ... }`.
- `UnitTypeDef { unlock, kind: UnitKind, entity_def: EntityDefSpec, health, armor, speed, hit_size, rotate_speed, drag, accel, item_capacity, mine_tier, mine_speed, build_speed, payload_capacity, env_*, flying/low_altitude/hovering/omni_movement/can_boost/targetable/..., weapons: Vec<WeaponDef>, abilities: Vec<AbilitySpec>, parts: Vec<DrawPartSpec>, engines: Vec<EngineSpec>, immunities: IndexSet<StatusId>, target_flags, region, cell_region, leg/joint/foot/tread/wreck region names, derived fields (marked `derived`), research_cost_multiplier, hidden }`.
- `WeaponDef { name, kind: WeaponKind, reload, x, y, shoot_x, shoot_y, rotate, base_rotation, rotate_speed, rotation_limit, mirror, alternate, top, layer_offset, region, heat_region, heat_color, recoil_time, bullet: BulletId, shoot: ShootPatternSpec, inaccuracy, velocity_rnd, life_rnd, shoot_sound: SoundId, ... }`.
- `BlockDef { unlock, kind: BlockKind, size, health, scaled_health, armor, requirements: Vec<ItemStack>, research_cost: Option<Vec<ItemStack>>, research_cost_multiplier, build_cost_multiplier, category: Category, group: BlockGroup, flags: BitFlags<BlockFlag>, consumes: Vec<ConsumeSpec>, item_capacity, liquid_capacity, env_required/enabled/disabled, build_visibility: BuildVisibility, placeable_*, solid/floating/update/destructible/save_data/config_flags, map_color, has_color, square_sprite, bars: Vec<BarSpec>, stats: Vec<StatSpec>, priority, unit_cap_modifier, region names, derived fields }`. `ConsumeSpec` = enum (`Items(Vec<ItemStack>)`, `Liquids(..)`, `Power{usage, buffered}`, `Coolant{..}`, `Payloads`, `Heat`, …) with `optional`/`update`/`ignore` flags exactly as `Consume`.
- `PlanetDef { unlock, parent: Option<PlanetId>, radius, sector_tiles: u8, generator: GeneratorKind, mesh: MeshKind, cloud_mesh, default_env: EnvFlags, default_attributes, rule_setter: RuleSetterSpec, default_core: Option<BlockId>, start_sector: u32, sector_seed, always_unlocked, icon: String, tech_tree: Option<TreeId>, launch flags, campaign_rule_defaults, unlocked_on_land: Vec<BlockId>, sectors: Vec<Sector>, ... }`.
- `Sector { id: u16, tile: PtileRef, rect, plane, preset: Option<SectorId>, shield_target: Option<SectorRef>, threat, generate_enemy_base }`; `SectorPresetDef { unlock, planet: PlanetId, sector: SectorRef, capture_wave, rules: RuleOverrideSpec, difficulty: f32, start_wave_time_multiplier, add_starting_items, no_lighting, is_last_sector, require_unlock, show_hidden, override_launch_defaults, allow_launch_schematics/loadout, attack_after_waves, original_position, shield_sectors: Vec<SectorRef>, outline metadata, file_name }`.
- `TechNode { depth: u32, name: Option<String>, requires_unlock: bool, parent: Option<TechNodeRef>, root_node: Option<TechNodeRef>, research_cost_multipliers: Vec<(ItemId, f32)>, content: ContentRef, requirements: Vec<ItemStack>, finished_requirements: Vec<ItemStack>, objectives: Vec<ObjectiveSpec>, children: Vec<TechNodeRef>, planet: Option<PlanetId> }`; `ObjectiveSpec` enum (`SectorComplete(SectorId)`, `Research(ContentRef)`, `Produce(ContentRef)`, `OnSector(SectorId)`, `OnPlanet(PlanetId)`, … — full set owned by **12**, plan 02 ships the variants needed by the two vanilla trees).
- `UnitCommandDef { name, icon, keybind: Option<KeybindSpec>, controller: Option<ControllerKind>, switch_to_move, draw_target, reset_target, snap_to_building, exact_arrival, refresh_on_select, extra_stances: Vec<UnitStanceId>, localized_key: "command.<name>" }`; `UnitStanceDef { name, icon, keybind, toggle, incompatible_commands: Vec<UnitCommandId>, incompatible_stances: Vec<UnitStanceId>, incompatible_command_bits: BitSet32, incompatible_stance_bits: BitSet32, localized_key: "stance.<name>" }`; `ItemUnitStance { item: ItemId }`.
- `LoadoutDef { name, schematic_base64: String, schematic: Option<SchematicHandle> }` (plan 12 fills the handle).

### 6.2 Snapshot / audit formats (committed artifacts)

`parity/golden_content.json` (generated once from upstream; see §7a):
```json
{
  "mindustry_version": "v146",
  "generator": "parity/java/DumpContent.java",
  "counts": {"item":22,"block":418,"bullet":6,"liquid":11,"status":23,"unit":65,
             "weather":6,"sector":46,"planet":7,"team":0,"unitCommand":10,"unitStance":30},
  "types": [{"type":"item","entries":[
      {"id":0,"name":"copper","kind":"Item","localized":"Copper",
       "bundle":{"name":"item.copper.name"},
       "regions":["item-copper","copper"],
       "fields":{"hardness":1,"cost":0.5,"buildable":true,"hidden":false}}]}],
  "tech_trees": [{"root":"serpulo","nodes":[
      {"path":"core-shard","content":"core-shard","parent":null,"depth":0,
       "requirements":[["copper",1000]],"objectives":[]}]}],
  "mod_content_name_map": {"craters":"crater-stone","deepwater":"deep-water","water":"shallow-water","slag":"molten-slag"}
}
```
`mind-headless content dump` emits `content_ids.json` with the same `types` block (no upstream fields), byte-stable (sorted keys, LF, trailing newline).

`parity/ledgers/<registry>.md` (one per registry; example):
```markdown
# Ledger — blocks (upstream `content/Blocks.java`, 418 entries)
| pos | name | kind | ported | golden_sha | wave | notes |
|-----|------|------|--------|-----------|------|-------|
| 0 | air | AirBlock | [x] | 9f2c1a | B1 | |
| 1 | spawn | OverlayFloor | [x] | 71ab03 | B1 | `spawn` overlay |
...
- Unported: 0
- Bundle keys present: 418/418
- Region expectations satisfied: 418/418 (warnings: 0)
```

`parity/reports/content_audit.md` is regenerated by CI; exit code is the gate.

### 6.3 Audit inputs from sibling plans

- Plan 03: `parity/asset_manifest.json` (all atlas region names + per-content expected regions, generated by `tools:pack` equivalent).
- Plan 03: `core/assets/bundles/bundle*.properties` parsed directly for key presence (no need to load Godot).
- Plan 04: `TemporaryMapper` API and content-header writer/reader (cross-plan test `io::content_header_roundtrip`).
- Plan 20: `ContentAsset`/`PatchAsset` shapes; the trait impls from §3.6.

### 6.4 File paths

- Golden/ledgers: `mindustry-godot/parity/{golden_content.json, asset_manifest.json, ledgers/, reports/}`.
- Dump tool: `mindustry-godot/parity/java/DumpContent.java` (standalone; compiled and run against the upstream checkout’s `:core` classpath; writes only into `mindustry-godot/parity/`; **never modifies the upstream repo**).
- Rust registries: `client/rust/mind-core/src/content/**` as in §3.1.

---

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests (Mindustry `tests/src/test/java/**` → `cargo test`)

| Mindustry test | Rust test (crate `mind-core`) | Notes |
|---|---|---|
| `ApplicationTests.initialization` (content map present) | `content::tests::initialization` | Registry non-empty; 12 live types populated. |
| `ApplicationTests` bootstrap assertion `build2` exists | `blocks::tests::construct_block_present` | `get_by_name(Block, "build2")`. |
| `ApplicationTests.allBlockTest` (metadata portion) | `blocks::tests::all_metadata_valid` | Every block: `health > 0`, `size ∈ 1..=16`, `offset == ((size+1)%2)*4`, `size_offset == -((size-1)/2)`, `build_time > 0`, consumer partitions consistent, `requirements` non-null. Placement/update portion owned by **07**. |
| `ApplicationTests.testSectorValidity` (content wiring portion) | `sectors::tests::presets_resolve` | Every preset’s `planet/sector/generator` resolves; `planet.sector(i).preset` round-trips; `capture_wave` defaults; generator non-null; sector index in range; hidden rule (`description == None`). Full rule/generation validity owned by **06/12**. |
| `ApplicationTests.save` / `saveLoad` (content header) | `io::content_header_roundtrip` (owned by **04**, listed here) | Names in ID order → mapper → identical IDs; applies `mod_content_name_map`. |
| `DataAssetTests.basicItem` / `basicUnit` / `noContentAddedWithError` / `noNullFieldsAllowed` | `parser_hooks::tests::provider_contract` (fake provider until **20** lands) | Add → resolve by `dp-<name>`; error case leaves registry count unchanged and no dangling name. |
| `PatcherTests.unitWeapons` / `specificArrayRequirements` reset semantics | `parser_hooks::tests::index_snapshot_restore` | `begin_patch` → mutate/append → `restore_index` + `ResetAction`s reproduce prior state; ID set identical. |
| `ApplicationTests.blockInventories`, conveyor/liquid/router tests | — | Owned by **07/08/09** (listed to avoid false ownership). |
| `load77Save`…`load152Save` | — | Owned by **04** (format) using this plan’s name mapper. |
| — (new) | `content_framework::tests::content_type_ordinals` | All 18 ordinals/names/folders vs golden. |
| — (new) | `content_framework::tests::dense_ids` | `log_content()` passes for all types after base + fake mod content. |
| — (new) | `units::tests::entity_def_table_covers_all_units` | Every unit has an `EntityDefSpec` or explicit legacy default; no unknown component tags. |
| — (new) | `erekir::tests::rebalance_applies_once` | Shared bullet referenced twice scales once; non-Erekir bullets unchanged; Serpulo-requirement turrets unchanged. |
| — (new) | `tech::tests::sector_complete_insertion` | Child of a `SectorPreset` parent gets `SectorComplete` inserted at position 0; explicit duplicate not re-added; `getDependencies` → `Research` objectives. |
| — (new) | `units::tests::mirrored_weapon_reload_doubled` | `mirror = true` weapon yields doubled reload/recoil at metadata level. |
| — (new) | `parity::tests::{names_ids_match_golden, bundle_keys, regions, field_diff}` | Mechanical audit (below) as `cargo test` with committed golden. |

`cargo test -p mind-core` must run with no Godot, no network, no JVM (golden is committed).

### 7b. Headless harness scenarios (`mind-headless`)

| Command | Behavior | Assertions |
|---|---|---|
| `content load` | Boots registry, `createBaseContent` + fake `createModContent` + `init` + `postInit`, prints per-type counts. | Exit 0; counts match golden. |
| `content ids --out parity/out_content_ids.json` | Deterministic JSON (sorted keys, LF) of type/id/name/kind (`content dump`/`audit` land in M7). | Diff vs golden `types` block is empty. |
| `content load-order-bad` | Debug scenario registering `UnitTypes` before `Items` (simulated via a test-only loader permutation). | Must fail with the `RegistryEpoch` load-order assertion naming the missing `ItemId`. |
| `content audit --golden … --manifest … --bundle … --out …` | Runs all mechanical checks (names/IDs/counts vs golden; bundle `.name` keys; region expectations; tech-tree structure; field diff; dangling refs). | Exit 0 on full parity; nonzero + report otherwise. CI gate. |
| `content bench --runs 20 --json` | Measures `createBaseContent` + `init` + `postInit` (excludes asset loading). | Median ≤ 200 ms release on the dev machine; result recorded in the plan Changelog. |

### 7c. MCP playtest scenario (concrete, open-godot-mcp)

1. `godot_health check` → `{ok:true}`.
2. `godot_game play` with the plan-00 spine scene (`res://scenes/dev/spine.tscn`); wait for `godot_log` to show `content loaded`.
3. `godot_exec eval 'MindCore.content_counts()'` → dictionary; assert `item == 22`, `block == 418`, `unit == 65`, `sector == 46`, `status == 23`, `planet == 7`, `unitStance == 30` (golden numbers).
4. `godot_exec eval 'MindCore.content_list("item").size()'` → `22`; `godot_exec eval 'MindCore.content_list("item")[0]'` → `"copper"`.
5. `godot_runtime_state inspect /root/Spine/StateInspector/ContentList` → item count equals 22; `godot_screenshot` (inspector showing the content counts/list) saved and attached to the plan evidence.
6. `godot_log errors` → empty; `godot_game stop`.
7. Optional UI list check: open the dev inspector’s `Content` tab, select `block`, verify the first/last names (`air`, last golden block name) after scrolling.

### 7d. Performance budget & measurement

| Metric | Budget | Method |
|---|---|---|
| Base content load (`createBaseContent` + `init` + `postInit`, in-memory bundle) | ≤ 200 ms, release, median of 20 | `mind-headless content bench --runs 20 --json` |
| Content audit (golden + bundle + manifest) | ≤ 2 s | `content audit`, timed in CI |
| Registry lookup (`get_by_id`, `get_by_name`) | ≤ 50 ns/op average; zero allocations | `content bench --lookup 1_000_000` (criterion-style loop in harness) |
| Per-tick content access | zero allocations, no registry mutation | Debug counter assert in `content_access` test; enforced by plan 05 tick benchmark |
| Registry memory (no assets) | ≤ 4 MiB | `--report-memory` from the harness allocator counter |

Regressions block the P1 gate (HIGH_LEVEL_PLAN §7.4).

### 7e. Exit criteria checklist

- [x] `ContentType` ordinals/names/folders match golden (`content_type_ordinals`).
- [x] Dense-ID invariant passes for all 12 live types (`log_content`).
- [ ] Per-type counts and ordered names match golden for every type (M2/M4/M5 ledgers show 0 unported).
- [ ] Tech trees: node count, parent/depth, requirement stacks, objective order match golden for Serpulo and Erekir.
- [ ] `ErekirTechTree.rebalance()` test green (scale-once guard, requirement filter). *(M2: guard green; requirement filter awaits M3/M5 units/turrets.)*
- [ ] Bundle audit: 0 missing `<type>.<name>.name` keys; localized names equal golden.
- [ ] Region audit: 0 missing required regions (soft warnings listed and triaged).
- [ ] Mod/patch contract tests green with fake provider; hand-off note delivered to plan 20.
- [x] `content load-order-bad` fails as expected.
- [ ] MCP inspector scenario passes with screenshot evidence; `godot_log errors` empty.
- [ ] Perf budgets met and recorded (median load ≤ 200 ms).
- [x] `cargo clippy -p mind-core -- -D warnings` clean; no `unwrap()` on runtime content paths.
- [ ] Ledgers, golden, audit report, and this plan’s Changelog committed.

---

## 8. Risks & open decisions

| # | Risk / decision | Default taken | Status |
|---|---|---|---|
| R1 | Golden dump provenance: generating `golden_content.json` requires a one-off JVM run (JDK 17 + upstream `:core` classpath) on the dev machine. | Ship `parity/java/DumpContent.java`; run manually per upstream content change; commit the JSON. It never writes into the Mindustry checkout. Fallback if no JVM: source-order parser + bundle/asset cross-check (weaker, flagged). | **NEEDS USER DECISION** (is a one-off JVM parity run acceptable?) |
| R2 | ObjectSet iteration order differs from Java. | Use insertion-ordered sets everywhere; sort where upstream sorts; UI lists re-sort. Any visible ordering mismatch is a bug against golden. | Resolved by design |
| R3 | Blocks.java split across behavior vs metadata may cause churn when plan 07 lands. | Contract: plan 07 reads `BlockDef` and never re-registers; behavior side tables keyed by `BlockId` live in 07. Orchestrator to reconcile field ownership at plan-07 kickoff. | Needs cross-plan reconciliation |
| R4 | `BulletDef` shape must anticipate plan 10 (behavior) and plan 20 (JSON `type` field). | Ship data-faithful fields + `kind` tag; plan 10 extends via behavior side table, not by editing records. Reconcile with 10 before both ship. | Needs cross-plan reconciliation |
| R5 | `EntityDefSpec` component vocabulary must match plan 11’s component model exactly. | Freeze the vocabulary from `entities/comp/*` names at M5; plan 11 may append, never rename (ABI for saves/JSON). Reconcile with 11. | Needs cross-plan reconciliation |
| R6 | `EffectId`/`EffectMeta` minimal table could conflict with plan 17’s effect registry. | Plan 17 owns the full registry; plan 02’s table is a pre-registration seed with identical names. Reconcile with 17. | Needs cross-plan reconciliation |
| R7 | Planet JSON `getData().presets` sector remap is unavailable until plan 12. | `SectorRemapProvider` trait with identity default; plan 12 supplies. | Deferred, flagged |
| R8 | `Loadouts` base64 blobs are committed strings; if plan 12’s decoder rejects one, parity breaks. | Treat base64 as opaque; add decode assertion test when plan 12 lands (`12` owns). | Deferred |
| R9 | Mod script/JAR content cannot run (OD1). | JSON data mods only via plan 20; registry supports `ContentParserHook` data. | Locked (HIGH_LEVEL_PLAN §10 OD1) |
| R10 | Rust borrow ergonomics for `link()`/cross-content mutation could be misused later. | `link()` is an explicit, documented pass; debug asserts forbid mutation outside it during sweeps. | Resolved by design |
| R11 | Bundle must load before content; plan 03 ordering could regress. | Startup-order invariant test in plan 00/03 + a runtime `debug_assert(bundle_loaded)` in `create_base_content`. | Cross-plan note |

---

## 9. References

- `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0, §2, §4, §5, §6–§9)
- `mindustry-godot/PRELIMINARY_PLAN.md`
- Mindustry AGENTS.md: `Mindustry/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/AGENTS.md`, `content/AGENTS.md`, `ctype/AGENTS.md`, `type/AGENTS.md`, `world/AGENTS.md`, `entities/AGENTS.md`, `annotations/AGENTS.md`, `core/AGENTS.md`, `mod/AGENTS.md`, `io/AGENTS.md`, `ai/AGENTS.md`, `tests/AGENTS.md`
- Registries: `Mindustry/core/src/mindustry/content/{Blocks,UnitTypes,Items,Liquids,StatusEffects,Bullets,Fx,Weathers,Planets,SectorPresets,Loadouts,TeamEntries,TechTree,SerpuloTechTree,ErekirTechTree}.java`
- Bases/loaders: `Mindustry/core/src/mindustry/ctype/{Content,ContentType,MappableContent,UnlockableContent}.java`, `core/ContentLoader.java`
- Types: `Mindustry/core/src/mindustry/type/*.java`, `type/{unit,weapons,weather}/*.java`, `world/Block.java`, `entities/bullet/BulletType.java`, `ai/{UnitCommand,UnitStance,ItemUnitStance}.java`
- Tests/format inputs: `Mindustry/tests/src/test/java/{ApplicationTests,DataAssetTests,PatcherTests}.java`, `Mindustry/core/src/mindustry/io/SaveFileReader.java`, `Mindustry/core/src/mindustry/mod/{ContentParser,DataPatcher,DataManager}.java`, `Mindustry/core/src/mindustry/mod/data/*.java`
- Sibling plans: `00_FOUNDATION_IMPLEMENTATION_PLAN.md`, `03_ASSETS_IMPLEMENTATION_PLAN.md`, `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md`, `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md`, `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md`, `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md`, `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `17_FX_PARTS_IMPLEMENTATION_PLAN.md`, `20_MODS_IMPLEMENTATION_PLAN.md`

## Changelog

- 2026-10-01 — Plan authored (framework scope locked, parity-audit strategy defined, sibling interfaces specified). No milestones executed.
- 2026-10-01 — **M1 complete** (lane 02, branch `lane/02-content`). Delivered `mind-core/src/content/**`: `ContentType` (18 variants exact order incl. `_UNUSED`), `ContentKind`, `ContentRef`, `ContentId<T>` + aliases, `Content`/`Mappable`/`Unlockable` traits + `UnlockFields`, `ContentRegistry` (dense-ID assignment, duplicate-name rollback, lifecycle sweeps + `link()` pass, `log_content`, index snapshot/restore, `remove`/`remove_last`, `TemporaryMapper` seam), `NameMaps`/`transform_name`/`mod_content_name_map`, `BundleView`/`MemoryBundle`, `UnlockStore`/`MemoryUnlockStore`, `parser_hooks` traits, `Category`, stacks/seqs, and the vanilla `Items` (22), `Liquids` (11), `StatusEffects` (23), `Bullets` (6) registries plus `fx_meta` (267 `Fx` names/lifetimes, generated from source). The P0 `Blocks` placeholder (`air=0`, `stone-wall=1`) moved to `content/registries/blocks.rs` unchanged; M3 replaces it.
  - **Verification (verbatim):** `cargo fmt --all -- --check` clean; `cargo clippy -p mind-core -p mind-headless --all-targets -- -D warnings` clean; `cargo test -p mind-core` → **49 passed; 0 failed** (incl. `content_framework::content_type_ordinals`, `content_framework::dense_ids`, `items::matches_golden`, `liquids::gas_post_init`, `statuses::opposites_symmetric`, `statuses::affinity_transition_tables`, `bullets::damage_lightning_copy_flags`).
  - **Harness:** `content load` → item 22, liquid 11, status 23, bullet 6 (blocks/units/weather/sector/planet/team/command/stance 0 until M2/M3/M5); `content ids --out` writes the deterministic `types` block; `content bench --runs 5 --json` median **0.50 ms** debug (budget 200 ms release, `within_budget: true`).
  - **P0 goldens unchanged (no re-record needed):** `spine_place_break` `e53c9277bb8c28d1` pass; `spine_determinism` `e435247bbe23afb1` pass; `spine_many_commands` `faec40ccbe6ff9d8` pass. Plan-00 cross-reference note: block IDs are untouched at M1; re-recording is deferred to M3 when `Blocks.java` is ported.
  - **JVM/golden status:** no JDK on Windows or WSL and no install performed, so `parity/java/DumpContent.java` and `parity/golden_content.json` were **not** generated (NUD-10). All M1 goldens are source-derived constant tables; the committed JVM golden remains an **M7 blocker**.
  - **Recorded deviations (plan text updated in this commit):** `ModContentProvider` drops the plan-20 `ModSet` parameter until M6 reconciliation (§3.6); `TransitionSpec` collapsed to `Opposite | Affinity(AffinityTransition)` to represent composite handlers without data loss (§3.8); harness subcommand is `content ids` (M7 adds `content dump`/`audit`, §7b); `content/color.rs` added for `Rgba`.
  - **Blockers/risks:** JVM golden (M7); `UnitType`/`Planet`/etc. marker types in `id.rs` are placeholders that M2/M5 re-point to real records; blocks stay on the P0 placeholder until M3.
- 2026-10-01 — **M2 complete** (lane 02). Ported `UnitCommands` (10), `TeamEntries` (0 by design), `UnitStances` (8 core + one `ItemUnitStance` per item = 30, `load_after_mods`), `Weathers` (6, `snowing`/`Time.toMinutes`), `Planets` (7 incl. `make_asteroid`, sector grids `10·3^n+2`, kind tags for generators/meshes/rules), `SectorPresets` (46, Java `%` + `-1→0` wrap, `SectorRemapProvider` identity seam, `SectorDifficulty`), `Loadouts` (4 raw base64), `TechTree`/`TechNode`/`TechStore`/`TechTreeBuilder` with materialized `SectorComplete` insertion, inherited research-cost multipliers, `round_to_10`, objective data, and the Serpulo (223 nodes) + Erekir (152 nodes) trees verbatim. Tech-tree data is a mechanical conversion of the upstream source; the one-off converter is committed at `parity/tools/gen_trees.py`.
  - **Verification (verbatim):** `cargo fmt --all -- --check` clean; `cargo clippy -p mind-core -p mind-headless --all-targets -- -D warnings` clean; `cargo test -p mind-core` → **62 passed; 0 failed**, incl. `commands::count_and_ids`, `stances::incompatible_bits`/`load_order`, `sectors::presets_resolve`, `tech::sector_complete_insertion`, `tech::requirements_rounding`, `ekir::rebalance_applies_once`, `registries::load_order_bad`, `tech::vanilla_tree_node_counts`.
  - **Harness:** `content load` → item 22, bullet 6, liquid 11, status 23, weather 6, sector 46, planet 7, team 0, unitCommand 10, unitStance 30 (blocks/units 0 until M3/M5); `content ids --out` now lists all 12 live types; `content load-order-bad` → `content load-order-bad: failed as expected: content name 'wet' is unknown`; `content bench --runs 10 --json` median **3.25 ms** debug (budget 200 ms release) — the trees add ~2.7 ms debug vs M1.
  - **P0 goldens unchanged (no re-record needed):** `spine_place_break` `e53c9277bb8c28d1`, `spine_determinism` `e435247bbe23afb1`, `spine_many_commands` `faec40ccbe6ff9d8` all pass.
  - **Recorded deviations:** `LoadoutDef` is a named side table, not a content ID space (`ContentType.loadout_UNUSED` is historical; upstream `Loadouts` stores `Schematic`s); `TechNode.content` is `Option<ContentRef>` plus `content_name` so the verbatim trees can be built before M3/M5 — `TechTreeBuilder` reports unresolved names (`serpulo`: 49 resolved / 174 unresolved, `erekir`: 35 / 117), which M3/M5 must drive to zero; `ErekirTechTree.rebalance()` is a no-op pending units/blocks, with the tested `rebalance_bullet` scale-once guard ready for M3/M5; `PlanetDef.sector_capture_replacements`/`unlocked_on_land`/`default_core` hold block names until M3.
  - **Blockers/risks:** JVM golden still the M7 blocker; M3/M5 must assert `TechTreeBuildReport::missing == []`; `sector-shield` propagation has no vanilla users to exercise (kept for parity).
