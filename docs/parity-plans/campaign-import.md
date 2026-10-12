<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Campaign + profile import — implementation plan (parity session 5)

Audience: an executor that has NOT seen the session that produced this plan.
Every item below says exactly what to change, where, how to test it and how
to verify it in the running Godot client. Do the items in the order of
section 3 unless a dependency note says otherwise. Do not make judgement
calls beyond what is written; when something does not match this plan, stop
and report it.

Java reference (read-only): `/home/clintonnguyen/g/code_examples/Mindustry`
(`core/src/mindustry/...`), Arc: `/home/clintonnguyen/g/code_examples/Arc`.
All line numbers below were checked on 2026-10-12 against those trees and
against this worktree at commit `24f8ede`.

---

## 0. Environment & workflow

* Worktree: `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session5`,
  branch `parity/session-5`. Work and commit ONLY here. Never edit the main
  checkout `/home/clintonnguyen/g/code_examples/Mindustry-Godot` or other
  `-sessionN` worktrees. Do not push, do not merge.
* Read first: `client/AGENTS.md`, `client/rust/AGENTS.md`. Key invariants:
  SPDX header `GPL-3.0-only` on new files; GDScript is UI-only (never mutates
  sim state; calls native nodes); `.tscn`-first (any node created in code
  needs a `# code-instantiated: <reason>` comment); user-visible strings go
  through bundle keys (`_t("@key")`, `_tm("@key")` for markup);
  `mind-core` must not `use godot`/`use tokio`; workspace lints deny
  `unwrap_used`/`expect_used` outside tests.
* Generated assets: the worktree's `assets/` lacks the gitignored generated
  files on a fresh worktree (sprites atlas, locales, shader index...). If the
  game log says `[assets] missing manifest .../sprites.atlas.json`, copy them
  from the main checkout (read-only use of main is fine):
  ```bash
  M=/home/clintonnguyen/g/code_examples/Mindustry-Godot; S=/home/clintonnguyen/g/code_examples/Mindustry-Godot-session5
  (cd $M && git status --short --ignored assets | grep '^!!' | awk '{print $2}') | while read p; do
    mkdir -p "$S/$(dirname $p)"; cp -a --reflink=auto "$M/$p" "$S/$(dirname $p)/"; done
  ```
* Build (one cargo job at a time; RAM is shared by 4 agents):
  `cd /home/clintonnguyen/g/code_examples/Mindustry-Godot-session5 && CARGO_BUILD_JOBS=6 tools/build.sh`
  (~40 s - 4 min). Expected tail: `Finished \`dev\` profile` then
  `sync_scenarios: synced N file(s)`.
* Unit tests: `cd client/rust && CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib <filter>`.
  Code touching legacy Java saves needs `--features msav-import` (the
  gdext build enables it; plain `mind-core` tests do not). Lint:
  `cargo clippy -p mind-core --all-targets -- -D warnings`,
  `cargo fmt -p mind-core -p mind-gdext`.
* Godot session: bridge port 7010, display :14, isolated user dir
  `.../Mindustry-Godot/.opencode/parity-session/session-5/xdg-data/godot/app_userdata/Mindustry-Godot/`
  (call it `$U`; settings at `$U/config/settings.bin`, saves at `$U/saves/`).
  * `tools/session-play.sh 5` — restart editor (loads a new `.so`) + play `game.tscn` (~40 s).
  * `tools/session-play.sh 5 --play` — replay the scene only (GDScript-only changes).
  * `tools/session-play.sh 5 --import` — restart + Game Data import of
    `/home/clintonnguyen/g/code_examples/mindustry.save.full.tech.tree.zip` + replay.
    Use it whenever you need the pristine Java profile again (it re-extracts
    `sector-serpulo-15.msav`, which the remap then moves to 170).
* Driving the game from the shell (`G` alias used below):
  ```bash
  export OPEN_GODOT_MCP_PORT=7010
  G="python3 /home/clintonnguyen/g/code_examples/Mindustry-Godot/tools/gmcp.py"
  $G --eval 'return <single gdscript expression or statement>'   # evals MUST `return`
  $G --screenshot runs/20261012-session5/<name>.png              # 1152x648; view with the Read tool
  ```
  Keep evals single-statement and loop-free (a `for` on one line with a
  trailing statement is OK, as used below; a bad eval can pin the frame loop
  -> recover with `tools/session-play.sh 5 --play`). If every call times out
  after 15 s the game is stalled (seen once under memory pressure): run
  `tools/session-play.sh 5 --play`.
* Recipes:
  * open the planet page: `$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'`
  * press the selected sector's action like the UI does (Go/Resume/Launch):
    `$G --eval 'get_node("/root/Spine/Ui/UiRoot").dialog("planet")._sector_action_pressed("serpulo", 170, "go"); return true'`
  * enter a sector directly (what the UI does underneath):
    ```bash
    $G --eval 'get_node("/root/Spine/SimHost").load_sector("serpulo", 170); return true'
    $G --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 170)'
    $G --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
    ```
    (A bare `start_sector` without `load_sector` skips the world load ->
    instant "Game Over / no core".) Ground Zero is cell **170** (v8 preset
    remap), frozenForest **64**, not the legacy 15/86.
  * campaign state: `$G --eval 'return get_node("/root/Spine/MindCampaign").get_sector_state()'`
  * sector rows: `$G --eval 'var v = JSON.parse_string(get_node("/root/Spine/MindCampaign").campaign_views_json("serpulo")); for s in v["sectors"]: if int(s["id"]) == 170: return s
return null'`
  * read Godot's settings file (MGST format): `python3 -I /tmp/s5/mgst.py $U/config/settings.bin '<key regex>'`
    (script source in appendix A; recreate it if `/tmp/s5` was cleaned).
  * read a Java settings.bin / UBJSON values: appendix A `arcset.py`.
* Evidence screenshots go to `runs/20261012-session5/` (shared dir). The Java
  reference frames: `runs/20261012-overview/java-*.png`.
* Commits: one per item, message `Fixes PAR-XXXX: <summary>` (or a
  descriptive subject for non-ledger work), body explaining Java vs port,
  last line exactly `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
  Do NOT edit `chains.json` or `parity-ledger.json`.

---

## 1. Already done (do not redo)

| Commit | What |
|---|---|
| `ddda7fc` | `mind-core/src/io/json/ubjson.rs` (Arc `UBJsonReader` port); `SettingsStore::get_json` decodes Java `putJson` bytes (type 5); `JsonIo::read` retries Arc lenient JSON via `quote_bare_keys`; `saves.rs` `sector_of_slot/sector_key_of` use it, so the legacy remap runs (`sector-serpulo-15.msav` -> `sector-serpulo-170.msav`, info migrated, source info cleared); `MindCampaign::bootstrap` loads sector info after `Saves::load` and flushes. |
| `82786f3` | `MindUi.hint_finished/hint_finish` (`<name>-hint-done`), hints fragment honors/persists them. |
| `e0b6089` | `SettingsStore::force_save` merges only this store's changed keys onto the on-disk file (fixes facades clobbering each other's settings). |
| `7e14490` | Planet page: `SectorView.name` follows `Sector.name()` (+`named`), top-right **Sectors / N under attack** list panel, selected-sector panel matches `updateSelected` (Resources row, Stats button -> `full_text` dialog). |
| `5fa2b9c` | Hints only show when their `DefaultHint.shown` predicate holds (desktopMove/zoom/desktopPause/launch/presetLaunch evaluated, rest hidden); `get_sector_state` gains `preset` and `captured`. |
| `5c151f1` | Load Game cards show map / mode / wave / playtime / date from save meta. |
| `588a8c8` | **PAR-0154 fixed**: globe sector status rings, shadow fills, preset icons, padlocks, attack glyphs; `SectorView` gains `selectable`, `require_unlock`, `preset_unlocked`, `lock_icon`, `icon`, `content_icon`. |
| `24f8ede` | **PAR-0153 partial**: `saveinterval` is seconds (`DEFAULT_SAVE_INTERVAL_SECONDS = 60`), not minutes. In-engine verification still pending (item B). |

Verified in-engine (no code change needed):
* Sector preset unlock keys: Java keys are `<camelCaseName>-unlocked` and the
  port's preset names are already camelCase (`frozenForest`). The earlier
  "serpulo:86 is locked" was because 86 is **frontier** after the v8 remap
  (`core/assets/planets/serpulo.json`), which the profile has not unlocked.
  `start_sector("serpulo", 64)` (frozenForest) launches:
  `runs/20261012-session5/frozenforest-start.png`, state `preset: frozenForest`.
* Ground Zero resume from the imported profile loads the Java save with its
  core (`runs/20261012-session5/groundzero-ingame.png` vs
  `runs/20261012-overview/java-groundzero-ingame.png`).
* Research progress keys: Java `TechTree.java:173,198` `req-<content>-<item>`
  == `TechNode::requirement_key` (`mind-core/src/content/tech/mod.rs:112`).
  Only a verification step remains (A1.2).

---

## 2. Dependency map

* A (import residuals) is independent; do it first (small).
* PAR-0158 (item C) depends on nothing; it adds the `lastplanet` /
  `campaignselect` accessors that A2 (remap notice) also uses -> do C before A2,
  or do A2 inside C's commit.
* B (PAR-0153 autosave) independent.
* D (loadout, PAR-0085/0054) needs a launchable sector with a launcher: the
  imported profile has only Ground Zero based and it is *attacked*, so
  `_find_launcher` returns nothing. Use the test fixture in D.5 (capture Ground
  Zero first) to exercise it. PAR-0025/0026/0027 are dialog-button parity on
  top of D.
* E (PAR-0131 cutscene) independent, render-side.
* F (PAR-0112 generated sectors) depends on D's launch-loadout placement
  (`place_launch_loadout`) — do D first.
* G (PAR-0156) independent, sim-side.

---

## 3. Items

### A. Import residuals

#### A1. Verification-only checks (no code unless they fail)

A1.1 Numbered slot loading. Symptom to rule out: Load Game -> pick a Java slot
does nothing / crashes.
1. `tools/session-play.sh 5`; `$G --eval 'return get_node("/root/MindUi").open_dialog("load", "")'`.
2. Load slot `3` the way the card does: `$G --eval 'return get_node("/root/Spine/MindCampaign").load_slot("3")'`
   (`campaign.rs` `load_slot`, line ~1552; the dialog calls
   `campaign_call("load_slot", [stem])`, `load_dialog.gd:~256`).
3. `$G --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'`, wait 5 s,
   screenshot `runs/20261012-session5/slot3-loaded.png`.
   Acceptance: the map ("Ancient Caldera", wave 16 in the Java meta) renders
   with buildings. If it fails, record the log lines
   (`$U/logs/` newest file) and file it as a new gap; do not fix inside A.

A1.2 Research progress. `$G --eval 'return get_node("/root/MindUi").open_dialog("research", "")'`
screenshot; compare a node with partial progress in the Java profile, e.g.
`req-lancer-silicon = 1900` (`python3 -I /tmp/s5/arcset.py /tmp/s5/prof/settings.bin '^req-lancer'`).
Acceptance: the lancer node shows 1900 silicon already paid. If not, the bug
is in `mind-core/src/game/tech_tree.rs:229` load path; report it.

**Status 2026-10-12:** A1.1 **pass** — slot 3 (sandbox) loads and renders; the
plan's named map "Ancient Caldera" wave 16 is actually slot **7**, which loads
with 40 buildings (core-acropolis + 4 power sources, wave 16 in state);
`runs/20261012-session5/slot3-loaded.png`, `slot7-ancient-caldera.png`. A1.2
**fail** (filed as a new gap, not fixed inside A): lancer is already unlocked in
the imported profile, and the read model carries no requirement data at all for
it (`requirements: []`, `finished: []`); 374 of 375 nodes have empty
`requirements` because `TechTreeBuilder` never materializes
`content.researchRequirements()` (only `tech_tree::effective_requirements()` at
spend/complete time loads `req-` progress), so the research dialog cannot show
costs or partial payments for any node. The dialog also renders the raw
`techtree.<null>` key. Evidence: `research-before.png`; compare
`python3 -I /tmp/s5/arcset.py /tmp/s5/prof/settings.bin '^req-lancer'`
(silicon 1900 / lead 2200 / copper 1900 / titanium 900).

#### A2. Legacy remap residuals (new gaps)

Symptom 1: after the remap the planet camera still looks at legacy cell 15
(`serpulo-last-sector = 15` from the Java profile). Java has the same key and
does not rewrite it in `Saves.load` (Saves.java:115-190), so **this is parity
— do not change it.**

Symptom 2 (real gap): Java shows a one-time "Campaign Changes Notice" dialog
after a Serpulo remap. Java: `PlanetDialog.java:216-222`
```java
}else if(hadSerpuloRemaps && settings.getBoolOnce("serpulo-remaps-notice")){
    var diag = new BaseDialog("@campaign.rework.title");
    diag.cont.add("@campaign.rework.serpulo").labelAlign(Align.center).growX().wrap().maxWidth(500f);
    diag.buttons.button("@ok", Icon.ok, diag::hide).size(280f, 64f);
```
`Settings.getBoolOnce(key)` = return `!getBool(key)` and put `key=true`.
`Vars.hadSerpuloRemaps` is set by `Saves.load` (Saves.java:173).
Port state: `Saves::had_serpulo_remaps` exists (`mind-core/src/game/saves.rs:93`),
set in `assign_sector_saves`; nothing reads it. Bundle keys exist
(`assets/bundles/bundle.properties:235-236`).

Changes:
1. `client/rust/mind-gdext/src/campaign.rs`: add
   ```rust
   /// `Vars.hadSerpuloRemaps && settings.getBoolOnce("serpulo-remaps-notice")`.
   #[func]
   pub fn take_serpulo_remaps_notice(&mut self) -> bool {
       if !self.saves.had_serpulo_remaps || self.settings.get_bool("serpulo-remaps-notice", false) {
           return false;
       }
       self.settings.put_bool("serpulo-remaps-notice", true);
       if let Err(error) = self.settings.force_save(&NativeFs, &self.paths) {
           log::warn!("remap notice flush failed: {error}");
       }
       true
   }
   ```
   Note `had_serpulo_remaps` is only true in the boot that performed the
   remap (same as Java's static flag).
2. `client/ui/dialogs/planet_dialog.gd` `shown()` (line 67): after the chooser
   decision, when the chooser was NOT shown, call
   `campaign_node().call("take_serpulo_remaps_notice")`; if true, open
   `full_text` with `{"title": _t("@campaign.rework.title"), "text": _tm("@campaign.rework.serpulo")}`
   via `get_node("/root/MindUi").call("open_dialog", "full_text", JSON.stringify(...))`,
   deferred (`call_deferred`) because `shown()` runs inside `MindUi.open_dialog`.
Verify: `tools/session-play.sh 5 --import` (re-import triggers the remap), open
the planet page -> notice appears once; close, reopen the page -> no notice;
restart (`tools/session-play.sh 5`) -> no notice. Screenshot
`runs/20261012-session5/remap-notice.png`.
Commit: `Planet page: one-time Serpulo remap notice (hadSerpuloRemaps)`.

---

### B. PAR-0153 — campaign sector autosave never runs

Symptom: play a sector, mine/complete objectives, quit/restart, resume ->
progress lost; the sector `.msav` mtime does not move during play.
Acceptance: with `saveinterval = 10` (seconds) the file
`$U/saves/sector-serpulo-170.msav` mtime advances every ~10 s of unpaused
play and the log prints `autosave queued`; after a restart, Resume shows the
mined items / objective progress.

Java: `Saves.update()` `core/src/mindustry/game/Saves.java:205-235`
```java
if(state.isGame() && !state.gameOver && current != null && current.isAutosave()){
    time += Time.delta;                                   // ticks
    if(time > Core.settings.getInt("saveinterval") * 60 && !Vars.disableSave){
        saving = true; current.save(); ... time = 0;
```
`saveinterval` is seconds (`SettingsMenuDialog.java:373`
`sliderPref("saveinterval", 60, 10, 5 * 120, 10, seconds)`).
`SaveSlot.save()` (Saves.java:330) -> `SaveIO.save(file)`; `SaveVersion.writeMeta`
also calls `state.rules.sector.saveInfo()` for campaign saves.
`SaveSlot.isAutosave()` = `getBool("save-" + index() + "-autosave", true)` (Saves.java:445).

Port state:
* Policy: `Saves::update_policy` `mind-core/src/game/saves.rs:226-266` (fixed
  to seconds in `24f8ede`).
* Driver: `MindCampaign::pump_runtime` `mind-gdext/src/campaign.rs:285`
  (calls `update_policy`, then `host.request_save(file)` when `autosaved`).
* Current slot: set in `start_sector_with_loadout` (`campaign.rs:~704-731`) via
  `self.saves.set_current_by_file` (resume) or `prepare_sector_slot` (new).

Changes (do them one at a time, re-verifying after each):
1. Verify first (the seconds fix may be all that was missing):
   ```bash
   tools/session-play.sh 5
   $G --eval 'return get_node("/root/MindUi").settings_set("saveinterval", 10)'
   # enter Ground Zero with the recipe (cell 170), then:
   stat -c %Y $U/saves/sector-serpulo-170.msav; sleep 25; stat -c %Y $U/saves/sector-serpulo-170.msav
   grep -c "autosave queued" $U/logs/$(ls -t $U/logs | head -1)
   ```
   Expected: mtime increases, count >= 2. If yes, go to step 4.
2. If no `autosave queued`: add a temporary
   `log::info!("saves policy: current={:?} autosave={} is_game={}", ...)` in
   `pump_runtime` right after `update_policy` to see which guard fails:
   * `current` None -> `start_sector_with_loadout` did not reach the
     `installed && save_name` branch (`campaign.rs:~704`). Make the resume path
     always call `self.saves.set_current_by_file(&mut self.settings, &file)`
     when `has_save` (move it out of the `installed` guard).
   * `is_game` false -> `runtime.session.phase.is_game()`; check
     `State::Playing` is set by `play_sector`.
   * `delta_ticks == 0` -> `self.last_pump_tick` vs `host.get_tick()`; the
     sim must be unpaused.
   Remove the temporary log before committing.
3. If `autosave queued` prints but the file does not change: the queued save
   goes through `MindSimHost.request_save` (`sim_host.rs:850`) and is executed
   at the next `IoSet` boundary; check `io_pending()` drains
   (`$G --eval 'return get_node("/root/Spine/SimHost").io_pending()'`).
4. Persist the sector record with each autosave (Java writeMeta ->
   `saveInfo`): in `pump_runtime`, inside `if saves_result.autosaved && ...`,
   call `self.persist_sector_record();` (exists, `campaign.rs:1632`) before
   `request_save`.
5. Resume check: after >= 1 autosave, mine some copper (or wait for a wave),
   note `$G --eval 'return get_node("/root/Spine/SimHost").get_state_json()'`
   core items; `tools/session-play.sh 5`; resume Ground Zero (Go); compare.
Tests: in `mind-core/src/game/saves.rs` tests add
`update_policy_interval_is_seconds`: settings `saveinterval = 10`, slot current
+ autosave, `update_policy` with `delta_ticks = 9 * 60` -> not autosaved, then
`2 * 60` -> autosaved. (`cargo test -p mind-core --lib saves`.)
Commit: `Fixes PAR-0153: campaign sector autosave runs on the saveinterval cadence`.

**Status 2026-10-12:** done — commit `3a948bb`. Step 1 passed on the pre-existing
seconds fix (`saveinterval=10`: `autosave queued` every ~10 s,
`io_pending()==0`), so only step 4 was needed: `persist_sector_record()` now
runs with every autosave. In-engine after the fix: `sector-serpulo-170.msav` and
`config/settings.bin` mtimes advance together each autosave, and a marker wall
placed at (131,54) survives `--play` restart + resume (hasCore, wave 1);
evidence `runs/20261012-session5/sector-autosave-resume.png`. New test
`update_policy_interval_is_seconds`; `cargo test -p mind-core --lib` = 1867
passed.

---

### C. PAR-0158 — "Select Starting Campaign" chooser reappears

Symptom: on a fresh boot, Play > Campaign shows the chooser although a planet
was chosen before (Java remembers `campaignselect`) — and the page opens on
serpulo even if the player last viewed erekir.
Acceptance: (1) with the imported profile (`campaignselect=true`) the chooser
never shows; (2) on a clean profile, choosing a planet in the chooser puts
`campaignselect=true` and after `tools/session-play.sh 5` the chooser does not
show; (3) switching to Erekir then restarting reopens on Erekir (`lastplanet`).

Java: `PlanetDialog.java:182-213`: show chooser iff
`!settings.getBool("campaignselect") && !content.planets().contains(p -> p.sectors.contains(Sector::hasBase))`;
OK button: `state.planet = selected; lookAt/selectSector(startSector); settings.put("campaignselect", true)`.
`lastplanet`: read at `PlanetDialog.java:84` (`getString("lastplanet","serpulo")`),
written at 247 and 902 (`settings.put("lastplanet", state.planet.name)` on show/planet switch).

Port: `client/ui/dialogs/planet_dialog.gd`
* `shown()` line 67: chooser iff `_selected_planet.is_empty()` and
  `_planets_with_base()` empty — ignores `campaignselect`; `_selected_planet`
  is per-session only.
* `_boot_planet()` line 154: hard-codes serpulo (comment says no accessor).
* `_on_select_ok()` line 524: does not persist anything.

Changes:
1. `client/rust/mind-gdext/src/ui/ui_host.rs` (next to `hint_finished`, which
   was added in `82786f3`): add
   ```rust
   #[func] pub fn settings_get_flag(&self, key: GString) -> bool { self.settings.get_bool(&key.to_string(), false) }
   #[func] pub fn settings_put_flag(&mut self, key: GString, value: bool) -> bool { self.settings.put_bool(&key.to_string(), value); self.persist_settings(); true }
   #[func] pub fn settings_get_text(&self, key: GString, default: GString) -> GString { GString::from(self.settings.get_string(&key.to_string(), &default.to_string()).as_str()) }
   #[func] pub fn settings_put_text(&mut self, key: GString, value: GString) -> bool { self.settings.put_string(&key.to_string(), &value.to_string()); self.persist_settings(); true }
   ```
   (Doc comment each with the Java `Core.settings` call it mirrors.) The
   `force_save` merge from `e0b6089` makes these safe alongside MindCampaign.
2. `planet_dialog.gd` `shown()`: replace the chooser condition with
   `if not _flag("campaignselect") and based.is_empty(): _show_select()`;
   when skipping, `_show_planet(_boot_planet(based))` where `_boot_planet`
   returns `_text("lastplanet", "serpulo")` if that planet row exists and is
   selectable (alwaysUnlocked landable or has a base: use
   `_find_planet(name)` non-empty and (`name in based` or
   `bool(planet.get("accessible", false))`)), else `based[0]`, else "serpulo".
   Add helpers `_flag(key)` / `_text(key, default)` calling the MindUi funcs
   (`get_node("/root/MindUi")`, guard `has_method`).
3. `_on_select_ok()`: before `_show_planet`, call
   `MindUi.settings_put_flag("campaignselect", true)`; select the planet's start
   sector (`_selected_sector = int(planet.start_sector)` from `_find_planet`).
4. `_show_planet()` (line ~528) and `_switch_planet()`: call
   `MindUi.settings_put_text("lastplanet", planet_name)`.
Tests: none in Rust (thin accessors). GDScript parse gate:
`bash tools/godot.sh --headless --editor --quit --path client` must print no
`SCRIPT ERROR`.
Verify: acceptance (1) with the imported profile; (2) by temporarily removing
the key: `$G --eval 'return get_node("/root/MindUi").settings_put_flag("campaignselect", false)'`
**and** a profile with no based sector — easiest: skip (2) on the imported
profile (it has a base, so Java would not show the chooser either) and
instead assert with the eval that after pressing OK on the chooser of a
fresh profile (`tools/session-play.sh 5` after moving `$U` aside —
`mv $U $U.bak`, restore with `mv $U.bak $U`) the key is set:
`python3 -I /tmp/s5/mgst.py $U/config/settings.bin '^campaignselect|^lastplanet'`.
Commit: `Fixes PAR-0158: honor campaignselect and lastplanet on the planet page`.

---

### D. Launch loadout — PAR-0085 (+ PAR-0054, PAR-0025/0026/0027)

Symptom: the schematic picked in Configure Loadout is ignored; every launch
uses the empty loadout. PAR-0054 (old 3-arg arity error) is already gone —
`_launch` calls 2-arg `start_sector` (`planet_dialog.gd:1391-1415`); close it
by verifying, not by code.
Acceptance: launching a sector from a based launcher with loadout schematic X
places X's core with `X.requirements` + launch resources in it, removes those
items from the launcher sector's stored items, and persists
`lastloadout-<core>` = X's file name.

Java:
* `PlanetDialog.playSelected` `PlanetDialog.java:1366-1440`: no launcher ->
  `universe.clearLoadoutInfo(); control.playSector(sector)`; launcher ->
  `loadouts.show(block, from, sector, () -> { loadout = universe.getLastLoadout();
  from.removeItems(loadout.requirements()); from.removeItems(universe.getLaunchResources()); ... control.playSector(from, sector) })`.
* `LaunchLoadoutDialog.show` `ui/dialogs/LaunchLoadoutDialog.java:36-220`:
  `@back` hides (line 41); `@resources.max` toggle (125); `@resources`
  opens LoadoutDialog with capacity `planet.launchCapacityMultiplier *
  core.itemCapacity` (60, 131-140) and stores `universe.updateLaunchResources`;
  `@launch.text` -> `universe.updateLoadout(core, selected); confirm.run()` (150-152);
  header `launch.from` (165).
* `Universe` `game/Universe.java:99-160`: `clearLoadoutInfo`,
  `getLaunchResources` (`launch-resources-seq` UBJSON ItemSeq),
  `updateLaunchResources`, `updateLoadout(block, schem)` ->
  `settings.put("lastloadout-" + block.name, schem.file.nameWithoutExtension())`,
  `getLastLoadout()` default `planet.generator.defaultLoadout` (`Loadouts.basicShard`).
* Placement: `Schematics.placeLaunchLoadout(x, y)` `game/Schematics.java:465-469`
  = `placeLoadout(universe.getLastLoadout(), x, y)` + core items add
  `universe.getLaunchResources()`; called by `FileMapGenerator.java:111,128`
  and every planet generator (`SerpuloPlanetGenerator.java:719`, ...).

Port state:
* `planet_dialog.gd` `_launch(planet, sector, loadout=-1, resume=false)`
  (1391): ignores `loadout`; `_on_loadout_chosen(index)` (1360) passes the
  schematic index; `_find_launcher` (1091) = any based, non-attacked sector.
* `launch_loadout_dialog.gd`: emits `loadout_chosen(index)` (`index` =
  `Schematics.all` index); rows from `campaign_section("loadouts")` with
  `{core, index, name, is_default, requirements: [[item, amount]], file?}`
  (`mind-core/src/ui/campaign.rs` `loadout_views`, ~line 921).
* `campaign.rs` `start_sector_with_loadout(planet, sector, loadout_json)`
  (line 501): applies `[{item, amount}]` to `session.rules.loadout`, consumed by
  `mind_core::game::play::apply_launch_loadout` (`play.rs:316`) — items only,
  into the existing core; the schematic's core *block* is not placed.
* `update_loadout(core, file)` / `get_last_loadout()` funcs exist
  (`campaign.rs:~2101-2125`) but nothing calls them from the UI.
* Launch resources: `get_launch_resources` (JSON) exists (used by the dialog).

Changes:
1. `launch_loadout_dialog.gd` `_confirm()`: before emitting, find the selected
   loadout row and call
   `campaign_node().call("update_loadout", str(row.core), str(row.get("file", "")))`
   (`Universe.updateLoadout`). If rows lack `file`, add `file: Option<String>`
   to the loadout view struct in `mind-core/src/ui/campaign.rs` from
   `schematic.file` (the schematic view already has it, line ~914).
2. `planet_dialog.gd` `_launch()`: when `loadout >= 0` and not `resume`, build
   the stacks: `requirements` of the chosen loadout row merged with
   `campaign_json("get_launch_resources")` (sum per item) into
   `[{"item": name, "amount": n}]`, then call
   `campaign.call("start_sector_with_loadout", planet_name, sector_id, JSON.stringify(stacks))`
   instead of `start_sector`. Keep the free-launch branch (`loadout < 0`)
   calling a new `campaign.call("clear_loadout_info")` first (step 3) then
   `start_sector`.
3. `campaign.rs`: add `#[func] clear_loadout_info(&mut self) -> bool` calling
   `campaign.universe.clear_loadout_info(&mut self.settings)`
   (`mind-core/src/game/universe.rs:134`) + `self.persist()`.
4. Remove the launched items from the launcher: add
   `#[func] remove_sector_items(planet: GString, sector: i32, stacks_json: GString) -> bool`
   in `campaign.rs` that subtracts from `campaign.sector_mut(...).info.info.items`
   (clamp at 0, Java `ItemSeq.remove` + `checkNegative`) and persists; call it in
   `_launch` with the launcher id (`_find_launcher(planet).id`) before starting.
5. Core block of the schematic: `apply_launch_loadout` only fills items. Java
   places the schematic's core block (`Schematics.placeLoadout`). For preset
   sectors the map already has a core (`FileMapGenerator` replaces it with the
   loadout core: `FileMapGenerator.java:100-130`). Port the replacement:
   in `mind-core/src/game/play.rs` `apply_launch_loadout`, if
   `session.rules.loadout_core: Option<String>` (new field on `PlaySession`,
   set by `start_sector_with_loadout` from a new optional `core` key in the
   JSON — send `{"core": row.core, "stacks": [...]}` and accept the old bare
   array too) differs from the existing core block, swap the core block at the
   core's tile through the existing world mutation path used by
   `replay_lost_sector` (`sim_host.rs`, search `fn replay_lost_sector`). If
   that is too large, land steps 1-4 first, commit, and record step 5 as the
   remainder of PAR-0085.
Tests (`mind-core`):
* `game::play::tests::apply_launch_loadout_fills_core_with_stacks` — session with a
  campaign sector, `allow_launch_loadout = true`, `rules.loadout =
  [copper 300, lead 200]`, assert the core inventory equals exactly that.
* `game::universe::tests::update_loadout_persists_key` — `update_loadout(settings,
  "core-shard", Some("big"))` -> `settings.get_string("lastloadout-core-shard","")=="big"`;
  `clear_loadout_info` removes it and `launch-resources-seq`.
D.5 Fixture to exercise the dialog (the imported profile has no launcher):
capture Ground Zero then pick an adjacent unlocked sector:
```bash
# enter Ground Zero (recipe), then:
$G --eval 'return get_node("/root/Spine/MindCampaign").capture_sector()'
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'
# frozenForest (64) is unlocked; select it and press Launch:
$G --eval 'var d = get_node("/root/Spine/Ui/UiRoot").dialog("planet"); d._selected_sector = 64; d._sector_action_pressed("serpulo", 64, "launch"); return true'
$G --screenshot runs/20261012-session5/launch-loadout.png   # Configure Loadout dialog
```
Then press the dialog buttons through evals
(`get_node("/root/Spine/Ui/UiRoot").dialog("launch_loadout")._confirm()`), and check
`get_state_json()` core items after load. PAR-0025 (Launch confirms), PAR-0026
(Resources opens the `loadout` dialog), PAR-0027 (Back/close cancels — the
dialog uses `add_close_button`; Java's button is `@back` with `Icon.left`:
change `add_close_button()` to `add_button(_t("@back"), hide_dialog, "left", 210.0)`
if `MindDialog` has no back variant — check `ui/mind_dialog.gd` for
`add_back_button` first) are verified with the same fixture and screenshots
compared to the Java soak frames in `runs/20261010-234250-ref-PAR-0070-combat-fx/java/`.
Pitfall: capturing Ground Zero mutates the session profile; re-import
(`tools/session-play.sh 5 --import`) afterwards.
Commits: `Fixes PAR-0085: launch with the chosen loadout and launch resources`;
separate commit for the PAR-0025/26/27 dialog-button parity.

---

### E. PAR-0131 — no landing cutscene

Symptom: entering a sector jumps straight to the static world; Java drops
the core in with a camera zoom (2.7 s, 160 ticks), a white fade-out and a
sector-name announce.
Acceptance: frames captured 0.5 s / 1.5 s / 3 s after `start_sector` show the
core sprite scaled/rotated and the camera zoomed in, then normal view; the
announce text `[accent]<sector name>\nResources: ...` appears at landing end.
Java:
* trigger: `Control.java:198-222` (`Trigger.newGame`): camera to core,
  `if(!settings.getBool("skipcoreanimation") && !state.rules.pvp) { coreDelay =
  core.launchDuration(); player.deathTimer = Player.deathDelay - coreDelay;
  renderer.showLanding(core); }`.
* `Renderer.java:180-199` (update: `landTime -= Time.delta`, `camerascale =
  launchAnimator.zoomLaunch()`, `camera.width = graphics.getWidth() / camerascale`),
  `567-574` `showLanding`.
* `CoreBlock.CoreBuild`: `launchDuration()` = `landDuration = 160f` (line 57,
  359); `beginLaunch(false)` (369-418: fullscreen white image fading out over
  35 ticks; after `launchDuration` -> `launchEffect`, shake(5,5), announce);
  `drawLaunch()` (424-478: particles, `drawLanding`, white mixcol
  `pow5In(fout)`, clouds with `cloudAlphas={0,.5,1,.1,0,0}`, `cloudAlpha .81`,
  `cloudScaling 1700`, `cfinScl -2`, `cfinOffset .3`, `calphaFinOffset .25`);
  `drawLanding(x,y)` (480-538: scale `region.width*scl*3.6*pow2Out(fout)`,
  rotation `pow2In(fout)*135`, thrusters `thrusterSizes` line 44);
  `zoomLaunch()` (655-658): `pow3.apply(landZoomFrom=0.02, landZoomTo=4, getLandTimeIn())`.
Port state: state machine exists and is unit-tested —
`mind-core/src/render/cutscene.rs` (`Cutscene::{show_landing, update,
land_scale, get_land_time_in}`, trait `LaunchAnimator`); nothing constructs it
outside tests. Camera: `mind-gdext/src/camera.rs` (`MindCamera2D`). World
draw: `mind-gdext/src/render.rs` / `render/`.
Changes:
1. `mind-core/src/render/cutscene.rs`: add `pub struct CoreLandAnimator { duration: f32, land_zoom_from: f32, land_zoom_to: f32, fin: f32 }`
   implementing `LaunchAnimator` (`launch_duration` 160.0; `zoom_launch` =
   `pow3(from, to, fin)` where `fin` is pushed by the cutscene each update — add
   `fn set_land_time_in(&mut self, fin: f32)` to the trait with a default no-op,
   called from `Cutscene::update`). Port `Interp.pow3` (`a + (b-a) * (x<=0.5 ? (2x)^3/2 : ((x-1)*2)^3/2+1)`)
   locally; check `mind-core/src/math` for an existing `interp` module first
   (`grep -rn "fn pow3" client/rust/mind-core/src/math`).
2. `mind-gdext`: own a `Cutscene` in the node that owns the camera (find with
   `grep -n "struct MindCamera2D" -A30 client/rust/mind-gdext/src/camera.rs`).
   Add `#[func] show_landing(core_x: f32, core_y: f32, core_block: GString)`
   that builds a `CoreLandAnimator` and calls `show_landing`; per frame call
   `cutscene.update(delta*60, paused)` and multiply the camera zoom by
   `cutscene.land_scale()` while `is_cutscene()`.
3. Draw: while `is_cutscene()`, draw the core region (`block-<core>-full` or
   the core's base region) at the core position scaled by
   `3.6*pow2Out(fout)*4/camerascale`, rotated `pow2In(fout)*135°`, and a
   fullscreen white overlay with alpha `pow5In(fout)` (a `ColorRect` in the
   `OverlayLayer` of `scenes/ui/ui_root.tscn`, driven from GDScript reading a new
   `get_land_time_in()` func). Hide the real core building sprite during the
   cutscene (renderer skip by position).
4. Trigger: in `campaign.rs` `start_sector_with_loadout`, after a successful
   play (both fresh and resume branches), if the setting `skipcoreanimation`
   is false and rules are not pvp, call the camera's `show_landing` with the
   player core position (`MindSimHost` has the core list; search
   `fn player_core_blocks` in `sim_host.rs`).
5. Announce at landing end: `ui.announce` equivalent — check
   `grep -rn "fn announce" client/rust/mind-gdext/src/ui` and use it with
   `"[accent]" + sector name + "\n[lightgray]" + @sectors.resources + "[white] " + resource emojis`.
Tests: `cutscene.rs` `core_land_animator_zoom_runs_from_to` — after
`show_landing`, `land_scale()` ~= 0.02*... at start and returns 1.0 after 160
update ticks.
Verify: enter Ground Zero, take screenshots at +0.5 s, +1.5 s, +3 s
(`sleep` between `$G --screenshot` calls); compare against Java frames in
`runs/20261009-065416-ref-ground-zero/` (the ledger's `run`).
Pitfall: `sleep` granularity — gmcp calls take ~1 s each; pause the sim
(`get_node("/root/Spine/SimHost").set_paused(true)`) to freeze a frame if needed.
Commit: `Fixes PAR-0131: core landing cutscene (zoom, drop-in, fade, announce)`.

---

### F. PAR-0112 — planet-generated sectors have no core / enemy base

Symptom: launching a non-preset cell yields terrain with no player core and
no enemy base/ruins.
Acceptance: `load_sector("serpulo", <generated id>)` + `start_sector` gives a
world with a player core at `SerpuloPlanetGenerator.spawn` holding the launch
loadout, and for `generateEnemyBase` sectors an enemy core + walls.
Java: `SerpuloPlanetGenerator.generate` (`maps/planet/SerpuloPlanetGenerator.java`,
ends ~line 700-760): `Schematics.placeLaunchLoadout(spawn.x, spawn.y)` (719);
enemy base via `BaseGenerator.generate` when `sector.hasEnemyBase()`; ruins
via `BaseGenerator` decoration; waves `Waves.generate`.
Port: `mind-core/src/maps/planet/serpulo.rs` (1605 lines) documents these as
not applied (header lines 7-12); hooks are trait seams in
`mind-core/src/maps/generators/base.rs` (`SchematicHooks::place_launch_loadout`,
`place_loadout`, `BaseRegistryView`) with only `NoopSchematicHooks`. Spawn
position is computed (`self.spawn`, line ~883).
Changes:
1. Implement `SchematicHooks` for a real type in `mind-core` (new file
   `mind-core/src/maps/generators/schematic_hooks.rs`) that places the loadout
   core block (default `core-shard` or the planet's `default_core`) at (x, y)
   for the default team in `Tiles` (set `block` + `team` + `build` origin the
   same way `FileMapGenerator` port places cores — find with
   `grep -rn "fn place_core\|core_shard" client/rust/mind-core/src/maps`).
2. In `serpulo.rs` `generate` (line 750), after spawn selection, call
   `hooks.place_launch_loadout(tiles, spawn.0, spawn.1, content)`; thread the
   hooks reference through `WorldParams` or the generator constructor (check
   how `PlanetGenerator` is constructed in `mind-gdext/src/sim_host.rs`
   `load_sector`).
3. Enemy base: depends on `BaseRegistry` (schematic parts). If
   `BaseCatalog` (used by `run_team_base_builder`, `runtime.rs:~817`) is
   loadable here, port `BaseGenerator.generate` core placement only (enemy
   core at the farthest enemy spawn) as a first slice.
Tests: `maps::planet::serpulo::tests::generated_sector_places_launch_core` —
generate a non-preset sector with a recording hooks impl and assert one
`place_launch_loadout` call at `generator.spawn`.
Verify: pick a generated selectable cell on the globe (a white-ring cell
without preset; list via the sector-rows recipe filtering `preset == null and
selectable`), enter with the recipe, screenshot.
Commit: `PAR-0112: place the launch core on planet-generated sectors` (state
"partially fixed" if the enemy base is not done).

---

### G. PAR-0156 — enemy AI residuals

G1. `estimateStats` ignores turret DPS.
Java `ai/RtsAI.java:329-366`: enemy turrets (`indexer.getEnemy(team, BlockFlag.turret)`)
within `range + 50` of the segment add `t.health` and `t.estimateDps()`;
`Turret.TurretBuild.estimateDps` (`Turret.java:335-338`):
`hasAmmo ? shoot.shots / reload * 60 * peekAmmo().estimateDPS() * potentialEfficiency * timeScale : 0`;
`ContinuousTurret` (43-46): `shootType.damage * 60 / (ContinuousBulletType ? damageInterval : 5)`;
`TractorBeamTurret` (150-153): `damage * 60 * efficiency * coolantMultiplier`; base 0.
Port: `mind-core/src/game/runtime.rs:1072-1095` `team_ai_enemy_stats` maps
only units (comment says turrets missing); `estimate_stats`
(`ai/rts_ai.rs:402`) already consumes generic `EnemyStat` rows.
Change: extend `collect_team_ai_rows` (same file, ~line 950-1000) to also
return turret rows `{team, x, y, health, range, target_air, target_ground,
dps}` for buildings whose block kind is a turret, computing `dps` per the
Java formulas from the block's content (shots, reload, ammo bullet damage;
look up the turret def fields with
`grep -n "pub struct TurretDef\|reload\|shoot_shots" client/rust/mind-core/src/content/registries/blocks/*.rs`)
and `hasAmmo` from the building's ammo state; append them in
`team_ai_enemy_stats`.
Test: `game::runtime::tests::enemy_stats_include_loaded_turrets` — a duo with
ammo yields one row with dps > 0; without ammo dps == 0.

G2. `prebuildAi` core defenders never spawn.
Java `core/Logic.java:573-585`: for each team with `rules.prebuildAi` and
not editor, for each core: if no unit of `core.unitType` with
`flag == core.pos()`, spawn one at the core, set `flag`, `Fx.spawn`.
`BuilderAI.fallback/useFallback` (`ai/types/BuilderAI.java:240-255`) ->
`PrebuildAI` for those units.
Port: `TeamRule.prebuild_ai` exists (`io/json/rules.rs:62`); `team_ai_system`
(`runtime.rs:740`) has no prebuild branch.
Change: in `team_ai_system`, per team with `rule.prebuild_ai`, enqueue a spawn
request for each core lacking a flagged unit, through the same request
channel the wave spawner uses (find with
`grep -rn "SpawnRequest\|spawn_unit" client/rust/mind-core/src/game/runtime.rs | head`);
store `flag = core pos` on the unit. `PrebuildAI` behavior itself is a
separate port; record it as remaining if absent.
Test: `game::runtime::tests::prebuild_ai_spawns_one_defender_per_core` —
one AI core with `prebuild_ai`; run the system twice; exactly one spawn request.
Verify (both): attack sector `serpulo:225` per the ledger `to` field.
Commit: `PAR-0156: turret DPS in estimateStats and prebuildAi core defenders`.

---

### H. New gaps found (file as ledger items via the report; fix only if time remains)

1. **Settings store fan-out** — MindUi, MindCampaign and MindSimHost each own a
   `SettingsStore` over one file (Java has one `Core.settings`). `e0b6089` made
   saves merge, but readers still see stale values until their own next save
   (e.g. MindUi reads `campaignselect` set by MindCampaign). Proper fix: one
   shared store (e.g. an `Arc<Mutex<SettingsStore>>` owned by the `MindUi`
   autoload and borrowed by the others). Files: `mind-gdext/src/ui/ui_host.rs`,
   `campaign.rs` (`self.settings`), `sim_host.rs`.
2. **Hint predicates** — only 5 of 27 `DefaultHint.shown` predicates are
   evaluated (`client/ui/fragments/hints_fragment.gd` `_hint_shown`); the rest
   need sim state (enemy count, unit payloads, placed-block/`events` hooks).
   Java: `ui/fragments/HintsFragment.java:168-310`.
3. **`Campaign` built with `EmptyNeighborhood`** (`campaign.rs` bootstrap,
   `Campaign::from_registry(&registry, &EmptyNeighborhood)`): every
   `Sector.near()` consumer in mind-core (sector invasion, import/export,
   `allowLanding`) sees no neighbours. `588a8c8` computes neighbours locally
   from `PlanetGrid` for the globe only; a `SectorNeighborhood` impl over
   `PlanetGrid` should be passed at bootstrap.
4. Planet page: sector **rename pencil** and **icon picker** buttons in the
   selected-sector title (`PlanetDialog.java:1199-1283`) are missing; the
   **Stats** view uses the generic `full_text` dialog instead of Java's
   `showStats` BaseDialog layout (`PlanetDialog.java:1054-1130`);
   sector-list panel background (`Styles.black8`) is not visible.
5. Globe: icons of cells beyond the limb still draw at the rim (Java culls by
   facing); `drawSpecialSelection` for `showHidden` presets and the derelict
   pulse (`absin(time,10,1)`) are static.
6. Load Game: no save **previews** (Java renders `previewFile()` from the
   minimap, `Saves.java:343-352`); cards show the placeholder map glyph.
7. In-game HUD vs Java Ground Zero frame: objective panel lacks the core icon
   and item icon; objective marker text lacks the inline item emoji; block
   palette opens on the units category instead of the last-used one.
8. Sector-info Java-only fields (`wavesSurvived`, `sumHealth`,
   `waveHealthBase/Slope`, `waveDpsBase/Slope`, `secondsPassed`, `bossWave`, ...)
   are dropped by the port's `SectorInfo` (`io/json/rules.rs:940`) and lost on
   the next `save_all`.

---

## Appendix A — helper scripts (recreate under /tmp/s5 if missing)

`/tmp/s5/mgst.py` (Godot MGST settings reader):
```python
import sys,struct,re
b=open(sys.argv[1],'rb').read(); p=8
n=struct.unpack('>H',b[6:8])[0]
def s():
    global p; l=struct.unpack('>H',b[p:p+2])[0]; p+=2; r=b[p:p+l].decode('utf8','replace'); p+=l; return r
for _ in range(n):
    k=s(); t=b[p]; p+=1
    if t in (0,6): v=s()
    elif t==1: v=struct.unpack('>i',b[p:p+4])[0]; p+=4
    elif t==2: v=struct.unpack('>q',b[p:p+8])[0]; p+=8
    elif t==3: v=struct.unpack('>f',b[p:p+4])[0]; p+=4
    elif t==4: v=bool(b[p]); p+=1
    elif t==5: l=struct.unpack('>I',b[p:p+4])[0]; p+=4; v='<bytes %d>'%l; p+=l
    if re.search(sys.argv[2],k): print(k,t,str(v)[:400])
```
`/tmp/s5/arcset.py` reads a Java `settings.bin` (zlib optional; `i32` count;
`readUTF` key; type byte bool0/int1/long2/float3/string4/binary5) and decodes
binary values as Arc UBJSON (same grammar as
`client/rust/mind-core/src/io/json/ubjson.rs`). Extract the profile once:
`mkdir -p /tmp/s5/prof && cd /tmp/s5/prof && unzip -o -q /home/clintonnguyen/g/code_examples/mindustry.save.full.tech.tree.zip`.
Usage: `python3 -I /tmp/s5/arcset.py /tmp/s5/prof/settings.bin '<key regex>'`.
`/tmp/s5/meta.py <file.msav> [maxlen]` prints a Java save's meta tags
(zlib-inflate, `MSAV`, i32 version, i32 len, i16 count, UTF key/value pairs).
