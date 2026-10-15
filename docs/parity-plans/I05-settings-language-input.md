<!-- SPDX-License-Identifier: GPL-3.0-only -->
# I05 — One settings store, language persistence, block hotkeys, settings that take effect, input re-verification

Round I, written 2026-10-15. Read `docs/I-environment.md` fully, then
`docs/I-INDEX.md` §3, then this plan. T1 is a provider for I07 T1 and others: run it first.

Carried from round H: H04 T11 (PAR-0164, the shared settings store) and H05 T18's acceptance,
H08 T5 (PAR-0214, block hotkeys) and H08 V1 (evidence). New: PAR-0135 (language not persisted;
never reached by round H), and a settings-effect audit.

---

## 1. State on `main` (verified 2026-10-15)

* FOUR independent `SettingsStore`s (Java has ONE `Core.settings`):
  `MindUi` (`gdext/ui/ui_host.rs:58`, loaded at :536/:930/:1036, persisted by `persist_settings` :1079),
  `MindCampaign` (`gdext/campaign.rs:79`, loaded at :196; workarounds `live_setting_flag` :281,
  `live_setting_flag_default` :296, `live_save_interval_seconds` :379 re-read the file to see MindUi's writes),
  `MindMods` (`gdext/mods.rs:35`, `SettingsStore::new()`: never loaded or saved, see I07 T1),
  and three throwaway loads in `gdext/input/mod.rs:358/365/371` (bindings). Stores merge only their
  changed keys on save, which is why nothing has been lost so far, but a value written through one
  facade is invisible to the others until a reload.
* `MindUi` settings funcs: `settings_categories_json` :603, `settings_rows_json(category)` :625,
  `settings_set(key, value)` :635, `settings_get_i32` :664, `settings_get_bool` :676, `settings_reset` :686,
  `settings_get_flag` :749, `settings_get_flag_default` :756, `settings_put_flag` :762,
  `settings_get_text(key, default)` :771, `settings_put_text` :781, `settings_action` :810.
* Language: `ui/dialogs/language_dialog.gd` (77 lines) `_select(locale)` :72 sets a static var, emits
  `locale_selected` (no listener) and shows `@language.restart`; it never writes a setting.
  `gdext/assets/loader.rs:17-30` `resolve_locale()` reads the Godot ProjectSettings key
  `mindustry/locale` (absent), so the locale is always `en`. Callers: `gdext/assets/atlas.rs:171`
  (`load_bundle`), `gdext/ui/hud.rs:291`, `gdext/campaign.rs:201`. `core/io/settings.rs:61`
  `KEY_LOCALE = "locale"`. Locales come from `MindAssets.locales()` (`assets/locales`).
* Hotkeys (H08 T5 evidence 2026-10-14): `1`,`1` selects the Nth block of the current category
  (`conveyor`; Java `duo`); `3` picks `plastanium-conveyor` without changing the category;
  `.` changes the category but keeps the block; middle-click pick selects the block but not its
  category; F1 over a configurable block opens its config panel instead of the content dialog.
  Code: `gdext/input/desktop.rs` `select_catalog_block` :1115, `block_info_hotkey` :1506,
  `pick_block` :1534, `"block_info"` dispatch :1037; `ui/fragments/placement_fragment.gd _on_hotkey` :822.

---

## 2. Tasks

Order: T1 -> T2 -> T3 -> T4 -> T5.

### T1 — One shared settings store (Fixes PAR-0164)

**Java.** One `Core.settings` (`arc.Settings`) for the whole client.

**Changes.**
1. New `gdext/settings_hub.rs` (`mod settings_hub;` in `gdext/lib.rs`):
   ```rust
   static HUB: std::sync::OnceLock<std::sync::Mutex<SettingsStore>> = std::sync::OnceLock::new();
   pub fn init(paths: &Paths)                                            // idempotent load
   pub fn with_settings<R>(f: impl FnOnce(&mut SettingsStore) -> R) -> R  // a poisoned lock -> into_inner
   pub fn reload(paths: &Paths)                                          // after data import / clear game data
   pub fn save(paths: &Paths) -> Result<(), IoError>                     // force_save
   ```
2. Migrate, one facade at a time, in this order: `MindUi`, then `MindCampaign` (its
   `with_campaign_mut(f: FnOnce(&mut Campaign, &mut SettingsStore))` gets the hub store inside the
   closure: call `settings_hub::with_settings(|s| ...)` AROUND the campaign closure, never nested inside
   another `with_settings`), then `MindMods` (I07 T1 may have given it a private store: replace it),
   then the input bindings loads. Data import / clear game data -> `settings_hub::reload`.
   Delete `live_setting_flag`, `live_setting_flag_default`, `live_save_interval_seconds` and their uses.
3. Every put through `MindUi` saves through the hub (`persist_settings` -> `settings_hub::save`).

**Tests.** `settings_hub.rs` `#[cfg(test)]` with a temp dir: two closures share a write; `save` then a
fresh `SettingsStore::load` sees it.

**Verify in-engine.**
(a) `get_node("/root/MindUi").settings_put_flag("skipcoreanimation", true)` then a fresh never-played
preset launch (I-environment §4.1, e.g. sector 97) -> `$C.is_cutscene()` is false right after
`start_sector`. Restore the flag.
(b) `grep -n "live_setting_flag\|live_save_interval_seconds" client/rust/mind-gdext/src/campaign.rs`
prints nothing, and (a) still passes.
(c) H05 T18's acceptance: `settings_put_flag("campaignselect", true)` via MindUi, then the
`MindCampaign` getter that reads it (`grep -n "campaignselect" gdext/campaign.rs`) returns true in the
same session.

**Pitfalls.** `std::sync::Mutex` is not re-entrant: never call Godot or another facade inside
`with_settings`. Grep every closure after the migration.

**Commit.** `Fixes PAR-0164: one shared settings store for MindUi, MindCampaign, MindMods and input`

---

### T2 — Language choice persists and selects the bundle at boot (Fixes PAR-0135)

**Java.**
* `LanguageDialog` (`J/ui/dialogs/LanguageDialog.java`): title `@settings.language`, close button; one
  `Styles.flatTogglet` button per locale (400x50, a `ButtonGroup`, checked = current) labelled with
  `displayNames` (the 36-entry map at the top of the file; unknown locales show their code; `in_ID` is
  shown as `id_ID`); a click on a different locale does
  `Core.settings.put("locale", loc.toString()); player.locale = loc; ui.showInfo("@language.restart")`.
  `getLocale()` reads `locale`; `"default"` resolves through `findClosestLocale()` (exact match with the
  OS locale, then the same language, else `en`) and STORES the result.
* Boot (`J/Vars.java:501` default `locale = "default"`; `:536-563` `loadSettings` bundle selection):
  `"default"` -> `Locale.getDefault()`; otherwise `lang[_COUNTRY]`; `Core.bundle = I18NBundle.createBundle(handle, locale)`
  (fallback chain `bundle_<lang>_<COUNTRY>` -> `bundle_<lang>` -> `bundle`); the special `router` locale
  replaces every non-space character of the English bundle with the router icon; then
  `bundles/global.properties` is merged in. Fonts: `Fonts.loadExtraFonts` adds CJK glyph fonts for those
  locales (grep `loadExtraFonts` in `J/ui/Fonts.java`).

**Changes.**
1. `language_dialog.gd`: Java's layout and display names (copy the map), write `locale` through
   `MindUi.settings_put_text("locale", code)` (the T1 hub), show `@language.restart`.
2. `gdext/assets/loader.rs resolve_locale()`: read `locale` from the settings hub (T1; if T1 is not on
   `main`, read `user://config/settings.bin` with `SettingsStore::load`); `"default"` -> the OS locale
   (`OS.get_locale()`, e.g. `de_DE`) resolved like `findClosestLocale` against `MindAssets.locales()`, and
   store the result; otherwise use the stored value. Keep `mindustry/locale` (ProjectSettings) as an
   override for tests only.
3. Bundle loading (`gdext/assets/bundle.rs`): Java's fallback chain and the `router` transform; merge
   `global.properties`.
4. CJK: when the locale is `zh_CN`, `zh_TW`, `ja` or `ko`, add `assets/fonts/font_jp.woff` (check which
   fonts Java adds per locale) as a fallback font of the theme's default font
   (`MindThemeBuilder`/`MindAssets` font setup).

**Tests.** `resolve_locale_default_picks_closest` (pure helper), `bundle_fallback_chain`.

**Verify in-engine.** Settings > Language > Deutsch -> the restart info shows;
`tools/session-play.sh N --play` (restart the game process) -> the main menu is German (screenshot);
switch back to English and restart.

**Commit.** `Fixes PAR-0135: language choice persists and selects the bundle at boot`

---

### T3 — Block hotkeys per PlacementFragment (Fixes PAR-0214)

**Acceptance.** (a) `1`,`1` -> `duo`; (b) `3` with a block selected switches to distribution and selects
its remembered or first block; (c) middle-click on a placed `graphite-press` selects it AND switches the
palette to `crafting`; (d) F1 over a sorter with nothing selected opens the `content` dialog for `sorter`,
never the config panel.

**Java.** `PlacementFragment.updatePick` (`J/ui/fragments/PlacementFragment.java:132-172`) and
`gridUpdate` (`:177-273`). Read both in full (`sed -n 132,273p`). Rules:
* Pick (`:134`): `keyTap(pick) && player.isBuilder() && !scene.hasDialog()`; `build` is null in fog;
  `tryBlock = build instanceof ConstructBuild c ? c.current : build.block`;
  `tryConfig = build.block.copyConfig ? build.config() : null`; a non-breaking player plan whose bounds
  contain the cursor overrides both; accept only `tryBlock.isVisible() && unlocked(tryBlock)`; then
  `input.block = tryBlock; tryBlock.lastConfig = tryConfig; currentCategory = block.category`.
* `gridUpdate` runs only `if(!control.input.commandMode && gridUpdate(...))` (`:745`): in command mode,
  digits are control groups and pick does not run.
* Digits/arrows/categories (`:186-258`): arrows: left `(j-1+n)%n`, right `(j+1)%n`, up
  `j = j>3 ? j-4 : n - n%4 + j; j -= j<n ? 0 : 4`, down `j < n-4 ? j+4 : j%4`; first digit when
  `blockSelectEnd || timeSince(seqMillis) > 400`: category `Category.all[i]` if non-empty, and
  `if(input.block != null) input.block = getSelectedBlock(cat)`; second digit: `blockSelectSeq = i+1`;
  third digit: `i += (blockSelectSeq - (i != 9 ? 0 : 1)) * 10; blockSelectEnd = true`; then
  `blocks = getByCategory(cat); if(i >= size || !unlocked) return; input.block = blocks[i];
  selectedBlocks.put(cat, block); seqMillis = now`.
* `Category.all` order: turret, production, distribution, liquid, power, defense, crafting, units,
  effect, logic (digits 1..0).
* `categoryPrev/Next` (`:240-258`): cycle, skipping empty categories, then
  `input.block = getSelectedBlock(currentCategory)` (selects even when nothing was selected).
* `blockInfo` (`:260-271`): a hovered unit with `type.unlockedNow()` -> `ui.content.show(unit.type)`;
  else `display = menuHoverBlock ?? input.block ?? hovering (construct -> current)`; show it if unlocked.

**Changes.**
1. `desktop.rs` `dispatch_action`: outside command mode, digits, `block_select_left/right/up/down` and
   `category_prev/next` only `fire_hotkey(name)`. Delete `select_catalog_block` if unused afterwards.
2. `placement_fragment.gd _on_hotkey(action)`: the Java logic with `_selected_blocks: Dictionary`
   (category -> block), `_block_select_seq := 0`, `_block_select_end := true`, `_block_select_ms := 0`
   (`Time.get_ticks_msec()`). Category for digit `i` = the `_catalog.categories` entry with `ordinal == i`
   (absent = empty -> ignore). Record `_selected_blocks[cat]` on palette clicks too. On `"pick"`, switch
   `_selected_category` to the entry containing `selected_block()`.
3. `pick_block` per `updatePick` (player unit present, no dialog; construct -> `current`; queued
   non-breaking player plans override: add `MindSimHost::player_plan_at_world(wx, wy) -> Option<(BlockId, ConfigValue)>`;
   only unlocked catalog blocks; store the copied config when `copy_config`/`save_config`). Nothing in
   command mode.
4. `block_info_hotkey`: never the config panel. (1) the nearest selectable unit within the tap radius ->
   `open_dialog("content", {"content": <unit type>})`; (2) else the selected block, else the block under the
   cursor (construct -> current) -> `open_dialog("content", {"content": name})`. Confirm the context key in
   `ui/dialogs/content_info_dialog.gd`.

**Verify in-engine** (Ground Zero; find the fragment with
`get_node("/root/Spine/Ui/UiRoot").find_child("placement*", true, false).get_path()`): (a)-(d) above with
`godot_input` key events (two `1` presses within 400 ms in one `gmcp` call) and `$H.selected_block()`;
deselect with `$I.select_block_by_name("air")` (Q is `clear_building`, not deselect). Screenshots.

**Pitfalls.** Digits must not fire while a text field has focus (already gated). The 400 ms window is
wall-clock. Coordinate with I04 T1, which also edits `placement_fragment.gd` (keep the diff to the named
functions; merge `main` first).

**Commit.** `Fixes PAR-0214: block hotkey combos, arrow navigation, pick and F1 as in PlacementFragment`

---

### T4 — Settings rows take effect (audit against SettingsMenuDialog)

**Java.** `J/ui/dialogs/SettingsMenuDialog.java`: the game, graphics and sound categories, every
`checkPref`/`sliderPref`/`textPref` with its default and change listener, and where each key is READ
(`grep -rn 'settings.getBool("<key>"\|settings.getInt("<key>"' J/`).

**Procedure.** List every row the port shows (`MindUi.settings_rows_json(category)` for each category).
For each key, find the Java reader and the port reader (`grep -rn '"<key>"' client/rust client/ui`). Build
a table in your summary: key, Java effect, port reader (file:line or "none"), verdict. Then fix every row
whose port reader is missing or ignores changes, one commit per category (`Gameplay I05 T4: <category>
settings take effect`). Examples of keys that must work: `uiscale`, `fpscap`, `vsync`, `fullscreen`,
`borderlesswindow`, `screenshake`, `effects`, `bloom`, `bloomintensity`, `animatedwater`,
`animatedshields`, `pixelate`, `linear`, `hidedisplays`, `playerindicators`, `indicators`, `minimap`,
`position`, `fps`, `blockreplace`, `conveyorpathfinding`, `hints`, `doubletapmine`, `autotarget`,
`keyboard`, `crashreport`, `savecreate`, `saveinterval`, `musicvol`, `sfxvol`, `ambientvol`,
`mutemusic`, `mutesound`, `alwaysmusic`, `chatopacity`, `lasersopacity`, `unitlaseropacity`,
`bridgeopacity`, `skipcoreanimation`, `modcrashdisable`, `logiclocalization`, `showweather`.
(Check the actual list in Java; this list is a reminder, not the spec.)

**Commit(s).** `Gameplay I05 T4: <category> settings take effect`

---

### T5 — Input re-verification with the real player (H08 V1; evidence only)

Re-run with the sim PAUSED during the readback so instant completion cannot hide a queue:
1. Queued plan: `$H.set_paused(true)`, select `mechanical-drill`, real click on free ore -> right after,
   `player_build_plans_json()` lists the plan; `step(1)` -> progress > 0; `step(200)` -> built.
2. Paste: 3 conveyors `place_block_rot(134..136,64,"conveyor",0)`, F-drag them (KeyDown `f` at (134,64),
   move to (136,64), KeyUp), move to (140,64), left click -> `player_build_plans_json()` has conveyors at
   (139..141,64) rotation 0, QUEUED.
3. Rebuild: `place_block(144,60,"copper-wall")`, `debug_kill_building(144,60)`, B-drag over it -> a queued
   plan built by the alpha (`construct_progress` grows), the ghost leaves `team_ghost_plans_json()`.
4. Command-mode objective: 2 player daggers, Shift-drag-select, right-click a point ->
   `$H.command_mode_objective()` true; Shift release -> false.
5. Tap-unit deposit: `transfer_item(129,55,"copper",30,false)`, a duo next to the core, press on the alpha,
   release on the duo -> stack 15.
Record every output. If a check fails, stop, describe it, and fix it in its own commit
(`Gameplay I05 T5: <what>`).

---

## 3. Hot files

`gdext/ui/ui_host.rs`, `gdext/campaign.rs`, `gdext/mods.rs`, `gdext/input/mod.rs` (T1),
`gdext/assets/loader.rs` + `bundle.rs` (T2), `gdext/input/desktop.rs` + `ui/fragments/placement_fragment.gd` (T3).

## 4. Ledger changes to report

| Item | After |
|---|---|
| PAR-0164 | verified (T1) |
| PAR-0135 | verified (T2) |
| PAR-0214 | verified (T3) |
| PAR-0209 | verified if T5.2/T5.3 show configs carried (rebuild/sorter `nextConfig`) |

## 5. Done criteria

All tasks committed with their checks; suites green; parse gate clean; summary in
`REPO/docs/I05-settings-language-input_summary.md`.
