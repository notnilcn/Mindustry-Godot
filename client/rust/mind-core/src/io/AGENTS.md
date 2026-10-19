# AGENTS.md — mind-core/src/io (persistence & serialization)

`io/` owns persistence and serialization: the `MGRS` save container, big-endian wire/`TypeIO`
codecs, entity revision IO, `JsonIO`, settings, save slots and map headers/previews. It ports
upstream `mindustry.io` (`SaveIO`, `SaveVersion`, `SaveFileReader`, `TypeIO`, `JsonIO`, `MapIO`),
`game/Saves.java` and Arc `Settings`/`Fi`; `MSAV` import is gated by the default-off `msav-import`
feature. Untrusted-input paths return [`IoError`]; crate-wide rules: [`mind-core/AGENTS.md`](../../AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | Module root; re-exports `IoError`, `FileSystem`/`MockFs`/`NativeFs`/`Paths`, `SaveIo`, `SimIoHandler`, `WireReader`/`WireWriter`; defines `StringMap` (ordered) and `IoResult`. |
| `wire.rs` | Big-endian `Writes`/`Reads` primitives (`WireReader`/`WireWriter`), strings (`u16` byte length + UTF-8), string maps, byte caps. |
| `error.rs` | `IoError` (`thiserror`) for every save/map/settings/serialization failure; upstream message text kept where tools/tests match on it. |
| `fs.rs` | `FileSystem` trait, `NativeFs`, deterministic `MockFs` with fault injection, `Paths` data-directory layout, atomic `write_atomic` (tmp + sync + rename). |
| `typeio/` | `TypeIO` tagged-object codec: `write_object`/`read_object`/`read_object_safe`, frozen tags, `ContentMapper`, array/string caps. |
| `save/` | The `MGRS` engine: `SaveIo` read/write/backup/meta plus `chunk`, `meta`, `options`, `state`, `slot`, `version` and the append-only `versions/` (`v1.rs`, `v2.rs`). |
| `entity/` | Entity codec contract and revisions: `EntityCodec`, `FieldDesc`/`FieldFlag`, `registry`, `idmap`, `class_ids`, `idfile`, `revisions`. |
| `json/` | `JsonIO` (`JsonIo`), `Rules`/`GameStats`/`MapLocales`/`SectorInfo`, content serde, objectives. |
| `map/` | `MapIo` (create/write/load/is_image/generate_preview), `MapHeader`, preview pixels + `encode_png`/`decode_png`. |
| `settings.rs` | `SettingsStore` (`MGST` file), typed `SettingValue`, upstream key names, atomic and debounced flush. |
| `data_archive.rs` | Data export/import zip (`SettingsMenuDialog.exportData`/`importData`): settings + saves/maps/mods/schematics/cache trees, `settings.bin` validation, zip-slip/entry caps. |
| `sim_io.rs` | `SimIoHandler`, the live-sim `IoHandler` fulfilling `IoSet::Capture`/`Apply` save/load requests; `LiveEntitySource`. |
| `java_entities.rs` | One-way upstream `MSAV` entity decoder (unit, fire, puddle chunks) keyed by the Java class ids. |
| `legacy.rs` | `open_legacy`: handles the upstream `MSAV` magic and returns the standard unknown-version error. |

## Native `MGRS` save container

`SaveIo::write` emits the raw 4-byte magic `MAGIC` (`MGRS`), then zlib-deflates everything after it:
a big-endian `u32` format version plus named, length-prefixed regions in `REGION_ORDER` (`meta`,
`patches`, `content`, `map`, `entities`, `markers`, `custom`). A region is `u8` name length + UTF-8
name + `u32` payload length + payload; readers tolerate unknown names and missing trailing regions,
and every handler consumes its payload exactly (`MAX_REGION_BYTES` 128 MiB, `MAX_DECOMPRESSED_BYTES`
512 MiB). `SaveIo::save` writes `<file>.tmp`, syncs and renames over the target, rotating an existing
file to `<stem>-backup.msav` first and restoring it on failure; `load`/`get_meta`/`is_save_valid`
fall back to that backup, and `load_bytes`/`get_meta_bytes` open an in-memory container.

## Key types

- Wire/container: `WireReader`, `WireWriter`, `StringMap`, `IoResult<T>`; `SaveIo`, `SaveWriter`,
  `SaveReader`, `SaveScratch`, `SaveMeta`, `WriteContext`, `SaveReadState`, `SaveOptions`;
  `save::version::{SaveVersion, version_array, get_writer, current_writer}`.
- Object codec: `typeio::TypeValue`, `EntityRef`, `read_object_safe` (the untrusted-input entry
  point), `typeio::tags` (frozen tag table), `typeio::mapper::{ContentMapper, TemporaryMapperGuard}`.
- Entity IO: `entity::EntityCodec` (`NAME`, `CLASS_ID`, `SERIALIZE`, `SYNC`, `NEWEST_REVISION`),
  `EntityDefMeta`, `FieldDesc`, `FieldFlag`, `EntityIdMap`, `DuplicateIdTracker`; `JsonIo`;
  `MapHeader`, `MapIo`, `SaveSlot`, `SettingsStore`, `SettingValue`.

## Boundaries

- [`../maps/AGENTS.md`](../maps/AGENTS.md) owns the `Maps` registry, discovery/filters, generators,
  shuffle and the on-disk preview cache; `io::map` owns only `MapIo` read/write, `MapHeader` and raw
  preview pixels/PNG, which `maps::Map` calls down into.
- [`../world/AGENTS.md`](../world/AGENTS.md) owns the live `WorldGrid`, tile mutation and
  `world::building_io::BuildingCodec`. `world::map_source::EcsMapSource` implements the `MapSource`
  view and `io::sim_io::LiveEntitySource` the entity half; only `save::fixture` walks a grid directly.
- [`../game/AGENTS.md`](../game/AGENTS.md) owns save-slot policy (autosave, playtime, sector remap);
  `io::save::slot` provides only the file/meta/preview primitives those policies call.

## Invariants

- **Untrusted bytes error, never panic.** Truncated/corrupt streams flow through `IoError`; panics
  are reserved for debug invariants. The temporary content mapper is cleared on every load attempt,
  including failures (`TemporaryMapperGuard`/the `SaveIo::load_bytes` epilogue).
- **Ordered maps on serialized paths.** `StringMap` is an `IndexMap`; insertion order is the
  serialized order, and `serde_json` uses `preserve_order`. Entity/ID maps iterate in insertion order.
- **Append-only formats.** Format versions, region names, entity class IDs
  (`entity_class_ids.toml` -> `class_ids.rs`), revision manifests (`revisions/<NAME>/<N>.json`) and
  the content fallback table are appended, never renumbered or deleted.

## Rules

- Entity field order is serialization order: append `#[entity(since = N)]` fields, never reorder.
  Renames require a revision bump plus an `aliases` entry; `revisions::check_def` compares field
  names and types positionally.
- New regions append to `REGION_ORDER`; new native versions add `save/versions/vN.rs` and append to
  `version_array()`, overriding only the changed region hooks.
- Settings keys follow the upstream names (`save-<n>-name`, `save-<n>-autosave`, `saveinterval`,
  `last-sector-save`); a corrupt `settings.bin` falls back to defaults with a warning.
- `MSAV` is gated by the default-off `msav-import` feature. With it on, `SaveIo::load_bytes` sniffs
  via `legacy::sniffs_as_legacy`, inflates the stream, parses the 8-byte header and walks the
  version-gated regions with `v1::read_legacy_regions`; with it off, `MSAV` input fails with the
  standard unknown-version message via `legacy::open_legacy`.

## Verification

Round-trip tests compare byte-for-byte and checksum equality: `typeio::tests::all_tags_roundtrip`,
`wire::tests::primitive_roundtrip_big_endian`, the settings round-trips, and
`save::fixture::tests::fixture_save_load_roundtrip`, which reloads a `FixtureWorld` and asserts the
xxh3 `checksum_hex`, team plans and tile data match. `map::tests::create_map_reads_meta_only` covers
meta-only listing; `entity::tests::revision_check_add_field` proves the append-only manifest chain.

Run from `client/rust/` (or add `--manifest-path client/rust/Cargo.toml` from the repo root):

```bash
cargo test -p mind-core io                        # IO unit + round-trip tests
cargo test -p mind-core --features msav-import io # exercise the MSAV boundary gate

# Headless fixtures over the same engine
cargo run -p mind-headless -- io roundtrip
cargo run -p mind-headless -- io check-revisions
```
