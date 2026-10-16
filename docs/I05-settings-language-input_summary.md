# I05 — settings/language/input summary (session 4, 2026-10-15)

Worktree `Mindustry-Godot-session4`, branch `parity/session-4`, bridge 7000, display :13.
Plan: `docs/parity-plans/I05-settings-language-input.md`. Environment: `docs/parity-plans/I-environment.md`.
Ran the start-of-session steps: `parity-session.sh --session 4`, clean status, `git merge --ff-only main`
-> `c2ba1f69` (I00 marker `c2ba1f69`), no pack needed (`assets/sprites/sprites.atlas.json` present).

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 one shared settings store (PAR-0164) | done | `42962c0b` |
| T2 language persistence + bundle at boot (PAR-0135) | done | `f61f3bbc` |
| T3 block hotkeys / pick / F1 (PAR-0214) | done | `1d40819f` |
| T4 settings-effect audit (+ graphics fixes) | done for the graphics row set; audit table below; 19 rows still inert and recorded | `7ad3385c` |
| T5 input re-verification (evidence only) | done (all 5 checks PASS, one recipe deviation recorded) | none (plan prescribes a commit only when a check fails) |

## Commits

```
7ad3385c Gameplay I05 T4: graphics settings take effect
1d40819f Fixes PAR-0214: block hotkey combos, arrow navigation, pick and F1 as in PlacementFragment
f61f3bbc Fixes PAR-0135: language choice persists and selects the bundle at boot
42962c0b Fixes PAR-0164: one shared settings store for MindUi, MindCampaign, MindMods and input
```

## Tests (all on the final branch state)

```
cargo test -p mind-core --lib --features msav-import   -> 2554 passed; 0 failed; 4 ignored   (I00 baseline 2551)
cargo test -p mind-headless --lib                     -> 133 passed; 0 failed
cargo test -p mind-gdext --lib                        -> 48 passed; 0 failed              (I00 baseline 47; +1 new)
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings -> clean
cargo fmt -p mind-core -p mind-gdext -- --check       -> FMT_OK
cargo run -q -p mind-headless -- parity goldens       -> 97 PASS / 0 non-PASS
bash tools/godot.sh --headless --editor --quit --path client | grep -E "SCRIPT ERROR|Parse Error" -> nothing
```

New tests: `settings_hub::tests::closures_share_writes_and_save_persists`,
`assets::locale::tests::resolve_locale_default_picks_closest`,
`assets::locale::tests::parse_locale_normalizes_case`,
`assets::bundle::tests::bundle_fallback_chain`.

## T1 — one shared settings store (Fixes PAR-0164)

Seam exactly as I-INDEX §3: `gdext/settings_hub.rs` `init(paths)` / `with_settings(f)` /
`reload(paths)` / `save(paths)` (`ARC/arc-core/src/arc/Core.java:24`,
`ARC/backends/backend-sdl/src/arc/backend/sdl/SdlApplication.java:45`, `J/Vars.java:503`).
MindUi / MindCampaign / MindMods / input bindings all migrated; `live_setting_flag`,
`live_setting_flag_default`, `live_save_interval_seconds` deleted.

In-engine (Ground Zero):
```
(a) settings_put_flag("skipcoreanimation", false); fresh GZ launch (abandon_sector first)
    -> Camera2D.is_cutscene() = true      (runs/20261015-s4-I05-t1/t1a-cutscene-on-flag-false.png)
    settings_put_flag("skipcoreanimation", true); fresh GZ launch
    -> Camera2D.is_cutscene() = false     (runs/20261015-s4-I05-t1/t1a-gz-no-cutscene-flag-true.png)
(c) settings_put_flag("duo-unlocked", false) -> MindCampaign.block_catalog_json().find("duo") = -1.0
    settings_put_flag("duo-unlocked", true)  -> find("duo") = 115.0      (cross-facade, same session)
(b) grep -n "live_setting_flag\|live_save_interval_seconds" gdext/campaign.rs -> nothing
```
`U/config/settings.bin` rewritten by the writes; newest log panic count 0.

## T2 — language choice persists (Fixes PAR-0135)

`language_dialog.gd` writes `locale` through `MindUi.settings_put_text` and shows
`@language.restart` (`LanguageDialog.java:78-84`); `resolve_locale()` reads the shared store
(`Vars.java:538`), resolves `"default"` through the OS locale (`findClosestLocale`,
`LanguageDialog.java:110-128`) and stores it; `load_bundle` applies the `router` transform
(`Vars.java:556-564`) and merges `global.properties`; `apply_locale_fonts` ports
`Fonts.loadExtraFonts` (`Fonts.java:101-119`, `ja` only — the plan's zh/ko list was checked
against the source).

In-engine (a real mouse click on the Deutsch row, then a real process restart per locale):
```
"de"      -> main menu German (Spielen/Datenbank/Editor/Mods/Einstellungen/Verlassen)
             t2-de-boot-menu.png
"en"      -> English menu                                      t2-en-boot-menu.png
"router"  -> every label is the router glyph; bundle_get("language.restart") = all \uf88e
             with spaces kept                                  t2-router-boot-menu.png
"ja"      -> Japanese menu with font_jp as the default font    t2-ja-boot-menu.png
"default" -> locale "en" stored (findClosestLocale(en_AU))     t2-default-resolves-en.png
restart info toast (German) on the click                       t2-en-click-restart-info.png
```
Profile left at `locale = "en"`.

## T3 — block hotkeys / pick / F1 (Fixes PAR-0214)

`desktop.rs` digits/arrows/`category_prev`/`category_next` now only fire the hotkey; the fragment
owns `PlacementFragment.gridUpdate` (`:177-273`) against the live unlock-filtered catalogue;
`pick_block` = `updatePick` (`:132-172`) with the new `MindSimHost::player_plan_at_world(wx, wy)`
and `pick_target_at`; `block_info_hotkey` = the `blockInfo` branch (`:260-271`) and never opens
the config panel.

In-engine (Ground Zero):
```
(a) "1","1"                    -> selected_block() = "duo"          (turret[0])
(b) "3"                        -> "conveyor", category distribution
    "3","2" -> "titanium-conveyor"; "1" -> "duo"; "3" -> "titanium-conveyor" (remembered)
    arrows: Right -> scatter, Left -> duo, Down -> wave (blocks[4]); no-op with nothing selected
(c) middle-click on a placed graphite-press -> "graphite-press", category "crafting"
    runs/20261015-s4-I05-t3/t3c-middle-click-graphite-press.png
(d) deselect, F1 over a sorter -> dialog_stack ["content"], context {"content":"sorter"},
    block_config hidden   runs/20261015-s4-I05-t3/t3d-f1-sorter-content-dialog.png
```

## T4 — settings-effect audit + graphics fixes

Full machine-generated audit (key / Java reader / port reader) is
`runs/20261015-s4-I05-t4/audit.tsv`; verdicts and the in-engine probes are in
`runs/20261015-s4-I05-t4/notes.md`.

Fixed in `7ad3385c` (previously inert): `vsync`, `fpscap`, `fullscreen` (row change listeners
via `MindUi.apply_row_change` + boot; `ClientLauncher.java:220`, `Control.java:690-694`),
`uiEdgePadding` (`UI.java:253-270`), `drawhitboxes` (`Renderer.java:435`), `coreitems`
(`HudFragment.java:594`).

In-engine probes:
```
boot: Engine.get_max_fps() = 240; window_get_vsync_mode() = ENABLED
settings_set("fpscap", 60)      -> 60           (restored 240)
settings_set("vsync", false)    -> DISABLED     (restored true)
settings_set("uiEdgePadding",60)-> margin_left/right 60, margin_top 0 (landscape; restored 0)
settings_set("drawhitboxes",true)-> Renderer/DebugCollision.visible true (restored false)
settings_set("coreitems", false) -> CoreItems.visible false (restored true)
fullscreen: the call runs but Xvfb (no WM) ignores the mode change (window_get_mode stays
WINDOWED even for a direct DisplayServer.window_set_mode(FULLSCREEN))
```

Already-reading rows (verdict "read", no change): `saveinterval`, `blockreplace`,
`conveyorpathfinding`, `hints`, `buildautopause`, `distinctcontrolgroups`, `doubletapmine`,
`commandmodehold`, `uiscale`, `screenshake`*, `lasersopacity`, `bridgeopacity`, `drawlight`,
`destroyedblocks`, `blockstatus`, `minimap`, `smoothcamera`, `detach-camera`, `position`,
`mouseposition`, `fps`, `indicators`, `showweather`, `animatedshields`†, `bloom`†,
`skipcoreanimation`, `alwaysmusic`, `musicvol`, `sfxvol`, `ambientvol`, `console`,
`showperformance`, `modcrashdisable`.
(*) `screenshake` has the camera field + `set_screenshake` but no reader (listed as inert below);
(†) `animatedshields` is not a Java key at all, `bloom`'s port hits are a stage name only —
both counted inert.

Still inert (recorded, each needs a renderer/feature seam that does not exist yet):
`communityservers`, `savecreate`, `backgroundpause`, `screenshake`, `bloomintensity`,
`bloomblur`, `chatopacity`, `unitlaseropacity`, `playerchat`, `playerindicators`, `showpings`,
`showotherbuildplans`, `animatedwater`, `hidedisplays`, `logiclocalization`, `atmosphere`,
`effects` (`MindFx.set_effects` exists, nothing reads the setting), `pixelate`
(`MindRender.set_pixelate` exists, nothing reads the setting), `linear`.
Java has no reader for `maxmagnificationmultiplierpercent`/`minmagnificationmultiplierpercent`
either.

## T5 — input re-verification (evidence)

`runs/20261015-s4-I05-t5/notes.md` holds every output. Summary:

1. Queued plan (paused): click on ore (131,49) -> plans `[]`; `step(1)` ->
   `mechanical-drill {x:131,y:49,progress:0.1667,stuck:false}`; `step(200)` -> built
   (`tile_debug(131,49).name = "mechanical-drill"`). PASS (with the deviation below).
2. Paste: F-copy 3 conveyors (134..136,58) -> paste at (140,58) -> plans at (139..141,58)
   rotation 0, QUEUED. PASS.
3. Rebuild: `copper-wall` at (144,60) killed -> ghost list; B-drag -> queued plan;
   `construct_progress` 0.5 after `step(20)`; built after `step(200)`, ghost list empty. PASS.
4. Command mode: 2 daggers drag-selected -> `selection_json [1000000001,1000000002]`;
   right-click -> `command_mode_objective()` true after `step(1)`; mode off -> false. PASS.
5. Tap-unit deposit: 30 copper on the alpha -> press on the alpha, release on a duo ->
   `player_stack_json()` = 15; a second tap -> `{}`. PASS.
   Extra (PAR-0209 condition): a sorter configured to copper, middle-click picked, then placed
   -> the new sorter carries `{"type":"item","id":0}` (`Block.lastConfig` -> `nextConfig`).

## Deviations (Java file:line)

1. **T1 (c) probe**: the plan's "MindCampaign getter that reads `campaignselect`" does not exist
   (`grep campaignselect gdext/campaign.rs` -> nothing; the reader is `planet_dialog.gd`). Used
   the equivalent cross-facade probes (T1 (a) MindUi write -> MindCampaign read; T1 (c)
   `duo-unlocked` write -> `MindCampaign.block_catalog_json`).
2. **T2**: Java's `player.locale = loc.toString()` has no port field (no-op);
   `Fonts.loadExtraFonts` is `ja`-only per `Fonts.java:101-119` (the plan listed zh/ko too);
   the dialog's display-name sort uses `nocasecmp_to` (code point) vs Java's
   `compareToIgnoreCase` (UTF-16) — same order for the shipped names.
3. **T3**: `unlocked(tryBlock)` (`PlacementFragment.java:169`) is enforced as far as the bridge
   registry allows (`placeable_player`, build visibility, not removed); the bridge's registry has
   no live unlock store (`MemoryUnlockStore`), so the tech-unlock half stays with the campaign
   placement gate. `state.rules.editor` floor/overlay picking and `menuHoverBlock` (palette
   hover) are not modelled.
4. **T4**: `fullscreen` cannot be observed changing on Xvfb (no window manager); all other fixed
   rows were probed live.
5. **T5**: while the sim is paused, an input placement command waits for the tick boundary
   (`desktop.rs enqueue_commands` -> `SimCommand::Place`; Java's `InputHandler.placeBlock` calls
   `player.unit().addBuild` directly), so the recipe's "plan listed right after the click" holds
   after `step(1)` in the port. Not user-visible (a paused game shows a dialog that owns the
   clicks). Also: the port's `try_drop_items` pause gate is Java's own
   (`InputHandler.java:2289` `|| state.isPaused()`), so checks 3-5 ran unpaused.
6. **Test harness notes**: a click on a dialog button must be one `godot_input` *sequence*
   (press+release); a *world* click needs the press and the release as separate calls. A failed
   eval (`get_node` on a missing path, `Array.get` with the wrong arity) stalls the frame loop;
   recovery is `tools/session-play.sh 4 --play`.

## Blockers

1. **`git merge --no-edit main` cannot run**: `main` advanced to `046ab375` ("manual commit"),
   which tracks `parity-ledger.json`, while this worktree has `parity-ledger.json` as an
   untracked symlink to the main checkout's copy. The merge refuses
   ("untracked working tree files would be overwritten") and aborts; nothing was merged. No
   later main content is needed by I05 (the only new commit is that ledger/session-runtime
   commit). The coordinator should decide whether the ledger stays tracked on main.
2. The session-4 profile has no imported tech tree (9 -> 285 unlocks during the session), so
   imported preset sectors (e.g. 98) are locked; T1's cutscene control used Ground Zero with
   `abandon_sector()` instead.

## Ledger changes to apply (coordinator)

| Item | After |
|---|---|
| PAR-0164 | verified (I05 T1, commit `42962c0b`) |
| PAR-0135 | verified (I05 T2, commit `f61f3bbc`) |
| PAR-0214 | verified (I05 T3, commit `1d40819f`) |
| PAR-0209 | verified (I05 T5 extra: sorter pick -> place carries the config; see T5 notes) |

Suggested notes for the ledger (not applied here): the T4 inert-row list above (19 keys, each
with its Java reader in `runs/20261015-s4-I05-t4/notes.md`) and the T5 pause/queue deviation.

## Done criteria

All five tasks are committed with their checks; the suites are green (mind-core 2554,
mind-headless 133, mind-gdext 48, clippy/fmt clean, goldens 97 PASS, parse gate clean); this
summary is written. T4's fixes cover the graphics/dev rows that had seams; the inert rows are
recorded with their Java readers and verdicts rather than silently claimed.
