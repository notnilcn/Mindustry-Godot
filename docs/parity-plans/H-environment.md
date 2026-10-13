<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Shared environment for the round-H gameplay-parity plans (written 2026-10-14)

Every plan whose name starts with `H` (H00, H01, ...) in `docs/` points here
instead of repeating the setup. Read this file once, fully, before your first
task. Then read `docs/H-INDEX.md` §3 (cross-plan seams), then your own plan.
If anything here does not match what you see, stop and write down what you
saw in your final report. Do not improvise.

The round-G plans (`docs/parity-plans/`, possibly moved to an archive) are
history. Do not open them unless your task explicitly names one. Never open
any `*.zip` under `docs/`. Never mine git history for `parity-ledger.json`
or plan/markdown files.

Path abbreviations used by every plan:

| Short | Absolute / meaning |
|---|---|
| `REPO` | `/home/clintonnguyen/g/code_examples/Mindustry-Godot` (main checkout; do not edit it while sessions run) |
| `WT` | your session worktree `/home/clintonnguyen/g/code_examples/Mindustry-Godot-sessionN` (for H00: `REPO` itself) |
| `core/` | `WT/client/rust/mind-core/src/` |
| `gdext/` | `WT/client/rust/mind-gdext/src/` |
| `headless/` | `WT/client/rust/mind-headless/src/` |
| `ui/` | `WT/client/ui/` |
| `scenes/` | `WT/client/scenes/` |
| `J/` | Java reference, read-only: `/home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry/` |
| `ARC/` | Arc (Java UI/math/audio library), read-only: `/home/clintonnguyen/g/code_examples/Arc/` |

Java line numbers in the plans were checked on 2026-10-14. Port line numbers
drift: if a line moved, search for the quoted function name with `grep -n`.
If the function does not exist at all, or the code already does what the
task asks, stop that task, write down what you saw, and go to the next task.

---

## 1. Session, worktree, build

* You are given a session number `N` (2..5). Session N = worktree `WT`,
  branch `parity/session-N`, Godot bridge port `6970 + 10*(N-1)`
  (2=6980, 3=6990, 4=7000, 5=7010), X display `:(9+N)`.
  H00 is the exception: it runs in the main checkout (session 1, port 6970,
  display `:10`) with no other session running.
* Start of every plan (sessions 2-5):
  ```bash
  cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
  bash tools/parity-session.sh --session N        # creates/starts the worktree, display and editor
  cd ../Mindustry-Godot-sessionN
  git status --short                              # must be empty (except the untracked parity-ledger.json symlink)
  git merge --ff-only main                        # start from current main
  git log --oneline main | grep -m1 'H00'         # must print a line (H00 landed)
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
    (Session 1 only: this wrapper has hung on its final eval; if it does
    not return within 5 minutes, kill it, then check
    `$MC.get_tech_state()` yourself and continue with `--play`.)
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
* `tools/parity-session.sh --stop` does not kill an editor/game started by
  `session-play.sh`; stop those by pid (`pgrep -af "godot --path"`, only the
  ones whose command line contains your `WT`).

### 1.0 Cross-plan dependencies (read `H-INDEX.md` §3)

Some tasks depend on a task of another H plan (the task text says so, e.g.
"depends on H02 T5"). Every plan's commit subjects are fixed
(`Gameplay H<NN> T<n>: ...` or `Fixes PAR-XXXX: ...` with the task id in
the body), so check whether the dependency is on `main` with
`git log main --oneline | grep -m1 "H02 T5"` (or the exact subject the
other plan gives), AND grep for the seam itself (function name) — a seam
can land under a `Fixes PAR-` subject. If it is not there: skip the task (or
the dependent half of it), continue with the next task, and list it under
"skipped: waiting for <dep>" in your summary. To pick up a dependency that
landed on `main` after you started: `git merge --no-edit main` in your
worktree (a merge commit is fine; never rebase, never reset). If that merge
conflicts, run `git merge --abort` and report it.

When a plan says "define the seam here if the provider has not landed",
define it exactly with the name and signature given in `H-INDEX.md` §3 so
the later provider's merge is a no-op or a trivial conflict.

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
$L cargo test -p mind-headless --lib
$L cargo test -p mind-gdext --lib
$L cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings   # must print nothing
cargo fmt -p mind-core -p mind-gdext                           # then re-run clippy
cargo run -q -p mind-headless -- parity goldens                # every line PASS
```
* Baseline on `main` @ `ffbe84dd` (2026-10-14, before H00):
  `mind-core --lib --features msav-import` **2202 passed / 0 failed / 4
  ignored**; `mind-headless --lib` **132 passed / 0 failed**;
  `mind-gdext --lib` **34 passed**; `parity goldens` all PASS. H00 records
  the post-H00 numbers in `docs/H00-blockers_summary.md`; that is your
  baseline. Any failure not listed there is yours.
* Known pre-existing failure outside those suites:
  `cargo test -p mind-headless --test fx_golden` (`fx_program_explosion` y2
  is 1 ULP off). Owned by H03 T1 (float tolerance); everyone else ignores it.
* Put `#[cfg(test)] mod tests` at the END of a file (clippy
  `items_after_test_module`).
* Sim-level test helpers: `core/sim/production_tests.rs` (`runtime_sim()`,
  `place(&mut sim, "name", x, y)`, `item()`, `liquid()`, `set_floor()`,
  `stored_item()`, `stored_liquid()`, `configure()`, `spawned_unit()`);
  combat/unit helpers in `core/sim/combat_tests.rs` (`live_sim(size)`,
  `place(&mut sim, x, y, name, team)`, `sim.spawn_live_unit(name, team, px, py, rot)`);
  a sim with a player unit: `sim_with_player()` in `core/sim/builder.rs`
  tests; the real Ground Zero map in tests:
  `core/sim/unit_runtime.rs` `mod msav_tests` (loads
  `REPO/assets/maps/serpulo/groundZero.msav`; copy its loader prologue).
  Always check a helper exists with `grep -n "fn <name>"` before using it.
* `cargo fmt` sometimes reformats the unrelated `core/io/settings.rs`; run
  `git checkout client/rust/mind-core/src/io/settings.rs` if it shows up in
  `git status` and you did not edit it.
* GDScript parse gate (after any `.gd`/`.tscn` edit), from `WT`:
  `bash tools/godot.sh --headless --editor --quit --path client 2>&1 | grep -E "SCRIPT ERROR|Parse Error"`
  must print nothing.
* Goldens:
  * `parity/golden_content.json` is a port-derived snapshot (not a JVM
    oracle). Change it only when the new value matches the Java source;
    regenerate with `cargo run -p mind-headless -- content dump --out parity/golden_content.json --bundle parity/bundle_keys.json`
    (from `WT`; check `--help` first) and update its sha256
    (`"id": "content_ids"`) in `parity/golden_manifest.json`
    (`sha256sum parity/golden_content.json`) in the same commit.
  * Campaign sector golden: `cargo run -q -p mind-headless -- campaign sector --planet serpulo --sector groundZero --ticks 600 --dump`
    (diff against the committed file named in `golden_manifest.json`
    `harness_campaign_sector_cycle`; copy + update the sha256 when the
    change is explained by Java).
  * UI goldens: `cargo test -p mind-headless ui_goldens_match`. A change to
    a `core/ui/campaign.rs` view struct changes
    `client/rust/mind-core/tests/goldens/ui/campaign.json`; regenerate with
    `cargo run -p mind-headless -- ui campaign --dump client/rust/mind-core/tests/goldens/ui/campaign.json`
    (check `--help` for the flag name) and update its sha256 in
    `parity/golden_manifest.json`.
  * Any golden you regenerate must be named in the commit body with the
    reason (the Java behavior that changed it). Never hand-pick a hash.

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
  another node or into Godot (re-entrant borrow panic). This includes
  `match host.bind().foo() { ... }`: the guard lives for the whole `match`;
  bind into a `let` first.
* Shaders: until H03 T6 lands, `ShaderRegistry` loads the packed copies in
  `REPO/assets/shaders/godot/*.gdshader` BEFORE `res://shaders/` — an edit
  to `client/shaders/x.gdshader` alone is silently ignored at runtime. If
  your task edits a shader before H03 T6 is on `main`, edit both copies
  identically (or check `git log main --oneline | grep -m1 "H03 T6"` first).
* `Env` bits are Java's (`J/world/meta/Env.java`): terrestrial 1, space 2,
  underwater 4, spores 8, scorching 16, groundOil 32, groundWater 64,
  oxygen 128 (Serpulo default 233, Erekir 17). After H00 there is exactly
  one bit table: `core/game/planet.rs::env_bit`. Never write another.

### 3.1 Live-sim architecture (what most tasks touch)

* `Sim` (`core/sim/mod.rs`): `ecs.0` (bevy `World`), `grid` (authoritative
  tiles, NOT in the World), `block_runtime: Option<SimRuntime>`
  (`core/sim/runtime.rs`: place/break/destroy/pickup via `remove_building`),
  `player_unit: Option<Entity>`.
* `UnitRuntime` (`core/sim/unit_runtime.rs`), a non-send World resource:
  `content`, `grid` (snapshot clone of `Sim.grid`, refreshed by
  `Sim::refresh_unit_runtime()`), `pathfinder`, `units`, `bullets`, `rng`,
  `seq`. Take it out with `world.remove_non_send::<UnitRuntime>()` and always
  put it back.
* Session rules live in the campaign `PlaySession` (`core/game/play.rs`) and
  are mirrored into the ECS world (`crate::game::runtime::rules_of(world)`);
  `Sim::apply_rules_to_world(&Rules)` re-derives world state (`grid.limit_area`
  etc.) and `CampaignRuntime.rules_world_dirty = true` requests it.
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
  `/root/Spine/Input` (`gdext/input/mod.rs`, `MindInput`; desktop logic in
  `gdext/input/desktop.rs`), `/root/Spine/World/Camera2D`
  (`gdext/camera.rs`), `/root/Spine/MindAudio` (`gdext/audio/`), the world
  renderer (`gdext/render/`), and the autoloads `/root/MindUi`
  (`gdext/ui/ui_host.rs`) and `/root/MindHud` (NOT under `/root/Spine`).
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
$G --screenshot runs/<YYYYMMDD>-s<N>-<plan>-<task>/<name>.png   # 1152x648; open it with the Read tool
```

Eval rules (each broke a session before):
* Every `--eval` is ONE statement that `return`s something
  (`return X`, or `X; return true`). No `while`, no `await`. A single
  `for` line followed by `return` on the next line is allowed but slow.
* Calling a method that does not exist, or a Rust `pub fn` without
  `#[func]`, raises a SCRIPT ERROR; break-on-error pauses the game, every
  later call times out and screenshots go stale (the HUD `_process`
  accumulator also stops — do not trust "HUD did not update" after an
  error). Recover with `tools/session-play.sh N --play`. Before calling an
  unfamiliar method, check it:
  `grep -rn -B1 "fn <name>" client/rust/mind-gdext/src` must show a
  `#[func]` line directly above it.
* Pass ids/positions as plain numbers; a wrong argument type wedges the
  frame loop the same way.
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
  `F6`, `Shift`, `A`, ...). Screen position of a tile:
  `return $C.tile_to_screen(x, y)` (viewport coords; valid the frame AFTER
  a `center_on_tile`). Wheel: `MOUSE_BUTTON_WHEEL_UP/DOWN`. World mouse
  events work, and synthetic `godot_input` clicks DO reach dialog, HUD and
  menu buttons (verified 2026-10-14, H08 T13); calling a dialog's handler
  from an eval is still fine when the button's position is unknown.
* The camera follows the player unit every frame, so `center_on_tile` is
  ignored while a player exists. To look at another area:
  `$C.set_process(false); $C.center_on_tile(X, Y)`; restore with
  `$C.set_process(true)`. `tile_to_screen` is valid either way.
* World pixel of a tile CENTRE = tile*8 (Java `Tile.worldx()`; the tile
  spans tile*8-4 .. tile*8+4). A building's centre is `tile*8 + block.offset`
  where `offset = ((size + 1) % 2) * 4` (even sizes shift +4). Do not use
  `(tile + 0.5) * 8` anywhere (H01 T1 removes the places that do). The
  editor sim runs about 75-100 ticks per wall-clock second.

### 4.1 Entering a sector

Ground Zero (sector **170**, the v8 remap of the legacy 15), after an
`--import` boot:
```bash
$G --eval "$H.load_sector(\"serpulo\", 170); return true" \
   --eval "return $MC.start_sector(\"serpulo\", 170)" \
   --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
$G --eval "return $MC.get_hud_state()"     # expect "hasCore": true, "sectorName": "groundZero"
$G --eval "return $H.has_player_unit()"    # after H00: true (alpha at the core)
$G --eval "return JSON.parse_string($MC.get_rules_json()).env"   # after H00: 233
```
Facts: core-shard centre tile (129,55) = world (1032,440)
(`best_core_position()` returns WORLD PIXELS, not a tile); the player alpha
spawns at world (1036,444) until H01 T1 fixes building positions, (1032,440)
after; free buildable ground for test fixtures: tiles (132..147, 57..72)
(x >= 150 is static wall, y > 80 is outside the playable map area); copper ore at (130..131,48..49); a
`mechanical-drill` at (130,48) feeding a `duo` at (132,48) works (the
drill's east edge touches the duo; no belt needed); derelict `scrap-wall`s
(team 0) near (169,167); wave daggers appear near world (1156,1844), flares
near (1352,1347). The core holds `{}` items at load (the tutorial asks for
15 copper); seed with `debug_add_core_items`. The imported Ground Zero save
has `waveTimer` false until its world processor sets it (`allowWaves` flag).

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
217, 221, 223, 228, 266, 271. Several of those come up `hasCore:false`
(attack-style presets entered without a launch base) — check `get_hud_state`
before relying on a core.

Sandbox-only blocks (item/liquid/power source and void) are rejected by the
campaign placement gate for player plans; the API `place_block` bypasses the
gate (instant, no cost) and reads the tile back (returns false when the
placement was rejected). The sandbox `power-source` is 1x1.

### 4.2 Playtest seams (all `#[func]`; verify with grep before relying on one)

Full lists (2026-10-14): `grep -A1 '#\[func\]' client/rust/mind-gdext/src/<file> | grep -o "fn [a-z_0-9]*"`.

`/root/Spine/SimHost` (`gdext/sim_host.rs`):

| Seam | Returns / does |
|---|---|
| `get_tick()`, `set_paused(b)`, `is_paused()`, `step(n)` | sim clock |
| `get_group_counts()`, `get_group_count("unit"/"bullet"/"weather")` | entity counts |
| `place_block(x,y,"name")`, `place_block_rot(x,y,"name",rot)`, `break_block(x,y)` | instant API placement (no gate/cost); false when rejected |
| `apply_sim_command_json(json)` | (ALWAYS pass `"team":1` for player-side places: the op defaults `team` to 0 = derelict, which bypasses the player queue and builds instantly) — `place` takes a NUMERIC block id in `"block"` (`content_list("block").find("duo")`) and the key `"rotation"`; a block NAME silently becomes air; ops `place` / `break` (`"player":0` routes through the player build queue), `spawn_unit` `{"op":"spawn_unit","unit":<index in content_list("unit")>,"team":2,"px":..,"py":..}`, `unit_command`, `unit_stance` |
| `content_list("block"/"unit"/"item")` | names in id order (`.find("copper-wall")` = id) |
| `debug_add_core_items("copper", 300)`, `core_items_json()` | player core items |
| `debug_add_building_items(x,y,"silicon",10)`, `building_items_json(x,y)` | building inventory (turret ammo is NOT in the item module) |
| `building_debug(x,y)`, `building_config_json(x,y)`, `building_config_string(x,y)`, `building_enabled(x,y)`, `light_color(x,y)`, `building_bars_json(x,y)`, `building_team_at(x,y)` | building readbacks |
| `tile_debug(x,y)`, `power_debug(x,y)`, `liquid_debug(x,y)`, `heat_debug(x,y)`, `hover_info(x,y)` | tile probes |
| `player_build_plans_json()`, `construct_progress(x,y)`, `clear_player_build_plans()`, `set_player_building_paused(b)`, `team_ghost_plans_json()` | build queue / rebuild ghosts |
| (before H00, `debug_spawn_player` is useless on Ground Zero: env 31 kills the alpha next tick) `spawn_player_unit()`, `has_player_unit()`, `player_unit_status()`, `player_unit_pos()`, `player_stack_json()`, `debug_spawn_player(name,team,x,y)` | player unit |
| `unit_debug_json()`, `unit_debug_positions()`, `selectable_units_json()`, `command_targets_json()` | units |
| `debug_kill_building(x,y)`, `debug_apply_status(...)`, `debug_create_weather(...)`, `weather_json()`, `boss_json()`, `drain_build_damage_json()` | debug / HUD feeds |
| `logic_set_code`, `logic_get_state`, `logic_set_flag`, `logic_set_rule`, `logic_rules_probe`, `logic_display_commands` | logic |
| `hint_state_json()`, `hint_event(name)`, `set_command_mode_satisfied(b)`, `command_mode_objective()` | hints/objectives |
| `best_core_position()` | `Vector2` core centre in WORLD PIXELS (e.g. (1032,440) on Ground Zero), `(-1,-1)` when coreless |

`/root/Spine/Input` (`gdext/input/mod.rs`): `configure_building(x,y,kind,value)`
(kind `none/bool/number/int/string/content`), `select_block_by_name(n)`,
`get_input_state_json()`, `placement_preview_json()`, `toggle_command_mode()`,
`command_mode()`, `selection_json()`, `set_unit_stance(...)`, `use_schematic(i)`,
`rotate_placement()`, `rotate_placed()`, `transfer_item(...)`, `pickup_payload()`,
`drop_payload()`, `is_shooting()`, `take_hint_keys()`, `push_event(json)`,
`keybinds_json()`.

`/root/Spine/MindCampaign` (`gdext/campaign.rs`): `get_hud_state()`,
`get_sector_state()`, `get_rules_json()`, `set_rules_json(json)`,
`run_wave()`, `run_turn()`, `capture_sector()`, `get_tech_state()`,
`start_sector(p,id)`, `start_sector_with_loadout(p,id,json)`,
`get_objective_state()`, `objective_markers_json()`, `game_stats_json()`,
`run_exit_save()`, `abandon_sector()`, `fog_query(...)`.

`/root/Spine/World/Camera2D` (`gdext/camera.rs`): `tile_to_screen`,
`screen_to_tile`, `center_on_tile`, `camera_state_json`, `show_landing`,
`is_cutscene`, `set_camera_scale`.

`/root/Spine/Ui/UiRoot/OverlayLayer/block_config` (moves to the HUD group after H08 T14 — re-find it with `get_node("/root/Spine/Ui/UiRoot").find_child("block_config", true, false)`): block config panel
(`.visible`, `._config`).

### 4.3 Clean re-import (when a task mutates the profile)

```bash
rm -rf "$U"                              # only YOUR session's dir
tools/session-play.sh N --import         # expect {"nodes": 375.0, "unlocked": 282.0}
ls "$U/saves" | grep serpulo             # sector-serpulo-170.msav present; no sector-serpulo-15.msav
```

---

## 5. Java reference client (only when a task says so)

The Java client runs only for reference captures, at most one at a time on
the host. On Xvfb the Java game pauses when its window is unfocused: set
`backgroundpause` to false in the session's own `parity-java-home` settings (back up
`settings.bin` first, restore it after), or the world never advances. Follow `/home/clintonnguyen/g/code_examples/.claude/skills/parity-eval/SKILL.md`
§0 ("Launch both clients"), with the data dir `REPO/parity-java-home/`
(already seeded from the fully-unlocked profile). Never re-import data
through the Java UI, never clear Game Data, research or saves. Reference
frames already captured live under `REPO/runs/*ref*` and
`REPO/runs/20261009-065416-ref-ground-zero/java/`; prefer them over a new
Java launch. The playtest recipes are in
`/home/clintonnguyen/g/code_examples/.claude/skills/playtest/SKILL.md`; the
recorded call chains are in `REPO/chains.json` (query with `jq`, never read
it whole: `jq -r '.chains[] | [.id, .status, .title] | @tsv' chains.json`).

---

## 6. Commits and reporting

* One task = one commit, in your worktree only. Subject:
  `Fixes PAR-XXXX: <summary>` for a ledger item, `PAR-XXXX: <summary>` for a
  partial slice, `Gameplay H<NN> T<n>: <summary>` for a task without a
  ledger id. Body: the task id (`H02 T5`), what changed, the Java file:line it
  matches, how you verified it (test names + in-engine eval output), any
  deviation. Last line exactly your own model's co-author line (e.g.
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; never a wrong
  model name or version).
* Stage only `client/`, `parity/`, `tools/` paths (and `docs/` only when a
  task says so). `runs/` is a shared symlink: write evidence there, never
  `git add` it.
* Never push, never merge into `main`, never edit `chains.json` or
  `parity-ledger.json`: list ledger changes in your summary instead.
* Never claim "Fixes" without the task's acceptance check; if the in-engine
  half is impossible, say "headless only" and why.
* Evidence: `runs/<YYYYMMDD>-s<N>-<plan>-<task>/` (PNG + JSON).
* The plan files live (untracked) in the MAIN checkout: `REPO/docs/`. Read
  them there by absolute path.
* When you stop (done or out of budget) write
  `REPO/docs/<plan-file-stem>_summary.md` (main checkout, absolute path, do
  not commit it): per-task status table (done / done headless-only /
  skipped: waiting for X / not reached), `git log --oneline main..HEAD`,
  exact test commands and results, in-engine evidence per task (eval output
  quoted, screenshot paths), deviations from Java (each with the Java
  file:line), blockers, and the ledger changes to apply. Do not write
  "done" for a task whose acceptance check you did not run.
