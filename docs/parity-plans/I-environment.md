<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Shared environment for the round-I parity plans (written 2026-10-15)

Every plan whose name starts with `I` (I00, I01, ...) in `docs/` points here
instead of repeating the setup. Read this file once, fully, before your first
task. Then read `docs/I-INDEX.md` §3 (cross-plan seams) and then your own
plan. If anything here does not match what you see, stop, write down what you
saw in your final report, and do not improvise.

Round H (`docs/parity-plans/H*.md` and their `_summary.md` files) is history.
Do not open those files unless your task names one section explicitly. Never
open any `*.zip` under `docs/` (in particular
`docs/parity-plans/archived_plans_do_not_read.zip`). Never mine git history
for `parity-ledger.json` or for plan/markdown files. Never read
`parity-ledger.json`; query `chains.json` with `jq` only.

Path abbreviations used by every plan:

| Short | Absolute path / meaning |
|---|---|
| `REPO` | `/home/clintonnguyen/g/code_examples/Mindustry-Godot` (the main checkout; do not edit it while sessions run, except I00) |
| `WT` | your session worktree `/home/clintonnguyen/g/code_examples/Mindustry-Godot-sessionN` (for I00 this is `REPO` itself) |
| `core/` | `WT/client/rust/mind-core/src/` |
| `gdext/` | `WT/client/rust/mind-gdext/src/` |
| `headless/` | `WT/client/rust/mind-headless/src/` |
| `ui/` | `WT/client/ui/` |
| `scenes/` | `WT/client/scenes/` |
| `J/` | the Java reference, read-only: `/home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry/` |
| `JS/` | the Java dedicated server, read-only: `/home/clintonnguyen/g/code_examples/Mindustry/server/src/mindustry/server/` |
| `ARC/` | Arc (the Java UI/math/audio library), read-only: `/home/clintonnguyen/g/code_examples/Arc/` |

Java line numbers in the plans were checked on 2026-10-15. Port line numbers
drift: if a line moved, search for the quoted function name with `grep -n`.
If the function does not exist at all, or the code already does what the
task asks, stop that task, write down what you saw, and go to the next task.

---

## 1. Session, worktree, build

* You are given a session number `N` (2..5). Session N means worktree `WT`,
  branch `parity/session-N`, Godot bridge port `6970 + 10*(N-1)`
  (2=6980, 3=6990, 4=7000, 5=7010) and X display `:(9+N)`.
  I00 is the exception: it runs in the main checkout (session 1, port 6970,
  display `:10`) with no other session running.
* Start of every plan (sessions 2-5):
  ```bash
  cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
  bash tools/parity-session.sh --session N        # creates/starts the worktree, display and editor
  cd ../Mindustry-Godot-sessionN
  git status --short | grep -v '^??'              # must print nothing (untracked files are fine)
  git merge --ff-only main                        # start from current main
  git log --oneline main | grep -m1 'I00'         # must print a line (I00 landed)
  ```
  If `git status` shows modified tracked files, or `git merge --ff-only main`
  fails, stop and report. Do not stash, rebase, reset or check out files.
  (I00 cleans every worktree before round I starts; a dirty worktree after
  that means another executor is using it.)
* Fresh worktree only: `bash tools/pack.sh pack` (2-4 min). Without it the
  log says `[assets] missing manifest .../sprites.atlas.json` and sprites are
  placeholders.
* Build: `CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh`.
  Expect `Finished \`dev\` profile` and `sync_scenarios: synced N file(s)`.
* The editor mmaps the GDExtension `.so`, so after EVERY Rust build restart it:
  * `tools/session-play.sh N` restarts the editor and plays `res://scenes/game.tscn`.
  * `tools/session-play.sh N --import` restarts, runs Settings > Game Data import
    of `/home/clintonnguyen/g/code_examples/mindustry.save.full.tech.tree.zip`,
    then plays again. Its last line ends with `{"nodes": 375.0, "unlocked": 282.0}`.
    If it has not returned within 5 minutes, kill it, check
    `get_node("/root/Spine/MindCampaign").get_tech_state()` yourself and continue
    with `--play`.
  * `tools/session-play.sh N --play` replays the scene only. Use it after
    GDScript-only edits and to recover a stalled game.
  * The script prints an `export OPEN_GODOT_MCP_PORT=...` line. Run it.
* GDScript-only changes need no build; `--play` is enough.
* The session's Godot user dir is
  `U=REPO/.opencode/parity-session/session-N/xdg-data/godot/app_userdata/Mindustry-Godot`
  (for session 1: `U=/home/clintonnguyen/.local/share/godot/app_userdata/Mindustry-Godot`).
  It holds `$U/config/settings.bin`, `$U/saves/`, `$U/logs/` (the newest file is
  the current run log) and `$U/crashes/` (Rust panic reports).
* After every in-engine step, `grep -c "panic" $(ls -t $U/logs/* | head -1)`
  must print `0`.
* `tools/parity-session.sh --stop` does not kill an editor/game started by
  `session-play.sh`. Stop those by pid: `pgrep -af "godot --path"`, and kill
  only the ones whose command line contains your `WT`.

### 1.0 Cross-plan dependencies (read `I-INDEX.md` §3)

Some tasks depend on a task in another I plan; the task text says so (e.g.
"depends on I05 T2"). Every plan's commit subjects are fixed
(`Gameplay I<NN> T<n>: ...` or `Fixes PAR-XXXX: ...` with the task id in
the body). To check whether a dependency is on `main`, run
`git log main --oneline | grep -m1 "I05 T2"`, AND grep for the seam itself
(the function name), because a seam can land under a `Fixes PAR-` subject.
If it is not there, skip the task (or the dependent half of it), continue with
the next task, and list it under "skipped: waiting for <dep>" in your summary.
To pick up a dependency that landed on `main` after you started, run
`git merge --no-edit main` in your worktree (a merge commit is fine; never
rebase, never reset). If that merge conflicts, run `git merge --abort` and
report it.

When a plan says "define the seam here if the provider has not landed",
define it exactly with the name and signature given in `I-INDEX.md` §3, so
that the later provider's merge is a no-op or a trivial conflict.

### 1.1 RAM / process limits (hard rules)

The 14 GB of RAM is shared by up to 4 parallel sessions.
* Only ONE cargo command at a time on the whole host: always prefix it with
  `flock /tmp/opencode/mg-build.lock` and use `CARGO_BUILD_JOBS=6`.
* Never start a second editor for your session. Never start the Java client
  unless your task explicitly says so (§5).
* If every gmcp call times out and `free -g` shows < 2 GB available, wait a
  minute, then run `tools/session-play.sh N --play`.
* `pgrep -af "godot --path"` lists games. Kill only your own session's stale
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
$L cargo run -q -p mind-headless -- parity goldens             # every line PASS
```
* Baseline on `main` @ `b560e9b1` (2026-10-15, end of round H, before I00,
  measured by the plan writer): `mind-core --lib --features msav-import`
  **2543 passed / 0 failed / 4 ignored**, `mind-headless --lib` **133 passed**,
  `mind-gdext --lib` **44 passed**, `parity goldens` **97 PASS / 0 other**.
  I00 merges more work and records the exact post-merge numbers in
  `docs/I00-coordinator_summary.md`; that summary is your baseline. Any
  failure not listed there is yours.
* `cargo test -p mind-headless --test fx_golden` passes since H03 T1 (float
  tolerance). If it fails, the failure is yours.
* Put `#[cfg(test)] mod tests` at the END of a file (clippy
  `items_after_test_module`).
* Sim-level test helpers (check that each exists with `grep -n "fn <name>"`
  before using it):
  * `core/sim/production_tests.rs`: `runtime_sim()`, `place(&mut sim, "name", x, y)`,
    `item()`, `liquid()`, `set_floor()`, `stored_item()`, `stored_liquid()`,
    `configure()`, `spawned_unit()` (these are `pub(super)`).
  * `core/sim/combat_tests.rs`: `live_sim(size)`, `place(&mut sim, x, y, name, team)`,
    `sim.spawn_live_unit(name, team, px, py, rot)`.
  * A sim with a player unit: `sim_with_player()` in `core/sim/builder.rs` tests
    (it sets `BuildRules.default_team = 1` like a campaign load).
  * The real Ground Zero map in tests: `core/sim/unit_runtime.rs` `mod msav_tests`
    (loads `REPO/assets/maps/serpulo/groundZero.msav`; copy its loader prologue).
  * Logic campaign tests: `core/sim/logic_campaign_tests.rs` (world-processor
    programs over a live sim; T7/T8 of H06 are the templates).
* `cargo fmt` sometimes reformats the unrelated `core/io/settings.rs`. If it
  shows up in `git status` and you did not edit it, run
  `git checkout client/rust/mind-core/src/io/settings.rs`.
* GDScript parse gate (after any `.gd`/`.tscn` edit), from `WT`:
  `bash tools/godot.sh --headless --editor --quit --path client 2>&1 | grep -E "SCRIPT ERROR|Parse Error"`
  must print nothing.
* Goldens:
  * `parity/golden_content.json` is a port-derived snapshot, not a JVM
    oracle. Change it only when the new value matches the Java source.
    Regenerate it with `cargo run -p mind-headless -- content dump --out parity/golden_content.json --bundle parity/bundle_keys.json`
    (run from `WT`; check `--help` first), then update its sha256
    (`"id": "content_ids"`) in `parity/golden_manifest.json`
    (`sha256sum parity/golden_content.json`) in the same commit.
  * Campaign sector golden: `cargo run -q -p mind-headless -- campaign sector --planet serpulo --sector groundZero --ticks 600 --dump`
    (diff it against the committed file named by `golden_manifest.json`
    `harness_campaign_sector_cycle`; copy it and update the sha256 when the
    change is explained by Java).
  * UI goldens are part of `parity goldens` and of `mind-headless --lib`
    (`campaign_scenarios`). A change to a `core/ui/campaign.rs` view struct
    changes `client/rust/mind-core/tests/goldens/ui/campaign.json`; regenerate
    it with `cargo run -p mind-headless -- ui campaign --dump client/rust/mind-core/tests/goldens/ui/campaign.json`
    (check `--help` for the exact flag) and update its sha256 in
    `parity/golden_manifest.json`.
  * Name every golden you regenerate in the commit body, with the reason
    (the Java behaviour that changed it). Never hand-pick a hash.

---

## 3. Code rules (keep these invariants)

* Read `WT/client/AGENTS.md` and `WT/client/rust/AGENTS.md` once. Logic work
  also reads `core/logic/AGENTS.md`.
* New Rust files start with
  `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
  and `// SPDX-License-Identifier: GPL-3.0-only`. New `.gd` files start with
  `# SPDX-License-Identifier: GPL-3.0-only`.
* Cite the Java source (`File.java:line`) in the doc comments of ported code.
* No `unwrap()`/`expect()` outside `#[cfg(test)]` (the workspace lints deny it).
* `mind-core` never uses `godot` or `tokio`. `mind-gdext` holds no game rules
  (it forwards to `mind-core`). GDScript is UI only: it calls native nodes and
  never mutates sim state directly.
* Nodes created from code carry a `# code-instantiated: <reason>` comment.
* User-visible strings use bundle keys (`_t("@key")`, markup `_tm("@key")`).
  The Java bundle is `REPO/assets/bundles/bundle.properties`. Search for the
  key there and never invent English text when Java has a key.
* Determinism: iterate in `EntitySeq` order (tie-break `entity.index()`), use
  only the `SimRng` you are handed (`rng.range(RngStream::Sim, a, b)`,
  `rng.chance(RngStream::Sim, p)`), and never iterate a `HashMap` for game
  logic. View-only randomness uses the `Fx` stream or `AudioViewRng`.
* Panic safety: code reachable from input or rendering must not index
  `grid.tiles` without a bounds check. Use `grid.tiles.getn(x, y)` (returns
  `Option`) or check `in_bounds` first.
* A `#[func]` that panics shows `panic before unwinding into Godot` in the
  log and writes `$U/crashes/crash-report-*.txt`.
* Never hold a `bind()`/`bind_mut()` of one gdext node across a call into
  another node or into Godot (re-entrant borrow panic). This includes
  `match host.bind().foo() { ... }`, because the guard lives for the whole
  `match`. Bind into a `let` first.
* Shaders: `res://shaders/*.gdshader` (`client/shaders/`) is the single
  source of truth since H03 T6. Do not edit `REPO/assets/shaders/godot/`.
* `Env` bits are Java's (`J/world/meta/Env.java`): terrestrial 1, space 2,
  underwater 4, spores 8, scorching 16, groundOil 32, groundWater 64,
  oxygen 128 (Serpulo default 233, Erekir 17). There is exactly one bit table:
  `core/game/planet.rs::env_bit`. Never write another.
* World coordinates: a tile CENTRE is `tile*8` (Java `Tile.worldx()`; the
  tile spans `tile*8-4 .. tile*8+4`). A building's centre is
  `tile*8 + block.offset` with `offset = ((size + 1) % 2) * 4`. Never write
  `(tile + 0.5) * 8`. Java `World.toTile(px) = Math.round(px / 8)`.

### 3.1 Live-sim architecture (what most tasks touch)

* `Sim` (`core/sim/mod.rs`): `ecs.0` (the bevy `World`), `grid` (authoritative
  tiles, NOT in the World), `block_runtime: Option<SimRuntime>`
  (`core/sim/runtime.rs`: place/break/destroy/pickup via `remove_building`),
  `player_unit: Option<Entity>`.
* `UnitRuntime` (`core/sim/unit_runtime.rs`) is a non-send World resource:
  `content`, `grid` (a snapshot clone of `Sim.grid`, refreshed by
  `Sim::refresh_unit_runtime()`), `pathfinder`, `units`, `bullets`, `rng`,
  `seq`. Take it out with `world.remove_non_send::<UnitRuntime>()` and
  always put it back.
* Unit controllers: `ControllerSlot.kind: AiKind` dispatched in
  `unit_update_system` (`core/sim/unit_runtime.rs`, the chain
  `if slot.kind == AiKind::Player ... else if ... Command / Missile / Suicide /
  Builder / Boost / Defender / FlyingFollow / Hug / Cargo / Assembler / NoAi
  ... else` generic ground/flying). Movement writes `Vel` only; integration is
  `entities::comp::unit::velocity::{move_at, move_pref, impulse, vel_update}`.
* Session rules live in the campaign `PlaySession` (`core/game/play.rs`) and
  are mirrored into the ECS world (`crate::game::runtime::rules_of(world)`).
  `Sim::apply_rules_to_world(&Rules)` re-derives world state (`grid.limit_area`,
  `world::limits::sync_build_rules`, `check_all_allow_update`), and
  `CampaignRuntime.rules_world_dirty = true` requests it.
* `Sim::tick()` order (read `Sim::tick` in `core/sim/mod.rs` to confirm):
  `pump_io(Apply)` -> `Trigger::Update` -> schedule ->
  `update_fires_and_puddles()` -> `update_builders()` -> `resolve_deaths()`
  -> `flush_events()` -> clock -> `Trigger::AfterGameUpdate` ->
  `pump_io(Capture)`.
* Schedule (`core/sim/schedule.rs`): `update_buildings`,
  `turret_update_system`, `unit_update_system`, `bullet_update_system`,
  `bullet_collide_system`, `run_wave_system`, `enemy_count_system`, the campaign
  systems, `weather_system`, `team_ai_system`. Never reorder the sets.
* Behaviour hooks for blocks: `trait BuildingBehavior` (`core/world/behavior/mod.rs`):
  `create_state`, `update_tile`, `configured`, `config`, `read`/`write`/`version`,
  `logic_sense`, `logic_control`, `on_configure_build_tapped`, `on_removed`,
  `unit_on`, ... (the trait has an `Any` supertrait, so stats code can downcast).
* gdext facades (Godot nodes): `/root/Spine/SimHost` (`gdext/sim_host.rs`,
  `MindSimHost`), `/root/Spine/MindCampaign` (`gdext/campaign.rs`),
  `/root/Spine/Input` (`gdext/input/mod.rs`, `MindInput`; desktop logic in
  `gdext/input/desktop.rs`), `/root/Spine/World/Camera2D`
  (`gdext/camera.rs`), `/root/Spine/MindAudio` (`gdext/audio/`), the world
  renderer (`gdext/render.rs` + `gdext/render/*.rs`), `/root/Spine/MindLogic`
  (`gdext/logic.rs`), `/root/Spine/MindEditor` (`gdext/editor/`),
  `/root/Spine/MindNet` (`gdext/net/`), and the autoloads `/root/MindUi`
  (`gdext/ui/ui_host.rs`), `/root/MindHud` (`gdext/ui/hud.rs`),
  `/root/MindAssets`, `/root/MindMods` (`gdext/mods.rs`), `/root/StdbConnector`
  (autoloads are NOT under `/root/Spine`).
* UI dialogs: `ui/dialogs/*.gd`, registered in `ui/dialogs_manifest.json`,
  instanced statically in `scenes/ui/ui_root.tscn`, and listed in the
  `EXPECTED_*_DIALOGS` list in `core/ui/manifest.rs`. A new dialog needs all
  three or the manifest test fails. Open one with
  `get_node("/root/MindUi").open_dialog("<name>", "<arg>")` and reach its
  instance with `get_node("/root/Spine/Ui/UiRoot").dialog("<name>")`.
* Block config / inventory panels live in the HUD group since H08 T14:
  `/root/Spine/Ui/UiRoot/HudGroup/block_config`, `.../HudGroup/block_inventory`,
  `.../HudGroup/plan_config`. `fade_in` stays under `OverlayLayer`.

---

## 4. Driving the game (gmcp)

```bash
export OPEN_GODOT_MCP_PORT=$((6970 + 10*(N-1)))
G="python3 /home/clintonnguyen/g/code_examples/Mindustry-Godot/tools/gmcp.py --timeout 140"
$G --eval 'return get_node("/root/Spine/SimHost").get_tick()'
$G --screenshot runs/<YYYYMMDD>-s<N>-<plan>-<task>/<name>.png   # 1152x648; open it with the Read tool
```

Write the node paths out in full inside every `--eval` string. A shell
variable holding an eval prefix (`H='get_node(...)'`) is not expanded inside
single quotes, and GDScript then reads `$H` as node shorthand, raises a
SCRIPT ERROR and wedges the game (see the rules below). The plans write
`$H`, `$I`, `$C`, `$MC` as shorthand for:

| Shorthand | Write this in the eval |
|---|---|
| `$H` | `get_node("/root/Spine/SimHost")` |
| `$I` | `get_node("/root/Spine/Input")` |
| `$C` | `get_node("/root/Spine/World/Camera2D")` |
| `$MC` | `get_node("/root/Spine/MindCampaign")` |
| `$UI` | `get_node("/root/MindUi")` |
| `$ROOT` | `get_node("/root/Spine/Ui/UiRoot")` |

Eval rules (each of these has broken a session before):
* Every `--eval` is ONE statement that `return`s something
  (`return X`, or `X; return true`). No `while`, no `await`. Do not
  `return` a void method (`return $C.set_process(false)` raises an error); write
  `$C.set_process(false); return true`. A one-line `for c in arr: body; return out`
  returns after the first iteration, so put the loop body on its own line or
  avoid loops.
* Calling a method that does not exist, or a Rust `pub fn` without `#[func]`,
  raises a SCRIPT ERROR. Break-on-error then pauses the game, every later call
  times out and screenshots go stale (the HUD `_process` accumulator also
  stops, so do not trust "HUD did not update" after an error). Recover with
  `tools/session-play.sh N --play`. Before calling an unfamiliar method, check it:
  `grep -rn -B1 "fn <name>" client/rust/mind-gdext/src` must show a `#[func]`
  line directly above it.
* Pass ids and positions as plain numbers. A wrong argument type wedges the
  frame loop in the same way.
* The eval budget is 15 s, and a timed-out eval keeps running. Advance long
  stretches in chunks: `$H.set_paused(true)`, then repeated `$H.step(600)`.
  An eval that exceeds the budget can leave the frame loop stalled
  (`is_paused()` reads false but the tick no longer advances, and the
  `MindCampaign` HUD mirror freezes). `step()` readbacks stay authoritative.
  Recover with `--play`.
* Never return `get_state_json()` whole on a campaign map (it is MBs). Never call
  `campaign_views_json` or `MindRender.get_stats()` while a sector is loaded.
* `Input.parse_input_event` from inside an eval does not reach the port's
  binding dispatch. For key/mouse tests use the bridge input tool:
  ```bash
  python3 REPO/tools/gmcp.py godot_input '{"action":"key","params":{"key":"Space","pressed":true}}'
  python3 REPO/tools/gmcp.py godot_input '{"action":"key","params":{"key":"F","pressed":true,"modifiers":["shift"]}}'
  python3 REPO/tools/gmcp.py godot_input '{"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}'
  ```
  Send the same with `"pressed":false` to release. `key` takes Godot key
  names as accepted by `OS.find_keycode_from_string` (`Space`, `Escape`,
  `F6`, `Shift`, `A`, ...). Get the screen position of a tile with
  `return $C.tile_to_screen(x, y)` (viewport coords; valid the frame AFTER a
  `center_on_tile`). The wheel buttons are `MOUSE_BUTTON_WHEEL_UP/DOWN`.
  Synthetic `godot_input` clicks reach the world, dialogs, HUD and menu
  buttons. If a world click does not register, the same coordinates through
  `$I.push_event('{"t":"MouseButton","button":1,"pressed":true,"x":X,"y":Y}')`
  (check the exact JSON keys in `gdext/input/mod.rs` `fn push_event`) do.
* After a scene restart a stray `restart` dialog can be open
  (`$UI.dialog_stack()` returns `["restart"]`). It silently swallows keys and
  scroll. Close it with `$UI.close_top_dialog()` before key-driven recipes.
* `configure_building` commands queue on the sim. With `set_paused(true)`,
  call `$H.step(1)` before reading a config back.
* The camera follows the player unit every frame, so `center_on_tile` is
  ignored while a player exists. To look at another area:
  `$C.set_process(false); $C.center_on_tile(X, Y); return true`. Restore it with
  `$C.set_process(true)`. For stable close-up captures set the zoom on the
  node (`$C.set_zoom(Vector2(4,4))`) while processing is off;
  `set_camera_scale(N)` only changes the target.

### 4.1 Entering a sector

Ground Zero (sector **170**, the v8 remap of the legacy 15), after an
`--import` boot:
```bash
$G --eval 'get_node("/root/Spine/SimHost").load_sector("serpulo", 170); return true' \
   --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 170)' \
   --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
$G --eval 'return get_node("/root/Spine/MindCampaign").get_hud_state()'    # expect "hasCore": true, "sectorName": "groundZero"
$G --eval 'return get_node("/root/Spine/SimHost").has_player_unit()'       # true (alpha at the core)
```
Facts: the core-shard centre tile is (129,55) = world (1032,440)
(`best_core_position()` returns WORLD PIXELS, not a tile); the player alpha
spawns at (1032,440); free buildable ground for test fixtures: tiles
(132..147, 57..72) (x >= 150 is static wall, y > 80 is outside the playable
map area, and units outside the map area are killed at once); copper ore at
(130..131,48..49); a `mechanical-drill` at (130,48) feeding a `duo` at
(132,48) works (the drill's east edge touches the duo; no belt needed); wave
daggers appear near world (1156,1844), flares near (1352,1347). The core
holds `{}` items at load (the tutorial asks for 15 copper); seed it with
`debug_add_core_items`. The imported Ground Zero save has `waveTimer` false
until its world processor sets it (`allowWaves` flag).

Enter like the UI "Go" button (resume, no cutscene):
```bash
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'get_node("/root/Spine/Ui/UiRoot").dialog("planet")._sector_action_pressed("serpulo", 170, "go"); return true'; sleep 4
```

A fresh, never-played preset sector (fires the landing cutscene):
```bash
$G --eval 'get_node("/root/Spine/SimHost").load_sector("serpulo", 98); return true' \
   --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 98)' \
   --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
```
(98 = saltFlats). A bare `start_sector` without `load_sector` skips the world
load and ends in an instant Game Over. Unlocked presets without a base in the
imported profile: 23, 41, 64 (frozenForest), 97, 98, 99, 142, 164, 165, 213,
217, 221, 223, 228, 266, 271. Several of those come up `hasCore:false`
(attack-style presets entered without a launch base), so check `get_hud_state`
before relying on a core. Erekir: Onset is the Erekir fixture used by H09
(fog). Find its id with
`jq -r '.ledger | to_entries[] | select(.value.t|test("Onset";"i")) | .key' chains.json`
or the planet dialog.

Sandbox-only blocks (item/liquid/power source and void) are rejected by the
campaign placement gate for player plans. The API `place_block` bypasses the
gate (instant, no cost) and reads the tile back (it returns false when the
placement was rejected). The sandbox `power-source` is 1x1. Power staging
order that works: machine -> `power-node` -> `power-source`, with a few
ticks between them (see I01 T6 for the re-placement bug).

### 4.2 Playtest seams (all `#[func]`; verify with grep before relying on one)

Full lists: `grep -A1 '#\[func\]' client/rust/mind-gdext/src/<file> | grep -o "fn [a-z_0-9]*"`.

`/root/Spine/SimHost` (`gdext/sim_host.rs`):

| Seam | Returns / does |
|---|---|
| `get_tick()`, `set_paused(b)`, `is_paused()`, `step(n)` | sim clock |
| `get_group_counts()`, `get_group_count("unit"/"bullet"/"weather")` | entity counts |
| `place_block(x,y,"name")`, `place_block_rot(x,y,"name",rot)`, `break_block(x,y)` | instant API placement (no gate/cost); false when rejected |
| `apply_sim_command_json(json)` | ALWAYS pass `"team":1` for player-side places (the op defaults `team` to 0 = derelict, which bypasses the player queue and builds instantly). `place` takes a NUMERIC block id in `"block"` (`content_list("block").find("duo")`) and the key `"rotation"`; a block NAME silently becomes air. Ops: `place` / `break` (`"player":0` routes through the player build queue), `spawn_unit` `{"op":"spawn_unit","unit":<index in content_list("unit")>,"team":2,"px":..,"py":..}`, `unit_command` (optional `"target_unit": <id>`), `unit_stance` |
| `content_list("block"/"unit"/"item")` | names in id order (`.find("copper-wall")` = id) |
| `debug_add_core_items("copper", 300)`, `core_items_json()` | player core items |
| `debug_add_building_items(x,y,"silicon",10)`, `building_items_json(x,y)` | building inventory (turret ammo is NOT in the item module) |
| `building_debug(x,y)`, `building_config_json(x,y)`, `building_config_string(x,y)`, `building_enabled(x,y)`, `light_color(x,y)`, `building_bars_json(x,y)`, `building_team_at(x,y)` | building readbacks (`building_debug` also carries the `force_*` fields) |
| `tile_debug(x,y)`, `power_debug(x,y)`, `liquid_debug(x,y)`, `heat_debug(x,y)`, `hover_info(x,y)` | tile probes |
| `player_build_plans_json()`, `construct_progress(x,y)`, `clear_player_build_plans()`, `set_player_building_paused(b)`, `team_ghost_plans_json()` | build queue / rebuild ghosts |
| `spawn_player_unit()`, `has_player_unit()`, `player_unit_status()`, `player_unit_pos()`, `player_stack_json()`, `debug_spawn_player(name,team,x,y)` | player unit |
| `unit_debug_json()`, `unit_debug_positions()`, `selectable_units_json()`, `command_targets_json()`, `debug_kill_unit(seq)` | units |
| `debug_kill_building(x,y)`, `debug_apply_status(...)`, `debug_create_weather(...)`, `weather_json()`, `boss_json()`, `drain_build_damage_json()`, `fog_debug_json()`, `debug_set_fog(...)` | debug / HUD feeds |
| `logic_set_code(x,y,code)`, `logic_get_code(x,y)`, `logic_get_state(x,y)`, `logic_last_error()`, `logic_set_flag`, `logic_rules_probe`, `logic_cutscene_json` | logic (`logic_set_code` writes directly and bypasses the campaign configure gate: a test seam) |
| `hint_state_json()`, `hint_event(name)`, `set_command_mode_satisfied(b)`, `command_mode_objective()` | hints/objectives |
| `best_core_position()` | `Vector2` core centre in WORLD PIXELS, `(-1,-1)` when coreless |
| `valid_place_preview(x,y,block,rot)`, `preview_config_tap(x,y,ox,oy)` | placement / tap previews |
| `launch_payloads_json()`, `landing_pads_json()` | campaign pods |

`/root/Spine/Input` (`gdext/input/mod.rs`): `configure_building(x,y,kind,value)`
(kind `none/bool/number/int/string/content`), `select_block_by_name(n)`,
`get_input_state_json()`, `placement_preview_json()`, `toggle_command_mode()`,
`command_mode()`, `selection_json()`, `set_unit_command(name)`,
`set_unit_stance_named(name, enabled)`, `select_only_type(id)`, `deselect_type(id)`,
`config_selected_json()`, `close_block_config()`, `use_schematic(i)`,
`rotate_placement()`, `rotate_placed()`, `transfer_item(...)`, `pickup_payload()`,
`drop_payload()`, `is_shooting()`, `take_hint_keys()`, `push_event(json)`,
`keybinds_json()`.

`/root/Spine/MindCampaign` (`gdext/campaign.rs`): `get_hud_state()`,
`get_sector_state()`, `get_rules_json()`, `set_rules_json(json)`,
`run_wave()`, `run_turn()`, `capture_sector()`, `get_tech_state()`,
`start_sector(p,id)`, `start_sector_with_loadout(p,id,json)`,
`get_objective_state()`, `objective_markers_json()`, `game_stats_json()`,
`run_exit_save()`, `abandon_sector()`, `fog_query(...)`,
`take_unlocked_content()`, `content_info_json(...)`.

`/root/Spine/World/Camera2D` (`gdext/camera.rs`): `tile_to_screen`,
`screen_to_tile`, `center_on_tile`, `camera_state_json`, `show_landing`,
`is_cutscene`, `set_camera_scale`.

`/root/MindUi` (`gdext/ui/ui_host.rs`): `open_dialog(name, arg)`,
`hide_dialog(name)`, `dialog_stack()`, `close_top_dialog()`, `toast(text, kind)`,
`announce(...)`, `settings_get_*`/`settings_put_*` (check the exact names with grep).

### 4.3 Clean re-import (when a task mutates the profile)

```bash
rm -rf "$U"                              # only YOUR session's dir
tools/session-play.sh N --import         # expect {"nodes": 375.0, "unlocked": 282.0}
ls "$U/saves" | grep serpulo             # sector-serpulo-170.msav present; no sector-serpulo-15.msav
```
The session profile persists sector progress (wave number, built blocks). If a
task needs wave 1 or an untouched Ground Zero, re-import first.

---

## 5. Java reference client (only when a task says so)

The Java client runs only for reference captures, at most one at a time on
the host. On Xvfb the Java game pauses when its window is unfocused: set
`backgroundpause` to false in the session's own `parity-java-home` settings
(back up `settings.bin` first and restore it afterwards), or the world never
advances. Follow `/home/clintonnguyen/g/code_examples/.claude/skills/parity-eval/SKILL.md`
§0 ("Launch both clients") with the data dir `REPO/parity-java-home/`
(already seeded from the fully-unlocked profile). Never re-import data
through the Java UI, and never clear Game Data, research or saves. Reference
frames already captured live under `REPO/runs/*ref*` and
`REPO/runs/20261009-065416-ref-ground-zero/java/`; prefer them over a new
Java launch. The playtest recipes are in
`/home/clintonnguyen/g/code_examples/.claude/skills/playtest/SKILL.md`. The
recorded call chains are in `REPO/chains.json` (query it with `jq`, never read it
whole: `jq -r '.chains[] | [.id, .status, .title] | @tsv' chains.json`,
`jq '.ledger["PAR-0133"]' chains.json`, `jq -r '(.notes["PAR-0133"] // [])[]' chains.json`).

---

## 6. Commits and reporting

* One task = one commit, in your worktree only. Subject:
  `Fixes PAR-XXXX: <summary>` for a ledger item, `PAR-XXXX: <summary>` for a
  partial slice, `Gameplay I<NN> T<n>: <summary>` for a task without a
  ledger id. Body: the task id (`I03 T5`), what changed, the Java file:line it
  matches, how you verified it (test names plus the in-engine eval output), and any
  deviation. The last line is exactly your own model's co-author line (e.g.
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`). Never write a
  wrong model name or version.
* Stage only `client/`, `parity/`, `tools/`, `server/` and `assets/` paths
  (and `docs/` only when a task says so). `runs/` is a shared symlink: write
  evidence there and never `git add` it. Never stage `.opencode/`.
* Never push, never merge into `main`, and never edit `chains.json` or
  `parity-ledger.json`. List ledger changes in your summary instead.
* Never claim "Fixes" without the task's acceptance check. If the in-engine
  half is impossible, say "headless only" and why.
* Evidence goes in `runs/<YYYYMMDD>-s<N>-<plan>-<task>/` (PNG + JSON + a short `notes.md`).
* The plan files live, untracked, in the MAIN checkout: `REPO/docs/`. Read
  them there by absolute path.
* When you stop (done or out of budget), write or extend
  `REPO/docs/<plan-file-stem>_summary.md` (main checkout, absolute path, do
  not commit it). Append a `# Pass N (session S, date)` section if the file
  already exists. It holds: a per-task status table (done / done headless-only /
  skipped: waiting for X / not reached), `git log --oneline main..HEAD`, the
  exact test commands and results, in-engine evidence per task (quoted eval
  output, screenshot paths), deviations from Java (each with the Java
  file:line), blockers, and the ledger changes to apply. Do not write
  "done" for a task whose acceptance check you did not run.
* Budget rule: commit each task as soon as its checks pass. Never leave
  finished work uncommitted at the end of a session. If you run out of budget
  mid-task, commit the partial work as `WIP I<NN> T<n>: <state>` ONLY if it
  builds and its tests pass; otherwise `git stash push -u -m "I<NN>-T<n>-wip"`
  and name the stash in your summary.
