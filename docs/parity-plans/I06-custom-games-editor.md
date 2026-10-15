<!-- SPDX-License-Identifier: GPL-3.0-only -->
# I06 — Custom games, rules, saves import, map previews and the map editor

Round I, written 2026-10-15. Read `docs/I-environment.md` fully, then
`docs/I-INDEX.md` §3, then this plan.

Round H scoped itself to in-campaign play, so everything off the campaign path
never got an owner. This plan owns that work: playing a custom map (PAR-0133), the
Custom Rules dialog and live rule edits (PAR-0132, carried from H05 T11a/T11b),
importing saves (PAR-0134), map previews (PAR-0114), and the map editor (PAR-0109
blank canvas, PAR-0110 no New Map / no menu, PAR-0111 no unsaved-changes confirm,
PAR-0113 no pan/pick/zoom). Java reference frames for these exist under
`REPO/runs/20261010-234250-ref-PAR-0070-combat-fx/java/` (e.g. `step-316-rules.png`,
`step-374-pause.png`) and `REPO/runs/*ref*`; find more with
`jq -r '.ledger["PAR-0133"]' chains.json` and `jq -r '(.notes["PAR-0109"] // [])[]' chains.json`.

---

## 1. State on `main` (verified 2026-10-15 @ `b560e9b1`)

Custom game flow (PAR-0133):
* `ui/dialogs/custom_game_dialog.gd`: `shown()` :41 -> `_refresh_registry()` :47
  (`/root/Spine/MindPreview.refresh()`), `_maps()` :55 (rows from
  `MindPreview.maps_list()`), `_build_map_tile()` :188 (a placeholder `map` glyph
  instead of a preview, :226-234), `_choose_map()` :248 ->
  `MindUi.open_dialog("map_play", {"map": name})`.
* `ui/dialogs/map_play_dialog.gd`: `set_context_json` :32, mode buttons :48-58, Play :66,
  Playtest :63-64. `_play(mode, playtest)` :88-104 emits `play_requested` (no listener
  anywhere), calls `MindCampaign.start_map_mode` only `if has_method(...)` (it does not
  exist), else `MindCampaign.start_map(name)`, which DROPS the mode and the playtest flag.
* `gdext/campaign.rs:1293` `start_map(name)`: sets `rules.mode_name = name`, runs
  `play_map(&mut self.session, rules, ...)` and the landing animation. It NEVER loads the
  map into the `SimHost`, so the game shows whatever world was loaded before (an empty
  grey world from the menu).
* `core/game/play.rs:240` `play_map`: `reset_world`, `rules.sector = None`,
  `rules.editor = false`, `apply_rules_load`, `logic_play`, `NewGame` event.
* `core/game/gamemode.rs:14` `enum Gamemode { Survival, Sandbox, Attack, Pvp, Editor }`
  with `ALL`, `name()`, `hidden()`, `valid(MapView{spawns, teams})`, `apply(&mut Rules)`
  (a faithful port of `J/game/Gamemode.java`). Used only by `core/editor/playtest.rs:95,129-131`.
* `core/maps/map.rs:164` `Map::rules(&self, base: Rules, hooks: &dyn MapGenHooks) -> Rules`
  (the `Map.rules(base)` half of Java `applyRules`). There is no `apply_rules(mode)`.
* `core/maps/mod.rs`: `Maps::load` :173 (18 `DEFAULT_MAP_NAMES` from
  `<assets>/maps/default/<name>.msav`, then the custom dir `user://maps`),
  `load_map_file` :235, `load_internal_map` :250, `import_map` :347.
  `core/io/map/mod.rs:69` `MapIo::load_map`.
* World loading lives only in `gdext/sim_host.rs`: `#[func] load_sector` :2283 ->
  `fn load_sector_world` :2290 (read it: it is the template for T1), the private helpers
  `load_preset_map_grid` :7295 and `load_grid_file(path, &mut registry) -> Option<LoadedGrid>` :7335,
  `request_load(path)` :1958 (save slots), `load_scenario` :2118.
* Campaign runtime install: `MindCampaign.start_sector` (`gdext/campaign.rs:807`, read
  :807-1140) builds `CampaignRuntime::new(self.session.clone(), campaign.clone())`, calls
  `host.bind_mut().install_campaign_runtime(runtime)` (:1093) and then
  `host.bind_mut().sync_campaign_session(&registry, fresh)` (registers cores/spawns, spawns
  the player).

Custom rules (PAR-0132): `ui/dialogs/custom_rules_dialog.gd` (53 lines) shows a tab rail with
raw keys and no rows. `MindCampaign.set_rules_json` (`gdext/campaign.rs`, grep it) assigns
`self.session.rules` only, and the next `pump_runtime` copies `runtime.session.rules` back
over it, so an edit during play is lost. `MindCampaign` has no `set_campaign_rule`
`#[func]`, so `campaign_rules_dialog.gd`'s `has_method("set_campaign_rule")` check fails.

Saves (PAR-0134):
* `ui/dialogs/load_dialog.gd` (324 lines): `_refresh_slots` :52 (`MindCampaign.list_save_slots()`),
  `_build_card` :151, the four card icon buttons (autosave toggle, delete, rename, export)
  :177-180 have no `pressed` handler, the preview is a placeholder :189-198. Import:
  `add_button(_t("@save.import"), _open_import, "add")` :89; `_open_import` :303 opens
  `file_chooser` with `{"open": true}` and no extension filter, and never connects
  `file_chosen`, so a picked file goes nowhere.
* `core/game/saves.rs:420` `Saves::import_save(fs, paths, settings, from)` exists and is
  not exposed. `gdext/campaign.rs:3037` `load_slot(name)` works; `save_slot` :3017;
  `list_save_slots` :3507.
* `ui/dialogs/file_chooser_dialog.gd:96-98` emits `file_chosen(path)` and hides.

Map previews (PAR-0114): `gdext/editor/preview.rs` `MindPreview` (`/root/Spine/MindPreview`):
`reload` :82-92, `maps_list` :133 (rows `{name, path, custom, author, description, width,
height, preview(bool)}`), `preview_path` :181, `texture_for(map_file) -> Option<Texture2D>`
:196 (pipeline first, then the cached PNG `<user>/previews/<stem>_v2.png`). Nothing ever
GENERATES previews: `core/editor/preview.rs` `PreviewPipeline::{load_previews,
create_all_previews, create_new_preview, texture_for, check_previews}` and
`core/io/map/mod.rs` `MapIo::generate_preview(registry, bytes)` / `preview_from_tiles` are
never called from gdext. No `.gd` calls `texture_for`.

Map editor:
* Scenes: `scenes/editor/map_editor_dialog.gd` (259) + `.tscn` (171), instanced at
  `/root/Spine/Ui/EditorDialog` (`scenes/game.tscn:72`); `map_view.gd` (135) + `.tscn`
  (has `ViewportContainer/SubViewport/EditorWorldView`, a script-less, child-less `Node2D`);
  sub-dialog scripts `map_info_dialog.gd` (88), `map_resize_dialog.gd` (111),
  `map_generate_dialog.gd` (141), `wave_info_dialog.gd` (104) + `wave_graph.gd` (68),
  `map_objectives_dialog.gd` (77) + `map_objectives_canvas.gd` (77), `map_locales_dialog.gd`
  (100), `map_processors_dialog.gd` (78), `map_load_dialog.gd` (73),
  `sector_generate_dialog.gd` (59), `banned_content_dialog.gd` (71),
  `data/map_assets_dialog.gd` (66). NONE of these sub-dialogs is in
  `ui/dialogs_manifest.json` or instanced anywhere, so they are unreachable.
* `map_view.gd` `_draw()` :99-118 draws only the border rect, optional grid lines and the
  brush outline from `MindEditor.status()`. No tile is ever drawn: that is PAR-0109.
  Input: wheel zoom (`map_view_zoom`), LMB draw, RMB temporary eraser (:42-70). No pan,
  no middle-click, no WASD, no shift/alt pick.
* `map_editor_dialog.gd`: `LeftTools` (Title, `BackButton`, `ToolGrid`, `TeamRow`,
  `BrushSlider`, `ShowChecks`) and `RightPalette`. No menu button. Save only via Ctrl+S
  (:216-217, the returned dict is ignored). `BackButton` (:65) and Escape (:226) call
  `_exit_to_maps()` :57 directly, with no confirm.
* `gdext/editor/mod.rs` (1639) `MindEditor` (`/root/Spine/MindEditor`): `grid: WorldGrid`
  :93, `render: NoopRenderHooks` :97/:139; `#[func]`s: tool/brush/team/rotation getters and
  setters, `show_terrain/floor/buildings` + setters, `grid`/`set_grid`, `can_undo/can_redo/undo/redo`,
  `tags/set_tag/status/last_error/palette_blocks/block_info/palette_teams/brush_sizes`,
  `begin_new` :552, `begin_edit_map` :575, `begin_edit_image` :608, `resize_map` :653,
  `resume_editing`, `resume_after_playtest`, `try_exit` :1178, `playtest_status`,
  `filters_json/set_filters_json/apply_filters/sector_generate`,
  `objectives_json/set_objectives_json/waves_json/set_waves_json/locales_json/set_locales_json/wave_graph/processors/assets`,
  `export_map/export_image/save` :1011/`edit_in_game/playtest`,
  `map_view_pan` :1204, `map_view_center` :1216, `map_view_zoom/state/project/unproject/brush_outline/touch/drag/flush`,
  and `dev_*` test seams.
* `ui/dialogs/editor_maps_dialog.gd` (194): only Import and Export buttons (:34-39), plain
  text buttons per map (:82-97), `_open_map` :102 -> `_open_in_editor` :169 ->
  `MindEditor.begin_edit_map(path)` -> shows `EditorDialog`.
* Core: `core/editor/` has the model (`mod.rs` 1162, `lifecycle.rs`, `tool.rs` 499,
  `draw_op.rs`, `tile_op.rs`, `stack.rs`, `grid.rs` 482, `context.rs`, `maps_glue.rs` 458,
  `gen.rs` 348, `objectives.rs` 540, `wave_graph.rs`, `processors.rs`, `banned.rs`,
  `assets.rs`, `playtest.rs` 274, `preview.rs`). There is no port of
  `J/editor/EditorRenderer.java` (266) or `EditorSpriteCache.java` (171).
* The live floor renderer `gdext/render/floor.rs` (`FloorRenderer`) is bound to
  `Gd<MindSimHost>` (`world_size()`, `world_revision`), but its per-tile quad builder
  `fn tile_quads(world: &WorldGrid, content: &ContentRegistry, tile: &Tile, layer: CacheLayerId)`
  (~:445) is grid-generic.

---

## 2. Tasks

Order: T1 -> T2 -> T3 -> T4 -> T5 -> T6 -> T7 -> T8 -> T9 -> T10 -> T11 -> T12.

### T1 — Playing a custom map loads it, applies the gamemode and starts a session (Fixes PAR-0133)

**Symptom.** Menu > Play > Custom Game > pick "Ancient Caldera" > Survival > Play shows an
empty grey world with the previous HUD (`runs/20261014-s3-verify3/H05-T11b-sandbox.png`).

**Acceptance.** After Play: the selected map's tiles, buildings and cores are loaded;
`get_rules_json()` shows the gamemode's rules merged with the map's own rules (e.g.
Sandbox -> `infiniteResources: true`, `waveTimer: false`); `get_hud_state().hasCore` is
true and the player alpha spawns at the map's player core; waves run on a survival map;
a map without a player core shows `@map.nospawn` (formatted with the team name) and stays
in the menu; the Play button is disabled for modes the map does not support.

**Java.**
* `MapPlayDialog.show(map, playtesting)` (`J/ui/dialogs/MapPlayDialog.java:37-89`):
  if the selected mode is not `valid(map)`, pick the first valid one (fallback survival);
  `rules = map.applyRules(selectedGamemode)`; mode buttons (`Styles.flatToggleMenut`,
  140 x (mobile ? 44 : 54), 2 per row, disabled when `!mode.valid(map)`, hidden modes
  skipped); a `?` button (width 50) shows `displayGameModeHelp` (`[accent]<mode>:[] [lightgray]<description>`
  per visible mode, width 400, `@ok` 110x50); `@customize` (Icon.settings, 230x50) opens
  `CustomRulesDialog(true).show(rules, () -> rules = map.applyRules(selectedGamemode))`;
  the preview `BorderImage(map.safeTexture(), 3f)` 250x250; when `Gamemode.survival.valid(map)`,
  `Core.bundle.format("level.highscore", map.getHighScore())`; buttons: close + `@play`
  (Icon.play, 210x64) -> `control.playMap(map, rules, playtesting)`, hide, `ui.custom.hide()`.
* `Map.applyRules(mode)` (`J/maps/Map.java:87-94`): `Rules out = new Rules(); mode.apply(out); return rules(out);`
  and `rules(base)` (`:101-115`) reads the map's `rules` tag over `base`, switches the planet to
  erekir for a scorching-env serpulo map, defaults the planet to serpulo, and fills empty
  `spawns` with the default waves.
* `Control.playMap(map, rules, playtest)` (`J/core/Control.java:400-426`):
  `logic.reset(); world.loadMap(map, rules); rules.retainContentFields(oldRules); state.rules = rules;`
  `if(playtest) state.playtestingMap = map; state.rules.sector = null; state.rules.editor = false;`
  `RulesLoadEvent; logic.play();` then `if(settings.getBool("savecreate") && !world.isInvalidMap() && !playtest) control.saves.addSave(map.name() + " " + <"MMM dd h:mm" date>);`
  then `Trigger.newGame`; a playtest of an invalid map returns to the editor
  (`ui.editor.resumeAfterPlaytest(map)`).
* `World.loadMap(map, checkRules)` (`J/core/World.java:349-395`): `SaveIO.load(map.file, new FilterContext(map))`
  (error -> `@map.invalid`, back to the menu, `invalidMap = true`); `state.map = map`; then the
  validity checks: no core of `checkRules.defaultTeam` and not pvp -> `invalidMap`,
  `map.nospawn` formatted with `defaultTeam.coloredName()`; pvp needs >= 2 teams with cores
  (`@map.nospawn.pvp`); attack mode needs a wave-team core (`map.nospawn.attack`).

**Changes.**
1. `core/maps/map.rs`: add `pub fn apply_rules(&self, mode: Gamemode, hooks: &dyn MapGenHooks) -> Rules`
   = `let mut out = Rules::default(); mode.apply(&mut out); self.rules(out, hooks)`. Check that
   `Map::rules` implements the erekir/planet/spawns defaults from `Map.java:101-115`; add any
   that are missing. Test: `apply_rules_sandbox_sets_infinite_resources`.
2. `core/game/gamemode.rs`: a `pub fn map_view(map: &Map, grid_spawns: usize, teams_with_cores: &[TeamId]) -> MapView`
   helper if `valid` needs data the `Map` header has (Java `Gamemode.valid(map)` reads
   `map.spawns` and `map.teams`, which the Map header stores as tags; check how
   `core/editor/playtest.rs:129` builds the `MapView` and reuse that).
3. `gdext/sim_host.rs`: add `#[func] pub fn load_map_world(&mut self, path: GString) -> bool`.
   It is `load_sector_world` with the sector-specific parts removed: no `sector_save_path`,
   no generator fallback, no `sector_polygon`, no `pending_gen_extras`, no generated loadout;
   `loaded = load_grid_file(&path, &mut registry)`; `None` -> log, put the registry back,
   set `self.last_map_error = Some("map.invalid")`, return false. Keep everything else in the
   same order (`count_spawns`, `self.sector_rules = rules`, `sector_plans`, base catalog,
   `Sim::new` + grid swap, `install_logic_globals`, `install_live_block_runtime`,
   `materialize_map_buildings`, `apply_decoded_entities`, `install_sim_seams`,
   `reset_logic_presentation`, map locales, `world_dirty`, `emit_state`, `emit_world_changed`).
   Factor the shared tail into a private `fn finish_world_load(...)` used by both
   functions, so the two cannot drift. Resolve `path`: absolute paths as-is,
   `res://`/`user://` through `ProjectSettings.globalize_path`, and a bare map name through
   `MindPreview.maps_list()` rows (`path` key).
4. `gdext/campaign.rs`: add
   `#[func] pub fn start_map_mode(&mut self, map: GString, mode: GString, playtest: bool, rules_json: GString) -> bool`:
   (a) find the `Map` header (from `MindPreview`/`Maps`; the row's `path`);
   (b) `host.load_map_world(path)`; false -> show `@map.invalid` via `MindUi.show_error`
   (grep the exact `MindUi` error-dialog func) and return false;
   (c) `rules = if rules_json non-empty { parse it } else { map.apply_rules(Gamemode::from_name(mode), hooks) }`;
   `rules.retain_content_fields(&old_rules)` (grep `fn retain_content_fields`; port Java
   `Rules.retainContentFields` if missing: it keeps `bannedBlocks`/`bannedUnits`... read
   `grep -n "retainContentFields" -A12 J/game/Rules.java`);
   (d) validity checks per `World.loadMap` on the LIVE world (cores per team:
   `host.bind().team_core_counts()` or equivalent; add a `#[func]`-free Rust helper if missing).
   Invalid -> show the formatted `map.nospawn*` error, `set_menu_visible(true)`, return false;
   (e) build a fresh `PlaySession::new(rules)` with `sector = None`, run `play_map(...)`;
   (f) install a `CampaignRuntime` exactly like `start_sector` :1088-1140 (it is the
   session runtime for any game, campaign or not; `CampaignRuntime::new(session, campaign)`
   with the existing `campaign` value: check that nothing in the runtime treats
   `session.sector == None` as an error), then `sync_campaign_session(&registry, true)`;
   (g) `if playtest { self.playtesting_map = Some(path) }`;
   (h) `savecreate`: when the setting is true and not playtest, call `save_slot` with
   `"<map name> <MMM dd h:mm>"` (use `chrono`-free formatting: the port has a date helper,
   `grep -rn "fn format_save_date\|MMM" core/game/saves.rs`);
   (i) the landing animation block already in `start_map`.
   Keep `start_map(name)` as `start_map_mode(name, "survival", false, "")` for old callers.
5. `ui/dialogs/map_play_dialog.gd`: rebuild to Java's layout (T2 covers details); `_play()`
   calls `start_map_mode(_map_name, _mode, playtest, _rules_json)` directly (remove the
   `has_method` probe and the dead `play_requested` signal), hides itself and the
   `custom_game` dialog, and hides the menu only when the call returned true.

**Tests.** `apply_rules_sandbox_sets_infinite_resources`, `apply_rules_keeps_map_tag_overrides`
(`core/maps/map.rs`); a headless test that loads the built-in custom map
`assets/maps/default/caldera.msav` (`DEFAULT_MAP_NAMES` in `core/maps/mod.rs:44` lists
`maze, fortress, labyrinth, islands, tendrils, caldera, ...`) through the same core
functions `load_map_world` uses
(`load_grid_file` is in gdext; if it cannot be reached from a core test, move the
grid-building core of it into `mind-core` as `io::map::load_map_world(path, registry) -> LoadedGrid`
and keep the gdext function a thin wrapper), and asserts the player core count is >= 1.

**Verify in-engine.** From the menu (no sector loaded):
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("custom_game", "")'; sleep 2
$G --screenshot runs/<date>-s<N>-I06-T1/list.png
$G --eval 'return get_node("/root/Spine/MindCampaign").start_map_mode("Ancient Caldera", "survival", false, "")'
$G --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return get_node("/root/Spine/MindCampaign").get_hud_state()'
```
Expect `hasCore: true`, a wave countdown, the map visible (screenshot `play.png`). Then the
real click path: menu Play > Custom Game > a map tile > Sandbox > Play, and
`JSON.parse_string($MC.get_rules_json()).infiniteResources == true`. Compare with a Java
capture of the same map if one exists under `runs/*ref*`.

**Commit.** `Fixes PAR-0133: custom maps load, apply the selected gamemode and start a session`

---

### T2 — Map Play dialog layout per Java (PAR-0133 UI half)

**Java.** `MapPlayDialog.show` above (`:37-89`) and `displayGameModeHelp` (`:91-112`).
Reference: grep the ledger for a Java capture of the map play dialog:
`jq -r '.ledger | to_entries[] | select(.value.t|test("Map Play";"i")) | .key + " " + (.value.run // "")' chains.json`.

**Changes** (`ui/dialogs/map_play_dialog.gd` + its `.tscn`): title = map name; `@level.mode`
label (colspan 2); a `Tex.button` table of the visible modes (`flatToggleMenut`, 140x54, two
per row, checked = selected, disabled = `!valid`; validity from a new
`#[func] MindPreview.map_modes(map_name) -> Dictionary` `{survival: true, sandbox: true, attack: false, pvp: false}`
computed with `Gamemode::valid`); the `?` help button; `@customize` (opens
`custom_rules` with `{"rules": <current rules JSON>, "reset": <map.apply_rules(mode) JSON>}`
and stores the edited JSON from `rules_edited`); the preview image (T5's texture, 250x250,
`BorderImage` thickness 3); the high score line when survival is valid (the high score
lives in the map's settings key: grep `getHighScore` in `J/maps/Map.java` for the key; read
it through `MindUi.settings_get_int`, adding that func if missing); buttons `@back` (close)
and `@play` (Icon.play, 210x64). Expose `#[func] MindPreview.map_rules_json(map_name, mode) -> GString`
(= `apply_rules(mode)` serialized) for the dialog.

**Verify.** Screenshot next to the Java reference; the Attack button is disabled for a map
without an enemy core.

**Commit.** `PAR-0133: Map Play dialog modes, help, customize, preview and high score`

---

### T3 — Custom Rules dialog: rule rows, search, edit menu (PAR-0132 part 1)

**Evidence (2026-10-14):** `runs/20261014-s3-verify3/H05-T11a-rules.png` vs Java
`runs/20261010-234250-ref-PAR-0070-combat-fx/java/step-316-rules.png`: the port shows a tab
rail with raw keys (`rules.title`, `rules.environment`, ...), no rows, no search, a centred
panel (Java is fill-parent).

**Java** (`J/ui/dialogs/CustomRulesDialog.java`, read it whole):
* ctor (`:52-110`): title `@mode.custom`, `setFillParent(true)`, `shown(this::setup)`, close
  button, `@edit` (Icon.pencil) -> an `@edit.menu` dialog with `@copy.clipboard` (rules JSON
  without spawns), `@load.clipboard` (keeps spawns/objectives; error `@rules.invaliddata`),
  `@settings.reset` (calls `resetter`); those buttons are 280x64 `Styles.cleart`.
* `setup()` (`:122-139`): `Icon.zoom` + a search field (`@players.search`; text lower-cased,
  runs of spaces collapsed), then `cont.pane(main)`.
* `setupMain()` (`:140-345`): categories in this order: **waves, resourcesbuilding, unit,
  enemy, environment, light, music, planet, teams**, with the exact rows in the Java file.
  Transcribe every `check/number/numberi/text/team` call: label key, setter field, getter,
  enable condition, min/max, and the `*60` / `/60` (seconds <-> ticks) and `*tilesize`
  scale factors. Buttons inside categories: `@configure` (loadout dialog, cap 999999,
  reset to `copper x100`), `@bannedblocks`, `@bannedunits`, `@rules.weather` (weather
  dialog), an ambient light colour picker, planet buttons (`flatTogglet` 140x50, 3 per row,
  `planet.applyRules(rules, true)`, plus `@rules.anyenv`), per-team collapsers
  (`Team.baseTeams`, 260x55 `togglet`). `showRuleEditRule` adds `@rules.allowedit` at the
  top of "teams".
* `addToMain` (`:347-353`): title label (accent) padTop 20 padRight 100 padBottom -3, an accent
  image of height 3 padBottom 20, then the category table.
* Row builders (`:393-470`): `check` (checkbox, disabled when the condition is false);
  `number`/`numberi` (label white or gray by condition, field width 120, padRight 50/100,
  validated by min/max and `canParsePositiveFloat`); `text` (width 300); `team` (label plus
  one 60x60 `squareTogglei` per base team, tinted with the team colour); `ruleInfo` adds the
  tooltip `<key>.info` when the bundle has that key.
* Search: a row is skipped unless `bundle(key).toLowerCase().contains(ruleSearch)`.

**Port today.** `ui/dialogs/custom_rules_dialog.gd` (53 lines). Rules JSON is serde
camelCase (`core/io/json/rules.rs`: `waveTimer`, `winWave`, `waveSpacing`, `teams`, ...).
Dialogs to reuse: `loadout_dialog.gd`, `scenes/editor/banned_content_dialog.gd` (register it
in T8), `color_picker.gd`. The bundle has the `rules.*` keys
(`grep -c "^rules\." REPO/assets/bundles/bundle.properties`).

**Changes.**
1. Context: `open_dialog("custom_rules", {"rules": <json>, "reset": <json>, "show_rule_edit": bool})`.
   Keep a `_rules: Dictionary` working copy.
2. Data-driven rows: one GDScript const array per category, each entry
   `{"kind": "check|number|numberi|text|team|button", "key": "@rules.waves", "field": "waves", "scale": 1.0, "min": 0, "max": INF, "cond": "waves"}`,
   where `cond` names a small GDScript predicate (`_cond_waves()` returns `_rules.waves`; one
   function per distinct Java condition). Transcribe ALL Java rows, in order. Team sub-rules
   write `_rules.teams[<team id>].<field>` (check the JSON shape with `$MC.get_rules_json()` first).
3. Layout per `addToMain` + the row builders; the search field re-filters.
4. The `@edit` menu with the three actions (`DisplayServer.clipboard_get/set`).
5. On hide emit `rules_edited(json: String)`. Consumers: T2's map play dialog and T4.

**Verify.** `open_dialog("custom_rules", JSON.stringify({"rules": JSON.parse_string($MC.get_rules_json())}))`,
screenshot vs `step-316-rules.png`; type `wave` in the search -> only wave rows remain.

**Commit.** `PAR-0132: Custom Rules dialog rule rows, search and edit menu`

---

### T4 — Pause-menu rules button applies edits live (Fixes PAR-0132)

**Java** (`J/ui/dialogs/PausedDialog.java:20-37`):
```java
stack(cont, new Table(t -> {
    t.bottom().left();
    t.button(Icon.book, () -> {
        Rules toEdit = Vars.state.rules.copy();
        rulesDialog.show(toEdit, () -> state.rules.copy());
        rulesDialog.hidden(() -> { Vars.state.rules = toEdit; Call.setRules(toEdit); });
    }).size(70f).tooltip("@customize").visible(() -> state.rules.allowEditRules && (net.server() || !net.active()));
})).grow().row();
```
Reference frame `step-374-pause.png` (book button bottom-left in a sandbox custom game).

**Changes.**
1. `MindCampaign.set_rules_json`: after `apply_set_rules` succeeds, if a live runtime exists
   (the `host.bind_mut().take_campaign_runtime()` / `put_campaign_runtime` pattern), write the
   same rules into `runtime.session.rules`, set `runtime.rules_world_dirty = true` and put the
   runtime back. Keep the `allowEditRules` guard.
2. `ui/dialogs/paused_dialog.gd`: a 70x70 icon button (`book` glyph, tooltip `@customize`)
   anchored bottom-left, visible only when `JSON.parse_string($MC.get_rules_json()).allowEditRules`
   and not a network client (`/root/Spine/MindNet.session_state()` is `offline` or the host);
   pressed -> open `custom_rules` with `{"rules": current, "reset": current}` stacked over the
   pause menu; on `rules_edited(json)` -> `set_rules_json(json)`.
3. Add `#[func] MindCampaign.set_campaign_rule(key: GString, value: Variant) -> bool` that
   writes one `CampaignRules` field (the planet dialog's campaign rules dialog probes it with
   `has_method`), persists, and returns true; read
   `grep -n "set_campaign_rule\|campaign_rules" ui/dialogs/campaign_rules_dialog.gd` for the keys it sends.

**Tests.** A gdext-free core test: `set_rules` on a session with a live `CampaignRuntime`
changes `runtime.session.rules` and survives one `pump_runtime`-equivalent mirror.

**Verify in-engine.** Custom Game -> any map -> Sandbox -> Play; Escape -> the book button ->
untick `@rules.fire`, close -> after `$H.step(10)` `JSON.parse_string($MC.get_rules_json()).fire == false`.
Screenshot the pause menu vs `step-374-pause.png`. In a campaign sector the book must be absent.

**Commit.** `Fixes PAR-0132: in-game rule editing from the pause menu applies live`

---

### T5 — Map previews are generated and shown (Fixes PAR-0114)

**Java.**
* `Maps.loadPreviews()` / `createAllPreviews()` / `createNewPreview(map)` / `readCache`/`writeCache`
  (`grep -n "void loadPreviews\|createAllPreviews\|createNewPreview\|void writeCache\|readCache" -A25 J/maps/Maps.java`):
  previews are generated off-thread from the map file (`MapIO.generatePreview(map)`),
  cached as PNG in `previews/<file>_v2.png`... read the exact name rules there; `map.texture`
  is the preview texture, `safeTexture()` falls back to `Core.atlas.find("nomap")`.
* `MapIO.generatePreview` (`J/io/MapIO.java`, grep it): a pixmap with one pixel per tile, the
  colour from `MapIO.colorFor(floor, wall, ore, team)`.
* `MapListDialog` (`J/ui/dialogs/MapListDialog.java`, 373 lines; `CustomGameDialog` extends it
  in 16 lines): a search field (`@editor.search`) with a filter button (`Icon.filter`,
  `showMapFilters`: search author/description/mod, sort priority, show builtin/custom); a grid
  of map buttons (`Styles.grayt`, width `mapsize`): the name (ellipsis), a gray line, the
  preview stacked with a `BorderImage`, and the source label (`@custom` / `@workshop` /
  mod name / `@builtin`, gray).

**Changes.**
1. `gdext/editor/preview.rs` `MindPreview::reload` (or a new `#[func] generate_previews()` the
   dialogs call on `shown()`): run `PreviewPipeline::check_previews`/`create_all_previews`
   for every map whose cached PNG is missing or stale, using
   `MapIo::generate_preview(registry, bytes)` (the pipeline's generator), and write
   `<user>/previews/<stem>_v2.png`. Do it in small batches per frame (`_process`, e.g. 2 maps
   per frame) so the menu does not stall, and emit a `previews_changed` signal when a batch lands.
2. `#[func] MindPreview.texture_for(map_file)` already exists; add
   `#[func] MindPreview.texture_for_name(map_name)` for the dialogs.
3. `ui/dialogs/custom_game_dialog.gd`: replace the placeholder (:226-234) with the preview
   (`TextureRect`, keep aspect, size `mapsize - 20`) stacked under a border, and rebuild the
   list as Java's `MapListDialog` grid (search + filter popup + source label). Refresh tiles on
   `previews_changed`.
4. `ui/dialogs/editor_maps_dialog.gd` uses the same grid (T9).
5. Save slots: Java `LoadDialog` shows `slot.previewTexture()`. Java writes it on every save
   from the minimap pixmap (`J/game/Saves.java:343-355` `savePreview`), to
   `mapPreviewDirectory.child("save_slot_" + <file name without extension> + ".png")`
   (`:373-375`). Port: when `save_slot` (or an autosave) writes a slot, also write
   `<user>/previews/save_slot_<stem>.png` from the live grid with
   `MapIo::preview_from_tiles` (the minimap colours); `#[func] MindCampaign.save_preview_texture(name) -> Texture2D`
   loads it (null when missing). Show it in `load_dialog.gd` (:189-198).

**Tests.** `preview_from_tiles_colors_core_team` (core) if not already present; the gdext side
is verified in-engine.

**Verify.** Custom Game list shows 18+ previews; screenshot next to a Java capture.

**Commit.** `Fixes PAR-0114: map previews are generated, cached and shown in the map lists`

---

### T6 — Load dialog: import a save, card actions (Fixes PAR-0134)

**Java** (`J/ui/dialogs/LoadDialog.java`, 283 lines): `@save.import` (Icon.add) ->
`platform.showFileChooser(true, saveExtension, file -> { if(SaveIO.isSaveValid(file)){ try{ control.saves.importSave(file); rebuild(); }catch(e){ ui.showException("@save.import.fail", e); } } else ui.showErrorMessage("@save.import.invalid"); })`
(`:198-216`). Card buttons (`grep -n "button(Icon" J/ui/dialogs/LoadDialog.java`): autosave toggle,
delete (confirm `@save.delete.confirm`), rename (`@save.rename`, text input), export
(file chooser save, `saveExtension`).

**Changes.**
1. `gdext/campaign.rs`: `#[func] import_save(path: GString) -> GString` returning `""` on success or
   `"invalid"` / `"fail:<message>"`; it calls `SaveIo` validity (grep `fn is_save_valid` in
   `core/io/save/`; port Java `SaveIO.isSaveValid` (header + version check) if missing) and then
   `Saves::import_save(fs, paths, settings, from)` (`core/game/saves.rs:420`). Also
   `#[func] delete_save_slot(name)`, `rename_save_slot(name, new_name)`,
   `export_save_slot(name, path)`, `set_slot_autosave(name, on)` (grep `Saves` for the core
   halves; add them if missing).
2. `ui/dialogs/load_dialog.gd`: `_open_import` opens the file chooser with
   `{"open": true, "extensions": ["msav"]}` and connects `file_chosen` ONE-SHOT to
   `_on_import_chosen(path)` -> `import_save(path)`, then show `@save.import.invalid` /
   `@save.import.fail` or rebuild. Check `file_chooser_dialog.gd` accepts an `extensions`
   context key; add it if not. Wire the four card buttons with Java's confirms/text inputs.

**Verify.** Save a custom game (T1) as slot `a`, copy `$U/saves/a.msav` to `/tmp/imp.msav`,
delete slot `a` in the dialog, then Import `/tmp/imp.msav` (drive the chooser by calling the
dialog's `_on_import_chosen("/tmp/imp.msav")` if the native chooser cannot be clicked): the
list grows by one and the slot loads. A text file renamed to `.msav` shows
`@save.import.invalid`.

**Commit.** `Fixes PAR-0134: Load dialog imports saves; card autosave/delete/rename/export actions work`

---

### T7 — Map editor renders the map: floor, walls, blocks, team borders (Fixes PAR-0109)

**Java.** `EditorRenderer` (`J/editor/EditorRenderer.java`, 266 lines; read all) draws the map
inside the `MapView` rectangle: the shared floor renderer (`renderer.blocks.floor.drawFloor`,
without terrain when `!editor.showTerrain`), the shadow/darkness buffer, the `walls` cache
layer when `showTerrain`, then per 60x60-tile chunk an `EditorSpriteCache` of every non-air,
`CacheLayer.normal`, centre tile's `block.fullIcon` (drawn at
`x*8 + block.offset - w/2, y*8 + block.offset - h/2`, rotated by `build.rotdeg()` only when
`block.rotate && block.rotateDrawEditor`), synthetic blocks first, plus the `block-border`
region tinted with the team colour for buildings. `updateBlock` re-caches the chunk of an
edited tile; `updateStatic` re-caches the floor tile and its 4 neighbours.
`MapView.draw` (`J/editor/MapView.java:236-330`) clips to the view, draws the renderer at
`(centerx - sclwidth/2, centery - sclheight/2, sclwidth, sclheight)`, then the grid
(`Pal.accent`-ish lines, read the colour), the brush outline, and the border.

**Changes.**
1. `gdext/render/floor.rs`: abstract the data source. Add
   `pub trait FloorSource { fn world_size(&self) -> (i32, i32); fn revision(&self) -> i64; fn dirty_tiles(&mut self) -> Vec<(i32, i32)>; fn with_world(&self, f: &mut dyn FnMut(&WorldGrid, &ContentRegistry)); }`,
   implement it for the existing `MindSimHost` path (unchanged behaviour) and for `MindEditor`
   (its `grid` and content registry; the revision bumps on every tile op, and dirty tiles come
   from the tile-op recorder in `core/editor/context.rs`). `FloorRenderer` holds a
   `Box<dyn FloorSource>` or is generic over it. Do not change `tile_quads`.
2. New `gdext/editor/editor_renderer.rs` (`EditorRenderer` `Node2D`, the `EditorWorldView` node in
   `scenes/editor/map_view.tscn` gets this class): owns a `FloorRenderer` over the editor source
   (floor bands as child nodes), plus 60x60-tile block chunk nodes whose `draw()` emits the
   `fullIcon` and `block-border` quads per Java (`canvas::draw_region`), re-built only when a
   tile in the chunk changed. Honour `show_floor`, `show_terrain`, `show_buildings`.
3. `scenes/editor/map_view.gd`: drive the `SubViewport` camera from
   `MindEditor.map_view_state()` (offset/zoom) so the rendered world sits exactly under the
   view's projected rect (`map_view_project`/`unproject` are the Java `project`/`unproject`
   math), and draw the grid/brush/border on top as today.
4. Re-render after every edit: `MindEditor` emits `tiles_changed` (add the signal) from its
   tile-op flush; the renderer marks the affected chunks.

**Verify in-engine.**
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("editor_maps", "")'; sleep 2
$G --eval 'return get_node("/root/Spine/MindEditor").begin_edit_map("<abs path to assets/maps/default/<map>.msav>")'
$G --eval 'get_node("/root/Spine/Ui/EditorDialog").show_dialog(); return true'; sleep 2
$G --screenshot runs/<date>-s<N>-I06-T7/editor.png
```
The map's floor, ore, walls and buildings are visible; draw with the pencil (a mouse drag via
`godot_input`) and the new blocks appear without reopening. Compare with a Java editor
capture (`jq -r '(.notes["PAR-0109"] // [])[]' chains.json` names one).

**Commit.** `Fixes PAR-0109: the map editor renders floor, walls, blocks and team borders`

---

### T8 — Editor menu and sub-dialogs are reachable (PAR-0110 part 1)

**Java** `MapEditorDialog` (`J/editor/MapEditorDialog.java`, 885 lines): the `menu` dialog
(`:65-270`) with, in order: `@editor.savemap` (Icon.save), `@editor.mapinfo` (Icon.pencil),
`@editor.generate` (Icon.terrain), `@editor.resize` (Icon.resize), `@editor.import`
(Icon.download: a sub-dialog with `@editor.importmap`, `@editor.importfile`,
`@editor.importimage`, each with a `.description`), `@editor.export` (Icon.upload:
`@editor.exportfile`, `@editor.exportimage`), `@editor.ingame` (Icon.right),
`@editor.playtest` (Icon.play), (Steam only: workshop), `@editor.sectorgenerate`
(Icon.terrain), `@quit` (Icon.exit -> `tryExit()`). Read `:65-270` for the exact grid
(button sizes, rows). `MapInfoDialog` (`J/editor/MapInfoDialog.java`, 134) is the hub for name,
author, description, rules, waves (`WaveInfoDialog`), objectives, locales, processors, assets
(bundle keys `@editor.waves`, `@editor.rules`, `@editor.objectives`, ...; read it).
Escape in the editor opens the menu (`doInput`, `:750-754`); it does not exit.

**Changes.**
1. Register every editor sub-dialog in the three places (`ui/dialogs_manifest.json`, a static
   instance under the dialog layer in `scenes/ui/ui_root.tscn`, `EXPECTED_*_DIALOGS` in
   `core/ui/manifest.rs`): `map_info`, `map_resize`, `map_generate`, `wave_info`,
   `map_objectives`, `map_locales`, `map_processors`, `map_load`, `sector_generate`,
   `banned_content`, `map_assets`, plus a new `editor_menu` dialog (the Java `menu`). Use the
   existing scene files under `scenes/editor/`. If a script `extends` something other than
   `MindDialog`, convert it (keep its logic).
2. `editor_menu` dialog: Java's buttons, wired to `MindEditor.save()` (show `@editor.saved`
   via the info fade on success, `@editor.save.noname` / `@editor.save.overwrite` per
   `MapEditorDialog.save()` `:410-450`), `open_dialog("map_info")`, `map_generate`,
   `map_resize`, the import/export sub-dialogs (file chooser with `msav` / `png`, then
   `begin_edit_map` / `begin_edit_image` / `export_map` / `export_image`), `edit_in_game`,
   `playtest` (calls T1's `start_map_mode(..., playtest=true, ...)` path via
   `MindEditor.playtest()`; check that it does), `sector_generate`, `@quit` -> T10's confirm.
3. `map_editor_dialog.gd`: Escape opens `editor_menu` (Java), not `_exit_to_maps`. Add the Java
   menu button where `MapEditorDialog.build()` puts it (`:537-733`; read for the left-panel
   layout: the menu button, then tools).
4. `map_info` dialog per `MapInfoDialog.java` (name/author/description fields with
   `maxNameLength` etc., and buttons to the rules (T3 `custom_rules` with
   `show_rule_edit`), waves, objectives, locales, processors, assets dialogs).

**Verify.** In the editor press Escape -> the menu; every button opens its dialog (screenshot
each); Save writes the map (`MindEditor.status().saved == true`).

**Commit.** `PAR-0110: editor menu and map info hub reach every editor dialog`

---

### T9 — New Map and the editor maps dialog per Java (Fixes PAR-0110)

**Java** `EditorMapsDialog` (`J/ui/dialogs/EditorMapsDialog.java`, 165): extends
`MapListDialog` (T5's grid). Buttons (`:25-...`): `@editor.newmap` (Icon.add, 210x64) ->
`ui.showTextInput("@editor.newmap", "@editor.mapname", "", text -> ...)`; an existing name
shows `@editor.exists`, otherwise the editor opens on a fresh map (`MapEditorDialog`
`shown` :290-300 calls `editor.beginEdit(200, 200)` when no map was given), sets the `name`
tag to the text and fires `MapMakeEvent`. `@editor.importmap` (Icon.upload, multi-select
`mapExtension` files). Per-map `showMap(map)` -> a dialog with `@editor.openin` (edit),
`@delete` (custom maps only, confirm), the preview and info.

**Changes.** Rebuild `ui/dialogs/editor_maps_dialog.gd` on T5's grid; New Map calls
`MindEditor.begin_new(width, height)` (check its signature) then shows the editor; the per-map
dialog per Java.

**Verify.** Editor > New Map > name "test" > the editor opens on an empty 200x200 map
(`MindEditor.status()` width/height 200); draw, Save; the map appears in the list with a preview.

**Commit.** `Fixes PAR-0110: editor maps dialog with New Map, per-map actions and previews`

---

### T10 — Leaving the editor asks for confirmation (Fixes PAR-0111)

**Java.** `tryExit()` (`MapEditorDialog.java:789-795`): ALWAYS
`ui.showConfirm("@confirm", "@editor.unsaved", () -> { logic.reset(); hide(); })`. The menu's
`@quit` calls it. (Java has no "saved" shortcut here; `@editor.unsaved` is shown every time.)

**Changes.** `map_editor_dialog.gd` Back button and the menu's `@quit` call
`MindUi.show_confirm("@confirm", "@editor.unsaved", <callable>)` (grep the confirm API), whose
accept path calls `MindEditor.try_exit()` and then `_exit_to_maps()`.

**Verify.** Draw a tile, Back -> the confirm appears; Cancel keeps the editor; OK leaves.

**Commit.** `Fixes PAR-0111: leaving the map editor asks for confirmation`

---

### T11 — Map view: pan, WASD, middle-click zoom tool, shift/alt pick, key bindings (Fixes PAR-0113)

**Java.**
* `MapView` touch (`J/editor/MapView.java:66-150`): only pointer 0; RMB swaps to the eraser
  until release; MMB swaps to the `zoom` tool (dragging pans) until release; `tool.touched`
  on press; `touchedLine` on release for the line tool; `editor.flushOp()` on release;
  `resetSaved()` when the tool edits.
* `MapView.act` (`:175-205`): when no text field has focus and Ctrl is not held,
  `offset -= axis(moveX/moveY) * 15 * delta / zoom` (WASD/arrows); Shift or Alt pressed ->
  temporary `pick` tool until released; `zoom += axis(Binding.zoom) / 10 * zoom` (wheel),
  clamped `0.2..20`.
* `pan(x, y, dx, dy)` (`:339-345`) in the zoom tool: `offset += delta / zoom`.
* `MapEditorDialog.doInput` (`:733-787`): Ctrl+1..9 picks the tool's alt mode; otherwise each
  tool's key (`EditorTool.key`, read `J/editor/EditorTool.java` for the letters) selects it;
  Escape -> menu; R / E rotate +1 / -1; Ctrl+Z undo (Ctrl+Shift+Z redo), Ctrl+Y redo,
  Ctrl+S save, Ctrl+G grid.

**Changes.** `scenes/editor/map_view.gd` + `MindEditor.map_view_pan/zoom/center` (exist):
implement every rule above. Keep tool swaps in `MindEditor` (`set_tool`) so the palette UI
reflects them.

**Verify.** `godot_input` MMB drag pans (`map_view_state` offset changes), W moves the view,
holding Shift then LMB on a tile picks its block (`draw_block` changes), wheel zooms within
0.2..20.

**Commit.** `Fixes PAR-0113: editor map view pans, zooms and picks like MapView`

---

### T12 — Editor sub-dialog fidelity pass (PAR-0110 residual)

The port's sub-dialog scripts are much smaller than Java's: `MapGenerateDialog` 581 vs 141,
`WaveInfoDialog` 455 vs 104, `MapObjectivesDialog` 838 vs 77 (+canvas), `MapLocalesDialog` 774
vs 100, `BannedContentDialog` 210 vs 71, `MapProcessorsDialog` 155 vs 78. For each, open the
port dialog next to the Java code, list every Java control that is missing, and implement it.
One commit per dialog, in this order (most used first): wave info, map generate, banned
content, objectives, locales, processors, resize, sector generate, assets. Each commit body
lists the Java controls added and those still missing.

**Commits.** `PAR-0110: wave info dialog per WaveInfoDialog`, `PAR-0110: map generate dialog per MapGenerateDialog`, ...

---

## 3. Order and hot files

T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12 (commits in that order).
Hot files: `gdext/sim_host.rs` (T1: one new func + a refactor of the load tail; keep the diff
small and merge often), `gdext/campaign.rs` (T1, T4, T6), `gdext/render/floor.rs` (T7: the source
trait only), `scenes/ui/ui_root.tscn` + `ui/dialogs_manifest.json` + `core/ui/manifest.rs`
(T8: many entries in one commit).

## 4. Ledger changes to report

| Item | After |
|---|---|
| PAR-0133 | verified (T1 + T2, real click path) |
| PAR-0132 | verified (T3 + T4) |
| PAR-0114 | verified (T5) |
| PAR-0134 | verified (T6) |
| PAR-0109 | verified (T7) |
| PAR-0110 | verified (T8 + T9; T12 residuals noted) |
| PAR-0111 | verified (T10) |
| PAR-0113 | verified (T11) |

## 5. Done criteria

All tasks committed; suites green; parse gate clean; screenshots for every UI task next to the
Java reference; summary in `REPO/docs/I06-custom-games-editor_summary.md`.
