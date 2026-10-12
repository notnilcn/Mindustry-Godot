<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Audit + execution plan: campaign & Java-profile import (session 5)

This file is self-contained. You (the executor) have not seen the session
that wrote it, and you need no other plan file. Do the tasks in section 5
in the listed order. Every task says what is wrong, what Java does (file,
method, line), which port code to change, how to test it and how to verify
it in the running game. If something does not match what is written here,
stop that task and write down what you saw in your final report. Do not
improvise.

Java reference (read-only): `/home/clintonnguyen/g/code_examples/Mindustry`
(sources under `core/src/mindustry/...`), Arc: `/home/clintonnguyen/g/code_examples/Arc`.
Line numbers were checked on 2026-10-12.

---

## 1. Environment & workflow

### 1.1 Checkout and session

* Godot 4.7 client + Rust simulation. Main checkout:
  `/home/clintonnguyen/g/code_examples/Mindustry-Godot`. Read
  `client/AGENTS.md` and `client/rust/AGENTS.md` before changing code.
  Invariants you must keep: SPDX `GPL-3.0-only` header on new files;
  GDScript is UI-only (it calls native nodes, it never mutates sim state);
  nodes created from code carry a `# code-instantiated: <reason>` comment;
  user-visible strings use bundle keys (`_t("@key")`, markup `_tm("@key")`);
  `mind-core` never `use godot`/`use tokio`; no `unwrap()`/`expect()` outside
  tests (workspace lints deny them).
* You work in a parallel session worktree, never directly in the main
  checkout. Session N (N >= 2) = worktree
  `/home/clintonnguyen/g/code_examples/Mindustry-Godot-sessionN`, branch
  `parity/session-N`, Godot bridge port `6970 + 10*(N-1)`, X display `:(9+N)`.
  ```bash
  cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
  tools/parity-session.sh --session N          # creates/starts the worktree + editor
  cd ../Mindustry-Godot-sessionN
  git merge --ff-only main                     # start from current main
  bash tools/pack.sh pack                      # generated assets (atlas, locales, shader index); ~2-4 min
  CARGO_BUILD_JOBS=6 tools/build.sh            # Rust -> client/bin/rust; ends with "Finished `dev` profile" + "sync_scenarios: synced N file(s)"
  tools/session-play.sh N --import             # restart editor, play game.tscn, import the Java profile, replay (~60 s)
  ```
  * `tools/session-play.sh N` = restart the editor (needed after every Rust
    build, it loads the new `.so`) and play `res://scenes/game.tscn`.
  * `tools/session-play.sh N --play` = replay the scene only (enough after
    GDScript-only changes, and to recover a stalled game).
  * `tools/session-play.sh N --import` = restart + Settings > Game Data import
    of `/home/clintonnguyen/g/code_examples/mindustry.save.full.tech.tree.zip`
    + replay. Its output line ends with `{"nodes": 375.0, "unlocked": 282.0}`.
* If the log says `[assets] missing manifest .../sprites.atlas.json`, you
  skipped `bash tools/pack.sh pack`.
* The session's Godot user dir (call it `$U`):
  `/home/clintonnguyen/g/code_examples/Mindustry-Godot/.opencode/parity-session/session-N/xdg-data/godot/app_userdata/Mindustry-Godot`
  (`$U/config/settings.bin`, `$U/saves/`, `$U/logs/` newest file = current
  run log, `$U/crashes/` = Rust panic reports).

### 1.2 RAM / process limits

14 GB RAM is shared by up to 4 parallel sessions. Run ONE cargo command at a
time (`CARGO_BUILD_JOBS=6`), never start a second editor, never run the Java
client unless a task says so. If every gmcp call times out after 15 s and
`free -g` shows < 2 GB available, wait a minute, then
`tools/session-play.sh N --play`.

### 1.3 Driving the game (gmcp)

```bash
export OPEN_GODOT_MCP_PORT=$((6970 + 10*(N-1)))
G="python3 /home/clintonnguyen/g/code_examples/Mindustry-Godot/tools/gmcp.py"
$G --eval 'return <expression>'                    # evals MUST `return` something
$G --screenshot runs/<your-run-dir>/<name>.png     # 1152x648; open it with the Read tool
```
Eval traps (each of these happened during the audit):
* Calling a method that does not exist (e.g. `get_state_json` on
  `/root/Spine/Input`) raises a SCRIPT ERROR; break-on-error pauses the game,
  every later call times out and screenshots return a stale frame. Recover
  with `tools/session-play.sh N --play`. Check method names with
  `grep -n "pub fn <name>" client/rust/mind-gdext/src -r` first.
* Keep evals to one statement, or one `for` line followed by `return` on the
  next line (as below). No `while` loops.
* A Rust panic inside a `#[func]` shows in the log as
  `panic before unwinding into Godot` and writes `$U/crashes/crash-report-*.txt`.
  After any in-engine step run
  `grep -c "panic" $(ls -t $U/logs/* | head -1)`; expected `0`.

Useful reads:
```bash
$G --eval 'return get_node("/root/Spine/MindCampaign").get_sector_state()'            # planet, sector, preset, wave, hasSave, captured...
$G --eval 'return get_node("/root/Spine/SimHost").core_items_json()'                   # player core items, e.g. {"copper":100}
$G --eval 'return JSON.parse_string(get_node("/root/Spine/Input").get_input_state_json()).get("locks")'   # ["cutscene"] while landing
$G --eval 'var c = get_node("/root/Spine/World/Camera2D"); return [c.is_cutscene(), c.zoom.x, c.get_land_time_in()]'
$G --eval 'var v = JSON.parse_string(get_node("/root/Spine/MindCampaign").campaign_views_json("serpulo")); for s in v["sectors"]: if int(s["id"]) == 170: return s
return null'
```

### 1.4 Clean re-import of the Java profile (expected values)

```bash
U=...                                   # see 1.1
# stop nothing: session-play restarts the editor itself
rm -rf "$U"                             # clean session user dir (never touch other sessions' dirs)
tools/session-play.sh N --import        # expect {"nodes": 375.0, "unlocked": 282.0}
ls "$U/saves" | grep serpulo            # expect sector-serpulo-170.msav and sector-serpulo-15.msav-backup.msav, NO sector-serpulo-15.msav
```
Expected state after the import boot (a second boot runs the legacy remap):
* 282 of 375 tech nodes unlocked.
* Ground Zero = sector **170** (v8 preset remap of the legacy 15), has a save,
  has a core, is "under attack" (waves on).
* frozenForest = sector **64** (unlocked, no save); 86 is frontier (locked).
* The first planet-page open after the import shows the one-time
  "Campaign Changes Notice" (needs audit fix `09ba3d0`).
* Load Game lists 24 numbered slots with map/mode/wave/playtime/date.
* No hint popups in Ground Zero (the profile has the `-hint-done` keys).

Read Godot's settings file (MGST format) with this script (save it as
`/tmp/mgst.py`):
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
    if re.search(sys.argv[2],k): print(k,t,str(v)[:300])
```
Usage: `python3 -I /tmp/mgst.py $U/config/settings.bin '^campaignselect|^lastplanet'`.

### 1.5 Recipes

Enter Ground Zero exactly like the UI "Go" button (resume, no cutscene):
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'
sleep 3
$G --eval 'get_node("/root/Spine/Ui/UiRoot").dialog("planet")._sector_action_pressed("serpulo", 170, "go"); return true'
sleep 4
```
Enter a fresh (never played) sector directly (fires the landing cutscene):
```bash
$G --eval 'get_node("/root/Spine/SimHost").load_sector("serpulo", 98); return true'     # 98 = saltFlats
$G --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 98)'
$G --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
```
A bare `start_sector` without `load_sector` skips the world load and ends in
an instant Game Over. Unlocked presets without a base in the imported
profile: 23, 41, 64, 97, 98, 99, 142, 164, 165, 213, 217, 221, 223, 228, 266, 271.

Launcher fixture (needed for the Configure Loadout dialog; the imported
profile's only base, Ground Zero, is under attack, so there is no launcher):
```bash
# enter Ground Zero with the "Go" recipe, then:
$G --eval 'return [get_node("/root/Spine/SimHost").debug_add_core_items("copper", 3000), get_node("/root/Spine/SimHost").debug_add_core_items("lead", 2000)]'
$G --eval 'return get_node("/root/Spine/MindCampaign").capture_sector()'
$G --eval 'return get_node("/root/Spine/MindCampaign").update_launch_resources_json("[100, 50]")'   # copper 100, lead 50 (item index order)
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'
sleep 2
$G --eval 'var d = get_node("/root/Spine/Ui/UiRoot").dialog("planet"); d._selected_sector = 64; d._sector_action_pressed("serpulo", 64, "launch"); return true'
sleep 2   # Configure Loadout is now open
$G --eval 'get_node("/root/Spine/Ui/UiRoot").dialog("launch_loadout")._confirm(); return true'   # may take >15 s (world load): a timeout here is normal, wait 10 s
```
The fixture mutates the profile; do a clean re-import (1.4) afterwards.

### 1.6 Tests, lint, commits

* `cd client/rust && CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib <filter>`;
  code that reads Java saves needs `--features msav-import`.
* The UI goldens live in `mind-headless`:
  `cargo test -p mind-headless ui_goldens_match`. Any change to a
  `mind-core/src/ui/campaign.rs` view struct changes
  `client/rust/mind-core/tests/goldens/ui/campaign.json`; regenerate with
  `cargo run -p mind-headless -- ui campaign --dump client/rust/mind-core/tests/goldens/ui/campaign.json`
  (run from the worktree root; check `cargo run -p mind-headless -- ui campaign --help`
  for the flag name if it differs) and update that file's sha256 in
  `parity/golden_manifest.json` in the same commit (`sha256sum <file>`), as
  commit `8aaa27d` did.
* Lint: `cargo fmt -p mind-core -p mind-gdext` and
  `cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings`.
* GDScript parse gate: `bash tools/godot.sh --headless --editor --quit --path client`
  must print no `SCRIPT ERROR`/`Parse Error`.
* One commit per task, in your session worktree only, message
  `Fixes PAR-XXXX: <summary>` (or a descriptive subject when there is no
  ledger id), body = Java vs port + how verified, last line exactly
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
  Do not push. Do not merge into main. Do not edit `chains.json` or
  `parity-ledger.json` (report ledger changes instead, section 6).
* Evidence screenshots: `runs/<YYYYMMDD>-session<N>-<topic>/` (shared dir).

---

## 2. What was audited and the verdicts

Scope: the commits a cheaper model made from the campaign/import plan, now in
`main` (`git log ea2a931..main`), plus the docs commits that claim
verification, the merge, and the `chains.json` batch. Every verdict below was
checked against the Java source and in-engine on a clean re-import
(evidence dir `runs/20261012-audit5/`).

| Commit | Item | Verdict | Evidence / reason |
|---|---|---|---|
| `3a948bb` | PAR-0153 autosave | **correct** | `saveinterval=10` -> `autosave queued` 4x in 30 s, `sector-serpulo-170.msav` mtime advanced (1791779274 -> 1791779304). Adds `persist_sector_record()` per autosave (Java `SaveVersion.writeMeta` -> `saveInfo`). Test `update_policy_interval_is_seconds` is meaningful. |
| `bb0e8df` | PAR-0158 campaignselect / lastplanet | **correct, minor perf** | Condition matches `PlanetDialog.java:183`; OK writes `campaignselect` (207-213); `lastplanet` read/write (84, 247, 902). Verified: after restart the page opens on Serpulo with sector 170, no chooser. Minor: `_show_planet` calls `settings_put_text("lastplanet")` (a full read-merge-write of settings.bin) on every sector click (task T9). |
| `fcc5c12` | one-time Serpulo remap notice | **partial (did not work in the real flow)** | The getBoolOnce port is right (`PlanetDialog.java:216-222`), but the post-import rebuild (`MindCampaign.reload_from_disk`) ran `Saves.load` in the process that then quits, so the remap and its `had_serpulo_remaps` flag were consumed before any boot could show the notice. Its docs commit `6dfab30` admits the test re-extracted the legacy save by hand. **Fixed by audit commit `09ba3d0`**; verified: clean re-import -> next planet-page open shows the notice (`remap-notice.png`). Styling still differs (task T5). |
| `abcb17e` | PAR-0085 launch with loadout | **wrong core contents** | Java `Schematics.placeLaunchLoadout` (`game/Schematics.java:465-469`) puts only `universe.getLaunchResources()` in the new core; the schematic's requirements are the cost paid by the launcher (`PlanetDialog.java:1408-1411`). The commit stocked the core with requirements + resources (claimed "core exactly {copper:1000, lead:800}" = basicShard's build cost). **Fixed by audit commit `a77e7b9`**; verified core = `{"copper":100,"lead":50}` = launch resources. Launcher deduction correct. Still missing: launch validity, Max toggle, capacity (task T1), schematic core block (T2). |
| `27b59e4` | PAR-0025/0026/0027 dialog buttons | **partial; commit message wrong** | Claimed "Java order Back/Launch/Resources"; Java is Back, `@resources.max` (toggle), `@resources`, `@launch.text`, all 160x64 (`LaunchLoadoutDialog.java:40-41,125-152`). Audit commit `a77e7b9` reordered to Back/Resources/Launch at 160 px. Max toggle and the `valid` gate on Launch are missing (T1). |
| `bdc29ec` | PAR-0131 landing zoom | **risky / partly wrong** | Zoom curve and end value are right: `CoreBlock.zoomLaunch` = `Interp.pow3(Scl.scl(0.02), Scl.scl(4), getLandTimeIn())` (`CoreBlock.java:655-658`), normal scale is 4 (`Renderer.java:59` `targetscale = Scl.scl(4)`), duration 160 ticks (`CoreBlock.java:57`). Wrong: (1) it fired on every `start_sector`, including plain resumes - Java fires `Trigger.newGame` only from `playNewSector`/`playMap` (`Control.java:415,589`); (2) no input lock (`InputHandler.java:86` `inputLocks` includes `renderer::isCutscene`); (3) zoom snapped from 4 to the rig scale instead of continuing the lerp; (4) off-map hover during zoom 0.02 triggered the `hover_info` panic (fixed separately in `9aa3ef7`). (1)-(3) **fixed by audit commit `09ba3d0`**; verified: Ground Zero resume -> no cutscene; fresh saltFlats launch -> `locks == ["cutscene"]`, zoom 0.02 -> 2.41 -> 4.0, lock released, 0 panics. Remainder (sprite, fade, announce, delta clamp, hints/pause during cutscene, playMap trigger): task T3. Timing: ~3 s wall at the session's frame rate; Java = 160 frames of `Time.delta` clamped to `maxDeltaClient = 4` (`Vars.java:181`, `ClientLauncher.java:84-87`); the port does not clamp (T3 step 6). |
| `9aa3ef7` | hover_info off-map panic | **correct** | Bounds check before `grid.tile(x, y)`; matches `world.tileWorld` returning null. |
| `ca29fc2` | docs: A1 + PAR-0153 results | **accurate** | All cited screenshots exist; A1.2 finding is real and important: only 1 of 223 Serpulo research rows has requirements (`campaign_views_json` research rows) -> task T4. |
| `76fd920` | docs: PAR-0158 | **accurate** | Screenshots exist; behavior reproduced. |
| `6dfab30` | docs: remap notice | **misleading** | "Verified" only via a manual re-extract of the legacy save; the import flow did not show the notice (fixed `09ba3d0`). |
| `8ba2ebe` | docs: PAR-0085/0025-0027 | **inaccurate** | Records the wrong core contents as the expected result; otherwise lists real remainders (capacity 0, imported schematics not offered). |
| `5b5fb56` | docs: PAR-0131 slice | **inaccurate** | Records "trigger for fresh and resume" as intended; resume must not land. |
| `403bc8f` | merge into main | **clean** | `git diff <merge-base> 403bc8f^2` and `git diff 403bc8f^1 403bc8f` are the same 12 files / +687 -35: no session hunk lost. |
| `8aaa27d` | goldens regen (campaign.json) | **correct** | Needed because `588a8c8` (session 5, PAR-0154) changed `SectorView` without regenerating; `cargo test -p mind-headless ui_goldens_match` passes on the audit head. |
| `9f81a7f` | chains.json batch | **partly inaccurate** | PAR-0153 verified (ok), PAR-0158 verified (ok), PAR-0054 verified (ok). Wrong: PAR-0085 "verified" (core contents were wrong; step 5 and dialog validity missing) -> should be `fixing`; PAR-0025/0026/0027 "verified" (Max toggle + valid gate missing) -> `fixing`; PAR-0131 note says resume lands -> update note; PAR-0154 still `seeded` although fixed by `588a8c8` -> `verified`. See section 6. |
| `4e71098`, `c8f1ffa`, `63b39b5` | other areas (waves, ConstructBlock/fires save) | not in scope | Touch saves/sector loss but belong to other workstreams; no campaign/import regression observed in the re-import walk. |

Import regression walk on the audit head (clean `rm -rf $U` + `--import`):
282/375 unlocked; `sector-serpulo-170.msav` present, `sector-serpulo-15.msav`
moved; `serpulo-s-170-info` migrated; `campaignselect=true`,
`desktopMove-hint-done=true`; Ground Zero resume renders the Java save with
its core and no hints (`gz-resume.png`); planet page = Ground Zero selected,
Sectors list, globe grid (`planet-after-restart.png`).

## 3. Audit fixes already made (branch `parity/session-5`, not merged)

| SHA | What |
|---|---|
| `a77e7b9` | PAR-0085: launched core holds only the launch resources; Configure Loadout buttons Back/Resources/Launch at 160 px. |
| `09ba3d0` | PAR-0131: landing only on `PlayEvent::NewGame`; `MindInput.set_cutscene_lock` raised/dropped from `MindCamera2D::process` (never from `show_landing`, and the `start_sector_with_loadout` let-chain no longer holds `host.bind()` across the camera call - that re-entrant borrow panicked in an intermediate version); landing zoom written into the rig's `camerascale` for a smooth handoff; `skipcoreanimation` read live from MindUi; `reload_from_disk` skips `Saves.load` so the remap + notice happen on the next boot. |

Before section 5, make sure your worktree contains both commits:
```bash
git merge --ff-only main
git merge-base --is-ancestor 09ba3d0 HEAD || git merge --no-edit parity/session-5
git merge-base --is-ancestor a77e7b9 HEAD && git merge-base --is-ancestor 09ba3d0 HEAD && echo AUDIT_FIXES_PRESENT
```
If the merge conflicts, abort (`git merge --abort`) and report it; do not
resolve conflicts in these files yourself.

---

## 4. Execution order and shared files

Order (each task is one commit unless stated):
1. **T4** research requirements (highest player impact: research costs nothing / shows nothing).
2. **T1** Configure Loadout validity, Max toggle, capacity, item rows (PAR-0085/0025/0026/0027 remainder).
3. **T2** schematic core block on launch (PAR-0085 step 5). Depends on T1.
4. **T3** landing cutscene remainder (PAR-0131).
5. **T5** remap notice styling; **T9** lastplanet write-on-change (small, GDScript).
6. **T6** PAR-0112 generated sectors (depends on T2's placement helper).
7. **T7** PAR-0156 enemy AI residuals.
8. **T8** single shared settings store (architectural; do after the above so they are not rebased onto it).
9. **T10-T16** remaining gaps.

Files shared with other workstreams (merge carefully, keep changes minimal):
`client/rust/mind-gdext/src/campaign.rs`, `mind-gdext/src/sim_host.rs`,
`mind-gdext/src/camera.rs`, `mind-gdext/src/input/mod.rs`,
`mind-gdext/src/ui/ui_host.rs`, `mind-core/src/io/settings.rs`,
`mind-core/src/game/runtime.rs` (T7, also used by the AI/waves workstream),
`mind-core/src/ui/campaign.rs` (+ the UI golden), `client/ui/dialogs/planet_dialog.gd`.

---

## 5. Tasks

### T4. Research nodes have no requirements (new, high)

Symptom: the Research (tech tree) dialog shows no item costs or partial
payments; 222 of 223 Serpulo research rows in `campaign_views_json("serpulo")`
have `requirements: []`. Imported partial payments (`req-lancer-silicon = 1900`
in the Java profile) are invisible.
Acceptance: every node whose content has build requirements shows them;
`lancer` shows silicon/lead/copper/titanium with the imported paid amounts
(the profile has `req-lancer-silicon=1900`, lead 2200, copper 1900, titanium 900).
```bash
$G --eval 'var v = JSON.parse_string(get_node("/root/Spine/MindCampaign").campaign_views_json("serpulo")); var n = 0
for r in v["research"]: if r["requirements"].size() > 0: n += 1
return [v["research"].size(), n]'
```
Expected after the fix: second number close to the first (only the root,
items, liquids, sector presets and other zero-cost content stay empty), today `[223, 1]`.

Java:
* `content/TechTree.java:24,32,59,67`: `node(content, content.researchRequirements(), ...)`.
* `TechTree.TechNode` constructor (`TechTree.java:160-176`): `requirements =
  researchRequirements` scaled by inherited `researchCostMultipliers`;
  `finishedRequirements[i] = new ItemStack(item, settings.getInt("req-" + content.name + "-" + item.name))` (line 173).
* `world/Block.java:1284-1295` `researchRequirements()`:
  `quantity = Mathf.round(60*researchCostMultiplier + Mathf.pow(req.amount, 1.11f)*20*researchCostMultiplier*researchCostMultipliers.get(item, 1f), 10)`;
  `UI.roundAmount(quantity)`. `UnitType.researchRequirements` (units) is
  already ported (`mind-core/src/content/registries/units/mod.rs:1798`).

Port:
* Builder: `mind-core/src/content/tech/mod.rs` `at_full_inner` (~line 296):
  `requirements: None => Vec::new()` - the `content.researchRequirements()`
  default is never materialized.
* Spend-time fallback exists: `mind-core/src/game/tech_tree.rs:89`
  `effective_requirements(registry, node_ref)` (computes
  `content_research_requirements` when the node's list is empty).
* View: `mind-core/src/ui/campaign.rs` `research_views` (~line 705) reads
  `node.requirements` / `node.finished_requirements` directly.

Changes:
1. Find where the tech tree is built after content init (search
   `grep -rn "fn build_tech\|TechTreeBuilder::new\|fn finalize" client/rust/mind-core/src/content/tech/`).
   After all nodes exist and block/unit requirements are initialized (blocks'
   `requirements` must be loaded - the materialization must run after
   `registry.init()`; if the tree is built before `init`, add a
   `pub fn materialize_requirements(registry: &mut ContentRegistry, store: &dyn UnlockStore)`
   in `content/tech/mod.rs` and call it at the end of `ContentRegistry::post_init`
   (search `pub fn post_init` in `mind-core/src/content/load.rs`)).
2. `materialize_requirements`: for each node with empty `requirements` and
   `Some(content)`: `node.requirements = tech_tree::effective_requirements(registry, node_ref)`
   (move the computation so it does not borrow the registry mutably while
   reading: first collect `Vec<(TechNodeRef, Vec<ItemStack>)>`, then write);
   then `node.finished_requirements` = same items with amounts from
   `store.get_i32(&TechNode::requirement_key(&node.content_name, &item_name))`.
3. Keep `effective_requirements` unchanged (now returns the stored list).
4. `research_views` needs no change once the nodes carry the lists.
Tests (`mind-core/src/content/tech/mod.rs` tests or `game/tech_tree.rs` tests):
* `materialized_requirements_match_block_research_cost`: base content; the
  `duo` node's requirements equal `registry.block_by_name("duo").research_requirements()`
  (the port's existing `Block.researchRequirements` port,
  `mind-core/src/content/registries/blocks/mod.rs:2417`, already tested by
  `research_requirements_formula`; `UI.roundAmount` = `ui_round_amount`, same file ~2800).
* `finished_requirements_load_req_keys`: `MemoryUnlockStore` with
  `req-duo-copper = 120` -> the duo node's finished copper is 120.
Regenerate the UI golden (research rows change) - see 1.6.
Verify: the eval above; screenshot the research dialog
(`$G --eval 'return get_node("/root/MindUi").open_dialog("research", "")'`) and
compare a node's cost with the Java client (if you cannot run Java, compare
against the formula). Also check researching still works: pick an affordable
locked node in a session with a captured sector and research it.
Pitfall: the 282/375 import count must not change (`--import` output).

### T1. Configure Loadout: validity, Max toggle, capacity, item rows (PAR-0085/0025/0026/0027 remainder)

Symptom: Launch is always enabled even when the launcher cannot pay (items are
clamped to 0 instead); there is no `@resources.max` toggle (Java default ON:
launch as many resources as fit); capacity reads 0; no "Launching From" header,
no per-item cost rows, no `@sector.missingresources` warning; imported
`schematics/*.msch` loadouts are not offered.
Acceptance (with the launcher fixture from 1.5, Ground Zero holding 3000
copper / 2000 lead, core-shard itemCapacity 4000, Serpulo
`launchCapacityMultiplier 0.5` -> capacity 2000):
* header `Launching From: Ground Zero`; `Capacity: 2000` (bundle `launch.capacity`);
* Max ON (default): launch resources = per item `clamp(stored - schematic cost, 0, 2000)`
  -> copper `min(3000-1000, 2000)=2000`, lead `min(2000-800, 2000)=1200`;
* item rows show `3000 (2000 + 1000)` style text;
* with the launcher emptied (`remove_sector_items` all), Launch is disabled
  and `@sector.missingresources` shows;
* after Launch the new core holds exactly the launch resources and the launcher
  loses requirements + resources.

Java: `ui/dialogs/LaunchLoadoutDialog.java`
* 36-55 show(): buttons cleared, `defaults().size(160, 64)`, `@back`;
  hide items not on the planet (`launch.set(item, 0)` when `!item.isOnPlanet`).
* 58-90 `update`: `cap = lastCapacity = (int)(sector.planet.launchCapacityMultiplier * selected.findCore().itemCapacity)`;
  resources `min(cap)`; if `!destination.allowLaunchLoadout()` -> resources =
  destination preset map `rules.loadout` (filtered to planet items); else if
  `getMax()` -> `resources.set(item, clamp(sitems.get(item) - schems.get(item), 0, capacity))`;
  `universe.updateLaunchResources(resources)`; `total = requirements + resources`;
  `valid = sitems.has(total)`.
* 92-117 item rows: icon + `(al + as) + "[gray] (" + al + " + " + as + ")"`,
  scarlet `have/needed` when short; 4 per row.
* 125-146 `@resources.max` (`Icon.add`, `Styles.togglet`, checked = `getMax()`)
  and `@resources` (disabled while Max) - only when `destination.allowLaunchLoadout()`.
* 150-152 `@launch.text` (`Icon.ok`) -> `universe.updateLoadout(core, selected); confirm.run(); hide()`, `disabled(!valid)`.
* 165 `launch.from` header; 167-205 schematic grid (200 px `SchematicImage`
  toggles, filtered by `supportsEnv`/`isOnPlanet`, `entry.key.size <= core.size`)
  and `launch.capacity` label; 213 `@sector.missingresources` visible when `!valid`.
* 222-228 `maxresources` setting, default **true**.
* `Sector.allowLaunchLoadout()` (`type/Sector.java:97-99`): preset with
  `overrideLaunchDefaults` ? `preset.allowLaunchLoadout` : `planet.allowLaunchLoadout`
  (Serpulo `true`, `content/Planets.java:140`).

Port:
* `client/ui/dialogs/launch_loadout_dialog.gd` (`_rebuild`, `_confirm`,
  `_open_loadout`): list of names only; `_capacity = int(campaign_views().get("launch_capacity", 0))`
  but the key does not exist.
* `client/ui/dialogs/planet_dialog.gd` `_launch` (launcher branch),
  `_find_launcher` (first based non-attacked sector; Java uses
  `generator.findLaunchCandidate`, out of scope here).
* `client/rust/mind-gdext/src/campaign.rs`: `campaign_views_json` builds
  `Schematics::new(); load_loadouts(registry)` (line ~1239) - vanilla only;
  `get_launch_resource_stacks`, `update_launch_resources_json`,
  `remove_sector_items`, `update_loadout`.
* Planet def field `launch_capacity_multiplier` exists
  (`mind-core/src/content/registries/planets.rs:313`, Serpulo 0.5 at 675).

Changes:
1. `mind-gdext/src/campaign.rs` add
   `#[func] fn launch_dialog_json(&mut self, planet: GString, launcher: i32, destination: i32, loadout_index: i32) -> GString`
   returning `{"from": <launcher Sector.name()>, "capacity": int, "allow_launch_loadout": bool,
   "stored": {item: n}, "requirements": {item: n}, "resources": {item: n},
   "max": bool, "valid": bool, "preset_loadout": {item: n}}` computed exactly as
   Java's `update` (use `Sector.allowLaunchLoadout` rules above; core
   `itemCapacity` from the loadout schematic's core block: registry block
   `item_capacity`; `max` from the live `maxresources` setting via MindUi
   `settings_get_flag`-style read with default true - add a
   `settings_get_flag_default(key, default)` func to `ui_host.rs` because
   `settings_get_flag` defaults to false). When `max` is on, also persist the
   computed resources (`universe.update_launch_resources`) like Java.
2. Replace `Schematics::new(); load_loadouts(registry)` in the views builder(s)
   (`campaign.rs` ~1239 and ~1485) with
   `schematics.load(&NativeFs, &self.paths.root().join("schematics"), registry)`
   (`Paths` has no `schematics()` accessor; Java's `schematicDirectory` is
   `dataDirectory/schematics`, and the import extracts `schematics/` there)
   so imported `.msch` loadouts appear (`Schematics.load` + `checkLoadout`,
   `game/Schematics.java:80-92,322-334`). Keep the `index` stable: it is the
   index into `schematics.all` after the sort in `load`.
3. `launch_loadout_dialog.gd`:
   * `_rebuild`: call `launch_dialog_json(planet, launcher_id, sector, _selected)`
     (the planet dialog passes `launcher` in the context: add
     `"launcher": int(_find_launcher(planet_name).get("id", -1))` to the
     `open_dialog("launch_loadout", ...)` payload in `planet_dialog.gd` `_open_launch_loadout`).
   * header label `MindWidgets.markup_format("@launch.from", [from])`;
   * schematic toggles (keep the existing row buttons; Java's 200 px images are
     a separate gap, T15);
   * `MindWidgets.markup_format("@launch.capacity", [capacity])`;
   * item rows (4 per row): item icon (`MindWidgets.icon_texture("item-%s-ui" % name)`)
     + text built exactly like Java (`[gray]`, `[scarlet]` markup through
     `MindWidgets.markup` of a raw string - check `bundle_markup` echoes raw
     text when the key is missing; otherwise add a `markup_text(raw)` func to
     `mind-gdext/src/assets/atlas.rs` that calls `mind_core::ui::text::render_markup`);
   * `@sector.missingresources` label when `!valid`;
   * buttons: Back, `@resources.max` toggle (icon `add`, toggle_mode, pressed =
     `max`; on toggle write `maxresources` through MindUi and rebuild), Resources
     (disabled while max), Launch (disabled when `!valid`); only show Max and
     Resources when `allow_launch_loadout`.
4. `planet_dialog.gd` `_launch`: unchanged logic (cost = requirements +
   resources from the launcher; core = resources only), but read the
   resources from `launch_dialog_json` so Max-computed values are used.
Tests (`mind-core`): put the pure computation in
`mind-core/src/game/universe.rs` as

```rust
/// `LaunchLoadoutDialog.update` (LaunchLoadoutDialog.java:58-90), item maps keyed by content name.
pub fn launch_plan(
    stored: &IndexMap<String, i32>, requirements: &IndexMap<String, i32>,
    resources: &IndexMap<String, i32>, capacity: i32, max: bool,
    allow_launch_loadout: bool, preset_loadout: &IndexMap<String, i32>,
    all_items: &[String],
) -> (IndexMap<String, i32> /* new launch resources */, bool /* valid */)
```

and test: `launch_plan_max_fills_to_capacity` (3000/2000 stored, shard cost
1000/800, cap 2000 -> copper 2000, lead 1200, valid), `launch_plan_invalid_when_short`
(stored 0 -> valid false), `launch_plan_disallowed_uses_preset_loadout`.
Verify: launcher fixture (1.5) then screenshot the dialog
(`runs/.../launch-loadout-t1.png`), toggle Max via eval
(`get_node("/root/Spine/Ui/UiRoot").dialog("launch_loadout")` -> call the toggle
handler you added), Launch, `core_items_json()` == resources.
Pitfalls: `update_launch_resources_json` takes a per-item-index array (item
registry order), `get_launch_resource_stacks` returns names - do not mix them;
after Launch the planet dialog is closed and the world reloads (the confirm
eval may time out; wait 10 s).

### T2. PAR-0085 step 5: place the loadout schematic's core block

Symptom: launching with a non-shard loadout (e.g. a foundation loadout, or an
imported schematic with extra blocks) still lands in the map's own core-shard;
the schematic's other blocks are not placed.
Acceptance: with a `core-foundation` loadout (`Loadouts.basicFoundation`) the
launched sector's player core block is `core-foundation`
(`$G --eval 'return get_node("/root/Spine/SimHost").best_core_position()'` +
the hover/block name at that tile) and holds only the launch resources.
Java: `maps/generators/FileMapGenerator.java:95-130` (preset maps): the map's
default-team core tile -> `if(sector.allowLaunchLoadout()) Schematics.placeLaunchLoadout(tile.x, tile.y)`;
other default-team cores removed when a `corePositionOverride` is used;
`if(preset.addStartingItems || !preset.planet.allowLaunchLoadout)` -> core items
= `rules.loadout`. `game/Schematics.java:465-500` `placeLaunchLoadout` /
`placeLoadout(schem, x, y, team)`: places every schematic tile centred on the
core tile (core offset), then core items `add(getLaunchResources())`.
Port: placement helper exists: `mind-core/src/game/schematics.rs:214`
`pub fn place_loadout(...)` (read its signature). The launch path:
`mind-gdext/src/campaign.rs` `start_sector_with_loadout` ->
`mind_core::game::play::play_new_sector` -> `apply_launch_loadout`
(`mind-core/src/game/play.rs:~316`) which only rewrites core items.
`parse_loadout_stacks` already receives `{"core": ..., "stacks": [...]}`.
Changes:
1. Extend the payload to carry the schematic index: in `planet_dialog.gd`
   `_launch` send `{"core": ..., "schematic": loadout, "stacks": ...}`.
2. `campaign.rs` `start_sector_with_loadout`: parse `schematic`; after a
   successful `play_new_sector` (only `PlayEvent::NewGame` launches, not
   resumes) and while no SimHost borrow is held, call a new SimHost func
   `place_launch_loadout(schematic_index: i32, stacks_json: GString) -> bool`.
3. `mind-gdext/src/sim_host.rs` `place_launch_loadout`: find the player core
   tile (`best_core_position` logic, `first_core_position`), remove it, call
   `mind_core::game::schematics::place_loadout` with the schematic from a
   `Schematics::load(...)` (same loader as T1) at that tile for the default
   team, then set the new core's items to the stacks (reuse the inventory path
   used by `debug_add_core_items`, line ~1913).
4. Keep `apply_launch_loadout` for the items-only fallback when no schematic
   index is given.
Tests: `mind-core/src/game/schematics.rs` test
`place_loadout_replaces_core_block` (flat world with a core-shard at (10,10);
place basicFoundation -> tile (10,10) block is core-foundation, size 4).
Verify: launcher fixture with a foundation loadout row (if the vanilla
`basicFoundation` row is filtered out because GZ's best core is a shard, use
`debug` to give Ground Zero a foundation core first - otherwise skip in-engine
and rely on the unit test; write that in the commit).
Pitfall: `bind`/`bind_mut` re-entrancy - never call a SimHost `#[func]` from
code that holds `host.bind()` (including a `let Some(..) = host.bind()...` in an
`if let` chain: the guard lives until the end of the `if` block).

### T3. PAR-0131 remainder: core drop sprite, white fade, announce, timing, locks

Symptom: on a fresh landing only the camera zooms; Java also draws the core
dropping in (scaled, rotating, thrusters), fades from white, announces the
sector, blocks the pause key and hints during the cutscene.
Acceptance (fresh saltFlats launch, recipe 1.5): screenshots at ~0.5 s, 1.5 s
and 3 s after `start_sector` show (a) a white screen fading out during the
first ~0.6 s, (b) the core sprite large and rotated, shrinking to its tile,
(c) after the landing the announce `[accent]Salt Flats\n[lightgray]Resources: ...`
for 5 s; Space does not pause while `is_cutscene()`; no hint appears during it.
Java:
* `world/blocks/storage/CoreBlock.java`: `landDuration = 160f` (57);
  `thrusterSizes = {0,0,0,0,0.3,1,1,1,1,1,1,1,1,0}` (44);
  `beginLaunch(false)` (369-418): full-screen white `Image` with alpha 1,
  `Actions.fadeOut(35f/60f)`, then after `launchDuration()` ticks:
  `launchEffect.at(this)`, `Effect.shake(5f, 5f, this)`, `thrusterTime = 1f`,
  and if campaign + `Vars.showSectorLandInfo` + (`preset == null ||
  preset.showSectorLandInfo`): `ui.announce("[accent]" + sector.name() + "\n" +
  (resources.any() ? "[lightgray]" + bundle.get("sectors.resources") + "[white] " + resources joined by " " as emoji : ""), 5)`.
* `drawLanding(x, y)` (480-538): `fin = renderer.getLandTimeIn()`, `fout = 1-fin`;
  `scl = Scl.scl(4f) / renderer.getDisplayScale()`;
  size `s = region.width * region.scl() * scl * 3.6f * Interp.pow2Out.apply(fout)`
  (the `circle-shadow` region tinted `Pal.lightTrail` at that size);
  rotation `Interp.pow2In.apply(fout) * 135f`; thruster circles (team color r=6*strength,
  white r=3.5*strength) at 4 directions; then `Drawf.spinSprite(region, x, y, rotation)`
  and the team region. `drawLaunch` (424-478) adds `Draw.mixcol(Color.white, Interp.pow5In.apply(fout))`.
* `core/Renderer.java:180-199` update (landTime decrements only when not paused);
  `core/Control.java:730-734` pause key ignored while `renderer.isCutscene()`;
  `ui/fragments/HintsFragment.java:59,112` no hints while `isCutscene()`.
* Delta: `ClientLauncher.java:84-87` `Time.delta = clamp(dt*60, 0.0001, maxDeltaClient)`,
  `Vars.java:181` `maxDeltaClient = 4f`.
* `Trigger.newGame` also fires from `Control.playMap` (`Control.java:415`).
Port:
* `mind-core/src/render/cutscene.rs` (`Cutscene`, `CoreLandAnimator`).
* `mind-gdext/src/camera.rs` `MindCamera2D::process` (cutscene update, lock),
  `show_landing(core_x, core_y, core_block)` (`core_block` unused), `get_land_time_in`.
* `mind-gdext/src/campaign.rs` `start_sector_with_loadout` (trigger, gated on
  `PlayEvent::NewGame`); `start_map` (`#[func]`, ~line 760) has no trigger.
* `client/ui/fragments/fade_in_fragment.gd` (`OverlayLayer/fade_in`, a black
  `ColorRect` with `start(duration)`).
* `MindUi.announce(text, duration)` (`mind-gdext/src/ui/ui_host.rs:339`).
Changes:
1. Delta clamp (camera.rs `process`): `let delta_frames = ((delta * 60.0) as f32).clamp(0.0001, 4.0);`
   (applies to the whole camera rig like Java's global `Time.delta`).
2. Pass the core block name: in `campaign.rs`, replace `GString::new()` with
   the player core's block name (add a SimHost func `best_core_block() -> GString`
   next to `best_core_position`, `sim_host.rs:1728`; read it in the same
   borrow-free statement as the position).
3. Landing draw: add a code-instantiated `Sprite2D` (comment
   `# code-instantiated: cutscene-only core drop sprite`) - simplest is a new
   GDScript `client/ui/fragments/landing_fragment.gd` + `.tscn` under
   `World` is NOT allowed (UI layer only draws screen-space); instead draw it in
   world space from Rust: in `mind-gdext/src/camera.rs` keep the data
   (`core_x, core_y, core_block`, `fin`), and expose
   `#[func] landing_draw_json() -> GString` (`{"active", "x", "y", "block", "fin"}`);
   then add a `Node2D` child `World/LandingCore` to `client/scenes/game.tscn`
   with a script `client/scenes/landing_core.gd` (`extends Node2D`) that, in
   `_process`, reads the JSON and in `_draw()` draws the core region
   (`MindWidgets.icon_texture("block-%s-full" % block)` or the atlas region
   `block` itself - check `find_region`) at (x, y) with
   `draw_set_transform(pos, deg_to_rad(rotation), Vector2(s, s)/tex_size)`;
   `s` and `rotation` per Java formulas (Interp.pow2Out(x) = `1 - (1-x)^2`,
   pow2In(x) = `x^2`); draw the `circle-shadow` region tinted `Pal.lightTrail`
   (`Pal.lightTrail = #ffe2a9`, port constant `pal::LIGHT_TRAIL` in `mind-core/src/render/drawf.rs:126`)
   behind it; skip thrusters in this slice (document). Hide the real core
   building while active: add `#[func] set_hidden_building_tile(x, y)` to
   the block renderer owner (search `fn draw_block\|visible_blocks` in
   `mind-gdext/src/render/blocks.rs`) - if that is invasive, draw an
   opaque floor-colored rect is NOT acceptable; instead skip this sub-step and
   record it.
   This step is GDScript drawing world content; it is allowed because it only
   reads Rust state (UI-only rule = no sim mutation).
4. White fade: in `camera.rs` `show_landing` nothing; in
   `client/ui/fragments/fade_in_fragment.gd` add
   `func start_white(duration: float) -> void` (color white, alpha 1, visible,
   tween alpha to 0 over `duration`); call it from GDScript when the landing
   starts: `landing_core.gd` detects `active` rising and calls
   `get_node("/root/Spine/Ui/UiRoot/OverlayLayer/fade_in").start_white(35.0/60.0)`.
5. Announce: when `active` falls (landing finished), `landing_core.gd` builds
   the text via `MindCampaign.get_sector_state()` (`preset`, sector name: use
   the planet dialog's localized name rule `sector.<preset>.name`, else the
   sector id) + resources from `campaign_views_json` sector row
   `stats.resources` (emoji: `MindWidgets.markup(":%s:" % name)` if the markup
   renderer supports `:name:` icon tokens - it does for `bundle_markup`;
   check `mind-core/src/ui/text.rs` `render_markup`) and calls
   `get_node("/root/MindUi").announce(text, 5.0)`. Only when `campaign` is true.
6. Pause key during cutscene: find the pause binding handler
   (`grep -rn "pause" client/rust/mind-gdext/src/input/*.rs | grep -i bind`); the
   input lock from `09ba3d0` already blocks gameplay input - verify Space is
   ignored (`is_paused()` stays false after `godot_input` key Space during the
   cutscene); if not, gate it on `locks.cutscene`.
7. Hints: `client/ui/fragments/hints_fragment.gd` `_process`: return early
   (no new hint) while `get_node("/root/Spine/World/Camera2D").is_cutscene()`.
8. `start_map` landing: after a successful `play_map`, trigger `show_landing`
   the same way (same gating) - Java `Control.java:415`.
Tests: `cutscene.rs` `land_clock_uses_clamped_delta` is camera-side; instead
test the formulas in a new pure fn `pub fn landing_sprite(fin: f32) -> (f32 /*scale*/, f32 /*rotation*/)`
in `cutscene.rs` (`fin=0` -> rotation 135, scale factor 3.6; `fin=1` -> rotation 0, scale 0).
Verify: fresh launch recipe; take 3 screenshots with `sleep 0.5`, `sleep 1`,
`sleep 1.5` between them; `grep -c panic` = 0; Space test as in step 6.

### T5. Remap notice styling

Symptom: the Campaign Changes Notice uses the generic full-text dialog with a
`Back` button; Java uses a `BaseDialog` titled `@campaign.rework.title`, the
text centered with `maxWidth(500)`, and an `@ok` button (`Icon.ok`, 280x64)
(`ui/dialogs/PlanetDialog.java:216-222`), shown over the planet page.
Port: `planet_dialog.gd` `_maybe_show_remap_notice` opens `full_text`.
Change: open `full_text` with an extra context key `"ok": true`; in
`client/ui/dialogs/full_text_dialog.gd` `shown()`, when `_context.get("ok")`
is true, replace the close button with `add_button(_t("@ok"), hide_dialog, "ok", 280.0)`
and set the label `custom_minimum_size.x = 500`, wrap on. Check the planet
page stays visible behind (`planet_dialog.gd` `_cover_stage`): if the planet
view hides when another dialog opens, keep it visible for this dialog.
Verify: clean re-import (1.4), open planet page, screenshot.

### T9. `lastplanet` written on every sector click

`planet_dialog.gd` `_show_planet` calls `_put_text("lastplanet", planet_name)`
each time, i.e. a full settings.bin read-merge-write per click. Change: keep a
`var _last_planet_written := ""`; write only when it differs. Verify with
`stat -c %Y $U/config/settings.bin` before/after clicking two sectors of the
same planet: unchanged.

### T6. PAR-0112: generated (non-preset) sectors - same map everywhere, no core, no enemy base

Symptom: every non-preset sector generates the identical map (the sector id
is ignored and the seed is fixed to 1), with no player core and no enemy base.
Acceptance: `load_sector("serpulo", A)` and `load_sector("serpulo", B)` for two
different non-preset selectable sectors produce different terrain
(screenshots differ) and each has a player core at the generator's spawn
holding the launch resources; a sector with `generateEnemyBase` gets an enemy core.
Java:
* `core/World.java:265-295` `loadSector`: `size = sector.getSize()`;
  `sector.planet.generator.generate(tiles, sector, params)`; then
  `postGenerate(tiles)` for bases; `spawnPosition = defaultTeam.core().pos()`.
* `maps/generators/PlanetGenerator.java:177-180` `getSectorSize` =
  `(int)(sector.rect.radius * getSizeScl())`, rounded up to even;
  `182-196` `generate(tiles, sec, params)`: `seed = params.seedOffset + baseSeed`,
  `rand.setSeed(sec.id + params.seedOffset + baseSeed)`, per-tile position
  `sector.rect.project(x / width, y / height)`.
* `maps/planet/SerpuloPlanetGenerator.java:719` `Schematics.placeLaunchLoadout(spawn.x, spawn.y)`;
  enemy base: same file, search `BaseGenerator` / `generateEnemyBase`.
Port:
* `mind-gdext/src/sim_host.rs` `load_sector` (~1310-1360): calls
  `mind_core::maps::planet::generate_sector(planet, sector, 1, SECTOR_SIZE, SECTOR_SIZE, &registry)`.
* `mind-core/src/maps/planet/mod.rs:52-80` `generate_sector`: `let _ = sector;`
  (ignored), `WorldParams { seed_offset: seed, .. }`.
* Generators take a `SectorView` (`mind-core/src/maps/generators/planet.rs:92`,
  `id()`, `rect()`, `tile_v()`); only `FlatSectorView` exists.
* Schematic hooks are no-ops (`mind-core/src/maps/generators/base.rs`
  `SchematicHooks`, `NoopSchematicHooks`).
Changes (one commit per numbered step):
1. Real `SectorView`: implement `PlanetGridSectorView { id, rect: SectorRect, tile_v }`
   built from the planet's `PlanetGrid` (`mind-core/src/render/g3d/grid.rs`
   `PlanetGrid::create(def.sector_tiles)`, tile `v`) and the sector rect
   (Java `Sector.makeRect`, `type/Sector.java`; port the rect builder if
   missing: search `fn make_rect` in mind-core). Pass it into the generator
   (find how `FlatSectorView` is installed: `grep -n "FlatSectorView" -r client/rust/mind-core/src/maps`).
   `generate_sector` uses `sector` for the view and `seed_offset = 0` (Java
   `WorldParams.seedOffset` default 0) and the size from `getSectorSize`.
   Test: `maps::planet::tests::sectors_differ_by_id` - generate serpulo 1 and
   2 at 64x64; tile floors differ.
2. Launch core: implement `SchematicHooks` for a real type
   (`mind-core/src/maps/generators/schematic_hooks.rs`, SPDX header) whose
   `place_launch_loadout(tiles, x, y, content)` places the planet's default core
   (`registry.planet(..).default_core`, Serpulo `core-shard`) for the default
   team (use `mind_core::game::schematics::place_loadout` with the vanilla
   loadout, as T2). Call it at the spot Java does (`serpulo.rs`, after spawn
   selection ~line 883 `self.spawn = ...`). Core items = launch resources
   (T1/T2 path).
   Test: `generated_sector_places_launch_core` (core block at `generator.spawn`).
3. Enemy base: first slice = enemy core at the farthest enemy spawn when
   `sector.generateEnemyBase` (Java `Sector.generateEnemyBase`, set from
   `assets/planets/serpulo.json` `attackSectors`, `Planet.java:477-490`);
   full `BaseGenerator` decoration is out of scope (record as remaining).
Verify: two non-preset selectable cells (list them with
`for s in ...["sectors"]: if s["preset"] == null and s["selectable"]: out.append(s["id"])`)
-> load each with the fresh-sector recipe, screenshot, `core_items_json()`.
Pitfall: preset sectors must keep using their `.msav` (`load_preset_map_grid`);
only the `None` branch changes. Re-run `cargo test -p mind-core --lib maps::`
and the goldens (map generation goldens may change: regenerate only those that
cover generated sectors and say so in the commit).

### T7. PAR-0156: enemy AI residuals

T7a `estimateStats` ignores turret DPS.
Java `ai/RtsAI.java:329-366`: enemy turrets (`indexer.getEnemy(team, BlockFlag.turret)`)
whose `targetAir/targetGround` match and whose segment distance
`<= t.range() + 50` add `t.health` and `t.estimateDps()`.
`Turret.TurretBuild.estimateDps` (`world/blocks/defense/turrets/Turret.java:335-338`):
`hasAmmo ? shoot.shots / reload * 60 * peekAmmo().estimateDPS() * potentialEfficiency * timeScale : 0`;
`ContinuousTurret.java:43-46`: `hasAmmo ? shootType.damage * 60 / (ContinuousBulletType ? damageInterval : 5) : 0`;
`TractorBeamTurret.java:150-153`: `damage * 60 * efficiency * coolantMultiplier`; `BaseTurret` 0.
Port: `mind-core/src/game/runtime.rs:1072-1095` `team_ai_enemy_stats` (units
only); rows from `collect_team_ai_rows` (same file ~950-1000);
`estimate_stats` (`mind-core/src/ai/rts_ai.rs:402`) consumes `EnemyStat`.
Change: `collect_team_ai_rows` also returns turret rows (team, x, y, health,
range, target_air, target_ground, dps by the formulas above; find the turret
def fields with `grep -n "reload\|shots\|target_air" client/rust/mind-core/src/content/registries/blocks/turret*.rs`
and the ammo state with `grep -rn "fn peek_ammo\|total_ammo" client/rust/mind-core/src`);
`team_ai_enemy_stats` appends them.
Test: `game::runtime::tests::enemy_stats_include_loaded_turrets` (duo with ammo
-> one row, dps > 0; without ammo dps == 0).

T7b `prebuildAi` core defenders.
Java `core/Logic.java:573-585`: for each active team with `rules.prebuildAi`
(not editor): for each core, if no unit of `core.unitType` has
`flag == core.pos()`, spawn one at the core, `unit.flag = core.pos()`,
`Units.notifyUnitSpawn`, `Fx.spawn`. `ai/types/BuilderAI.java:240-255`:
such units fall back to `PrebuildAI`.
Port: `TeamRule.prebuild_ai` exists (`mind-core/src/io/json/rules.rs:62`);
`team_ai_system` (`mind-core/src/game/runtime.rs:740`) has no prebuild branch.
Change: inside the per-team loop, `if rule.prebuild_ai { ... }` enqueue a unit
spawn per core lacking a flagged unit, through the same request type the wave
spawner uses (`grep -rn "SpawnRequest\|spawn_unit\|UnitSpawn" client/rust/mind-core/src/game/runtime.rs | head`);
store the flag (find the unit `flag` field: `grep -rn "pub flag" client/rust/mind-core/src/entities`).
`PrebuildAI` behavior is out of scope (record).
Test: `prebuild_ai_spawns_one_defender_per_core` (one AI core, two system runs,
exactly one spawn request).
Verify: attack sector `serpulo:225` (enter with the fresh-sector recipe),
`$G --eval 'return get_node("/root/Spine/SimHost").unit_debug_json()'` lists
one unit flagged per enemy core.
Commit: `Fixes PAR-0156: turret DPS in estimateStats and prebuildAi core defenders`.

### T8. One shared settings store (architecture)

Symptom/risk: `MindUi` (`mind-gdext/src/ui/ui_host.rs`, field `settings`),
`MindCampaign` (`mind-gdext/src/campaign.rs`, `self.settings`) and the sim
host each load their own `SettingsStore` from `config/settings.bin`. Since
`e0b6089` `force_save` merges only each store's changed keys, so writes no
longer clobber each other, but each store still reads a stale snapshot:
e.g. `MindCampaign` sees `skipcoreanimation`/`maxresources` only as of its
boot (the audit worked around this with live reads through MindUi,
`live_setting_flag` in campaign.rs). Java has one `Core.settings`.
Acceptance: a key written by any facade is read back by every other facade in
the same frame; a test proves it.
Change:
1. In `mind-gdext`, add `src/settings_hub.rs`: a process-wide
   `std::sync::OnceLock<std::sync::Mutex<SettingsStore>>` plus
   `pub fn with_settings<R>(f: impl FnOnce(&mut SettingsStore) -> R) -> R`
   and `pub fn reload_settings(paths: &Paths)` / `pub fn save_settings(paths)`.
   (`mind-gdext` may use statics; `mind-core` must not.)
2. Replace the per-facade `settings: SettingsStore` fields with calls to the
   hub, one facade per commit: MindUi first, then MindCampaign, then SimHost.
   Keep `force_save`'s merge (harmless with one store).
3. `data_import` / `clear_game_data` (ui_host.rs ~760-890) call `reload_settings`.
Pitfalls: never hold the mutex across a call into another facade or Godot
(deadlock / re-entrancy, same class of bug as `bind`/`bind_mut`); take it, copy
what you need, release.
Test: unit test in `settings_hub.rs` (two "facades" = two closures, write
then read). Verify: change `saveinterval` in Settings, autosave cadence follows
without restart; `skipcoreanimation` toggled in Settings takes effect on the
next fresh launch.

### T10. Hint conditions (22 of 27 unported)

Port: `client/ui/fragments/hints_fragment.gd` `_hint_shown` evaluates
desktopMove, zoom, desktopPause, launch, presetLaunch; every other hint is
never shown. Java: `ui/fragments/HintsFragment.java:168-310` (`DefaultHint`
enum: shown/complete predicates per hint) and the event hooks at 60-100
(`events` set: "break", "fire", "derelictmouse", "derelictbreak",
"cannotupgrade", "factorycontrol"; `placedBlocks`).
Change: add a Rust read model `MindSimHost.hint_state_json()` returning the
inputs the predicates need: `enemies` (`state.enemies`), `team_unit_count`,
`player_dead`, `player_has_item`, `player_unit_can_boost`,
`player_unit_payload` (`none|empty|any`), `spawned_by_core`, `conveyor_count`,
`selected_block`, `boss` (`{armor, flying}` or null), `plans_count`,
`core_block`, `core_copper`, `core_capacity`, `placed_blocks` (set since boot),
`events` (set); then port each predicate 1:1 in `_hint_shown` and each
completion in `_check_trigger`. One commit per 5 hints. Verify each hint by
creating its state in a sandbox/sector and screenshotting.

### T11. Sector neighbours (`Sector.near()`)

`MindCampaign::bootstrap` builds `Campaign::from_registry(&registry, &EmptyNeighborhood)`
(campaign.rs ~215), so every mind-core consumer of `near()` (sector invasion,
`allowLanding`, imports) sees none. Implement `SectorNeighborhood` over
`PlanetGrid` (tile `tiles` = neighbour ids, `mind-core/src/render/g3d/grid.rs:56`)
in `mind-core/src/game/planet.rs` (trait at line 31) as `GridNeighborhood`
(cache one `PlanetGrid` per planet `sector_tiles`), pass it at bootstrap, and
make `annotate_globe` (`mind-core/src/ui/campaign.rs`) use it instead of its
local grid. Test: serpulo sector 170's `near()` has 6 ids, all within range.

### T12. Planet page: rename pencil and icon picker

Java `ui/dialogs/PlanetDialog.java:1199-1283` (`updateSelected` title row):
pencil (`Icon.pencilSmall`) when `preset == null || !preset.requireUnlock`
-> `ui.showTextInput("@sectors.rename", "@name", 32, sector.name(), v -> sector.setName(v))`;
icon button -> grid of `accessibleIcons` + unlocked content icons, writing
`info.icon` / `info.contentIcon`. Port: `planet_dialog.gd` `_build_sector_panel`
title row has neither; `SectorView` already has `named`, `icon`, `content_icon`.
Add `MindCampaign.set_sector_name(planet, id, name)` and
`set_sector_icon(planet, id, icon, content_icon)` (persist via `save_info`),
then the two buttons (use the existing `icon_select` dialog from the manifest
for the picker). Verify: rename a based sector, restart, name persists and the
Sectors list shows it.

### T13. Stats dialog

Java `PlanetDialog.showStats` (`PlanetDialog.java:1054-1130`): BaseDialog
titled `sector.name()` with description, `@sectors.time` + playtime,
attempts, wave, threat (attacked or no base), resources icons, lockdown text,
production / export (with destination) / import / stored item rows
(`displayItems`, ~1020-1052: icon + `formatAmount(total) + " /min"`, 3 per row).
Port: `_show_sector_stats` in `planet_dialog.gd` puts text lines into the
`full_text` dialog. Change: a dedicated `sector_stats` dialog (add to
`ui/dialogs_manifest.json`, `scenes/ui/ui_root.tscn` static instance, and the
`EXPECTED_*_DIALOGS` list in `mind-core::ui::manifest` - all three are
required) rendering icon rows. Also add `playtime_ms` to `SectorStatsView`
(save meta `time_played` of the sector slot).

### T14. Load Game previews

Java `game/Saves.java:343-352` writes `previewFile()` (minimap pixmap PNG) on
every save; `LoadDialog` shows it. Port: cards show a placeholder glyph
(`client/ui/dialogs/load_dialog.gd` `_build_card`). Change: write
`saves/<slot>.msav.png`-equivalent (check Java `previewFile()` name in
`Saves.java`) from `MindRender` minimap after each save, and load it in the card
(`Image.load_from_file` + `ImageTexture`). Imported Java slots have no preview
files: show the placeholder for them (Java renders its own on load; record).

### T15. Cosmetic parity residuals

* Configure Loadout: 200 px `SchematicImage` toggles instead of name buttons
  (`LaunchLoadoutDialog.java:173-190`).
* Globe: icons of cells behind the limb draw at the rim (cull when
  `tile_v . camera_dir <= 0`, `client/ui/planet_view.gd` `_add_icon`);
  unlocked preset ring should pulse `derelict -> white` with
  `Mathf.absin(Time.time, 10, 1)` (`PlanetDialog.java:461-466`), currently static.
* Sectors list panel background (`Styles.black8`) not visible
  (`planet_dialog.gd` `_build_sector_list`).

### T16. Java-only SectorInfo fields dropped

The port's `SectorInfo` (`mind-core/src/io/json/rules.rs` ~940) lacks Java
fields (`wavesSurvived`, `sumHealth`, `waveHealthBase/Slope`,
`waveDpsBase/Slope`, `secondsPassed`, `bossWave`, `bossHealth`, `bossDps`,
`curEnemyHealth`, `curEnemyDps`, `damage`, `wavesPassed`, ...; full list:
`core/src/mindustry/game/SectorInfo.java`). They are lost on the next
`save_all`. Add them with `#[serde(default)]` and the Java defaults, in Java
field order. Test: round-trip a Java UBJSON sector info through
`get_json` + `put_json` and compare the field set.

---

## 6. Ledger corrections to report (do not edit chains.json yourself)

* PAR-0154 -> `verified` (fixed by `588a8c8`; globe grid/icons; evidence
  `runs/20261012-session5/planet-grid.png`).
* PAR-0085 -> `fixing` (core contents fixed in `a77e7b9`; T1 validity/Max,
  T2 schematic core block remain).
* PAR-0025 / PAR-0026 / PAR-0027 -> `fixing` (button order fixed in
  `a77e7b9`; Max toggle and `valid` gate remain, T1).
* PAR-0131 note: resume must not land (`Control.java:415,589`); fixed in
  `09ba3d0` with the input lock; remainder T3.
* Remap notice: real import flow fixed in `09ba3d0`.
* New item: research requirements not materialized (T4).
* New item: generated sectors ignore the sector id (T6 step 1).

## 7. Done criteria for the whole plan

All tasks committed on your session branch with the verify evidence in the
commit bodies; `cargo test -p mind-core --lib`, `cargo test -p mind-headless ui_goldens_match`,
clippy and the Godot parse gate pass; a clean re-import (1.4) still yields
282/375, Ground Zero 170 with core, frozenForest 64, the remap notice once,
and no `panic` in the log after a Ground Zero resume and a fresh launch.
