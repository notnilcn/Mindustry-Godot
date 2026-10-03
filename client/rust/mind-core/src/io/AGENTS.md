# AGENTS.md — mind-core/src/io (persistence & serialization)

`io/` is the Godot-free persistence and serialization layer of `mind-core`: the native `MGRS` save
container, the big-endian wire and `TypeIO` codecs, entity revision IO, `JsonIO`, settings
persistence, save slots, and map headers/previews. It ports upstream `mindustry.io` (`SaveIO`,
`SaveVersion`, `SaveFileReader`, `TypeIO`, `JsonIO`, `MapIO`), `game/Saves.java` and Arc
`Settings`/`Fi`; the upstream `MSAV` container enters only through the default-off `msav-import`
feature. Every untrusted-input path returns [`IoError`]; nothing here links Godot or tokio.
Read the root [`AGENTS.md`](../../../../../AGENTS.md) first; the crate-level map is
`mind-core/AGENTS.md` (`../../AGENTS.md`).

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | Module root; re-exports `IoError`, `FileSystem`/`MockFs`/`NativeFs`/`Paths`, `SaveIo`, `SimIoHandler`, `WireReader`/`WireWriter`; defines `StringMap` (ordered) and `IoResult`. |
| `wire.rs` | Big-endian `Writes`/`Reads` primitives (`WireReader`/`WireWriter`), strings (`u16` byte length + UTF-8), string maps, byte caps. |
| `error.rs` | `IoError` (`thiserror`) for every save/map/settings/serialization failure; upstream message text kept where tools/tests match on it. |
| `fs.rs` | `FileSystem` trait, `NativeFs`, deterministic `MockFs` with fault injection, `Paths` data-directory layout, atomic `write_atomic` (tmp + sync + rename). |
| `typeio/` | `TypeIO` tagged-object codec: `write_object`/`read_object`/`read_object_safe`, frozen tags, `ContentMapper`, array/string caps. |
| `save/` | The `MGRS` engine: `SaveIo` read/write/backup/meta, `chunk`, `meta`, `options`, `state`, `slot`, `version`, `versions/`. |
| `save/versions/` | Append-only format versions (`v1.rs`); `version_array()`, `get_writer`, `SaveV1` current writer. |
| `entity/` | Entity codec contract and revisions: `EntityCodec`, `FieldDesc`/`FieldFlag`, `registry`, `idmap`, `class_ids`, `idfile`, `revisions`. |
| `json/` | `JsonIO` (`JsonIo`), `Rules`/`GameStats`/`MapLocales`/`SectorInfo`, content serde, objectives. |
| `map/` | `MapIo` (create/write/load/is_image/generate_preview), `MapHeader`, preview pixels + `encode_png`/`decode_png`. |
| `settings.rs` | `SettingsStore` (`MGST` file), typed `SettingValue`, upstream key names, atomic and debounced flush. |
| `sim_io.rs` | `SimIoHandler`, the live-sim `IoHandler` fulfilling `IoSet::Capture`/`Apply` save/load requests. |
| `legacy.rs` | `open_legacy`: handles the upstream `MSAV` magic and returns the standard unknown-version error. |

## Native `MGRS` save container

`SaveIo::write` emits the raw 4-byte magic `MAGIC` (`MGRS`), then zlib-deflates everything after it:
a big-endian `u32` format version followed by named, length-prefixed regions in `REGION_ORDER`
(`meta`, `patches`, `content`, `map`, `entities`, `markers`, `custom`). A region is a `u8` name
length + UTF-8 name + `u32` payload length + payload; readers tolerate unknown names and missing
trailing regions, and every handler must consume its payload exactly (`require_consumed`). Caps are
`MAX_REGION_BYTES` (128 MiB) and `MAX_DECOMPRESSED_BYTES` (512 MiB, zip-bomb guard).

`SaveIo::save` writes to `<file>.tmp`, flushes and renames over the target, rotating any existing
file to `<stem>-backup.msav` first and restoring it if the write fails. `SaveIo::load`,
`get_meta` and `is_save_valid` fall back to that backup when the primary file is unreadable.
`SaveIo::load_bytes`/`get_meta_bytes` open an in-memory container through `open_native`.

## Key types

- Wire/container: `WireReader`, `WireWriter`, `StringMap`, `IoResult<T>`; `SaveIo`, `SaveWriter`,
  `SaveReader`, `SaveScratch`, `SaveMeta`, `WriteContext`, `SaveReadState`, `SaveOptions`.
- Object codec: `typeio::TypeValue`, `EntityRef`, `read_object_safe` (the untrusted-input entry
  point), `typeio::tags` (frozen tag table), `typeio::mapper::{ContentMapper, TemporaryMapperGuard}`.
- Entity IO: `entity::EntityCodec` (`NAME`, `CLASS_ID`, `SERIALIZE`, `SYNC`, `NEWEST_REVISION`),
  `EntityDefMeta`, `FieldDesc`, `FieldFlag`, `EntityIdMap`, `DuplicateIdTracker`.
- Versions: `save::version::{SaveVersion, WriteContext, version_array, get_writer, current_writer}`.
- Entity ID / JSON / maps: `EntityIdMap` and `DuplicateIdTracker`; `JsonIo`; `MapHeader`, `MapIo`,
  `SaveSlot`, `SettingsStore`, `SettingValue`.

## Invariants

- **No Godot or tokio.** No source under `mind-core` may `use godot`/`use tokio`; CI checks the
  dependency tree and greps the sources.
- **No `unwrap`/`expect` on runtime data.** Workspace lints deny `unwrap_used` and `expect_used`;
  untrusted bytes flow through `io/error.rs` `IoError` variants, never a panic. Panics are reserved
  for debug invariants. Truncated/corrupt streams must error, not abort.
- **Ordered maps on serialized paths.** `StringMap` is an `IndexMap`; insertion order is the
  serialized order, and `serde_json` is built with `preserve_order`. Entity/ID maps iterate in
  insertion order.
- **Append-only formats.** Format versions (`save/versions/`), region names, entity class IDs
  (`entity_class_ids.toml` -> `class_ids.rs`), revision manifests (`revisions/<NAME>/<N>.json`) and
  the content fallback table are appended, never renumbered, reordered or deleted; older readers
  keep their arms compiling.
- **Atomic writes and no leaked state.** Save and settings writes go through temp + sync + rename;
  the temporary content mapper is cleared on every load attempt, including failures
  (`TemporaryMapperGuard`/the `SaveIo::load_bytes` epilogue).

## Rules

- Entity field order is serialization order: append `#[entity(since = N)]` fields, never reorder.
  Renames require a revision bump plus an `aliases` entry; `revisions::check_def` compares field
  names and types positionally.
- New regions append to `REGION_ORDER`; new native versions add `save/versions/vN.rs` and append to
  `version_array()`, overriding only the changed region hooks.
- Settings keys follow the upstream names (`save-<n>-name`, `save-<n>-autosave`, `saveinterval`,
  `last-sector-save`); a corrupt `settings.bin` falls back to defaults with a warning.
- `MSAV` is gated by the default-off `msav-import` feature in `Cargo.toml`; `SaveIo::open_native`
  sniffs it via `sniffs_as_legacy` and delegates to `legacy::open_legacy`. With the feature off,
  `MSAV` input fails with the standard unknown-version message.
- Port upstream sources and cite them in file headers; new files are UTF-8/LF and GPL-3.0-only.
  Do not relax the boundary rules to land a feature.

## Verification

Round-trip tests compare byte-for-byte and checksum equality: `typeio::tests::all_tags_roundtrip`,
`wire::tests::primitive_roundtrip_big_endian`, `settings::tests::{typed_roundtrip_and_coercions,
flush_reload_equality}`, and `save::fixture::tests::fixture_save_load_roundtrip`, which writes a
`FixtureWorld`, reloads it, and asserts the xxh3 `checksum_hex`, collected team plans and tile data
match. `map::tests::create_map_reads_meta_only` and the slot/settings tests cover listing and
fallback paths; `entity::tests::revision_check_add_field` proves the append-only manifest chain.

Run from `client/rust/` (or add `--manifest-path client/rust/Cargo.toml` from the repo root):

```bash
cargo test -p mind-core io                                   # IO unit + round-trip tests
cargo test -p mind-core --features msav-import               # exercise the MSAV boundary gate
cargo clippy -p mind-core                                    # includes the unwrap/expect deny lints

# Headless fixtures over the same engine
cargo run -p mind-headless -- io roundtrip
cargo run -p mind-headless -- io check-revisions
cargo run -p mind-headless -- io check-class-ids
cargo run -p mind-headless -- io dump-meta
```
