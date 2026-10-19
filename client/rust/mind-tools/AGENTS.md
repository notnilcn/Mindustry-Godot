# AGENTS.md — mind-tools (asset generation & packing)

`mind-tools` is the **build-time, offline asset pipeline** for Mindustry-Godot: a `clap` CLI plus a
library half shared with the integration tests. It vendors the upstream `core/assets` and
`core/assets-raw` trees into this repo, generates derived sprites from content metadata, packs
everything into the runtime atlas under `assets/sprites/`, and writes the icon/sound/shader/locale
indexes. It is **never linked into the game** — nothing under `client/rust/mind-core`,
`client/rust/mind-gdext` or the server depends on it. Read the root [`AGENTS.md`](../../../AGENTS.md)
first; the Cargo-workspace map is [`client/rust/AGENTS.md`](../AGENTS.md). This file is the sole
owner of asset-pipeline internals.

## Layout

| Path | Responsibility |
|---|---|
| `src/lib.rs` | Library surface: re-exports the modules and `base_content()`, which builds the headless vanilla `ContentRegistry` (`create_base_content` + `init`/`post_init`) the generators read. |
| `src/main.rs` | The CLI (`Cli`/`Command`) and the `run_pack` stage driver. Subcommands: `migrate`, `pack`, `icons`, `sounds`, `shaders`, `determinism`, `mods classmap` (writes/verifies `parity/mod_classmap.json`). |
| `src/migrate.rs` | `migrate`: vendoring of the upstream asset trees; writes `build/assets/migration_manifest.json` and `assets/ASSET_PROVENANCE.md`; `--check` verifies against the manifest. |
| `src/staging.rs` | Stage `staging`: mirrors `assets-raw/sprites/**` into `build/assets/staging/` and applies the `ImagePacker.fixSubdirectory` flattens. |
| `src/generate/mod.rs` | `GenCtx`, `run_passes`, and `PASS_ORDER` — the generator driver run in upstream pass order. |
| `src/pack_atlas.rs` | `PackAtlas`, the in-memory fake atlas over the staging tree (`enumerate`/`get`/`save`/`replace`/`delete`) that generators mutate. |
| `src/pack_pipeline.rs` | Stages `pack`/`pack-fallback`: pass discovery over `pack.json` configs, page rendering, `sprites.atlas.json`; `write_asset_manifest`, `write_region_names`. |
| `src/antialias.rs` | Stages `move-ui-icons` and `antialias` (upstream AA skip predicate, threaded in-place rewrite). |
| `src/shaders.rs` | `shaders build`/`check`: copies ported `.gdshader` files from `client/shaders/` into `assets/shaders/godot/`, writes `assets/shaders/shader.index.json`, reports forward/reverse uniform drift. |
| `src/sounds.rs` | `sounds index`: writes `assets/sounds.index.json` (sounds + musics, keyword-mangled names, dense ids). |
| `src/generated_assets.rs` | Generated id tables: `icons.properties` sync, `icon_codes.json`, `assets/locales`. |
| `tests/pack_slice.rs` | End-to-end slice: source → staging → pack → manifest → `AtlasIndex` lookup, plus byte-determinism across two runs. |
| `../../../assets-raw/` (repo root) | Vendored source: `sprites/` (authored PNGs + `pack.json`), `fontgen/config.json`, `icons/`. |
| `../../../assets/` (repo root) | Vendored runtime assets plus generated outputs (`sprites/`, `icons/`, `shaders/`, `sounds.index.json`, `locales`, `parity/region_names.txt`). |
| `../../../build/assets/` (repo root) | Gitignored work/output dir (`staging/`, `last_pack_version`, `region_inventory.json`, timings, manifests). |

## Generator passes

| Module | Pass(es) | Output |
|---|---|---|
| `generate/autotile.rs` | `autotiles` | 47 slices per `blocks/environment/<name>-autotile[N]` source; preview and `gens` entry. |
| `generate/environment.rs` | `splashes`, `bubbles`, `cliffs`, `cracks`, `shallows`, `edges`, `scorches` | Water/effect frames, cliff masks, rubble, shallow-liquid blends, floor edges. |
| `generate/fx.rs` | `gas-frames` | `effects/fluid-liquid-<i>` and `effects/fluid-gas-<i>` frames over the `fluid` stencil. |
| `generate/blocks.rs` | `block-icons` | Outlines, team recolors, `block-<name>-full`, `ui/block-<name>-ui`, `block_colors.png`. |
| `generate/icons.rs` | `item-icons`, `sector-icons`, `team-icons` | Item/liquid/status and sector/team UI icons. |
| `generate/units.rs` | `unit-icons` | Unit/weapon outlines, tread slices, composites, wrecks, `ui/unit-<name>-ui`. |
| `generate/ore.rs` | `ore-icons` | Ore variant shadows and composite/UI icons. |
| `generate/metadata.rs` | — | Static content metadata tables (teams, per-block `BlockMeta`, icon/drawer rules, `UnitRegions`). |
| `generate/inventory.rs` | — | `RegionInventory` expected-region audit written to `build/assets/region_inventory.json`. |

## Pipeline stages

`mind-tools pack` runs the implemented stages in order: `staging`, `generate`, `move-ui-icons`,
`antialias`, `pack`, `pack-fallback`, `manifest`, `ids`, `shaders`. `--only a,b` selects a subset;
unknown names error and the accepted `enumerate` alias is folded into `generate`. `--no-fallback`
skips the 2048-capped fallback atlas, `--timings` writes `build/assets/pack_timings.json`, and
`staging` records the crate version in `build/assets/last_pack_version`.

## Key types

- Driver/pack: `base_content()`, `GenCtx` (`atlas`, `gens`, `extras`), `run_passes`, `PASS_ORDER`,
  `PackAtlas`, `PackOutput`, `PackSettings` (`mind_atlas::pack_json`), `pack_pipeline::pack`.
- Indexes/metadata: `IconsSyncReport`, `IconGlyph`, `ShaderIndex`/`ShaderEntry`/`ShaderCheckReport`,
  `SoundsIndex`/`SoundEntry`/`MusicEntry`, `RegionInventory`, `MigrationManifest`/`MIGRATION_MANIFEST`;
  `generate::metadata` owns `TeamSpec`, `BlockMeta`, `IconCtx`, `UnitRegions`.

## Invariants

- **`assets-raw/` is the authored source; `assets/` is the runtime tree; `build/assets/` is scratch.**
  Generated outputs (inventory: root [`AGENTS.md`](../../../AGENTS.md)) are never hand-edited — rerun
  the pipeline.
- **`pack` cleans only generated outputs.** The loose vendored art under `assets/sprites/`
  (`space.png`, `planets/`, `clouds.png`, `noise.png`, `error.png`, `logo.png`, …) migrates with
  `assets/` and must survive every pack (`clean_generated`), so `migrate` + `pack` reproduces a
  working tree.
- Region names are **flattened base names**; duplicate stems under `assets-raw/sprites/**` are a
  hard pack error (`assert_no_duplicate_region_names`, `PackAtlas::enumerate`).
- Every directory walk sorts before iterating, parallel decode/AA/regeneration collects results back
  in input order, and RNG seeds are derived from the output name (`mind_atlas::hash::seed`,
  `fnv1a`) so output is reproducible run to run.
- `sync_icons_properties` preserves existing `assets/icons/icons.properties` lines and allocates new
  PUA codes downward from `ICON_CODE_START` (`0xF8FF`); re-running with unchanged content adds 0.
- `generate` fails when `RegionInventory::missing_in_sources` is non-empty.
- `shaders check` reports required-but-unported shaders and forward uniform/texture drift; the
  upstream-disabled `shockwave` is intentionally allowed to stay unported.

## Rules

- Do not edit generated or packed files; change `assets-raw/` (or `generate/metadata.rs`) and rerun
  `bash tools/pack.sh pack`. Vendor only through `mind-tools migrate`; `--check` verifies the manifest and
  `assets/ASSET_PROVENANCE.md` records the source path, commit and tree hash.
- Workspace dependency and lint policy: [`client/rust/AGENTS.md`](../AGENTS.md).

## Conventions

- Reproducibility rests on the pinned workspace deps `png = "=0.17.16"` and `sha2 = "=0.10.8"` (used
  via `mind-atlas`), plus `mind-atlas`'s pure-Rust pixmaps and packer.
- Index manifests are pretty-printed JSON with a trailing newline, a `format` field, and
  name-sorted entries (e.g. `icon_codes.json`, `shader.index.json`, `sounds.index.json`).
- Sprite/pack algorithms live in `mind-atlas`; this crate wires them to the CLI stages.

## Verification

Run from the repo root (Linux); `bash tools/pack.sh` is the canonical wrapper.

```bash
# Library/CLI tests
cargo test --manifest-path client/rust/Cargo.toml -p mind-tools

# Full pipeline, then the byte-determinism gate (>= 2 full packs)
bash tools/pack.sh pack
bash tools/pack.sh determinism --runs 2

# Individual subcommands
bash tools/pack.sh migrate --from "$MIND_UPSTREAM" --to .
bash tools/pack.sh icons sync
bash tools/pack.sh sounds index
bash tools/pack.sh shaders check --reverse
bash tools/pack.sh mods classmap --check

cargo clippy --manifest-path client/rust/Cargo.toml -p mind-tools --all-targets -- -D warnings
```
