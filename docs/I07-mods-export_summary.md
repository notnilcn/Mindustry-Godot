# I07 — mods/exports summary (session 4, 2026-10-15)

Plan: `docs/parity-plans/I07-mods-export.md`; environment: `docs/parity-plans/I-environment.md`.
Worktree `WT=/home/clintonnguyen/g/code_examples/Mindustry-Godot-session4`, branch `parity/session-4`,
bridge 7000, display :13. Start-of-session: `git merge --no-edit main` (merge commit, clean).

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T1 (PAR-0117) | done | `runs/20261015-s4-I07-t1/`; commit `da687346` |
| T2 (PAR-0118) | done | `runs/20261015-s4-I07-t2/`; commit `68afbfc4` |
| T3 (PAR-0116) | done | `runs/20261015-s4-I07-t3/`; commit `6e0e8bb5` |
| T4 (PAR-0115 p1) | done | `runs/20261015-s4-I07-t4/`; commit `1a224774` |
| T5 (PAR-0115 p2) | done | `runs/20261015-s4-I07-t5/`; commit `9592c8cf` |
| T6 (Fixes PAR-0115) | done (campaign half; sandbox half skipped: waiting for I06 T1) | `runs/20261015-s4-I07-t6/`; commit `c0b28d19` |
| T7 (PAR-0119 p1) | not reached | — |
| T8 (Fixes PAR-0119) | not reached | — |

`git log --oneline main..HEAD`:

```
c0b28d19 Fixes PAR-0115: content mods load into the live game (registry, runtime, placement)
9592c8cf PAR-0115: mod sprites, overrides and sounds resolve through the asset overlay
1a224774 PAR-0115: one content boot path loads enabled mods' content, bundles and patches
6e0e8bb5 Fixes PAR-0116: mods with scripts are marked unsupported in the live client
68afbfc4 Fixes PAR-0118: launch-crash detection skips mods, marks them failed and offers Restore
da687346 Fixes PAR-0117: mod enabled/failed state persists in settings
```

## Tests (exact commands + results)

```
cargo test -p mind-core --lib --features msav-import
  -> ok. 2570 passed; 0 failed; 4 ignored          (I00 baseline 2551; +19 from I01 + this plan)
cargo test -p mind-headless --lib -> ok. 133 passed; 0 failed
cargo test -p mind-gdext --lib    -> ok. 48 passed; 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings -> exit 0, no output
cargo fmt -p mind-core -p mind-gdext -p mind-headless -- --check -> exit 0
cargo run -q -p mind-headless -- parity goldens -> 97 PASS / 0 non-PASS
godot.sh --headless --editor --quit --path client | grep -E "SCRIPT ERROR|Parse Error" -> nothing
```

New tests: `mods::tests::mods_enabled_state_round_trips_through_settings_file` (T1),
`mods::tests::skip_mod_loading_disables_and_marks_failed` (T2),
`mods::tests::client_bootstrap_order_marks_script_mod_unsupported` (T3),
`content::boot::tests::allkinds_boot_registry_contains_mod_content` (T4),
`mods::overlay::tests::sound_overlay_by_name` (T5),
`sim::tests::sync_blocks_from_content_preserves_base_ids` (T6).
Note: `cargo clippy -p mind-headless --all-targets` fails on two pre-existing
`expect()` uses in `mind-headless/src/campaign_scenarios.rs` (session-2 I01 code,
not this plan, not in the prescribed gate).

## In-engine evidence (bridge 7000, `$U=.../session-4/xdg-data/godot/app_userdata/Mindustry-Godot`)

Profile fixtures installed under `$U/mods/` (copies of `parity/mod_fixtures/*`, never committed):
`basic` (+ `sprites/blocks/test-wall.png` = the Mindustry logo), `assets-mod` (= the `assets`
fixture, real PNG/OGG swapped in for the dummy files), `scripted` (= basic + `scripts/main.js`,
renamed "Scripted Mod"). `mod-<name>-unlocked` flags set for both modded blocks.

* T1: list() enabled -> `set_enabled('basic-mod', false)` -> `--play` -> still disabled; settings.bin
  carries `mod-basic-mod-enabled`/`mod-basic-mod-failed` (`t1-*`). Panic 0.
* T2: `touch $U/launchid.dat` + `--play` -> `skipped_mod_loading()=true`; assets-mod/basic-mod
  `enabled:false, failed:true, state:"disabled"` (scripted-mod `failed:false`, was already disabled);
  Mods dialog shows **Restore Failed** (`t2-mods-dialog-restore.png`) and the rows read Failed;
  clicking it (godot_input at 892,200) re-enables both; after `--play` they stay enabled and no
  Restore button is shown (`t2-after-restore-restart-list.json`). A clean boot writes and deletes
  the marker. Panic 0.
* T3: scripted-mod `state:"unsupported"`, `has_scripts:true`,
  `reason:"script mods are not supported in this build"`; dialog row and details pane show it
  (`t3-mods-dialog-scripts-unsupported.png`). Panic 0.
* T4: `[content] mod boot: 4 content file(s), 1 patch set(s)`; block ids
  `basic-mod-test-wall`=448, `assets-mod-test-wall`=447, item `basic-mod-test-ingot`=23;
  `bundle_get("mod.assets-mod.greeting")` = "Hello from Assets Mod" (`t4-*`).
* T5: `probe("basic-mod-test-wall")` found (own page), `probe("assets-mod-test-sprite")` found,
  `probe("copper-wall")` now points at the mod page (sprites-override replaced the base region),
  `region_count` 5161 -> 5164, `sound_stream("test")` decodes (`t5-probes.json`,
  `t5-sound-menu.png`).
* T6: Ground Zero (serpulo 170): `content_info_json(...).locked=false` after the unlock flag;
  `place_block(133,62,"basic-mod-test-wall")` and `(139,66,"assets-mod-test-wall")` both true;
  `building_debug` reads blocks 448/447 team 1; the placed blocks render their mod sprites
  (Mindustry logo / cloud image) in `t6-placed-mod-blocks.png`; the Core Database lists
  `basic-mod-test-ingot` (`t6-database.png`). Panic counts 0 in every step.

## Deviations (Java file:line)

1. **T3 surface (no bundle key).** Java runs script mods (`Mods.loadScripts`, `Mods.java:803-834`);
   the port marks them `ScriptsUnsupported`. The dialog row shows `UnsupportedReason::message()`
   ("script mods are not supported in this build") instead of a Java bundle key, because Java has no
   key for this port-only state (`ModsDialog.getStateText`, `ModsDialog.java:369-407`; the closest
   key `mod.scripts.disable` is about refusing to join a server, not loading).
2. **T4 bundle snapshot.** `DataBundleLoader.load` merges mod layers; the port omits the
   `unload` snapshot/restore half (`DataBundleLoader.java:44-47`) because Java only unloads when
   mods reload, which exits the process in both implementations.
3. **T4 registry lifetime.** The session boot content is cached once per process; Java re-reads on
   `Mods.reload` and exits (`Mods.java:787-796`). The port quits the tree on reload
   (`MindMods::reload`) instead of showing an info dialog first (`show_info` is the port's toast).
4. **T6 sandbox half skipped** — I06 T1 (`load_map_world`/`start_map_mode`) is not on `main`, so per
   plan the walk used a campaign sector + the `place_block` API. Extra enablers that build relied on
   (each a parity gap found live, not in the plan):
   * the sim's `Blocks` table and the live block/unit runtime were vanilla-only
     (`Sim::install_block_runtime`, `sim/runtime.rs:1355`), so command ids and all `can_place`
     def lookups missed mod content;
   * mod content resolved `<name>-unlocked` against an empty `MemoryUnlockStore`
     (`ContentJsonParser`, `mods/json/mod.rs:85`), so mod blocks could never be campaign-unlocked
     (`placement_allowed`, `sim/runtime.rs:200-217`; `UnlockableContent`, `ctype.rs:125-158`).
5. **T2 editor marker (found live, fixed in T2).** `Vars.checkLaunch` ran in the Godot editor's
   scene instantiation too (`Engine.is_editor_hint` guard added in `lib.rs`/`platform/mod.rs`);
   otherwise every editor restart wrote `launchid.dat` and the next game boot skipped all mods.

## Blockers / skipped

* `skipped: waiting for I06 T1` — the T6 sandbox-custom-game half (verify in a sandbox custom game
  with `start_map_mode("<map>","sandbox",false,"")`). The campaign half is done.
* T7/T8 not reached (budget). T7 is independent (no dependency); T8 depends on T7.
* `cargo clippy -p mind-headless --all-targets` fails on pre-existing `expect()` uses in
  `mind-headless/src/campaign_scenarios.rs:663,1196` (session-2 I01 code, outside this plan's gate).

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0117 | verified (T1 restart check; `da687346`) |
| PAR-0118 | verified (T2 crash-skip + Restore walk; `68afbfc4`) |
| PAR-0116 | verified (T3; `6e0e8bb5`) |
| PAR-0115 | verified (T4-T6; `1a224774`,`9592c8cf`,`c0b28d19`); note the extra gaps found (deviation 4) and `skipped: waiting for I06 T1` for the sandbox half |
| PAR-0119 | **not** verified (T7/T8 not reached) |

## Done criteria

Plan §5: "All tasks committed; suites green; the fixture-mod walk recorded under `runs/<date>-s<N>-I07-*`;
summary written." — Suites green and the fixture walk recorded; T1-T6 are committed with their
prescribed subjects. **Not met in full: T7/T8 (PAR-0119) remain.** A follow-up pass should start at
T7 (asset-root resolution order + `tools/export.sh --with-assets` + the Linux preset), then T8 (CI
export smoke).

# Pass 2 (session 4, 2026-10-15)

Plan `docs/parity-plans/I07-mods-export.md` T7+T8 only. Worktree
`WT=/home/clintonnguyen/g/code_examples/Mindustry-Godot-session4`, branch `parity/session-4`,
bridge 7000, display :13. Session start: `git merge --no-edit main` (fast-forward to
`48bc7a23`), clean tracked status; `main` did not advance during the pass. No Java client
was launched.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T7 (PAR-0119 p1) | done | `runs/20261015-s4-I07-t7/`; commit `e19b75f0` |
| T8 (Fixes PAR-0119) | done — local equivalent of the CI job (the job itself runs on GitHub `workflow_dispatch`/nightly; this host is not a runner) | `runs/20261015-s4-I07-t8/`; commit `7664696c` |

`git log --oneline main..HEAD`:

```
7664696c Fixes PAR-0119: CI export smoke checks that exported builds load their assets
e19b75f0 PAR-0119: exports ship an assets directory beside the binary; asset root resolution order
```

## Tests (exact commands + results)

```
cargo test -p mind-core --lib --features msav-import
  -> ok. 2599 passed; 0 failed; 4 ignored          (I00 baseline 2551; main advanced since)
cargo test -p mind-headless --lib    -> ok. 133 passed; 0 failed
cargo test -p mind-gdext --lib       -> ok. 49 passed; 0 failed    (48 before; + new loader test)
  filtered: ./target/debug/deps/mind_gdext-<hash> pick_assets_dir --nocapture
  -> test assets::loader::tests::pick_assets_dir_follows_the_resolution_order ... ok
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings -> exit 0, no output
cargo run -q -p mind-headless -- parity goldens -> 97 PASS / 0 FAIL / 0 other lines
tools/build.sh --release -> Finished `release` profile [optimized] target(s) in 7m02s
bash tools/export.sh --help -> documents PLATFORM / --with-assets / --no-assets / --assets-only
```

## In-engine evidence (exported binary, display :13)

* Export: `tools/export.sh linux --verify` -> `Mindustry-Godot.x86_64` (83506512 bytes) +
  `assets/` (101 MB, 14 entries) + `libmind_gdext.so` beside the binary.
* `DISPLAY=:13 ./client/bin/export/Mindustry-Godot.x86_64 -- --capture <png>` ->
  `[I] [assets] assets root: `res://assets` (source: res://assets)` (Godot's exported
  `res://` falls back to the executable directory, so the beside-the-binary copy is reached
  as candidate 4; candidate 5 remains the direct path when that fallback does not apply).
  Menu renders real sprites and fonts (`t7-export-menu-exe-dir.png`); 0
  `[assets] missing manifest` and 0 panics in every log.
* Override paths end-to-end on the exported binary: `--assets=` -> `source: --assets=`;
  `MINDUSTRY_GODOT_ASSETS` -> `source: MINDUSTRY_GODOT_ASSETS` (both captures render).
* Negative control: with `client/bin/export/assets/` moved away, the same binary logs
  `missing manifest ../assets/sprites/sprites.atlas.json` three times — the smoke's grep is
  live.
* T8 smoke: `./client/bin/export/Mindustry-Godot.x86_64 --headless --quit-after 300` ->
  status 0, assets root logged, no `missing manifest` (`t8-export-headless.log`).

## Deviations (both tasks are port-side packaging, so no Java file:line)

1. **The plan's template-free PCK recipe cannot load the GDExtension.** `godot --main-pack
   <pck>` with the editor binary requests the debug lib that `--export-pack` does not ship
   (`t7-pck-only-editor-binary.log`); the release template refuses `--main-pack` entirely
   (`disable_path_overrides=no`, `t7-pck-only-template-binary.log`). Export templates were
   installed on this host (4.7.2.stable, 1.28 GB tpz), so the plan's preferred route — the
   full `tools/export.sh linux` binary — was used end to end; `--export-pack` was still run
   to confirm the no-templates half (9.9 MB PCK).
2. **Exports land on resolver step 4, not 5** (`res://assets` via the exe-dir fallback);
   the CI smoke accepts either source, and the exe-dir-before-dev ordering is covered by
   the new unit test.
3. **Component libraries sit beside the binary** (`libmind_gdext.so`,
   `libblastbullets2d*.so`), not in the PCK; the copy step ships only `assets/`.
4. **`tools/export.sh` gained a positional platform** (`export.sh linux`, matching the
   plan's T8 command text); `--platform linux` still works.
5. Pass-1 deviations 1-5 stand unchanged.

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0119 | verified: T7 (exported binary boots with `assets/` beside it; override paths work) + T8 (the `export-smoke` job's exact commands pass locally). The job's first live CI run is the next `workflow_dispatch`/nightly; commits `e19b75f0`, `7664696c`. |

## Done criteria

Plan §5: all tasks committed; suites green; evidence recorded; summary written. — T7/T8 are
committed with the prescribed subjects; suites are 2599/133/49 with clippy clean and goldens
97/97; evidence lives under `runs/20261015-s4-I07-t7/` and `runs/20261015-s4-I07-t8/`.
**The I07 plan is complete** (T1-T8); the only non-local half is the CI job's first run on
GitHub.

# Pass 3 (session 4, 2026-10-16): T6 sandbox half re-run

Plan `docs/parity-plans/I07-mods-export.md` T6, the half skipped in pass 1
(`skipped: waiting for I06 T1`). Worktree `WT=.../Mindustry-Godot-session4`, branch
`parity/session-4`, bridge 7000, display :13. Start: `git merge --no-edit main`
(fast-forward to `c13b14b3`); tracked status clean. No Java client launched.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T6 sandbox half | done — gap found + fixed in `bb6b0564` | `runs/20261016-s4-I07-t6b/` |

`git log --oneline main..HEAD` (new commit only; pass-1/2 commits are on main):

```
bb6b0564 Fixes PAR-0115: mod blocks reach the sandbox placement palette
```

## Tests (exact commands + results)

```
cargo test -p mind-core --lib --features msav-import -> ok. 2653 passed; 0 failed; 4 ignored
  incl. new ui::campaign::tests::block_catalog_from_lists_live_mod_content
cargo test -p mind-headless --lib -> ok. 133 passed; 0 failed
cargo test -p mind-gdext --lib   -> ok. 51 passed; 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings -> exit 0, no output
cargo fmt -p mind-core -p mind-gdext -p mind-headless -- --check -> clean
cargo run -q -p mind-headless -- parity goldens -> 97 PASS / 0 other
```

## In-engine evidence (bridge 7000)

Sandbox session: `$MC.start_map_mode("<U>/maps/sandbox.msav", "sandbox", false, "")` ->
`true` (internally `load_map_world`, `campaign.rs:1307`), then
`$ROOT.set_menu_visible(false)` (the API path leaves the menu up; §4.1 recipe). State:
`hasCore:true`, `campaign:false`, `infiniteResources:true`, `allowEditRules:true`,
`waves:true`, `waveTimer:false`, `defaultTeam:1`, player unit true, core tile (99,100).

* **Gap found**: `$UI.block_catalog_json()` missed the enabled mod block — pre-fix
  `{"has_duo": true, "has_mod_ingot": false, "has_mod_wall": false, "len": 11879}`. The
  catalog was built from a fresh base-content registry
  (`block_catalog_for`/`block_catalog_unlocked_for` -> `create_base_content`) instead of
  the live session registry; Java selects from the global `content.blocks()`
  (`PlacementFragment.getUnlockedByCategory`, `PlacementFragment.java:771-781`).
* **Fix verified**: same eval after the fix -> `has_mod_wall: true` (len 11943);
  `select_block_by_name("basic-mod-test-wall")` true, `selected_block()` =
  `basic-mod-test-wall`, `valid_place_preview(104,96)` true;
  `place_block(104,96,"basic-mod-test-wall")` and
  `place_block(107,96,"assets-mod-test-wall")` both true; `building_debug` reads
  448/447 team 1; `t6b-placed-mod-blocks.png` renders both mod sprites (2x2 logo wall +
  1x1 cloud wall, same as pass 1); `$UI.open_dialog("database","")` ->
  `t6b-database.png` lists `basic-mod-test-ingot` + `assets-mod-test-ingot`;
  `database_json` contains the mod block + ingot; `content_info_json` locked=false for
  both walls and the ingot. Panic count 0.

## Deviations / notes

1. The palette fix (commit `bb6b0564`): `mind-core` gains
   `ui::campaign::block_catalog_from(registry, store, planet)`; `block_catalog_with`
   delegates to it after building the base-content fallback; `MindCampaign::block_catalog_json`
   now passes the live session registry (mods included) for both the campaign and custom
   paths. Java: `PlacementFragment.java:771-781`.
2. `assets-mod-test-wall` fails `valid_place_preview` at every probed tile: its JSON has
   no `requirements` and no `buildVisibility`, so it stays `BuildVisibility.hidden`
   (Java `Block.java:367`; `ContentParser.java:608-610` only defaults requirements-bearing
   blocks to `shown`) and `Build.validPlace` rejects it for players
   (`Build.java:163-186`) while the API path (`PlaceFlags::API`, `checkVisible=false`)
   succeeds — Java parity, not a gap.
3. `MindInput.load_catalog` (`input/mod.rs:340`) still builds the `1`..`0` quick-select
   lists from base content; out of T6's acceptance (the placement palette). Ledger note.
4. Pass-1/2 deviations stand.

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0115 | **fully verified**: T4-T6 with the sandbox half now covered — palette + `select_block_by_name` + `valid_place_preview` + `place_block`/`building_debug` + database in a `start_map_mode` sandbox session; palette gap found live and fixed (`bb6b0564`). |
