# AGENTS.md — mind-core/src/content (content framework & registries)

The content framework for `mind-core`: the `Content` record model, dense per-type content IDs, the name/unlock tables, the tech-tree graph and the generated vanilla registries (items, blocks, units, bullets, liquids, statuses, planets, sectors, weathers, commands, stances, teams and loadouts). `create_base_content` boots every registry in `ContentLoader.createBaseContent()` order and returns a `ContentRegistry`. Read the root [`AGENTS.md`](../../../../../AGENTS.md) first; the crate-level map is `mind-core/AGENTS.md` (`../../AGENTS.md`).

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | Re-exports, `ContentType`/`ContentKind`, `ContentRef`, `ContentError` |
| `id.rs` | `ContentId<T>` (phantom `u16`) and typed aliases (`ItemId`, `BlockId`, `BulletId`, `LiquidId`, `StatusId`, `UnitTypeId`, `WeatherId`, `SectorId`, `PlanetId`, `TeamEntryId`, `UnitCommandId`, `UnitStanceId`) |
| `ctype.rs` | `Content`/`Mappable`/`Unlockable` traits, `UnlockFields`, `ModContentInfo`, `ModId`, `ErrorContent` |
| `load.rs` | `ContentRegistry`, dense-ID assignment, lifecycle sweeps, `link`, `TemporaryMapper`, `MappedId`, `content_counts` |
| `names.rs` | `NameMaps` (per-type + global), `transform_name` mod prefixing, save-name fallbacks |
| `category.rs` | `Category` block place-menu enum (`prev`/`next` cycling) |
| `color.rs` | `Rgba` value type (`from_hex`, `from_rgba8888`) |
| `bundle.rs` | `BundleView` trait + `MemoryBundle` |
| `settings_store.rs` | `UnlockStore` trait + `MemoryUnlockStore` |
| `snapshot.rs` | `RegistryIndexSnapshot` index capture/restore |
| `stacks.rs` | `ItemStack`/`LiquidStack`/`PayloadStack`, `ItemSeq` |
| `tech/` | `TechStore`, `TechNode`, `TechTreeBuilder`, `ObjectiveSpec`; `serpulo.rs` and `ekir.rs` node data |
| `parity.rs` | `GoldenContent`, `AssetManifest`, `audit`, `dump_golden` |
| `parser_hooks.rs` | Mod/data-patch provider and error-sink traits |
| `registries/` | Vanilla content modules, `create_base_content`, `create_base_content_bad_order` |
| `registries/blocks/` | `BlockDef`/`BlockSpec`, `BlockKind`/`BlockFlag`/`BlockGroup`/`BuildVisibility`, `Consume`, `Blocks`, `BlockSink`, one wave module per upstream region |
| `registries/units/` | `UnitTypeDef`/`UnitSpec`, `UnitKind`, `EntityDefSpec`, weapons/abilities/parts, `UnitSink`, `standard`/`erekir`/`special` waves |
| `registries/items.rs`, `liquids.rs`, `bullets.rs`, `statuses.rs` | Item/liquid/bullet/status records and load waves |
| `registries/planets.rs`, `sectors.rs`, `weathers.rs` | Planet/sector/weather records and load waves |
| `registries/commands.rs`, `stances.rs`, `teams.rs`, `loadouts.rs` | Unit commands/stances, team and starting-loadout tables |
| `registries/fx_meta.rs`, `sound_meta.rs`, `pal.rs` | Effect/sound name tables (not content ID spaces) and `Pal` color constants |

## Key types

- `Content` is the root trait: `const TYPE: ContentType`, `content_id`/`set_content_id`, `minfo`, `kind_name` and defaulted hooks `init_self`, `post_init`, `load_icon`, `load`, `after_patch`, `remove_content`. `Mappable` adds `name()`; `Unlockable` adds `unlock()`/`unlock_mut()` over `UnlockFields` (localized name/description, database category/tag, tech node, `unlocked` flag keyed `<name>-unlocked`).
- `ContentId<T>` is `#[repr(transparent)]` over `u16`; the phantom tag keeps id spaces from mixing. `.raw()`/`.get()` return the number and `.index()` casts to the vector index.
- `ContentRegistry` owns one `Vec<Record>` per live type plus `NameMaps`. Per-type accessors (`add_item`, `items`, `item`, `item_mut`, `item_by_name`, and the same for blocks/liquids/statuses/units/weathers/sectors/planets/teams/commands/stances) and `add_bullet`/`bullet` come from the `mappable_accessors!` macro.
- Name/id lookup: `get_by_name(type_, name)`, `by_name(name)` (global), `get_by_id(type_, id)`, and convenience `item_id`/`block_id`/`unit_id`/`liquid_id`/`status_id`/`planet_id`. `entries` exposes ordered id/name/kind rows and `type_len` reports per-type counts.
- `ContentRef { type_, id }` is the type-erased handle for cross-registry references; `ContentType::{name, folder, kind, ordinal}` and `ContentKind` mirror the upstream enum for bundles and audit.
- `create_base_content` runs `commands → teams → items → stances → statuses → liquids → bullets → units → blocks → loadouts → weathers → planets → sectors`, then builds the Serpulo and Erekir tech trees. `create_base_content_bad_order` is the negative load-order fixture.

## Invariants

- Construction order defines IDs. IDs are dense `0..len` per type, append-only and never reordered: `register_mappable`/`register_content` assign `records.len()`, and `log_content()` raises `ContentError::OutOfOrderIds` when an entry id differs from its index.
- `ContentType` ordinals, content names, bundle keys and sprite region names are parity/mod ABI and are never renamed. `_UNUSED` variants (`MechUnused`, `EffectUnused`, `LoadoutUnused`, `TypeIdUnused`, `AmmoUnused`) reserve historical ordinals and are never repurposed.
- Every vanilla record is appended in upstream `load()` order, so ids form an upstream prefix. `Blocks::assert_invariants` pins `air` at id `0` and `stone-wall` at id `80`.
- Per-type names are unique (`ContentError::DuplicateName`); the global name map is last-registration-wins. `remove` shifts later records without rewriting their stored ids, so the dense invariant is restored only by recreating content or `restore_index`.
- Maps use `IndexMap`; iteration is insertion-ordered and deterministic. Registry access on the hot path neither mutates the registry nor allocates. The id space is `u16` (`ContentError::IdSpaceExhausted`).

## Rules

- Add a record by calling the kind's `add_*` from its wave, preserving upstream declaration order; never insert, reorder or rename existing names, bundle keys or sprite regions.
- Register a new kind only by appending a `ContentType` variant with the next ordinal (never rearranging), then give it a `Content` record, an id alias, an accessor and a load wave.
- Mods extend registries through `set_current_mod` + `transform_name` (`<mod>-<name>` prefix) or `create_mod_content(&mut dyn ModContentProvider)`; per-content failures route through `ModErrorSink`.
- Cross-content references are typed `ContentId<T>` values or `ContentRef`s, never Rust pointers; resolves go through the registry accessors. Lifecycle is `init`/`link` then `post_init`/`post_init_link`, guarded by the registry so each sweep runs once.

## Verification

```bash
cargo test -p mind-core
cargo clippy -p mind-core
cargo fmt -p mind-core -- --check
```

The `content`, `content::registries`, `content::tech` and `content::parity` tests assert golden item/liquid/unit fields, dense ids, name maps, tech-node counts and the parity audit.
