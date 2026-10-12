<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Shared environment for the gameplay-parity plans (written 2026-10-13)

Every plan in this directory whose name starts with `G` (G00, G01, ...)
points here instead of repeating the setup. Read this file once, fully,
before your first task. If anything here does not match what you see, stop
and write down what you saw in your final report. Do not improvise.

Path abbreviations used by every plan:

| Short | Absolute / meaning |
|---|---|
| `REPO` | `/home/clintonnguyen/g/code_examples/Mindustry-Godot` (main checkout; do not edit it while sessions run) |
| `WT` | your session worktree `/home/clintonnguyen/g/code_examples/Mindustry-Godot-sessionN` |
| `core/` | `WT/client/rust/mind-core/src/` |
| `gdext/` | `WT/client/rust/mind-gdext/src/` |
| `headless/` | `WT/client/rust/mind-headless/src/` |
| `ui/` | `WT/client/ui/` |
| `scenes/` | `WT/client/scenes/` |
| `J/` | Java reference, read-only: `/home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry/` |
| `ARC/` | Arc (Java UI/math library), read-only: `/home/clintonnguyen/g/code_examples/Arc/` |

Java line numbers in the plans were checked on 2026-10-13. Port line
numbers drift: if a line moved, search for the quoted function name with
`grep -n`.

---

## 1. Session, worktree, build

* You are given a session number `N` (2..5). Session N = worktree `WT`,
  branch `parity/session-N`, Godot bridge port `6970 + 10*(N-1)`
  (2=6980, 3=6990, 4=7000, 5=7010), X display `:(9+N)`.
* Start of every plan:
  ```bash
  cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
  bash tools/parity-session.sh --session N        # creates/starts the worktree, display and editor
  cd ../Mindustry-Godot-sessionN
  git status --short                              # must be empty (except the untracked parity-ledger.json symlink)
  git merge --ff-only main                        # start from current main
  git merge-base --is-ancestor <G00-marker> HEAD && echo INTEGRATED   # the plan header gives the marker commit
  ```
  If `git merge --ff-only main` fails, the session branch has commits that
  are not in `main`. Stop and report; do not rebase or reset.
* Fresh worktree only: `bash tools/pack.sh pack` (2-4 min). Without it the
  log says `[assets] missing manifest .../sprites.atlas.json` and sprites are
  placeholders.
* Build: `CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh`.
  Expect `Finished \`dev\` profile` and `sync_scenarios: synced N file(s)`.
* The editor mmaps the GDExtension `.so`: after EVERY Rust build restart it:
  * `tools/session-play.sh N` = restart editor + play `res://scenes/game.tscn`.
  * `tools/session-play.sh N --import` = restart + Settings > Game Data import
    of `/home/clintonnguyen/g/code_examples/mindustry.save.full.tech.tree.zip`
    + replay. Its last line ends with `{"nodes": 375.0, "unlocked": 282.0}`.
  * `tools/session-play.sh N --play` = replay the scene only (after
    GDScript-only edits, and to recover a stalled game).
  * The script prints an `export OPEN_GODOT_MCP_PORT=...` line: run it.
* GDScript-only changes need no build; `--play` is enough.
* The session's Godot user dir is
  `U=REPO/.opencode/parity-session/session-N/xdg-data/godot/app_userdata/Mindustry-Godot`
  (`$U/config/settings.bin`, `$U/saves/`, `$U/logs/` newest file = current run
  log, `$U/crashes/` = Rust panic reports).
* After every in-engine step: `grep -c "panic" $(ls -t $U/logs/* | head -1)`
  must print `0`.

### 1.0 Cross-plan dependencies (read `INDEX.md` §3)

Some tasks depend on a task of another G-plan (the task text says so, e.g.
"depends on G02 T10"). Every plan's commit subjects are fixed
(`Gameplay G<plan> T<n>: ...` or `Fixes PAR-XXXX: ...`), so check whether
the dependency is on `main` with
`git log main --oneline | grep -m1 "G02 T10"` (or the exact subject the
other plan gives). If it is not there: skip the task (or the dependent half
of it), continue with the next task, and list it under "skipped: waiting
for <dep>" in your summary. To pick up a dependency that landed on `main`
after you started: `git merge --no-edit main` in your worktree (a merge
commit is fine; never rebase, never reset). If that merge conflicts, run
`git merge --abort` and report it.

### 1.1 RAM / process limits (hard rules)

14 GB RAM is shared by up to 4 parallel sessions.
* ONE cargo command at a time on the whole host: always prefix with
  `flock /tmp/opencode/mg-build.lock` and use `CARGO_BUILD_JOBS=6`.
* Never start a second editor for your session; never start the Java client
  unless a task explicitly says so.
* If every gmcp call times out and `free -g` shows < 2 GB available, wait a
  minute, then `tools/session-play.sh N --play`.
* `pgrep -af "godot --path"` lists games; kill only your own session's stale
  game (its command line contains your `WT` path).

---

## 2. Tests, lint, goldens

Run from `WT/client/rust`:
```bash
L="flock /tmp/opencode/mg-build.lock"; export CARGO_BUILD_JOBS=6
$L cargo test -p mind-core --lib <filter>                     # fast loop
$L cargo test -p mind-core --lib --features msav-import       # full suite (Java-save readers need the feature)
$L cargo test -p mind-headless --lib                          # see the known-failure list below
$L cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings   # must print nothing
cargo fmt -p mind-core -p mind-gdext                           # then re-run clippy
```
* Baseline numbers and the exact list of pre-existing `mind-headless`
  failures are recorded by plan G00 §5 in its summary file
  (`G00-integrate-sessions_summary.md`). A failure that is not on that list
  is yours.
* Put `#[cfg(test)] mod tests` at the END of a file (clippy
  `items_after_test_module`).
* Sim-level test helpers: `core/sim/production_tests.rs` (`runtime_sim()`,
  `place(&mut sim, "name", x, y)`, `item()`, `liquid()`, `set_floor()`,
  `stored_item()`, `stored_liquid()`, `configure()`, `spawned_unit()`);
  combat/unit helpers in `core/sim/combat_tests.rs`; a sim with a player
  unit: `sim_with_player()` in `core/sim/builder.rs` tests.
* `cargo fmt` sometimes reformats the unrelated `core/io/settings.rs`; run
  `git checkout client/rust/mind-core/src/io/settings.rs` if it shows up in
  `git status` and you did not edit it.
* GDScript parse gate (after any `.gd`/`.tscn` edit), from `WT`:
  `bash tools/godot.sh --headless --editor --quit --path client 2>&1 | grep -E "SCRIPT ERROR|Parse Error"`
  must print nothing.
* Goldens:
  * `parity/golden_content.json` is a port-derived snapshot (not a JVM
    oracle). Change it only when the new value matches the Java source;
    regenerate with `cargo run -p mind-headless -- content dump` (from `WT`)
    and update its sha256 (`"id": "content_ids"`) in
    `parity/golden_manifest.json` (`sha256sum parity/golden_content.json`)
    in the same commit.
  * UI goldens: `cargo test -p mind-headless ui_goldens_match`. A change to
    a `core/ui/campaign.rs` view struct changes
    `client/rust/mind-core/tests/goldens/ui/campaign.json`; regenerate with
    `cargo run -p mind-headless -- ui campaign --dump client/rust/mind-core/tests/goldens/ui/campaign.json`
    (check `--help` for the flag name) and update its sha256 in
    `parity/golden_manifest.json`.
  * Any golden you regenerate must be named in the commit body with the
    reason (the Java behavior that changed it).

---

## 3. Code rules (keep these invariants)

* Read `WT/client/AGENTS.md` and `WT/client/rust/AGENTS.md` once.
* New Rust files start with
  `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
  and `// SPDX-License-Identifier: GPL-3.0-only`; new `.gd` files with
  `# SPDX-License-Identifier: GPL-3.0-only`.
* Cite the Java source (`File.java:line`) in doc comments of ported code.
* No `unwrap()`/`expect()` outside `#[cfg(test)]` (workspace lints deny it).
* `mind-core` never uses `godot` or `tokio`; `mind-gdext` holds no game
  rules (it forwards to `mind-core`); GDScript is UI only (it calls native
  nodes, it never mutates sim state directly).
* Nodes created from code carry a `# code-instantiated: <reason>` comment.
* User-visible strings use bundle keys (`_t("@key")`, markup `_tm("@key")`);
  the Java bundle is `REPO/assets/bundles/bundle.properties` (search the key
  there; never invent English text if Java has a key).
* Determinism: iterate in `EntitySeq` order (tie-break `entity.index()`), use
  only the `SimRng` you are handed (`rng.range(RngStream::Sim, a, b)`,
  `rng.chance(RngStream::Sim, p)`), never iterate a `HashMap` for game logic.
* Panic safety: code reachable from input or rendering must not index
  `grid.tiles` without a bounds check; use `grid.tiles.getn(x, y)` (returns
  `Option`) or check `in_bounds` first.
* A `#[func]` that panics shows `panic before unwinding into Godot` in the
  log and writes `$U/crashes/crash-report-*.txt`.
* Never hold a `bind()`/`bind_mut()` of one gdext node across a call into
  another node or into Godot (re-entrant borrow panic).

### 3.1 Live-sim architecture (what most tasks touch)

* `Sim` (`core/sim/mod.rs`): `ecs.0` (bevy `World`), `grid` (authoritative
  tiles, NOT in the World), `block_runtime: Option<SimRuntime>`
  (`core/sim/runtime.rs`: place/break/destroy/pickup via `remove_building`).
* `UnitRuntime` (`core/sim/unit_runtime.rs`), a non-send World resource:
  `content`, `grid` (snapshot clone of `Sim.grid`, refreshed by
  `Sim::refresh_unit_runtime()`), `pathfinder`, `units`, `bullets`, `rng`,
  `seq`. Take it out with `world.remove_non_send::<UnitRuntime>()` and always
  put it back.
* `Sim::tick()` order (read `Sim::tick` in `core/sim/mod.rs` to confirm):
  `pump_io(Apply)` -> `Trigger::Update` -> schedule ->
  `update_fires_and_puddles()` -> `update_builders()` -> `resolve_deaths()`
  -> `flush_events()` -> clock -> `Trigger::AfterGameUpdate` ->
  `pump_io(Capture)`.
* Schedule (`core/sim/schedule.rs`): `update_buildings`,
  `turret_update_system`, `unit_update_system`, `bullet_update_system`,
  `bullet_collide_system`, `run_wave_system`, `enemy_count_system`, campaign
  systems, `weather_system`, `team_ai_system`. Never reorder sets.
* gdext facades (Godot nodes): `/root/Spine/SimHost` (`gdext/sim_host.rs`,
  `MindSimHost`), `/root/Spine/MindCampaign` (`gdext/campaign.rs`),
  `/root/Spine/Input` (`gdext/input/mod.rs`, `MindInput`),
  `/root/Spine/World/Camera2D` (`gdext/camera.rs`), `/root/Spine/MindAudio`
  (`gdext/audio/`), and the autoloads `/root/MindUi` (`gdext/ui/ui_host.rs`)
  and `/root/MindHud` (NOT under `/root/Spine`).
* UI dialogs: `ui/dialogs/*.gd`, registered in `ui/dialogs_manifest.json`,
  instanced statically in `scenes/ui/ui_root.tscn`, and listed in the
  `EXPECTED_*_DIALOGS` list in `core/ui/manifest.rs`. A new dialog needs all
  three or the manifest test fails. Open one with
  `get_node("/root/MindUi").open_dialog("<name>", "<arg>")`; reach its
  instance with `get_node("/root/Spine/Ui/UiRoot").dialog("<name>")`.

---

## 4. Driving the game (gmcp)

```bash
export OPEN_GODOT_MCP_PORT=$((6970 + 10*(N-1)))
G="python3 REPO/tools/gmcp.py --timeout 140"
H='get_node("/root/Spine/SimHost")'; I='get_node("/root/Spine/Input")'
C='get_node("/root/Spine/World/Camera2D")'; MC='get_node("/root/Spine/MindCampaign")'
$G --eval "return $H.get_tick()"
$G --screenshot runs/<YYYYMMDD>-s<N>-<topic>/<name>.png   # 1152x648; open it with the Read tool
```

Eval rules (each broke a session before):
* Every `--eval` is ONE statement that `return`s something
  (`return X`, or `X; return true`). No `while`, no `await`. A single
  `for` line followed by `return` on the next line is allowed but slow.
* Calling a method that does not exist, or a Rust `pub fn` without
  `#[func]`, raises a SCRIPT ERROR; break-on-error pauses the game, every
  later call times out and screenshots go stale. Recover with
  `tools/session-play.sh N --play`. Before calling an unfamiliar method,
  check it: `grep -rn "fn <name>" client/rust/mind-gdext/src` and confirm a
  `#[func]` line directly above it.
* Eval budget is 15 s and a timed-out eval keeps running. Advance long
  stretches in chunks (`$H.set_paused(true)` then repeated `$H.step(600)`).
* Never return `get_state_json()` whole on a campaign map (MBs); never call
  `campaign_views_json` or `MindRender.get_stats()` while a sector is loaded.
* `Input.parse_input_event` from inside an eval does not reach the port's
  binding dispatch. For key/mouse tests use the bridge input tool:
  ```bash
  python3 REPO/tools/gmcp.py godot_input '{"action":"key","params":{"key":"Space","pressed":true}}'
  python3 REPO/tools/gmcp.py godot_input '{"action":"key","params":{"key":"F","pressed":true,"modifiers":["shift"]}}'
  python3 REPO/tools/gmcp.py godot_input '{"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}'
  ```
  (send the same with `"pressed":false` to release; `key` takes Godot key
  names as accepted by `OS.find_keycode_from_string`: `Space`, `Escape`,
  `F6`, `Shift`, `A`, ...). Screen position of a
  tile: `return $C.tile_to_screen(x, y)` (viewport coords; valid the frame
  AFTER a `center_on_tile`). Wheel: `MOUSE_BUTTON_WHEEL_UP/DOWN`.
* World pixel = tile*8 (+4 = tile centre). The editor sim runs about 75-100
  ticks per wall-clock second.

### 4.1 Entering a sector

Ground Zero (sector **170**, the v8 remap of the legacy 15), after an
`--import` boot:
```bash
$G --eval "$H.load_sector(\"serpulo\", 170); return true" \
   --eval "return $MC.start_sector(\"serpulo\", 170)" \
   --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
$G --eval "return $MC.get_hud_state()"     # expect "hasCore": true, "sectorName": "groundZero"
```
Facts: core-shard centre tile (129,55); player alpha spawns at world
(1032,440) = tile (129,55); copper ore at (130..131,48..49); a
`mechanical-drill` at (130,48) feeding a `duo` at (132,48) works; derelict
`scrap-wall`s (team 0) near (169,167); wave daggers appear near world
(1156,1844), flares near (1352,1347). The core holds `{}` items at load
(the tutorial asks for 15 copper); seed with `debug_add_core_items`.

Enter like the UI "Go" button (resume, no cutscene):
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'get_node("/root/Spine/Ui/UiRoot").dialog("planet")._sector_action_pressed("serpulo", 170, "go"); return true'; sleep 4
```

A fresh, never-played preset sector (fires the landing cutscene):
```bash
$G --eval "$H.load_sector(\"serpulo\", 98); return true" \
   --eval "return $MC.start_sector(\"serpulo\", 98)" \
   --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
```
(98 = saltFlats). A bare `start_sector` without `load_sector` skips the world
load and ends in an instant Game Over. Unlocked presets without a base in the
imported profile: 23, 41, 64 (frozenForest), 97, 98, 99, 142, 164, 165, 213,
217, 221, 223, 228, 266, 271.

Sandbox-only blocks (item/liquid/power source and void) are rejected by the
campaign placement gate for player plans; the API `place_block` bypasses the
gate (instant, no cost). The sandbox `power-source` is 1x1.

### 4.2 Playtest seams (all `#[func]`; verify with grep before relying on one)

On `/root/Spine/SimHost` unless noted:

| Seam | Returns / does |
|---|---|
| `get_tick()`, `set_paused(b)`, `is_paused()`, `step(n)` | sim clock |
| `get_group_counts()`, `get_group_count("unit"/"bullet"/"weather")` | entity counts |
| `place_block(x,y,"name")`, `break_block(x,y)` | instant API placement (no gate/cost; returns true even when rejected, PAR-0165: read the tile back) |
| `apply_sim_command_json(json)` | ops `place` / `break` (`"player":0` routes through the player build queue), `spawn_unit` `{"op":"spawn_unit","unit":<index in content_list("unit")>,"team":2,"px":..,"py":..}`, `unit_command` |
| `content_list("block"/"unit"/"item")` | names in id order (`.find("copper-wall")` = id) |
| `debug_add_core_items("copper", 300)`, `core_items_json()` | player core items |
| `debug_add_building_items(x,y,"silicon",10)`, `building_items_json(x,y)` | building inventory |
| `building_debug(x,y)`, `building_config_json(x,y)`, `building_enabled(x,y)`, `light_color(x,y)` | building readbacks |
| `tile_debug(x,y)`, `power_debug(x,y)`, `liquid_debug(x,y)`, `hover_info(x,y)` | tile probes |
| `player_build_plans_json()`, `construct_progress(x,y)`, `clear_player_build_plans()`, `set_player_building_paused(b)` | build queue |
| `unit_debug_json()`, `unit_debug_positions()`, `selectable_units_json()` | units |
| `best_core_position()` | `Vector2` core tile, `(-1,-1)` when coreless |
| `/root/Spine/Input`: `configure_building(x,y,kind,value)` (kind `none/bool/number/int/string/content`), `select_block_by_name(n)`, `get_input_state_json()`, `toggle_command_mode()`, `command_mode()`, `selection_json()`, `pickup_payload()`, `drop_payload()`, `push_event(json)` | input |
| `/root/Spine/MindCampaign`: `get_hud_state()`, `get_sector_state()`, `run_wave()`, `capture_sector()`, `get_tech_state()`, `start_sector(p,id)`, `start_sector_with_loadout(p,id,json)` | campaign |
| `/root/Spine/Ui/UiRoot/OverlayLayer/block_config` | block config panel (`.visible`, `._config`) |

### 4.3 Clean re-import (when a task mutates the profile)

```bash
rm -rf "$U"                              # only YOUR session's dir
tools/session-play.sh N --import         # expect {"nodes": 375.0, "unlocked": 282.0}
ls "$U/saves" | grep serpulo             # sector-serpulo-170.msav present; no sector-serpulo-15.msav
```

---

## 5. Java reference client (only when a task says so)

The Java client runs only for reference captures, at most one at a time on
the host. Follow `REPO/../.claude/skills/parity-eval/SKILL.md` §0 ("Launch
both clients"), with the data dir `REPO/parity-java-home/` (already seeded
from the fully-unlocked profile). Never re-import data through the Java UI,
never clear Game Data, research or saves. Reference frames already captured
live under `REPO/runs/*ref*` and `REPO/runs/20261009-065416-ref-ground-zero/java/`;
prefer them over a new Java launch.

---

## 6. Commits and reporting

* One task = one commit, in your worktree only. Subject:
  `Fixes PAR-XXXX: <summary>` for a ledger item, `PAR-XXXX: <summary>` for a
  partial slice, `Gameplay G<plan> T<n>: <summary>` for a task without a
  ledger id. Body: what changed, the Java file:line it matches, how you
  verified it (test names + in-engine eval output), any deviation. Last line
  exactly: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` (or your
  own model line if you are a different model; never a wrong version).
* Stage only `client/`, `parity/`, `docs/`, `tools/` paths. `runs/` is a
  shared symlink: write evidence there, never `git add` it.
* Never push, never merge into `main`, never edit `chains.json` or
  `parity-ledger.json`: list ledger changes in your summary instead.
* Never claim "Fixes" without the task's acceptance check; if the in-engine
  half is impossible, say "headless only" and why.
* Evidence: `runs/<YYYYMMDD>-s<N>-<plan>-<task>/` (PNG + JSON).
* The plan files live (untracked) in the MAIN checkout:
  `REPO/docs/parity-plans/`. Read them there by absolute path.
* When you stop (done or out of budget) write
  `REPO/docs/parity-plans/<plan-file-stem>_summary.md` (main checkout,
  absolute path, do not commit it), using the shape of the existing
  `audit-*_summary.md` files: per-task
  status table, `git log --oneline main..HEAD`, exact test commands and
  results, in-engine evidence per task, deviations, blockers, ledger
  changes to apply.
