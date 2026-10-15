<!-- SPDX-License-Identifier: GPL-3.0-only -->
# I07 — Mods actually load; mod state persists; crash restore; exported builds ship assets

Round I, written 2026-10-15. Read `docs/I-environment.md` fully, then
`docs/I-INDEX.md` §3, then this plan.

Ledger items: PAR-0115 (installing a content mod changes nothing), PAR-0116 (script mods
never run and get no unsupported warning), PAR-0117 (disabling a mod does not survive a
restart), PAR-0118 (no Restore after a mod-caused crash), PAR-0119 (exported builds have
no assets). The reference soak staged only the Java half of these (the Mods dialog showed
"No mods found"); see `jq '.ledger["PAR-0115"]' chains.json` and the `notes` for each item.

The mod SYSTEM is already ported in `mind-core` (discovery, metadata, dependency
resolution, the content JSON parser, the patcher, asset overlays) and is unit-tested. It
is simply never connected to the live client. This plan connects it.

---

## 1. State on `main` (verified 2026-10-15 @ `b560e9b1`)

* `core/mods/` (all Godot-free, tested): `mod.rs` (917) `Mods { load, load_single, set_enabled,
  import_mod, remove_mod, load_scripts, skip_mod_loading :233, set_failed_to_launch :238,
  collect_content_files }`, `ModState` (same 7 values as Java `Mods.java:1491-1499`);
  `loaded.rs` `LoadedMod` (`failed(settings)` :120), `UnsupportedReason` :17-28
  (`Outdated, Blacklisted, VersionTooNew, ScriptsUnsupported, JavaModUnsupported`);
  `provider.rs` `ModsContentProvider::new(files)` :150 (`collect_content_files` :72 walks
  `content/<folder>/**.{json,hjson,json5}` per enabled mod in dependency order);
  `json/mod.rs` (2167) `ContentJsonParser` (block, item, liquid, status, unit, weather,
  sector, planet, team: Java `ContentParser.parsers` :577); `patch/mod.rs` (3496, DataPatcher);
  `overlay.rs` `ModsOverlay { region_names(), probe(name), texture_scale(name) }` + `build_overlay(...)` :102
  (sprites/ and sprites-override/); `assets/{audio,bundle,image}.rs`; `script.rs`
  `NoScriptHost` (`engine_name() == "none"`).
* `ContentRegistry::create_mod_content(&mut dyn ModContentProvider)` (`core/content/load.rs:629`).
  Its only non-test callers are `mind-headless` (`exec.rs:1639, 2036, 2149`), e.g.
  `exec.rs:1620-1640`:
  ```rust
  let mut mods = Mods::new(true, &dir);
  mods.load_single(&fs, &dir, &settings)?;
  let files = mods.collect_content_files(&fs);
  let mut provider = ModsContentProvider::new(files);
  let mut registry = create_base_content(&bundle, &store, true)?;
  registry.init()?; registry.post_init()?;
  let content_result = registry.create_mod_content(&mut provider);
  ```
* The live client builds FOUR registries, all base-only with an empty bundle:
  `gdext/sim_host.rs:536-545` (`bootstrap`), `gdext/campaign.rs:204-212`,
  `gdext/editor/mod.rs:109-116`, and test code at `gdext/sim_host.rs:7597`. None of them
  reads mods.
* `gdext/mods.rs` (444) `MindMods` autoload (`client/project.godot:26`): signals
  `mods_changed` :81, `mod_error(name, details)` :85, `mod_import_progress(path, ratio)` :92;
  `#[func]` `list` :96, `get` :106, `set_enabled` :119, `import_mod` :131, `remove_mod` :169,
  `requires_reload` :191, `has_scripts` :197, `errors` :203, `mod_strings` :226, `reload` :236,
  `mods_dir` :244 (`user://mods`, :27), `details` :250, `parse_listing` :319,
  `matching_release` :339, `game_build` :367, `game_revision` :373, `version_info` :379,
  `config_folder` :397. `bootstrap()` :65-78 calls only `Mods::load`.
  **The settings store is `SettingsStore::new()` (:44), an empty in-memory map:**
  `mod-<name>-enabled` is never read from or written to disk (PAR-0117), and
  `mod-<name>-failed` is never set.
* Working settings pattern: `gdext/campaign.rs:192-195`
  `SettingsStore::load(&NativeFs, &Paths::new(globalize_path("user://")))`, and
  `force_save` (`campaign.rs:224`, `ui/ui_host.rs:1081` `persist_settings`). Settings stores
  merge only their changed keys on save, so two stores on the same file are safe as long
  as each writes only its own keys.
* Launch marker: `gdext/platform/desktop.rs:35` `check_launch(root) -> bool` (writes
  `<root>/launchid.dat`, returns whether it already existed), `:46` `finish_launch`;
  `gdext/platform/mod.rs:92-97` only LOGS the result; `finish_launch` is only a debug `#[func]`
  (`platform/mod.rs:187-190`). A stale `client/launchid.dat` exists in the repo dir.
* `ui/dialogs/mods_dialog.gd` (489): buttons :63-67 (import file, import GitHub, browser,
  open folder, guide); no Restore; never reads `has_scripts`; shows `reason` only for
  `unsupported`/`incompatible` (:158-163).
* Export (PAR-0119): `client/export_presets.cfg` has ONE preset ("Windows Desktop",
  `export_filter="all_resources"`, `embed_pck=true`, `export_path="build/export/Mindustry-Godot.exe"`).
  `tools/export.sh` (80) picks a preset per platform (only Windows exists) and runs a bare
  `--export-<target>` (:68). It does not run `pack` or copy `assets/`. No export templates
  are installed on this host (`~/.local/share/godot/export_templates/` is empty).
  `gdext/assets/loader.rs:32-58` `resolve_assets_dir()`: ProjectSettings `mindustry/assets_dir`
  (:15; absent from `project.godot`) -> `res://assets` -> `<res://>/../assets` (the repo-root
  `assets/`, dev only) -> returns the dev path even when missing. The game data (atlas,
  fonts, bundles, sounds, music, maps, shaders, icons) is all in the repo-root `assets/`,
  OUTSIDE the Godot project, so an export contains none of it.
  `gdext/assets/loader.rs:17-30` `resolve_locale()` reads ProjectSettings `mindustry/locale`.

---

## 2. Tasks

Order: T1 -> T2 -> T3 -> T4 -> T5 -> T6 -> T7 -> T8.

### T1 — Mod enabled/failed state persists (Fixes PAR-0117)

**Java.** `Mods.setEnabled(mod, enabled)` (`J/mod/Mods.java:973-984`): writes
`mod-<name>-enabled`, sets `mod-<name>-failed` false, `requiresReload = true`, and updates the
state. `LoadedMod.shouldBeEnabled()` (:1297) reads `mod-<name>-enabled` (default true);
`failed()` (:1301) reads `mod-<name>-failed`. `Mods.load()` (:574-578): `!isSupported()` ->
`unsupported`, else `!shouldBeEnabled()` -> `disabled`.

**Changes.**
1. `gdext/mods.rs`: replace `settings: SettingsStore::new()` with a store loaded in
   `bootstrap()` (before `Mods::load`) from `user://` exactly like `campaign.rs:192-195`.
2. `#[func] set_enabled` (and `Mods::set_enabled` callers) -> after the core call,
   `settings.force_save(&NativeFs, &paths)` (only the `mod-*` keys changed, so the merge-save
   leaves other keys alone; confirm by reading `SettingsStore::force_save`).
3. If `I05 T1`'s shared settings hub (`gdext/settings_hub.rs`, see I-INDEX §3) is on `main`
   when you start, use it instead of a private store (one source of truth); otherwise keep the
   private store and list "MindMods settings to move onto the hub" in your summary.

**Tests.** A gdext-free core test `mods_enabled_state_round_trips_through_settings_file`:
`set_enabled(false)` + `force_save` to a temp `Paths`, a fresh `SettingsStore::load` +
`Mods::load` -> the mod state is `Disabled`.

**Verify in-engine.** Copy `REPO/parity/mod_fixtures/basic` to `$U/mods/basic`; `--play`;
`get_node("/root/MindMods").list()` shows it `enabled`; `set_enabled("<name>", false)`;
`tools/session-play.sh N --play` (a full restart of the game process); `list()` shows `disabled`.

**Commit.** `Fixes PAR-0117: mod enabled/failed state persists in settings`

---

### T2 — Crash detection, safe mod skip, Restore button, `@mods.initfailed` (Fixes PAR-0118)

**Java.**
* `Vars.checkLaunch()` (`J/Vars.java:398-408`) at startup (`ClientLauncher.java:47`): if
  `<dataDir>/launchid.dat` exists, `failedToLaunch = true`; else write it.
  `finishLaunch()` (`:411-418`) after loading completes (`ClientLauncher.java:253`, also :288,
  :309): `lastBuild`/`lastBuildString` settings, delete the file.
* `Mods.skipModLoading()` (`Mods.java:509-511`) = `failedToLaunch && settings.getBool("modcrashdisable", true)`.
* `Mods.loadMod` (`:1222-1227`): when skipping, `wasEnabled = mod-X-enabled (true)`, then
  `mod-X-enabled = false` and `mod-X-failed = wasEnabled`.
* `ModsDialog` ctor (`J/ui/dialogs/ModsDialog.java:55-61`): if any mod `failed()`, show
  `@mods.restore` (Icon.power) which re-enables every failed mod and rebuilds; otherwise the
  `@mods.guide` button (210x64).
* `Control` (`J/core/Control.java:75-80`): on `ClientLoadEvent`, if `skipModLoading() && mods.list().any()`,
  after 4 ticks `ui.showInfo("@mods.initfailed")`.

**Changes.**
1. `gdext/platform/mod.rs`: keep the `check_launch` result in a static/OnceLock
   (`failed_to_launch()` accessor). Call `desktop::finish_launch(root)` once the main menu
   is first shown (hook `MindUiRoot.set_menu_visible(true)` on first call through a `#[func]`
   `MindPlatform.finish_launch()` that already exists as a debug func: make it the real one,
   idempotent), and write `lastBuild`/`lastBuildString`.
2. `gdext/mods.rs` `bootstrap()`: `mods.set_failed_to_launch(platform::failed_to_launch())`
   before `load`; `Mods::load` (core) applies the `loadMod` skip rule (port Java `:1222-1227`
   into `core/mods/mod.rs` where each candidate is loaded; add a test
   `skip_mod_loading_disables_and_marks_failed`).
3. `mods_dialog.gd`: the Restore button exactly per Java (calls `MindMods.set_enabled(name, true)`
   for each `failed` mod; expose `failed: bool` in `list()` rows).
4. `ui_root.gd` (or the menu fragment): after boot, if `MindMods.skipped_mod_loading()` (new
   `#[func]`) and the list is non-empty, show `@mods.initfailed` with `MindUi.show_info`.
5. Delete the stale `client/launchid.dat` from the repo (and `.gitignore` it) so a fresh
   checkout does not boot in "failed" mode. Check with `git ls-files client/launchid.dat`
   whether it is tracked first; if it is, remove it with `git rm`.

**Verify in-engine.** With the fixture mod installed, kill the game process with `kill -9`
during boot (or create `$U/launchid.dat` by hand), `--play`: the mod is disabled,
`@mods.initfailed` shows, the Mods dialog has Restore; Restore + restart -> enabled again.

**Commit.** `Fixes PAR-0118: launch-crash detection skips mods, marks them failed and offers Restore`

---

### T3 — Script mods are marked unsupported (Fixes PAR-0116)

**Java.** `Mods.loadScripts()` (`:803-834`) runs JS for mods with `scripts/`. The port has no
JS engine by design (`core/mods/script.rs` `NoScriptHost`), so parity here is: such mods get
`ModState::Unsupported` with `UnsupportedReason::ScriptsUnsupported`, and the dialog says so
(the Java dialog shows the state text from `getStateText`/`getStateDetails`,
`ModsDialog.java:369-407`; the port uses its `reason` text).

**Changes.** `gdext/mods.rs` `bootstrap()`: call `self.mods.load_scripts(&NativeFs)` right after
`load` (Java order: `ClientLauncher.java:170`, before content creation). `mods_dialog.gd`: show
the unsupported state and the scripts reason for these mods (bundle key: find the closest Java
key, e.g. `grep -n "^mod\.\|^mods\." REPO/assets/bundles/bundle.properties`; if no key fits,
add `mod.scripts.unsupported` to the PORT bundle overlay file the project uses for port-only
strings: `grep -rn "port_only\|bundle_port" client/rust/mind-gdext/src/assets/` to find it, and
note the deviation).

**Tests.** `script_mod_marked_unsupported` exists in core; add a gdext-free test that the
bootstrap order (`load` then `load_scripts`) marks a fixture with `scripts/main.js` unsupported.

**Verify.** A copy of `parity/mod_fixtures/basic` with an added `scripts/main.js` shows as
unsupported in the dialog (screenshot).

**Commit.** `Fixes PAR-0116: mods with scripts are marked unsupported in the live client`

---

### T4 — One content boot path that includes mod content (PAR-0115 part 1)

**Java order** (`J/ClientLauncher.java:166-189`): `content.createBaseContent(); content.loadColors();`
then `mods.loadScripts(); content.createModContent();` (mod JSON content), ... then
`content.init(); content.load(); mods.loadModPatches();`.

**Changes.**
1. New `gdext/content_boot.rs`: `pub fn build_registry(bundle: &dyn Bundle, unlocks: &dyn UnlockStore) -> Result<ContentRegistry, String>`
   that does, in Java's order: `create_base_content(...)`; if the `MindMods` autoload has
   enabled, supported mods: `collect_content_files` -> `ModsContentProvider::new(files)` ->
   `registry.create_mod_content(&mut provider)` (collect errors into `MindMods` so the dialog's
   `@mod.errors` shows them and `ModState::ContentErrors` is set); `registry.init()`;
   `registry.post_init()`; then apply mod patches (`core/mods/patch`: find the entry point the
   headless `exec.rs:2036/2149` uses). If `create_mod_content` fails before `init` (the
   headless calls it after `post_init`), keep Java's order and fix the core so it works; if
   that is not possible, use the headless order and document the deviation.
   Get the enabled mod set from the `MindMods` autoload: add a Rust-side accessor
   `MindMods::content_files() -> Vec<ModContentFile>` (non-`#[func]`), reached through
   `Engine::get_singleton` / `get_node("/root/MindMods")`. Cache the result for the session;
   mod changes require a restart (Java exits to reload).
2. Replace the four `create_base_content` sites (`sim_host.rs:536`, `campaign.rs:206`,
   `editor/mod.rs:111`, and leave the test site alone) with `content_boot::build_registry`.
   The registries MUST stay identical across the three nodes (content ids are shared), so
   all three call the same function with the same mod list.
3. Bundles: mod `bundles/bundle*.properties` entries merge into the live bundle (the `MindAssets`
   bundle loader, `gdext/assets/bundle.rs`); use `core/mods/assets/bundle.rs`.
4. `MindMods.reload()` / the dialog's hide: Java `Mods.reload()` (`:784-796`) shows
   `@mods.reloadexit` and EXITS when a reload is required (`ModsDialog.java:83-87` calls it on
   hide). Port: same message, then `get_tree().quit()`.

**Tests.** A gdext test is awkward; put the order logic in core:
`core/content/boot.rs::build_registry_with_mods(bundle, unlocks, files, patches)` (pure) +
`content_boot.rs` as a thin gdext wrapper. Test with `parity/mod_fixtures/allkinds`: the
registry contains the modded block/item/unit with `minfo.is_modded()`, and the base content
ids are unchanged (compare the first 300 block ids with a base-only registry).

**Commit.** `PAR-0115: one content boot path loads enabled mods' content, bundles and patches`

---

### T5 — Mod sprites, sounds and overrides reach the renderer (PAR-0115 part 2)

**Java.** Mod `sprites/` are packed into the atlas at load (`Mods.packSprites`, grep it);
`sprites-override/` replaces base regions with the same name; `sounds/`/`music/` override by name.

**Changes.**
1. `gdext/assets/atlas.rs` (`MindAssets`): build the `ModsOverlay` (`core/mods/overlay.rs:102`
   `build_overlay`) at init from the enabled mods. Region lookup order: `sprites-override` region
   -> base atlas region -> mod `sprites/` region -> `error`. Mod regions are loose PNGs: load
   each once into an `ImageTexture` (respect `texture_scale`) and return it with a full-texture
   rect, so every caller that draws an atlas region (`canvas::draw_region`, the block mesh
   pass, UI icons) works unchanged. If a caller assumes "all regions share the atlas page
   texture" (mesh batching), give mod regions their own page index; grep how pages are chosen
   (`grep -rn "page" gdext/render/atlas_bind.rs`).
2. Sounds/music: `gdext/audio/` name lookup consults the mod overlay first
   (`core/mods/assets/audio.rs`).

**Verify in-engine.** Install `parity/mod_fixtures/assets` (it has `sprites/` and
`sprites-override/`); restart; the overridden base sprite renders the mod image (screenshot);
`/root/MindAssets` returns a texture for the mod region name.

**Commit.** `PAR-0115: mod sprites, overrides and sounds resolve through the asset overlay`

---

### T6 — A content mod is playable end to end (Fixes PAR-0115)

**Acceptance.** With `parity/mod_fixtures/basic` (or `allkinds`) installed and enabled: the
modded block appears in the Core Database and in the block palette (sandbox custom game, I06 T1),
can be placed, and renders its sprite; the modded item shows in the database.

**Changes.** Fix whatever the end-to-end walk breaks (palette catalog built from the
registry, icons, bundle names `<mod>-<name>`). Expect `invalidate_ui_block_catalog` and the
palette (`grep -rn "block_catalog" gdext/`) to need no change once T4 lands.

**Verify in-engine.** Database dialog screenshot showing the mod content; a sandbox custom game
(I06 T1 `start_map_mode("<map>", "sandbox", false, "")`) where `select_block_by_name("<modded block>")`
returns true and `place_block` succeeds; screenshot.
If I06 T1 is not on `main`, verify in a campaign sector with the API `place_block`.

**Commit.** `Fixes PAR-0115: content mods load into the live game (database, palette, placement)`

---

### T7 — Exported builds find their assets (PAR-0119 part 1)

**Design decision (binding).** The game data stays outside the Godot project. Exports ship it
in an `assets/` directory next to the executable (`Mindustry-Godot.exe` + `assets/`), the same
layout `tools/pack.sh` produces at the repo root.

**Changes.**
1. `gdext/assets/loader.rs` `resolve_assets_dir()`: new order:
   (1) ProjectSettings `mindustry/assets_dir` if set and valid;
   (2) the user command-line argument `--assets=<dir>` (`OS.get_cmdline_user_args()`) if valid;
   (3) the env var `MINDUSTRY_GODOT_ASSETS` if valid;
   (4) `res://assets` if valid;
   (5) `<OS.get_executable_path() base dir>/assets` if valid (exports);
   (6) `<res://>/../assets` (dev);
   and log the chosen dir once at INFO. Keep "valid" = `sprites/sprites.atlas.json` exists.
   Every asset reader already goes through this function; grep `resolve_assets_dir` to confirm,
   and fix any reader that builds a repo-relative path by hand
   (`grep -rn "\.\./assets\|\"assets/" client/rust/mind-gdext/src`).
2. `tools/export.sh`: add `--with-assets` (default on): after the Godot export, run
   `bash tools/pack.sh pack` if `assets/sprites/sprites.atlas.json` is missing, then copy the
   runtime subset of `assets/` next to the output (`sprites/` incl. `fallback/` and
   `block_colors.png`, `fonts/`, `bundles/`, `locales/`, `icons/`, `sounds/` + `sounds.index.json`,
   `music/`, `maps/`, `shaders/` (`shader.index.json` + `godot/` if still read), `baseparts/`,
   `cursors/`, `contributors/`; NOT `assets/parity/` or any build staging). Read
   `gdext/assets/` and `gdext/audio/` for every subdirectory actually read at runtime, and copy
   exactly those (list them in the script with a comment per directory).
3. Add a `Linux/X11` preset to `client/export_presets.cfg` (copy the Windows preset; platform
   `Linux`, `binary_format/embed_pck=true`, export path `build/export/linux/Mindustry-Godot.x86_64`).
4. `tools/build.sh --release` must produce `client/bin/rust/release/libmind_gdext.so` (it passes
   `"$@"` to cargo, so `--release` works; document it in `tools/AGENTS.md`).

**Tests.** `tools/export.sh --help` documents the flags. A unit test in `gdext/assets/loader.rs`
for the candidate ordering (pure function over a closure that answers "is valid").

**Verify (no export templates needed).** Export only the PCK (templates are not needed for
`--export-pack`):
```bash
CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh --release
godot --headless --path client --export-pack "Linux/X11" /tmp/mg-export/Mindustry-Godot.pck
mkdir -p /tmp/mg-export && bash tools/export.sh --assets-only /tmp/mg-export     # add this mode: copies the assets subset only
cd /tmp && godot --main-pack /tmp/mg-export/Mindustry-Godot.pck -- --assets=/tmp/mg-export/assets
```
The menu renders with real sprites and fonts (screenshot via the bridge if the MCP autoload is
in the pack, otherwise `import -window root` on the display); the log shows the chosen assets dir
and no `[assets] missing manifest`. If export templates ARE installed
(`~/.local/share/godot/export_templates/4.7.2.stable/version.txt`), also run the full
`tools/export.sh linux` and launch the binary.

**Commit.** `PAR-0119: exports ship an assets directory beside the binary; asset root resolution order`

---

### T8 — Export smoke in CI and docs (Fixes PAR-0119)

**Changes.** `.github/workflows/ci.yml`: a job (manual `workflow_dispatch` or nightly) that
installs Godot 4.7.2 + templates (the `godot-import` job :165 already downloads Godot; extend it),
runs `tools/build.sh --release`, `tools/pack.sh pack`, `tools/export.sh linux`, then launches the
export headless for 300 frames (`--quit-after 300`) and greps the log for `missing manifest`
(must be absent). Document the export layout in `tools/AGENTS.md`.

**Commit.** `Fixes PAR-0119: CI export smoke checks that exported builds load their assets`

---

## 3. Order and hot files

T1, T2, T3, T4, T5, T6, T7, T8. Hot files: `gdext/sim_host.rs`, `gdext/campaign.rs`,
`gdext/editor/mod.rs` (T4: the registry construction lines only), `gdext/assets/atlas.rs` (T5),
`gdext/assets/loader.rs` (T7).

## 4. Ledger changes to report

| Item | After |
|---|---|
| PAR-0117 | verified (T1 restart check) |
| PAR-0118 | verified (T2) |
| PAR-0116 | verified (T3) |
| PAR-0115 | verified (T6 end-to-end) |
| PAR-0119 | verified (T7 PCK run; T8 CI when templates are available) |

## 5. Done criteria

All tasks committed; suites green; the fixture-mod walk recorded under
`runs/<date>-s<N>-I07-*`; summary in `REPO/docs/I07-mods-export_summary.md`.
