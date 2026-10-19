# AGENTS.md — mind-core/src/maps (map registry, generation, previews)

`maps` owns the map registry (`Maps`/`Map`/`MapSources`), built-in and custom discovery,
save/import/remove, shuffle, generation filters, terrain/planet generators, `MapFixer`, map locales,
sector damage and the preview cache. It does not own `MapIo`/PNG codecs (`../io/AGENTS.md`), live
`WorldGrid`/tile mutation or `EcsMapSource` (`../world/AGENTS.md`), campaign planet/sector runtime
(`../game/AGENTS.md`), or editor dialogs and the `PreviewPipeline` pixel half (`../editor/preview.rs`,
`mind-gdext`). Parent rules: [`mind-core/AGENTS.md`](../../AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | `Maps` registry (`load`, `reload`, `save_map`, `import_map`, `remove_map`, `find_file`, `get_next_map`), `MapSources`, `DEFAULT_MAP_NAMES` (18), `PVP_MAPS` (3), `scan_map_source`, `write_map_source[_with_patches]`. |
| `map.rs` | `Map` metadata and behavior: tags, `rules`/`apply_rules`, `filters`, `map_view`, `compare_to`, preview/cache paths; `ENV_SCORCHING`, `UNKNOWN`. |
| `error.rs` | `MapException` plus `MapError` (`EmptyName`, `NoCore`, `NotFound`, IO carry-through). |
| `preview.rs` | Preview/cache paths, `PreviewCache` (`CACHE_VERSION` 0 = `spawns` + `teams`), `PreviewQueue`, cache read/write. |
| `shuffle.rs` | `GameMode`, `ShuffleMode`, `MapProvider`, `is_pvp`/`default_valid`, seeded Fisher-Yates. |
| `fix.rs` | `MapFixer` port (`fix_file`/`fix_dir`), `FixReport`, `-backup.msav` skipping. |
| `locales.rs` | Map-locale `.properties`/JSON helpers, `PropertyStatus`, `MapLocaleView`. |
| `sector_damage.rs` | `sector_damage::apply` over a caller-owned `DamageBuilding` list + `Tiles`; `DamageFx` seam. |
| `generators/` | `WorldGenerator` trait; `simplex`, `basic` (passes/median/path helpers), `planet` (hex projection/`StructureCtx`), `file_map` (asset-tree candidates + core check), `base` (enemy base/ruins), `astar`. |
| `filters/` | `GenerateFilter` buffered/unbuffered engine, 15 vanilla classes, default scatter+ore stack, `genfilters` JSON, `FilterOption` editor descriptors. |
| `planet/` | Vanilla generators `serpulo`, `erekir`, `tantros`, `asteroid`; `generator_for`, `generate_sector[_with_seed]`, `sector_size`, `PlanetColorTable`. |

## Data & determinism

- **Built-ins are parity ABI.** `DEFAULT_MAP_NAMES` is the 18-name discovery list (`maze` … `passage`);
  `PVP_MAPS` is `veins`, `glacier`, `passage`. File stems, `name` tags, PvP membership and
  `Map::compare_to` order (workshop, custom, non-PvP before PvP, color-stripped name) drive map
  discovery, the map dialog and the headless server's known-map lookup.
- **Assets.** Registry built-ins live in `assets/maps/default/<name>.msav`; only the listed names
  are discovered (other files in that directory are ignored). Designed campaign maps live in
  `assets/maps/serpulo/` and `assets/maps/erekir/`; preset sectors load
  `assets/maps/<owner>/<preset>.msav` (`mind-gdext::load_preset_map_grid`), with hidden sector maps
  under `assets/maps/serpulo/hidden/`. `FileMapGenerator::resolve_candidate` is the asset-tree
  candidate order (`maps/<planet>/<name>.msav`, bare, then mod-prefix-stripped). The Godot build
  wires `MapSources { builtin_dir: assets/maps, custom_dir: user://maps, use_default_folder: true }`
  (`mind-gdext/src/assets/loader.rs`).
- **Custom maps.** `Maps::load` walks `custom_dir` recursively for `.msav`, warning and skipping
  corrupt files; `Map::validate_name` rejects an empty `name`. `Maps::load_internal_map` reads
  `maps/<name>.msav` from the asset tree unregistered; `remove_map` deletes file, preview and cache.
- **Write path.** `Maps::save_map[_with_patches]` calls `write_map_source[_with_patches]` (map tags
  win over base tags; `MapIo` writes the `map` region), scans `spawns`/`teams`, writes the cache
  (failure is non-fatal) and re-sorts. Teams come only from `MapSource::core_team`, so tile-only
  sources leave `Map.teams` empty; use an ECS-backed source when teams matter. `import_map` rejects
  images and copies to a sanitized free filename. Editor `save_editor_map` calls the same writer.
- **Preview cache.** `<previews>/<stem>_v4.png` and `<stem>-cache_v2.dat` (workshop:
  `<parent>-workshop-cache.dat`). `_v4` is a deliberate deviation from Java's `_v2`; each bump
  regenerates previews made with an older pixel path (`_v3` for the J02 T3 colours, `_v4` for
  `Block.minimapColor(tile)` tile tints). `read_cache` failure leaves map defaults; the caller
  re-queues through `PreviewQueue` (deduplicated, batched with `take_up_to`).
- **RNG.** Shuffle takes `&mut JavaRandom` (exact `java.util.Random`) and uses `next_int_bound`;
  filters/ruins draw from `SimRng` `RngStream::MapGen`; `generators::base` uses `ArcRand` seeded by
  `sector.id`; planet generators reseed from `sector.id + seedOffset + baseSeed`. Selection and
  generation are replay-stable only when callers thread the seeded RNG.
- **Rules overlay.** `Map::rules` reads the `rules` tag over the caller's base, flips a scorching
  `serpulo` map to `erekir`, defaults an empty planet to `serpulo`, and fills empty `spawns` from
  `MapGenHooks::wave_groups`; `Map::apply_rules` puts the gamemode preset underneath. Load through
  these rather than constructing `Rules` directly.
- **Filters fail soft.** `read_filters("")` returns the default stack (a `ScatterFilter` per
  decorated floor, then `OreFilter`s); unknown class tags are warned and skipped and parse errors
  fall back to the default stack. A `build` tag below 83 (but not -1) with no `genfilters` intentionally
  gets an empty stack. `normalize_filter_numbers` restores `i32` fields after GDScript's float JSON.

## Verification

Run from `client/rust/`.

```bash
cargo test -p mind-core maps::                        # registry/map/filters/generators/planet/preview tests
cargo test -p mind-core --features msav-import maps:: # also loads committed MSAV built-ins + caldera core
cargo test -p mind-headless --test maps_editor_golden # maps roundtrip + editor playtest goldens
cargo run -p mind-headless -- maps list --dir mind-core/tests/fixtures/maps --json # sorted meta listing
cargo run -p mind-headless -- maps roundtrip --json   # save -> preview -> import checksum equality
cargo run -p mind-headless -- maps save-load-save --json # editor save/load byte idempotence
cargo run -p mind-headless -- maps preview-tiles --json  # preview pixels + PNG round-trip
cargo run -p mind-headless -- maps image-roundtrip --json # PNG image-map import/export
cargo run -p mind-headless -- maps fix --dry-run --json  # MapFixer report without writing
cargo run -p mind-headless -- maps registry-shuffle --seeds 8 --json # ordering + no-repeat selection
cargo run -p mind-headless -- world gen --generator simplex --seed 7 --width 128 --height 128 --json # 07e2c98b533790cc
```

## Gotchas

- **`maps fix` mutates the target directory.** Without `--dry-run` it rewrites each fixable `.msav`
  in place through `save_editor_map`, leaving `<stem>-backup.msav` (the walker skips backups), and
  `--dir <dir>` always seeds `<dir>/hidden/fixme.msav` first. Run it on a copy, never on
  `assets/maps/**` or a live data root; committed built-ins are legacy `MSAV` and need `msav-import`.
- **Discovery is name-driven, not directory-driven.** `Maps::load` iterates `DEFAULT_MAP_NAMES`; a new
  `.msav` in `assets/maps/default/` is ignored until it joins that const (and `PVP_MAPS`). Renaming a
  stem changes PvP detection and the preview/cache file names.
- **Bad `genfilters` fail soft.** A typo in a `class` tag is skipped with a warning and a malformed
  stack falls back to the default scatter+ore stack, so generation changes silently instead of
  failing the map load. Re-run the map/planet checksums after editing filters.
- **Do not "fix" the preview filenames.** `_v4.png` (Java uses `_v2`) and `-cache_v2.dat` are
  intentional compatibility names; reverting `_v4` can pick up previews from an older colour path
  (`_v3` = pre-`useColor`; `_v4` = pre-`Block.minimapColor` tints).
- **The editor shares this write path.** `editor::maps_glue::save_editor_map` calls
  `maps::write_map_source`, and `begin_edit_map` loads through `MapIo`; verify tag/region changes with
  both `maps save-load-save` and `maps roundtrip` (plus the editor goldens), not registry tests alone.
