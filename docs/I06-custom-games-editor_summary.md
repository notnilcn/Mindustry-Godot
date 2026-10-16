# I06 — Custom games, rules, saves import, map previews and the map editor — summary

Plan: `docs/parity-plans/I06-custom-games-editor.md`; environment: `docs/parity-plans/I-environment.md`.
Branches: session 5 = `parity/session-5` in `Mindustry-Godot-session5` (bridge 7010, display :14).

# Pass 1 (session 5, 2026-10-15)

Status: **T1-T4 done and verified in-engine; T5-T12 not reached.**

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T1 custom maps load + gamemode + session (Fixes PAR-0133) | done | `runs/20261015-s5-I06-T1/` (notes.md, 01-12 png); commit 41c7804f |
| T2 Map Play dialog per Java (PAR-0133 UI) | done (preview image pending T5) | `runs/20261015-s5-I06-T2/`; commit f96b277a |
| T3 Custom Rules dialog rows + search + edit menu (PAR-0132 part 1) | done | `runs/20261015-s5-I06-T3/`; commit 777d30e0 |
| T4 pause-menu rules button applies live (Fixes PAR-0132) | done | `runs/20261015-s5-I06-T4/`; commit 67bde842 |
| T5 map previews (Fixes PAR-0114) | not reached | — |
| T6 load dialog import + card actions (Fixes PAR-0134) | not reached | — |
| T7 editor rendering (Fixes PAR-0109) | not reached | — |
| T8 editor menu + sub-dialogs (PAR-0110) | not reached | — |
| T9 New Map + editor maps dialog (Fixes PAR-0110) | not reached | — |
| T10 editor exit confirm (Fixes PAR-0111) | not reached | — |
| T11 map view pan/pick/zoom (Fixes PAR-0113) | not reached | — |
| T12 sub-dialog fidelity (PAR-0110 residual) | not reached | — |

No `skipped: waiting for <dep>` lines: T1 defined the I-INDEX §3 seams itself
(`load_map_world`, `start_map_mode`, `Map::apply_rules`, `MindPreview.map_modes`,
`map_rules_json`), and T2/T4's only cross-plan dependency (T3's `rules_edited`)
landed in this same pass.

## Commits (`git log --oneline main..HEAD`)

```
67bde842 Fixes PAR-0132: in-game rule editing from the pause menu applies live
777d30e0 PAR-0132: Custom Rules dialog rule rows, search and edit menu
f96b277a PAR-0133: Map Play dialog modes, help, customize, preview and high score
41c7804f Fixes PAR-0133: custom maps load, apply the selected gamemode and start a session
```

## Tests (exact commands + results)

From `WT/client/rust` with `L="flock /tmp/opencode/mg-build.lock"; export CARGO_BUILD_JOBS=6`:

* `$L cargo test -p mind-core --lib --features msav-import` →
  `test result: ok. 2559 passed; 0 failed; 4 ignored` (baseline 2551; +8:
  `apply_rules_sandbox_sets_infinite_resources`, `apply_rules_keeps_map_tag_overrides`,
  `map_view_reads_scan_facts`, `caldera_map_loads_with_player_core`,
  `wave_map_hooks_expose_the_builtin_table`, `read_into_accepts_arc_bare_keys`,
  `read_coerces_whole_floats_for_integer_fields`,
  `live_set_rules_reaches_the_runtime_and_survives_the_mirror`).
* `$L cargo test -p mind-headless --lib` → `ok. 133 passed; 0 failed`.
* `$L cargo test -p mind-gdext --lib` → `ok. 47 passed; 0 failed`.
* `$L cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` → clean.
* `cargo fmt -p mind-core -p mind-gdext` → applied, no diff left.
* `$L cargo run -q -p mind-headless -- parity goldens` → 97 PASS, 0 non-PASS.
* `bash tools/godot.sh --headless --editor --quit --path client 2>&1 | grep -E "SCRIPT ERROR|Parse Error"` → nothing.
* `tools/build.sh` → `Finished dev profile` + `sync_scenarios: synced 11 file(s)`.
* Panic count in the newest session log after every in-engine step: 0.

## In-engine evidence (per task, with the quoted evals)

T1 (`runs/20261015-s5-I06-T1/`):
* `$MX.map_modes("Ancient Caldera")` → `{"attack": false, "editor": true, "pvp": false, "sandbox": true, "survival": true}`.
* Click path Custom Game > Ancient Caldera > Sandbox > Play →
  `get_rules_json()` = `{"infinite": true, "waveTimer": false, "waves": true, "spawns": 27, "loadout": [{"amount":200,"item":"copper"}]}`,
  `get_hud_state().hasCore == true`, `has_player_unit() == true` (`05-play-sandbox.png`).
* Survival: `start_map_mode("Ancient Caldera","survival",false,"")` → `waves:true, waveTimer:true, waveSpacing:7200`;
  `run_wave()` + `step(30)` → `{"wave": 2, "units": 3}` (`06-survival-wave.png`, "Wave 2 / 2 Enemies Remaining").
* Invalid map: `start_map_mode("Capital Ship Crash Site", …)` → false + "Error loading map: corrupted or invalid map file." with the menu restored (`10-nospawn-error.png`).
* Coreless map: `start_map_mode("pvp","sandbox",false,"")` → false + "This map does not have any cores for the player to spawn in! Add a **sharded** core to this map in the editor." (`12-map-nospawn.png`); `MenuGroup.visible == true`.
* `load_map_world("definitely-not-a-map")` → false, `last_map_error() == "map.invalid"`.

T2 (`runs/20261015-s5-I06-T2/`):
* `map_high_score_key("Ancient Caldera")` → `"hiscorecaldera"`; `settings_get_int(...,0)` → 16; the dialog shows "High Score: 16" (`01-map-play-caldera.png`, vs `step-01-mapplay-dialog.png`).
* Caldera buttons: `{survival pressed: true, attack.disabled: true, pvp.disabled: true}`.
* "4 corners of death" (the Java reference's map): `disabled: [true,false,false,false], pressed: [false,true,false,false]`, no high-score line (`05-map-play-4corners.png`).
* Sandbox click → `_rules_json` `infiniteResources: true, waveTimer: false`; Play starts sandbox (`03-sandbox-selected.png`, `04-sandbox-play.png`).
* `?` → mode help prompt (`02-mode-help.png`); Customize → `dialog_stack: ["custom","map_play","custom_rules"]` (`06-customize-rules-open.png`).

T3 (`runs/20261015-s5-I06-T3/` vs `step-316-rules.png`):
* Full dialog renders the nine categories in order; waves block matches the reference row for row (checked states, greyed "Waves Wait For Enemies", "Map Ends After Wave" 0, "Wave Spacing: (sec) 120.0", "Drop Zone Radius: (tiles) 37.5") — `03-custom-rules-final.png`.
* Search "wave" → `_rows` 190 → 9, only wave rows (`04-search-wave.png`, `05-search-wave-clean.png`).
* Edit menu (`06-edit-menu.png`); copy puts the spawns-less rules JSON (3024 bytes) on the clipboard.
* Sharded collapser expands to its 21 `TeamRule` rows (`08-team-collapser.png`).
* Hide emits `rules_edited` (log `T3_RULES_EDITED len= 5599 waves= true`).

T4 (`runs/20261015-s5-I06-T4/`, vs `step-374-pause.png`):
* Pause menu shows the book button bottom-left (node `customize`, 70x70, visible) — `01-pause-menu.png`.
* Book → `dialog_stack() == ["paused","custom_rules"]` — `02-rules-over-pause.png`.
* Untick `@rules.fire` → Back → `get_rules_json().fire == false` and still `false` after `step(10)` — `03-after-live-edit.png`.
* Ground Zero sector: `allowEditRules == false`, the book node `visible == false` — `04-campaign-pause.png`.
* `set_campaign_rule("serpulo","hideSpawns",true)` → true, the stored `CampaignRules` flips (restored afterwards).

## Deviations from Java (and noteworthy fixes)

1. **`JsonIo::read_into` did not accept Arc-lenient text** (bare keys). Every authored
   map whose `rules` tag is fastjson-style (all `assets/maps/default/*`, e.g.
   `caldera.msav`) silently fell back to `Rules::default()`: the gamemode preset AND
   the map's rules were dropped (`Map.rules(base)`, `Map.java:101-115`). Fixed by
   retrying through `quote_bare_keys`. (T1 commit.)
2. **`JsonIo::read`/`read_into` did not coerce whole floats to integers**, so every
   rules blob edited in GDScript failed to parse in Rust (`1.0` -> `u8`). Arc's
   `Json.readValue` coerces; fixed with `coerce_whole_floats`. (T4 commit.)
3. **`MindTable._grid()` counted invisible cells**, so the team collapsers inflated
   every row and pushed the Custom Rules body off-screen. It now skips invisible
   cells like every Godot Container and Arc's `Table`. (T3 commit.)
4. `set_campaign_rule` takes `(planet, key, value)` — the plan's 2-arg signature did
   not match the live caller `campaign_rules_dialog.gd` (`callv([_planet, key, value])`).
5. T3's Custom Rules rows are transcribed imperatively (one function per Java
   condition) instead of the plan's data array with named predicates; the
   `@configure`/`@banned*`/`@rules.weather`/planet/ambient-light sub-dialogs are
   code-built overlays and the weather/ban editors are compact (see the T3 notes).
6. T1's `map_play_dialog.gd` mode buttons are not disabled per `map_modes` until
   T2's rebuild (both landed here); the plan's T1 acceptance line "Play button
   disabled for unsupported modes" is satisfied by T2.
7. `anthill_v5.msav.msav`, `CrashSite.msav.msav` and
   `Attack_Secret_Island_Outpost_1.msav.msav` (user maps) fail to import:
   `JSON error: invalid type: map, expected a set` and `Error reading region "map"`.
   Pre-existing MSAV legacy-import gaps, unrelated to T1 (Java loads them).

## Blockers / notes

* **`git merge --no-edit main` conflicts** (attempted before T2/T3/T4, aborted each
  time per I-environment §1.0): main now carries I05 pass 1 (`546ceaab`); the single
  conflict is `gdext/campaign.rs` — main's I05 T2 replaced
  `self.live_setting_flag("skipcoreanimation")` with
  `settings_hub::with_settings(|store| store.get_bool("skipcoreanimation", false))`
  inside the old `start_map` body, which I06 T1 refactored into
  `start_loaded_world` (and T4 touched nearby). Resolution for the coordinator:
  keep I06's `start_loaded_world`/`start_map_mode` and adopt the settings-hub call
  in place of `live_setting_flag("skipcoreanimation")`; keep both sides' additions
  elsewhere. Everything else in that merge is non-conflicting.
* `tools/session-play.sh 5` had to be used with `--play` after one frozen frame
  loop (a timed-out `await` eval of mine); recovered with `--play`, no side effects.

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0133 | verified (T1 + T2, real click path + screenshots) |
| PAR-0132 | verified (T3 + T4: rows/search/edit menu + live pause-menu edits) |
| PAR-0114 | not reached (T5) |
| PAR-0134 | not reached (T6) |
| PAR-0109 | not reached (T7) |
| PAR-0110 | not reached (T8/T9; T12 residuals) |
| PAR-0111 | not reached (T10) |
| PAR-0113 | not reached (T11) |

New items worth adding (suggested):
* "Authored maps' fastjson `rules` tag silently fell back to `Rules::default()`
  (gamemode + map rules dropped)" — fixed in 41c7804f.
* "UI-edited rules JSON failed to parse (whole floats vs integer fields)" — fixed
  in 67bde842.
* "`MindTable` counted invisible cells (collapsers overflowed the dialog)" — fixed
  in 777d30e0.

## Continuation notes (for pass 2)

* Start with T5 (`Fixes PAR-0114`): `MindPreview` already holds a lazily built
  registry and `MapIo::generate_preview_with_facts`; `PreviewPipeline` +
  `read_cache/write_cache` exist, and `texture_for_name` is wired in the map-play
  dialog (currently returns null → the preview BorderImage is skipped). The
  preview cache write must use the generate facts (`Map.spawns`/`teams`) or
  `map_modes` keeps re-scanning on every open.
* Then T6..T12 in plan order; T7/T8/T9/T10/T11/T12 are all still untouched.
* `I06 T1`'s seams are on the branch and ready for I08 T4/T5 and I07 T6
  (`load_map_world`, `start_map_mode`, `start_loaded_world`).

# Pass 2 (session 5, 2026-10-15)

Status: **T5, T6, T7 and T10 done and verified in-engine; T8, T9, T11, T12 not
reached.** Pass 2 started from main @ `0361ad33` (I06 pass-1 merge + the
coordinator's `4f95c4e0` settings-hub fix + I01/I03/I07 wave merges).

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T5 map previews (Fixes PAR-0114) | done | `runs/20261015-s5-I06-T5/` (notes.md, 01–08 png); commit 49f0268b |
| T6 load dialog import + card actions (Fixes PAR-0134) | done | `runs/20261015-s5-I06-T6/`; commit e179d34b |
| T7 editor rendering (Fixes PAR-0109) | done | `runs/20261015-s5-I06-T7/`; commit 0eae7c22 |
| T8 editor menu + sub-dialogs (PAR-0110) | not reached | — |
| T9 New Map + editor maps dialog (Fixes PAR-0110) | not reached | — |
| T10 editor exit confirm (Fixes PAR-0111) | done (ahead of T8/T9; the menu `@quit` half is T8's) | `runs/20261015-s5-I06-T10/`; commit 4540431d |
| T11 map view pan/pick/zoom (Fixes PAR-0113) | not reached | — |
| T12 sub-dialog fidelity (PAR-0110 residual) | not reached | — |

No `skipped: waiting for <dep>` lines: T5–T7/T10 had no unlanded providers
(T6's `file_chooser` and T10's `MindUi.show_confirm` already existed).

## Commits (`git log --oneline main..HEAD`)

```
4540431d Fixes PAR-0111: leaving the map editor asks for confirmation
0eae7c22 Fixes PAR-0109: the map editor renders floor, walls, blocks and team borders
e179d34b Fixes PAR-0134: Load dialog imports saves; card autosave/delete/rename/export actions work
0361ad33 Merge branch 'main' into parity/session-5
49f0268b Fixes PAR-0114: map previews are generated, cached and shown in the map lists
```

`0361ad33` merged main cleanly (no conflicts; the T5 branch fixes for
`settings_get_int` / `start_map_mode`'s `savecreate` converged with main's
`4f95c4e0`).

## Tests (exact commands + results)

From `WT/client/rust` with `L="flock /tmp/opencode/mg-build.lock"; export CARGO_BUILD_JOBS=6`:

* `$L cargo test -p mind-core --lib --features msav-import` → `ok. 2603 passed;
  0 failed; 4 ignored` (baseline 2599 after the wave merges; +4 new:
  `preview_queue_batches`, `create_new_preview_writes_scan_facts_into_cache`,
  `map_preview_matches_java_rows_and_colors`, and the wave-merge tests).
* `$L cargo test -p mind-headless --lib` → `ok. 133 passed`.
* `$L cargo test -p mind-gdext --lib` → `ok. 49 passed`
  (+1: `chunk_constants_match_java`).
* `$L cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` →
  clean.
* `$L cargo run -q -p mind-headless -- parity goldens` → 97 PASS / 0 other.
* `bash tools/godot.sh --headless --editor --quit --path client 2>&1 | grep -E "SCRIPT ERROR|Parse Error"` → nothing (run after every `.gd`/`.tscn` change).
* `tools/build.sh` → `Finished dev profile` + `sync_scenarios: synced 11`.
* Panic count in the newest session log after every in-engine step: 0.

## In-engine evidence (per task, quoted output)

T5 (`runs/20261015-s5-I06-T5/`): `open_dialog("custom")` →
`generate_previews()` 27 queued (24 generated; the three known-bad user maps
from pass 1 fail), `ls $U/previews/*_v2.png | wc -l` = 24;
`archipelago_v2.png` top colours `(61,73,128)` water 93747 px,
`(255,223,188)`, `(112,187,103)`, `(71,84,143)`, `(167,137,111)` — the Java
reference histogram verbatim (`java/step-187-maplist.png`);
`04-custom-game-final.png` grid of previews; `05-map-play-preview.png` (the
Map Play BorderImage preview); `save_slot("t5-preview")` wrote
`$U/previews/save_slot_t5-preview.png` + `save_preview_texture != null`
(`08`). Two pre-existing Java-pixel bugs were fixed (air `mapColor` sentinel;
`CachedTile.setBlock` flipped row) — see the commit body.

T6 (`runs/20261015-s5-I06-T6/`): facade `import_save("/tmp/bad.msav")` →
`"invalid"`, `import_save("/tmp/imp.msav")` → `""`, `list_save_slots` 31 → 32,
sector save → `"nocampaign"`; dialog `_on_file_chosen(bad)` → `@error.title` /
`@save.import.invalid` prompt (`01`); `_rename_slot` prompt (`02`) +
`resolve_text_input("t6-renamed")` → `save-t5-preview-name` = `t6-renamed`
(`05`); `_set_slot_autosave(..., false)` → settings key false, card
“Autosave: Off”; `_export_slot` → chooser stacked (`03`) and the exported file
byte-identical (`cmp`); `_delete_slot` confirm (`04`, Cancel keeps the file) →
`resolve_confirm(true)` removes `24.msav` + its settings key (32 → 31).

T7 (`runs/20261015-s5-I06-T7/`): `begin_edit_map(caldera.msav)` +
`EditorDialog.show_dialog()` renders water/sand/stone/ore + the map's core
(`01`/`05`) with the Java reference palette (`java/step-310-editor-open.png`);
`EditorWorldView.baked_chunks()` = 25 (5x5 x 60), `dirty_chunks()` = 0; a
`godot_input` pencil drag over tiles 200..249 bumps `tiles_revision` 1 → 7
(`can_undo()` true) and the line appears without reopening (`02`); core-shard
placements show icon + team border (`03`), a conveyor click shows the icon
(`06`).

T10 (`runs/20261015-s5-I06-T10/`): draw (`can_undo()` true) → Back → confirm
“Are you sure you want to exit? Any unsaved changes will be lost.” above the
editor (`01`); Cancel keeps the editor open with the edit (`02`); OK hides the
editor and opens `editor_maps` (`03`, `dialog_stack == ["editor_maps"]`).

## Deviations / residuals

1. T7 does not introduce the plan's `FloorSource` trait: the editor renderer
   bakes with the shared `tile_quads`/`RegionAtlas` code directly (identical
   output; `FloorRenderer` behaviour unchanged apart from the extraction).
2. T7 has no shadow/darkness pass; building icons draw `block.region`
   unrotated (`Block.rotate`/`rotateDrawEditor` are not ported) with the base
   region standing in for `fullIcon`; invalidation is the
   `tiles_revision` counter rather than the `context.rs` tile-op recorder.
3. T5/T6 residuals: the Java map-list sort (`MapPriority.recent` via file
   mtime) and `error.png` fallbacks, the multi-file import (`submitMulti`),
   external-asset save export (always `@save.export.needsload`), and the
   fallback chooser's missing “save as” commit (same as
   `editor_maps_dialog.gd`; native path is plan 22/I07).
4. `MindPreview` still boots its own registry with `create_base_content`
   instead of I07 T4's `content_boot::build_registry` seam (the seam landed
   after the T5 commit); behaviour identical for base content, mod content in
   previews/map lists is a follow-up.
5. T10 was done before T8/T9 (the order says T8 first): the Back/Escape halves
   are complete and verified; the menu `@quit` half is untouched and belongs
   to T8's `editor_menu`. This was a deliberate budget call so the pass ends
   with a committed item instead of a partial T8.
6. The `include/preview`/`save` flow drives the model through `MindCampaign`;
   the port's prompts need `_prompt_cancel.call()` to free after a
   programmatic `resolve_confirm`/`resolve_text_input` (recorded in the T6
   notes).

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0114 | verified (T5: 24 previews generated from the shipped block colours, Custom Game grid + Map Play + save-slot previews; Java reference histogram match) |
| PAR-0134 | verified (T6: dialog import of valid/invalid/sector saves; card rename/autosave/export/delete with the Java prompts) |
| PAR-0109 | verified (T7: editor renders floor/walls/blocks/team borders; live pencil drag re-bakes; vs `step-310-editor-open.png`) |
| PAR-0111 | verified (T10: Back/Escape confirm; Cancel keeps, OK exits to the maps list) |
| PAR-0110 | partially: T8/T9/T12 not reached (menu, New Map, per-map actions, sub-dialog fidelity still missing) |
| PAR-0113 | not reached (T11) |

New items worth adding (suggested):
* "Map preview pixels ignored `sprites/block_colors.png`; air's R9 fallback
  painted over every floor; walls were vertically mirrored against floors"
  — fixed in 49f0268b (test `map_preview_matches_java_rows_and_colors`).
* "`map_editor_dialog`'s Back/Escape left without the Java confirm; `MindUi`
  prompts rendered behind `Ui/EditorDialog`" — fixed in 4540431d.

## Done criteria

Not all met: T8, T9, T11, T12 are unreached. All landed tasks are committed,
the suites/parse gate/build are green, and the worktree is clean (no stashes).

# Pass 3 (session 5, 2026-10-16)

Status: **T8, T9 and T11 done and verified in-engine; T12 not reached.** Pass 3
started from main @ `498c4a12` (I07 pass 2 + the pass-2 merge).

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T8 editor menu + sub-dialogs (PAR-0110) | done | `runs/20261015-s5-I06-T8/` (notes + 01-09 png); commit e4374820 |
| T9 New Map + editor maps dialog (Fixes PAR-0110) | done | `runs/20261015-s5-I06-T9/` (notes + 01-06 png); commit a7622e77 |
| T11 map view pan/pick/zoom (Fixes PAR-0113) | done | `runs/20261015-s5-I06-T11/` (notes + 01 png); commit 5599a522 |
| T12 sub-dialog fidelity (PAR-0110 residual) | not reached | — |

No `skipped: waiting for <dep>` lines: T8/T9/T11 had no unlanded providers.

## Commits (`git log --oneline main..HEAD`)

```
5599a522 Fixes PAR-0113: editor map view pans, zooms and picks like MapView
a7622e77 Fixes PAR-0110: editor maps dialog with New Map, per-map actions and previews
e4374820 PAR-0110: editor menu and map info hub reach every editor dialog
```

`git merge --no-edit main` was clean before and after this pass (no conflicts;
main already carried the pass-2 commits via `0f3bf6a2`).

## Tests (exact commands + results)

From `WT/client/rust` with `L="flock /tmp/opencode/mg-build.lock"; export CARGO_BUILD_JOBS=6`:

* `$L cargo test -p mind-core --lib --features msav-import` → `ok. 2604 passed;
  0 failed; 4 ignored` (new `editor_save_with_ecs_keeps_core_team`).
* `$L cargo test -p mind-headless --lib` → `ok. 133 passed`.
* `$L cargo test -p mind-gdext --lib` → `ok. 50 passed`.
* `$L cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` →
  clean (0 findings).
* `$L cargo run -q -p mind-headless -- parity goldens` → 97 PASS / 0 other.
* GDScript parse gate → clean after every `.gd`/`.tscn` change (one
  `var delta :=` inference error was caught by the gate and fixed).
* `tools/build.sh` → `Finished dev profile` + `synced 11 file(s)`.
* Panic count in the session log after the in-engine steps: 0.

## In-engine evidence (per task)

T8 (`runs/20261015-s5-I06-T8/`): Escape opens `editor_menu` (ten rows,
`01-editor-menu.png`); `map_info`, `map_generate`, `map_resize`,
`editor_import`, `editor_export`, `sector_generate` all open (stack checks;
`map_generate.applied` flips per entry point); `editor_import/export` show the
`createDialog` rows (`03`/`04`); `map_info` fields + all seven routes open;
built-in Save → `@editor.save.overwrite` (`05`), new-map Save →
`status().saved == true` (`06`); Playtest → `map_play` (`_playtest = true`,
`07`) → Play → `has_player_unit() && hasCore`, editor hidden (`09`), and a
drawn core survives `load_map_world` with `building_team_at == 1`.

T9 (`runs/20261015-s5-I06-T9/`): the maps dialog shows the preview grid + New
Map/Import Map (`01`/`03`); New Map → prompt (`02`) → "t9-test" opens an empty
**200x200** map; core+drag Save → `status().saved == true`, `t9-test.msav` +
`previews/t9-test_v2.png` exist and the tile shows the preview; the per-map
dialog shows preview/name/author/description + Open In Editor/Delete (`04`);
Delete → `map.delete` confirm (`05`) → map + preview removed; a built-in map
shows `custom == false`, author "Anuke", Delete disabled (`06`); Open In Editor
loads Caldera (256x256).

T11 (`runs/20261015-s5-I06-T11/`): MMB drag at zoom 1 moves the view offset
0,0 → 50,10; W held ~2.3 s moves `offset_y` 10 → 43.9; Shift → `pick`, LMB →
`draw_block() == "basalt"`, release → `pencil`; wheel 1.0 → 1.1 and
`map_view_zoom(±100)` clamps to 20.0/0.2; RMB hold → `eraser` and back
(`01-map-view-zoom.png`).

## Deviations / blockers

1. T8: Steam workshop publish skipped; `Icon.resize` has no `MindIcons` alias
   (Resize uses `grid`); `@editor.saved` uses the info dialog
   (`show_info_fade` has no publisher); `editInGame` saves then starts
   `start_map_mode(name, "editor", playtest = true)`.
2. T8 fixed two blocking gaps found while wiring the menu: editor placement now
   creates building entities (`EditorWorldHooks`) and `save`/`export` write the
   Java `writeAll` building chunk (`EditorMapSource::with_ecs`), and
   `MapEditor::draw_team` now defaults to `Team.sharded` — before this a drawn
   core had no team and every playtest failed `no core for team 1`.
3. T9: single-file import (no `submitMulti`); the fallback chooser cannot commit
   a fresh export name; the map list stays registry-ordered.
4. T11: WASD reads physical keys instead of the rebindable `Binding.moveX/Y`
   table; `resetSaved` fires on non-edit tools too.
5. Noted, out of scope: `ecs::TeamId::SHARDED` is a stale `TeamId(0)` while
   `game::team::SHARDED` is `1`; T12 is the whole sub-dialog fidelity pass
   (wave info, map generate, banned content, objectives, locales, processors,
   resize, sector generate, assets).

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0110 | verified for T8+T9 (editor menu, map info, all sub-dialogs reachable; editor maps dialog with New Map/per-map actions/previews); T12 residuals listed above |
| PAR-0113 | verified (T11: MMB/WASD pan, Shift/Alt pick, wheel zoom clamped, RMB eraser) |

## Done criteria

Not all met: **T12 remains** (the sub-dialog fidelity pass). All landed tasks
are committed, the suites/parse gate/build are green, and the worktree is clean
(no stashes).

# Pass 4 (session 5, 2026-10-16)

Status: **T12 started: 1 of the 9 dialogs done (wave info); 8 remain**
(map generate, banned content, objectives, locales, processors, resize,
sector generate, assets). Pass 4 started from main @ `3d029285` (the pass-3
merge + I01/I03 pass 2).

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T12.1 wave info dialog | done | `runs/20261015-s5-I06-T12/` (notes + 01-04 png); commit 3ddad18c |
| T12.2..T12.9 (map generate, banned content, objectives, locales, processors, resize, sector generate, assets) | not reached | — |

The map-resize rewrite was attempted and **reverted**: the faithful version
(the bundle-keyed labels, Cancel/OK buttons, validation and the unchanged-guard)
made the dialog stop rendering entirely (visible/in-tree with modulate 1.0 but
nothing drawn; the stock version renders — `/tmp/resize-orig.png`), for reasons
not isolated inside the remaining budget. `map_resize_dialog.gd` is byte-for-byte
the pre-pass version; the dialog stays on the T12 list.

## Commits (`git log --oneline main..HEAD`)

```
3ddad18c PAR-0110: wave info dialog per WaveInfoDialog
```

## Tests (exact commands + results)

From `WT/client/rust` with `L="flock /tmp/opencode/mg-build.lock"; export CARGO_BUILD_JOBS=6`:

* `$L cargo test -p mind-core --lib --features msav-import` → `ok. 2639 passed;
  0 failed; 4 ignored`.
* `$L cargo test -p mind-headless --lib` → `ok. 133 passed`.
* `$L cargo test -p mind-gdext --lib` → `ok. 50 passed`.
* `$L cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` →
  clean (0 findings).
* `$L cargo run -q -p mind-headless -- parity goldens` → 97 PASS / 0 other.
* GDScript parse gate → clean; `tools/build.sh` finished; panics 0.

## In-engine evidence (T12.1)

`runs/20261015-s5-I06-T12/`: `01-wave-info.png` (28 vanilla groups with
begin/copy/effect/unit/remove rows), `02-wave-expanded.png` (every Java field
row + the graph), `03-unit-picker.png` (69 units via the new `list_select`),
`04-guardian.png` (the check writes `effect = "boss"`). The `@add` pick appends
a group (28 → 29) and writes the spawns tag; `default_waves_json` supplies the
vanilla fallback when the tag is empty.

## Deviations / blockers

1. Unit/effect icons, the `@waves.spawn.first` cap notice, Java's modal
   sort/edit menus (inline strips here) and the header middle-click copy are
   still missing in the wave dialog (listed in its notes).
2. The `list_select` dialog is new and generic; later T12 dialogs can reuse it
   for their content pickers.
3. The map_resize rendering regression above is unresolved; keep the stock
   dialog until someone can diff it in a live editor session.

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0110 | partial: T8/T9 verified in pass 3; T12.1 (wave info) verified; 8 sub-dialogs remain (map generate, banned content, objectives, locales, processors, resize, sector generate, assets) |

## Done criteria

**Not met**: T12.2–T12.9 remain. Every landed task is committed, the suites,
parse gate and build are green, and the worktree is clean (no stashes).

# Pass 5 (session 5, 2026-10-16)

Status: **no new commits.** Main (now `90d2b688`, carrying I02 pass 1, I03 pass 3
and I01 pass 3 on top of the pass-4 merge) was merged cleanly into
`parity/session-5`; the branch fast-forwarded and is clean. T12.2–T12.9 were
surveyed against the Java sources but not landed inside the remaining budget.

## Per-task status

| Task | Status | Survey / remaining work |
|---|---|---|
| T12.2 map generate | not reached | Java 581 lines vs port 141: the missing part is the per-filter parameter editor (~440 lines of `BuildFilter`/`FilterOption` UI). Largest lift of the set. |
| T12.3 banned content | not reached | Port is a 71-line stub (search + two text labels). Needs bannable block/unit lists (`palette_blocks` rows / `wave_units_json`) and Java's two-pane icon grid (`BannedContentDialog.java:52-209`) toggling the `rules` tag JSON; `core/editor/banned.rs` already models add/remove/add_all/remove_all. |
| T12.4 objectives | not reached | Java 838 lines (+canvas); the port has the canvas + a small toolbar only. |
| T12.5 locales | not reached | Java 774 lines; the port has the list + a minimal editor. |
| T12.6 processors | not reached | Needs `LogicBlockState` on editor-placed processors (`EditorWorldHooks` creates only `Building`/`TeamComp`/`BuildingComp`, so `MindEditor.processors()` finds no tags) plus `@add`/rename/icon/delete facades and the in-game `showEditDialog` integration. |
| T12.7 resize | not reached | Java 67 lines; the faithful rewrite (bundle keys, Cancel/OK, validation, unchanged-guard) made the dialog stop rendering once (visible/in-tree/modulate 1.0 but nothing drawn; the stock version renders) and was reverted in pass 4. Retry by bisecting the rewrite one change at a time. |
| T12.8 sector generate | not reached | Java 112 lines: needs a planet list facade (`PlanetDef{accessible, generator, sectors}` exists) for the picker and the `[ NxN ]` size label (`Sector` has no `size` in the port); the Apply path is a heavy in-editor generation. |
| T12.9 assets | not reached | `MindEditor.assets()` returns empty — the plan-20 data-asset host (`ModDataManager`) is not mounted on the editor node, so the dialog would render empty. |

## Commits (`git log --oneline main..HEAD`)

```
(none)
```

## Tests / in-engine

No source change this pass; the full gate is main's (`2649 / 133 / 51`,
goldens 97 PASS per the coordinator). The merge was clean; no in-engine step
was run (nothing to verify).

## Deviations / blockers

* Budget: the session ended before any T12.2–T12.9 dialog could be
  completed and verified. Each entry above carries the concrete next step for
  a fresh pass.

## Ledger changes to report

None (PAR-0110 status unchanged from pass 4: T8/T9/T12.1 verified; eight
sub-dialogs remain).

## Done criteria

**Not met**: T12.2–T12.9 remain. The branch is clean, fast-forwarded to main,
with no stashes.


# Pass 6 (session 5, 2026-10-16)

Status: **T12.2–T12.8 worked; 6 of the 7 attempted sub-dialogs landed**
(banned content, processors, locales, resize, sector generate, objectives);
**T12.2 map generate not reached** and **T12.9 assets blocked**. Pass 6
started from main @ `90d2b688` (clean fast-forward, no conflicts) and the
worktree is clean with no stashes.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T12.1 wave info | done (pass 4) | `runs/20261015-s5-I06-T12/` |
| T12.2 map generate | **not reached** — the missing ~440 lines are the per-filter parameter editors (`GenerateFilter` option rows, preview pixmap, drag/mirror); the port still has the list + add/remove/move/randomize/apply half | — |
| T12.3 banned content | done | `runs/20261016-s5-I06-T12banned/`; commit 18e29957 |
| T12.4 objectives | done (first slice) | `runs/20261016-s5-I06-T12objectives/`; commit 37b486ee |
| T12.5 locales | done (core slice) | `runs/20261016-s5-I06-T12locales/`; commit 402c20c2 |
| T12.6 processors | done | `runs/20261016-s5-I06-T12processors/`; commit e442ef59 |
| T12.7 resize | done (the pass-4 regression is gone) | `runs/20261016-s5-I06-T12resize/`; commit 330a7041 |
| T12.8 sector generate | done | `runs/20261016-s5-I06-T12sectorgen/`; commit 77aeb02e |
| T12.9 assets | **blocked**: `MindEditor.assets()` is empty because the plan-20 data-asset host (`ModDataManager`) is not mounted on the editor node; the dialog would render empty | — |

## Commits (`git log --oneline main..HEAD`)

```
37b486ee PAR-0110: objectives dialog per MapObjectivesDialog (first slice)
77aeb02e PAR-0110: sector generate dialog per SectorGenerateDialog
330a7041 PAR-0110: map resize dialog per MapResizeDialog
402c20c2 PAR-0110: map locales dialog per MapLocalesDialog (core slice)
e442ef59 PAR-0110: world processors dialog per MapProcessorsDialog
18e29957 PAR-0110: banned content dialog per BannedContentDialog
```

## Tests (exact commands + results)

From `WT/client/rust` with `L="flock /tmp/opencode/mg-build.lock"`;
`export CARGO_BUILD_JOBS=6`:

* `cargo fmt -p mind-core -p mind-gdext`; every commit that touched Rust was
  followed by `$L cargo clippy -p mind-core -p mind-gdext --all-targets --
  -D warnings` -> clean (the last run after T12.4: `Finished`).
* `$L cargo test -p mind-core --lib --features msav-import` -> `ok. 2652
  passed; 0 failed; 4 ignored` (main gate was 2649; +3 new tests:
  `editor::banned::tag_json_round_trip_preserves_other_keys`,
  `editor::tests::editor_hooks_seed_logic_state` +
  `editor::processors` `free_processor_tile` assertion,
  `maps::planet::sector_size_matches_generation_and_seed_shifts`).
* `$L cargo test -p mind-gdext --lib` -> `ok. 51 passed`.
* GDScript parse gate after every `.gd`/`.tscn` edit -> clean.
* `CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh` ->
  `Finished \`dev\` profile` + `sync_scenarios: synced 11 file(s)`; the editor
  was restarted per I-environment §1 after every Rust build; panics 0 after
  every in-engine step.

## In-engine evidence (per task, bridge 7010)

* **T12.3** `runs/20261016-s5-I06-T12banned/`: window with search + 10
  category toggles + red/accent panes (`06-...png`); a real cell click
  writes `{"bannedBlocks":["graphite-press"]}` and swaps the panes; the
  turret category + both `@addall` buttons move 28 turrets in and out
  (`08-...png`); unit mode shows 56 cells with no category row
  (`09-...png`); search `"  wall  "` -> 25 rows, no match -> `@empty`;
  `custom_rules` caller mode keeps its working copy while leaving the editor
  tag untouched.
* **T12.6** `runs/20261016-s5-I06-T12processors/`: `@add` places at (0, 0);
  pencil -> `Name:` prompt -> tag `main`; icon cell -> `icon_select` ->
  `icon_tag 59406` with the row rendering the glyph (`07-...png`); name
  cell -> the logic editor in editor mode (`print "hello"`, 1 statement,
  privileged true); trash -> confirm -> `processors() == []`; search
  `"zz"`/`"ma"`.
* **T12.5** `runs/20261016-s5-I06-T12locales/`: current locale ensured;
  timer property card + value area (`03-...png`); `de` added and English
  selected -> the card tints accent for the missing `de` key
  (`05-...png`); Apply writes `{"de":{},"en":{"timer":...}}`; an edit +
  Back shows the translated `@editor.savechanges` confirm
  (`08-...png`), OK writes the tag, Cancel discards.
* **T12.7** `runs/20261016-s5-I06-T12resize/01-open.png`: the dialog renders
  100x80 + 0/0 + Cancel/OK (the pass-4 no-render regression is gone); width
  200 + real OK -> editor 200x80; `900` ignored; real Cancel leaves it.
* **T12.8** `runs/20261016-s5-I06-T12sectorgen/`: Erekir/0/0 + `[ 494x494 ]`
  (`01-...png`); the picker lists erekir (92)/serpulo (272); Serpulo ->
  `[ 334x334 ]` (`03-...png`); sector 500 rejected; Apply -> the editor
  becomes a 334x334 grid with `genfilters == "{}"` and the generated rules.
* **T12.4** `runs/20261016-s5-I06-T12objectives/`: the 13-class picker
  (`01-...png`); Timer default lands in the tag; the canvas + panel render
  and `duration = 3` -> `180.0` (`03-...png`); Destroy Block shows
  pos/team/block (`05-...png`); the block pick writes `router`; the panel
  type button changes class keeping `editorPos` (`06-...png`).

## Deviations / blockers (per task, full lists in each run's `notes.md`)

* T12.3: search also matches the raw name; no portrait branch, no
  `resized` rebuild.
* T12.6: no `Icon.eye` config-open (no input seam opens a building config by
  tile); `IconSelectDialog` has no `Icon.none` clear entry; the delete
  surrounded branch approximates `floor().wall` with `stone-wall`.
* T12.5: no `@locales.viewproperty`; `@locales.addicon` uses the accessible
  icon set only; no `resized(this::buildMain)`; `state.mapLocales` is not set
  (tag only).
* T12.7: Java disables OK while a size is out of range; the port ignores the
  value.
* T12.8: the generator structure passes (enemy base/ruins/launch loadout,
  sim-side `place_structures`) do not run in the editor; no `ui.loadAnd`
  loading screen.
* T12.4: markers, nested Objective/Seq/Map/Color fields, logic-code buttons,
  canvas drag/query and the node clipboard are still missing.
* T12.9: blocked on the plan-20 data-asset host (`ModDataManager`) mount.

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0110 | partial: T8/T9/T12.1 verified; T12.3, T12.5, T12.6, T12.7, T12.8 verified; T12.4 first slice; T12.2 (map generate) not reached; T12.9 (assets) blocked on the data-asset host |

## Done criteria

**Not met**: T12.2 and T12.9 remain (plus the T12.4/T12.5 residual lists).
Every landed task is committed, the Rust suites/clippy/parse gate are green
and the worktree is clean with no stashes.

# Pass 7 (session 5, 2026-10-16)

Status: **T12.2 (map generate) done and verified in-engine; T12.9 assets stays
blocked** (plan-20 data-asset host unmounted). Started from main @ `89b01296`
(clean fast-forward, no conflicts); worktree clean, no stashes.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T12.1 wave info | done (pass 4) | `runs/20261015-s5-I06-T12/` |
| T12.2 map generate | **done** | `runs/20261016-s5-I06-T12.2/` (notes + 10 png); commits c6a3807f, ca39eeb2 |
| T12.3 banned content | done (pass 6) | `runs/20261016-s5-I06-T12banned/` |
| T12.4 objectives | done (first slice, pass 6) | `runs/20261016-s5-I06-T12objectives/` |
| T12.5 locales | done (core slice, pass 6) | `runs/20261016-s5-I06-T12locales/` |
| T12.6 processors | done (pass 6) | `runs/20261016-s5-I06-T12processors/` |
| T12.7 resize | done (pass 6) | `runs/20261016-s5-I06-T12resize/` |
| T12.8 sector generate | done (pass 6) | `runs/20261016-s5-I06-T12sectorgen/` |
| T12.9 assets | **blocked** (unchanged): `MindEditor.assets()` is empty because the plan-20 data-asset host (`ModDataManager`) is not mounted on the editor node | — |

No `skipped: waiting for <dep>` lines: T12.2 had no unlanded providers.

## Commits (`git log --oneline main..HEAD`)

```
ca39eeb2 PAR-0110: map generate dialog per MapGenerateDialog
c6a3807f PAR-0110: map generate filter option model, block pickers and preview seam
```

## Tests (exact commands + results)

From `WT/client/rust` with `L="flock /tmp/opencode/mg-build.lock"`;
`export CARGO_BUILD_JOBS=6`:

* `cargo fmt -p mind-core -p mind-gdext`; `$L cargo clippy -p mind-core -p
  mind-gdext --all-targets -- -D warnings` -> clean.
* `$L cargo test -p mind-core --lib --features msav-import` -> `ok. 2661
  passed; 0 failed; 4 ignored` (+6 new: `options_follow_java_declarations`,
  `filter_options_json_merges_defaults`, `default_ores_match_stack_tail`,
  `normalize_filter_numbers_restores_integer_fields`,
  `predicates_round_trip_by_name`, `option_serialization_carries_field`).
* `$L cargo test -p mind-headless --lib` -> `ok. 133 passed`.
* `$L cargo test -p mind-gdext --lib` -> `ok. 51 passed`.
* `$L cargo run -q -p mind-headless -- parity goldens` -> 97 PASS / 0 other.
* GDScript parse gate after every `.gd`/`.tscn` edit -> clean.
* `tools/build.sh` -> `Finished \`dev\` profile` + `sync_scenarios: synced
  11 file(s)`; the editor was restarted per I-environment §1 after every Rust
  build; panics 0 after every in-engine step.

## In-engine evidence (T12.2, bridge 7010, Caldera 256x256, real clicks)

`runs/20261016-s5-I06-T12.2/`:
* `01-dialog-open.png`: dialog over the editor shell with 13 default cards
  (8 scatter + 5 ore) and the live 256x256 preview.
* `02-add-menu.png`: `@add` with 11 types (post filters hidden in applied
  mode) + Default Ores; Noise click -> 14 filters/rows.
* `03-noise-card.png` + slider click at 30% -> UI value and tag `scl` =
  146.590586322072.
* `04-block-picker.png`: `target` picker (167 `anyOptional` cells, air = X,
  localized tooltips) -> `target: "stone"` in the tag.
* Randomize -> all seeds change; Edit copy -> 1804-char clipboard; `Clear` ->
  `[]` + `@filters.empty` (`06`); `Load from Clipboard` -> 13 restored.
* Map-info mode: randomize on open; `@settings.reset` -> default stack seed 0;
  Back -> every tag seed 0.
* `07-mirror-line.png` / `08-mirror-drag.png`: axis + pivot drawn; drag ->
  `axisX 0.2484375`, `axisY 0.6`; rotate toggle -> `true`.
* Card copy (duplicate randomized) 14 -> 15, remove -> 14, move up -> order
  swap; Default Ores 13 -> 18.
* `10-logic-code.png`: the LogicFilter pencil opens the shared logic editor
  standalone (stack `["map_generate","logic"]`, `_filter_mode`, privileged);
  clipboard `print "gen"\nend` -> tag `code`.
* `09-applied.png`: Apply -> dialog hidden, `tiles_revision` 1 -> 2,
  `can_undo()` false.

## Deviations / blockers

1. T12.2 deviations (full list in the run's notes): slider label+value is a
   full-rect overlay on the track; synchronous preview generation with the
   33ms drag throttle (Java uses `mainExecutor`); the logic-code statement
   entry depends on I03's disabled `@add` (verified via the dialog's own
   `@load.clipboard`); air renders the `cancel` glyph for `Icon.none`.
2. Fixed a real serialization bug while wiring the dialog: GDScript's JSON
   parser turns every number into a float, so the old dialog's writes produced
   `"seed": 0.0` and `read_filters` fell back to the default stack (user edits
   silently dropped on Apply). `MindEditor.set_filters_json` now normalizes
   integral floats (new test `normalize_filter_numbers_restores_integer_fields`).
3. Input note (not a port bug): eval bodies with loops / invalid accesses trip
   the MCP break-on-error and pin the frame loop (`get_tick` frozen while
   evals answer); all interactions were re-run after
   `tools/session-play.sh 5 --play`.
4. T12.9 assets: blocked on the plan-20 data-asset host (`ModDataManager`)
   mount, unchanged from pass 5/6.

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0110 | partial: T8/T9 verified; T12.1, T12.2 (**new**), T12.3, T12.5, T12.6, T12.7, T12.8 verified; T12.4 first slice; T12.9 blocked on the data-asset host |
| (new) | "UI-edited generation filters were silently dropped: GDScript JSON round-trips wrote `\"seed\": 0.0`, so `read_filters` fell back to the default stack" - fixed in c6a3807f |

## Done criteria

**Not met**: only T12.9 remains (blocked), plus the T12.4/T12.5 residual
lists from pass 6. Every landed task is committed, the suites/clippy/parse
gate/build/goldens are green, and the worktree is clean with no stashes.

# Pass 8 (session 5, 2026-10-16)

Status: **T12.4 and T12.5 residuals closed as far as the port can carry them;
T12.9 re-investigated and stays blocked with the precise missing piece
recorded.** Started from main @ `51a55a23` (clean fast-forward, no conflicts);
worktree clean, no stashes. Three commits.

## Per-task status

| Task | Status | Evidence |
|---|---|---|
| T12.1 wave info | done (pass 4) | `runs/20261015-s5-I06-T12/` |
| T12.2 map generate | done (pass 7) | `runs/20261016-s5-I06-T12.2/` |
| T12.3 banned content | done (pass 6) | `runs/20261016-s5-I06-T12banned/` |
| T12.4 objectives | **residuals done** (2 commits) | `runs/20261016-s5-I06-T12.4b/` (+ pass-6 dir) |
| T12.5 locales | **residuals done** (1 commit) | `runs/20261016-s5-I06-T12.5b/` (+ pass-6 dir) |
| T12.6 processors | done (pass 6) | `runs/20261016-s5-I06-T12processors/` |
| T12.7 resize | done (pass 6) | `runs/20261016-s5-I06-T12resize/` |
| T12.8 sector generate | done (pass 6) | `runs/20261016-s5-I06-T12sectorgen/` |
| T12.9 assets | **blocked** (precise findings below) | — |

T12.4 residual list (pass 6) vs this pass:
* markers (`Seq<ObjectiveMarker>`, all 8 types) — **done** (descriptors +
  marker picker + per-marker editors).
* `Objective` reference fields — **no upstream field uses one**: `MapObjective`
  has no objective-typed public field (only the transient `parents`/`children`
  collection). The port's `FieldKind::Objective` stays for the descriptor ABI.
* `Seq`/`Map` field editors — **Seq done** (flagsAdded/flagsRemoved/positions/
  markers); **no upstream `ObjectMap` field exists** among the objectives/
  markers, so the `Map` kind is unused (kept for the ABI).
* `Color` fields — **done** (shared `ui.picker`).
* `@LogicCode` buttons — **done** (pencil-only row, shared logic editor).
* Canvas drag-to-move / `@add` query placement / rendering beyond rects /
  Ctrl+C-V node clipboard — **done**. Connector/parent drag curves and the
  per-tile pencil/trash popups remain (recorded below).
* `@Second`/`@TilePos` stale display — **done** (re-sync from the model on
  focus loss).
* Still missing: `Vertices` (QuadMarker) row editing (raw list shown instead).

T12.5 residual list (pass 6) vs this pass:
* `@locales.viewproperty` — **done**.
* `@locales.addicon` full icon list — **done** (all `Iconc` names by their
  `Icon.icons` texture target + the 5 content types + 6 base teams; new
  `MindAssets.icon_names()`/`icon_texture()` seams).
* Duplicate-name card tint `f25555` — **done**.
* `resized(this::buildMain)` — **done**.
* `state.mapLocales` live apply — **not implementable in the port**: the
  editor has no running game whose bundle could change, and the port has no
  consumer of `map_locales` outside the sim (`mind-gdext/sim_host.rs:3165-3181`
  feeds it from the `locales` tag at world load; `logic/world.rs` map-locale
  print). The Java line (`MapLocalesDialog.java:72,79,85`) mutates a shared
  `GameState`; the port's equivalent effect happens at load time. Documented
  deviation, not code.

T12.9 investigation (kept blocked — `assets()` cannot be wired cheaply):
* The port **has** the plan-20 core (`mind_core::mods::assets::ModDataManager`,
  `DataAsset`/`DataAssetType`/`DataAssetCache`, `editor/assets.rs`
  `import_zip`/`export_zip`/`asset_record`), but no **client** side:
  * `ModDataManager` is constructed only in `mind-headless` benches/tests
    (`exec.rs`); no gdext node owns one. Java: `GameState.data = new
    DataManager()` (`GameState.java:47`), consumed by Mods boot, Logic
    (`Logic.java:305`), NetServer (`NetServer.java:331,539,974`), NetClient
    runtime textures (`NetClient.java:177-179`) and the editor views.
  * The gdext "decorator" that applies the manager's records to live effects
    (atlas image packing, audio registration, bundle merge) does not exist —
    `mods/assets/mod.rs` states it is "applied by the `mind-gdext` decorator
    from these records (plan 03/18)".
  * Editor plumbing: `save_editor_map_with_ecs` writes `patches: None`
    (`mind-core/src/maps/mod.rs:463`) even though `WriteContext.patches` and
    the `embed_assets` flag exist; `MindEditor.assets()` is a stub
    (`mind-gdext/src/editor/mod.rs:1595-1602`); the zip facades are unused.
  * `client/scenes/editor/data/map_assets_dialog.gd` remains a 66-line stub;
    Java's `MapAssetsDialog` + 5 views are 274 + ~1,300 lines.
  So T12.9 needs the plan-20 **client** half (manager host + effects decorator
  + save/load integration) before the dialog rewrite is meaningful.

## Commits (`git log --oneline main..HEAD`)

```
412a0383 PAR-0110: locales viewproperty/addicon gallery, tint, resize; picker fixes
0da2db0b PAR-0110: objective canvas drag/query/clipboard and node rendering
71f59cd2 PAR-0110: objective markers, seq/color/align/label-flag fields, logic-code buttons
```

## Tests (exact commands + results)

From `WT/client/rust` with `L="flock /tmp/opencode/mg-build.lock"`;
`export CARGO_BUILD_JOBS=6`:

* `cargo fmt -p mind-core -p mind-gdext`; `$L cargo clippy -p mind-core -p
  mind-gdext --all-targets -- -D warnings` -> clean (after every Rust edit).
* `$L cargo test -p mind-core --lib editor` -> `ok. 66 passed` (after the
  marker descriptors).
* `$L cargo test -p mind-core --lib --features msav-import` -> `ok. 2680
  passed; 0 failed; 4 ignored` (main gate was 2671 at the pass-7 merge; the
  delta is the new `marker_descriptors_match_serialized_keys` plus main's).
* `CARGO_BUILD_JOBS=6 $L tools/build.sh` -> `Finished \`dev\` profile` +
  `sync_scenarios: synced 11 file(s)`; the editor was restarted per
  I-environment §1 after every Rust build.
* GDScript parse gate after every `.gd`/`.tscn` edit -> clean.
* `grep -c panic <newest log>` -> 0 after every in-engine step.

## In-engine evidence (bridge 7010, real clicks where stated)

`runs/20261016-s5-I06-T12.4b/` (25 screenshots + `notes.md`):
* `04-markers.png`: the Point marker renders world/minimap/light checkboxes,
  `pos` as TilePos (`32` shows `4.000000`), radius/stroke and the color swatch.
* Real click on `light` -> `1`; color picker drag + OK -> `54a3f2ff` and the
  swatch rebuilt (`06-after-color.png`).
* `08-marker-picker.png`/`17`: the picker lists all 8 classes (Light falls back
  to the class name like Java's missing `marker.light.name` bundle key).
* ShapeText add -> `flags 3`, `color ffd37fff`, `textHeight 7`; `textAlign`
  pick -> `10`; `lineAlign` popup shows only left/center/right
  (`21-linealign-popup.png`).
* Radius garbage text resynced to `5.000000` on focus loss
  (`13-focus-moved.png`); `stroke` edit -> `7.0`.
* Drag node -> `editorPos -65533` (cell -1,3) with the drag ghost
  (`24-drag-ghost.png`); `@add` + click -> `editorPos 131067` (cell -5,1);
  Ctrl+C -> clipboard JSON; Ctrl+V -> `editorPos -5` (cell -5,-1)
  (`25-clipboard-paste.png`).
* Pencil opens the shared logic dialog.

`runs/20261016-s5-I06-T12.5b/` (21 screenshots + `notes.md`):
* `01-propedit-menu.png`: all four Java property rows.
* `03-viewproperty2.png`/`04-view-add.png`: the view dialog with the en card
  and the de `@add` -> `moai`.
* `07-gallery-full.png`/`09-gallery-search.png`: 628 icons + 22 items + 447
  blocks + 11 liquids + 23 statuses + 69 units + 6 teams; "copper" filters to
  4/1/3; the copper tile appends its glyph.
* Duplicate rename -> `#f25555` modulate, key kept.
* `20-content-picker.png`: the objectives content picker now opens (447
  blocks) and a real click writes `block: "spawn"` (the `PackedStringArray`
  guard bug).

## Deviations / blockers

1. T12.9 blocked: needs the plan-20 client half (manager host + decorator +
   save/load integration) before `MapAssetsDialog` can be ported (details
   above).
2. T12.4: connector/parent drag curves and per-tile pencil/trash popups not
   ported; `Vertices` rows shown raw; marker elements have no type switcher
   (Java's `defaultInterpreter` shows none either).
3. T12.5: `state.mapLocales` has no consumer in the editor (documented).
4. Environment note (not a port bug): the MCP addon's `godot_input` `"text"`
   action reuses one `InputEventKey` for press+release, so synthetic typing
   never inserts; typing was done with per-char eval events. The MCP `list_select`
   opens after the first timed-out eval can leave the editor in a degraded
   (frozen-layout) state; `tools/session-play.sh 5 --play` recovers.
5. `list_select` fixes ride in the locales commit: clearing on rebuild and the
   minimum size (it previously stacked tables and could render a zero-height
   body).

## Ledger changes to report

| Item | After |
|---|---|
| PAR-0110 | partial: T8/T9 verified; T12.1-T12.8 verified; **T12.4 residual slice landed** (markers/seq/color/align/label-flag/logicode + canvas drag/query/clipboard); **T12.5 residual slice landed** (viewproperty/addicon/duplicate tint/resized; `state.mapLocales` deviation recorded); T12.2 verified (pass 7); T12.9 blocked on the plan-20 data-asset **client** half |
| (new) | "The block/unit content picker never opened from the objectives dialog: `SimHost.content_list` returns a `PackedStringArray` and the `is Array` guard rejected it" - fixed in 412a0383 |
| (new) | "`list_select` stacked a new content table per open and could render a zero-height body" - fixed in 412a0383 |

## Done criteria

**Not met**: T12.9 remains (blocked, precise scope recorded). T12.4/T12.5
residual lists are closed as far as the port architecture allows (each
remaining line is recorded above with its reason). Every landed task is
committed, the suites/clippy/parse gate/build are green, and the worktree is
clean with no stashes.
