# AGENTS.md — mind-core/src/mods (mod loader & patcher)

`mind-core::mods` ports `Vars.mods`/`Mods.java` Godot-free: discovery, metadata, dependency
resolution, enable state, content/patch collection and the sprite/bundle/audio overlay. Bytes and
`dp-`/`<mod>-` names cross to `mind-gdext`, which appends atlas pages and plays audio. There is no
JVM/Rhino: `script.rs` is a `ScriptHost` seam whose `NoScriptHost` marks script mods `Unsupported`,
while Java-class mods stay `Enabled` with an `UnsupportedReason` so JSON/assets still load. Files use
`io::FileSystem` ([`../io/AGENTS.md`](../io/AGENTS.md)); names/IDs belong to
[`../content/AGENTS.md`](../content/AGENTS.md). Read [`mind-core/AGENTS.md`](../../AGENTS.md) first.

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | `Mods` manager (`load`, `load_single`, `set_enabled`, `import_mod`, `remove_mod`, `ordered_mods`, `load_scripts`), `ModState`, `ModError`, `ModListEntry`/`ModLoadReport`. |
| `discovery.rs` | `ModRoot` (`Folder`/`Zip`), `scan_mod_directory`, `find_meta`, `read_zip` + zip-slip `valid_path`, single-dir root unwrap, config paths. |
| `meta.rs` | `ModMeta` parse/`cleanup`, comment-tolerant dialect, color stripping, meta-file order and blacklist/min-version constants. |
| `deps.rs` | `resolve_dependencies` DFS: required/soft dependencies, missing/incomplete/cycle states, insertion-ordered result. |
| `loaded.rs` | `LoadedMod` (config paths, `mod_string`, `should_be_enabled`/`failed`/`repo`) and `UnsupportedReason`. |
| `listing.rs` | GitHub `ModListing`/`ModRelease` parsing and `parse_version`/`parse_version_tag`/`matches_game_version`; HTTP is the Godot layer's. |
| `provider.rs` | `collect_content_files` (`content/<kind>/**`, `contentOrder` then stem) and `ModsContentProvider` into `ContentRegistry`. |
| `overlay.rs` | `ModsOverlay` for `sprites/`, `sprites-override/`, `bundles/`, `sounds/`, `music/`, plus texturescale/pregenerated and warnings. |
| `script.rs` | `ScriptHost`/`ScriptError` seam and `NoScriptHost` (engine name `"none"`). |
| `server.rs` | Join-handshake values: `ModFingerprint`, `RequiredMods`, `ExternalAssetRef`, `AssetTransport`/sink traits. |
| `catalog.rs` | `content_catalog` rows (`content_type`, name, `source_mod`) in the server-seed compact order. |
| `patch/` | `DataPatcher` replace-only JSON patching (`@NoPatch`, planet gating, `unapply`), `collect_mod_patches`/`load_mod_patches`, `fix_content_arrays`. |
| `assets/` | `DataAsset` records + binary codec, sha256 `DataAssetCache`, `ModDataManager` lifecycle and bundle/audio/image appliers. |
| `json/` | `ContentJsonParser` for the nine content kinds; `classmap.rs` plus GENERATED `classmap_gen.rs`. |

## Load flow

1. **Discover** — scan immediate children: `.zip`/`.jar` via `read_zip`, folders via
   `resolve_folder_root` + first of `META_FILES` (`mod.json`, `mod.hjson`, `plugin.json`, `plugin.hjson`).
2. **Meta** — `parse` strips comments; `cleanup` strips colors, defaults version/displayName and
   derives `internal_name` (lowercase, spaces→`-`); `load` truncates version at the first newline.
3. **Deps** — DFS over every meta keyed by `mod-<name>-enabled` (default true) → dependency-first
   `name → ModState`.
4. **Enabled** — `load` drops duplicate names (last wins), applies the failed-launch skip, marks
   unsupported/disabled mods, sorts by state then name, and reindexes.
5. **Content/patch** — after `load`, `collect_content_files` walks `content/<kind>/**` in
   `ordered_mods` order (contentOrder, then stem), then `load_mod_patches` applies per mod.
6. **Assets** — `build_overlay` packs `sprites/` (prefix), `sprites-override/` (replace +
   warning), `bundles/*.properties`, `sounds|music/*.ogg|mp3`, plus texturescale/pregenerated.

## Invariants

- **Internal name is the ABI.** `internal_name` keys settings (`mod-<name>-enabled`,
  `-failed`, `-repo`), content prefixes (`<mod>-<stem>`), sprite prefixes and the handshake
  `name:version`; never key any of these on the display name.
- **`ModState` ordinals are frozen.** `Enabled=0`, `ContentErrors=1`, `MissingDependencies=2`,
  `IncompleteDependencies=3`, `CircularDependencies=4`, `Unsupported=5`, `Disabled=6`; sorting is
  state ordinal then name, and `enabled()` means `Enabled | ContentErrors`.
- **Dependencies precede dependents; cycles fail closed.** `ordered_mods` is the dependency order
  of enabled mods, cached in `last_ordered` and invalidated by `reindex`. A missing required dep is
  `MissingDependencies`, a failed/disabled one `IncompleteDependencies`, a DFS back-edge
  `CircularDependencies`; soft deps only warn.
- **Zip safety, but no built-in caps.** `valid_path` rejects empty, absolute, `.`/`..` and backslash
  components, and `read_zip` strips a single top-level directory — but it inflates every entry into
  memory with no entry-count/byte cap, so hosts must cap untrusted packages before `import_mod`.
- **Patches replace, never stack.** `DataPatcher::apply` first `unapply`s the previous set, snapshots
  the registry index and records reverse resetters; `id`/`size`/`name` are `@NoPatch`, and
  `load_mod_patches` uses one patcher per mod so mods cannot reset each other.
- **Scripts and Java are surfaced, not executed.** `NoScriptHost` returns `Unsupported`; an enabled
  mod with `scripts/` becomes state `Unsupported` in `load_scripts` (its content drops out of the
  enabled set), while a supported Java/`main` mod stays `Enabled` with `JavaModUnsupported` so
  JSON/assets still load.
- **`json/classmap_gen.rs` is generated.** Never edit it; regenerate with `mind-tools mods classmap`
  and gate with `--check`. Block aliases derive at runtime from `BlockKind::ALL`; `audit` rejects
  duplicates and non-round-tripping entries.
- **`parity/mod_fixtures/` is read-only input.** Tests use `MockFs` for new cases.

## Fixtures & verification

`parity/mod_fixtures/` holds 16 fixture dirs plus `expected_list.json` (groups are catalogued in
[`parity/AGENTS.md`](../../../../../parity/AGENTS.md)); the cases this loader uses:

- Content kinds: `allkinds/` (all nine + `expected.json`), `basic/` (item + wall), `unlock/`.
- Parse/patch coverage: `classes/` (ClassMap scope aliases), `consumers/`, `drill/`, `plans/`,
  `drawer/`, `effects/`, `weapon_fx/`, `patch/` (40 → 1234 → 40 health round-trip).
- Loading/package coverage: `deps/{a,b,cycle,soft}` (scan `--dir ../../parity/mod_fixtures/deps`),
  `zip/testmod.zip` (scan `--dir ../../parity/mod_fixtures/zip`), `scripts/` (unsupported reason),
  `error/` (bad file isolated beside good records).
- Assets: `assets/` carries `patches/`, `content/`, `bundles/`, `sprites/`, `sprites-override/`,
  `sounds/` and `music/`. `expected_list.json` is the normalized `mods list` list.

Run from `client/rust/`:

```bash
cargo test -p mind-core mods
cargo run -p mind-headless -- mods list --dir ../../parity/mod_fixtures/deps
cargo run -p mind-headless -- mods list --dir ../../parity/mod_fixtures/zip --json
cargo run -p mind-headless -- mods content --fixture allkinds --json
cargo run -p mind-headless -- mods patch --fixture patch --json
cargo run -p mind-headless -- mods assets --fixture assets --json
cargo run -p mind-headless -- mods overlay --fixture assets --probe assets-mod-test-sprite --json
cargo run -p mind-headless -- mods error-isolation --fixture error --json
cargo run -p mind-tools -- mods classmap --check --root ../..   # or from repo root: bash tools/pack.sh mods classmap --check
cargo run -p mind-headless -- parity goldens          # includes the pinned mod_classmap sha256
```

## Gotchas

- `mods list --check` compares the scan against `parity/mod_fixtures/expected_list.json`; refresh
  that golden when landing fixture changes, and never delete rows to make it pass.
- The `zip/` fixture is invisible to a root scan: `resolve_folder_root` unwraps a single-entry
  folder to `testmod.zip` and then finds no `mod.json`. Point `--dir` at the zip's parent instead.
- `set_enabled` only flips `Enabled`/`Disabled`; it does not re-run dependency resolution or
  `load_scripts`, so a re-enabled mod can look enabled until the next `load`.
- `Mods::load` writes settings by design: with `failed_to_launch` set and `modcrashdisable` true
  (default), it disables every candidate and records `mod-<name>-failed` for those that were
  enabled. Set the flag only from a real crash beacon.
- `ModsOverlay::shaders()` is always empty here — shader collection is not implemented.
