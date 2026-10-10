---
name: playtest
description: Playtest and verify the Mindustry-Godot spine end-to-end — drive the running Godot client via open-godot-mcp (place/break blocks, pause/step the fixed 60 Hz sim, assert checksums/state JSON, screenshot, logs) and use the `spacetime` CLI for server-side setup/inspection of the local `mindustry_godot` module (publish, logs, SQL, describe)
type: prompt
whenToUse: When playtesting, verifying, or driving the Mindustry-Godot client — checking the spine scene, placing/breaking blocks through the Rust API or real mouse input, pausing/stepping the sim, asserting `get_state_json`/checksums, taking screenshots, reading logs, or inspecting the local SpacetimeDB module. Covers both the Godot MCP tools and the `spacetime` CLI.
---

# Playtest: Godot MCP + SpacetimeDB CLI (mindustry-godot)

## Which tool for what — read this first

**Anything a real player does in the game goes through the Godot MCP tools** (recipes below): placing/breaking blocks with the mouse, driving the camera, stepping the sim, reading the inspector. Do **not** drive gameplay with `spacetime call` — a CLI reducer call does not go through `MindSimHost`, nothing renders, the input path stays untested, and it does not verify what you were asked to verify.

**The `spacetime` CLI is for server-side setup and inspection around that playtest**: publishing the local module, checking module logs, reading rows with SQL, and confirming what the module exposes with `describe`. The P0 module is a skeleton (one `player` table + `set_username`), so in practice the CLI is mostly a publish/log/describe tool until plan 01 grows the schema. Use it to arrange the scenario, then verify the result through the running client.

Typical flow: `server/build.sh` (local publish + bindings) → `godot_game play` on `res://scenes/game.tscn` → verify via pid-stamped `godot_exec` evals and/or `spacetime sql` when the check is purely server-side.

Generic UI-driving mechanics (CLICK/DRAG coordinates, eval pitfalls, stall diagnosis) live in the MCP itself — **follow them, don't re-derive here**:

- `playtest` MCP prompt — the interactive + deterministic workflow with exact tool JSON.
- The `open_godot_mcp` addon sources (`client/addons/open_godot_mcp/handlers/`) and the MCP checkout docs (`../Open-Godot-MCP/Docs/` when present) — short reminders at the call site.

Project-specific sequences live in `parity-ledger.json` — every item embeds
its own exact calls: `path` (fast-forward navigation per leg), `reproduce`
(the exact calls for the action under test) and `steps` (mirrored steps with
frames). **At session start, load the ledger and read all item names (id +
title + surface + status) before composing calls**, so you know which stored
sequence answers the task; then open the matching item and replay its calls.
After a confirmed change, update that item's `path`/`reproduce`/`steps` and
`updated` stamp. Do not copy sequences into this skill.

Anchor flows already in the ledger (scan the titles; ids grow as more flows
are folded in):
- attach/boot spine → PAR-0001; campaign chooser → planet page → PAR-0009/0010/0012
- resume a saved Ground Zero → PAR-0037; fresh launch → PAR-0056
- WASD movement → PAR-0044; click-mining → PAR-0045; drill → PAR-0046;
  conveyor line → PAR-0049
- pause dialog + buttons → PAR-0038..0043; minimap/radar probe → PAR-0057

The boot/attach recipes below run first in any session; every other flow
assumes a connected runtime. Java-leg sequences (shell helpers, parity-eval)
belong to the mirrored parity runs in the `parity-eval` skill; e.g. the Ground
Zero soak lives in the `reference.java_path` blocks of PAR-0019/PAR-0028.

The MCP server is a stdio process (`open-godot-mcp`, binary `~/.local/bin/open-godot-mcp`, bridge default **ws://127.0.0.1:6970**, override with `OPEN_GODOT_MCP_PORT` before the editor starts). Every tool takes `action` plus an optional `params` object; action-specific arguments go inside `params` (e.g. `godot_editor_edit {"action":"open_scene","params":{"path":"res://..."}}`).

# Part 1 — Godot MCP recipes (project-specific)

## Session start

1. **`godot_health check` first.** If it reports `BRIDGE_NOT_CONNECTED` (or errors with "No Godot instance connected"), launch the editor yourself on the Linux host (display `:10`): the MCP bridge auto-loads with the editor.

   ```bash
   OPEN_GODOT_MCP_PORT=6970 setsid "${GODOT_BIN:-godot}" --editor \
     --path client >>runs/editor-console.log 2>&1 &
   ```

   Wait ~20 s, then `godot_instance list` (the instance appears adopted) and `godot_health check` (port 6970 by default). The editor stays up across playtests; it must be killed and relaunched after every `tools/build.sh` (the GDExtension library is mmap'd at editor start). A second editor on the same project needs `OPEN_GODOT_MCP_PORT=6980` and a matching MCP client.

2. **Identity preflight before any `godot_game` call.** The bridge is `127.0.0.1:6970` (or `$OPEN_GODOT_MCP_PORT`); if another Godot editor (a sibling checkout) already holds the port, the MCP server talks to *that* project. Stop the other editor, then verify: `godot_editor_read state` → `project_path` must contain `mindustry-godot`. (The `godot_exec` runtime identity check only works once a game is running; the smoke asserts it after play via `ProjectSettings.globalize_path("res://")`.)

3. **Always run `res://scenes/game.tscn`** — the only P0 entry point. `godot_editor_edit {"action":"open_scene","params":{"path":"res://scenes/game.tscn"}}`, then `godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}` with the scene passed explicitly. Never play whatever scene happens to be open. Wait for `godot_game {"action":"status"}` → `runtime_connected: true`. Omitting `params.scene` (bare `godot_game {"action":"play"}`) **appears to succeed** — it returns `runtime_ready: true` — but no game process attaches the bridge: `godot_game status` reports `runtime_connected: false` / `instance_count: 0`, and every `godot_exec` then fails with `RUNTIME_NOT_CONNECTED`. Passing the scene is the fix; if the runtime never connects, confirm the `scene` param was sent before anything else.

4. **Node map** (all static nodes declared in the `.tscn`):

   | Path | Role |
   |---|---|
   | `/root/Spine/SimHost` | `MindSimHost`: sim owner, fixed 60 Hz pump, API (`place_block`, `break_block`, `get_state_json`, `get_checksum`, `get_tick`, `is_paused`, `set_paused`, `step`, `load_scenario`, `selected_block`, `select_block`, `capture`) |
   | `/root/Spine/World/TileGrid` | grid + block quads (`MindTileGrid`) |
   | `/root/Spine/World/Camera2D` | `screen_to_tile`, `tile_to_screen`, `center_on_tile` |
   | `/root/Spine/Ui/StateInspector/Label` | GDScript inspector overlay text |
   | `/root/Spine/Input` | `MindInput`: `get_input_state_json`, `select_block_by_name`, `toggle_command_mode`, `set_selectable_units_json`, `selection_json` (NOT `/root/MindInput` — that path errors and pins the frame loop) |
   | `/root/Spine/MindCampaign` | campaign facade: `get_hud_state`, `get_objective_state`, `objective_markers_json`, `start_sector*`, `clear_planet_campaign_saves`, `clear_research` |
   | `/root/MindHud`, `/root/MindUi` | Rust autoloads (NOT under `/root/Spine` — that path reads null and the errored eval pins the frame loop) |
   | `/root/Spine/Ui/EditorDialog` | map-editor shell, sibling of `UiRoot` (NOT `UiRoot/EditorDialog`); full-rect and layered above `DialogLayer`, so it eats clicks on lists beneath it — exit via its BackButton before re-driving them |

## Project standing rules

- **Pid-stamp every verification eval** (`OS.get_process_id()`), compare against `godot_game {"action":"instances"}` / the pid from the first eval. If a pid stamp changes across calls, re-establish every assumed state before interpreting the next result. Two runtimes (an editor game + a stale instance) can make a misrouted call look successful.
- **The sim is a fixed 60 Hz pump.** While Playing, the tick advances with wall time between calls; use `set_paused(true)` for deterministic API/input work and `step(n)` for exact advancement. `set_paused` does *not* disable `_input`: mouse place/break still applies while paused.
- **Do not pause before the scenario step.** `load_scenario` + `step(60)` must run in `Playing`: the checksum hashes `phase`, so pausing first yields `bf2ab23165daa498`, not the golden (corrected §7c order: load → step in Playing → pause → API/input). Keep `load_scenario` + `step(60)` in **one** loop-free eval so the pump cannot advance between them.
- **Loads reset the sim**: `load_scenario` replaces the whole world/player, so re-read the pid/tick and re-establish the camera before interpreting later results.
- **`tile_to_screen` returns viewport coordinates** (tile center). Feed it to `godot_input` with `coords: "viewport"`; the runtime lifts it to window space correctly. Never hardcode a click point.
- **Eval bodies: single statement, one known method**: the MCP eval wrapper times out on `for`/`while` bodies; split heavy expressions and keep every `var` at the top level. Prefer a single statement or pure expression per eval — a statement-level `if`/`for` that mutates can pin the loop too. Query one known node+method per eval and check the exposing class first: a method that does not exist on the node you called errors and pins the loop like any bad body (`get_hud_state` is on `/root/Spine/MindCampaign`, not `SimHost`; `MindUi.active_dialog` does not exist; `player_unit_position` is Rust-only without `#[func]`; `core_items_json` is not on `MindCampaign` in this build). Project blacklist that times out and pins: `campaign_views_json`, `MindRender.get_stats()` (while a sector is loaded), `tile_blocks()` walked with a loop, `JSON.parse_string(get_state_json())` + filter, lambdas over mixed rows, for-loops over dialog children — and never `.slice()` a `call()` result, call the method separately. Read hover with `get_tree().root.gui_get_hovered_control()` (an eval node has no `get_viewport()` context), never `await`, and never call Rust `pub fn` without `#[func]` (use the `#[func]` sibling or the real input path).
- **Eval budget is 15 s and a timed-out call keeps running**, blocking later evals — chunk large advances (6–10 × `step(600)`), poll, and re-read. After a long step the campaign HUD mirror can go stale while `get_group_counts()` stays correct: assert liveness on group counts, not mirrored HUD.
- **Blocked tiles read back, not return values**: covered in recipes 3–4, plus select through `MindInput.select_block_by_name` (the `SimHost` API only changes the host field) and assert `get_input_state_json().block`; the drag preview paints over occupied tiles, so assert `get_state_json()`.
- **STOP controls swallow world clicks**: paused banner at viewport center, HUD `hints` full-screen panel (exhaust the catalogue first), `LoadingLayer` loading overlay (`io_pending()==1` → `step(30)` to drain). Probe `gui_get_hovered_control()` at the point first, and never probe at viewport center while paused.
- **UI transitions need their own round-trip**: a click can return `ok` and still miss while a submenu fades — drive menu walks one click per `execute` and probe the expected dialog/submenu node before the next click. Never issue an eval in the same round-trip as `godot_game play` (it races the attach: `RUNTIME_NOT_CONNECTED`), never resolve a click from a sentinel probe (`_selected_sector == -1` right after OK is pre-build — re-probe once), and after a sector launch wait/poll a trivial eval before UI evals (deferred launch work can time out even a single-statement eval).
- **Data-driven containers rebuild by `queue_free`**: a same-frame close+open (placement palette) leaves auto-renamed duplicate buttons; drive collapse/expand as two calls, then resolve targets by content (`tooltip_text`) and skip `is_queued_for_deletion()` nodes — never `get_node()` by name after a rebuild.
- **Camera is one frame late + drifts**: `tile_to_screen`/`screen_to_tile` update the frame after `center_on_tile`; resolve in a later eval. The OS pointer parks at a screen edge and edge-pans every frame — park it mid-window (or `Camera2D.set_process(false)` around the probe) and recover with `recenter_player()`.
- **Log reads**: `godot_log get count=N` returns the oldest N — ask `count` ≥ buffer for tails. `clear` right after boot loses the boot banner — capture `get` before clearing. `move_to_foreground()` is deprecated (ERROR entry) — `grab_focus()` only.
- **Freshness the mtime cannot prove**: relaunch the editor after every `tools/build.sh` (mmap'd lib) and probe `has_method` on the new symbol; a missing sprite atlas (`region_count==0`) means the worktree lacks `tools/pack.sh` outputs — pack, don't chase icon findings. `stop` leaves `launchid.dat` (benign crash warning) and skips scene teardown (dispose paths need `get_tree().quit()`); sweep orphan game processes after sessions.
- **Sector-launch calling conventions**: `start_sector_with_loadout(planet, sector, loadout_json)` takes a JSON **string** (`[{"item":"copper","amount":500},…]`, never the Java map shape); the planet dialog calling 3-arg `start_sector` errors and pauses the game. Tech unlocks and hint/research state persist in `user://config/settings.bin` (`<name>-unlocked`, `req-<content>-<item>`, `*-hint-done`) across relaunches — `clear_planet_campaign_saves` clears saves only, so call `MindCampaign.clear_research()` (or back up/remove settings.bin) before a fork that expects locked tech, and restart for a true fresh-profile baseline. Hide the `FpsBox`/`performance` debug overlays before world captures; assert menu state on `MenuGroup`/`HudGroup`, not the `menu` fragment (it stays visible).

## Recipes

### 1. Boot + pid stamp + liveness

```
godot_exec {"action":"eval","params":{"code":"return {\"pid\": OS.get_process_id(), \"project\": ProjectSettings.globalize_path(\"res://\"), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick(), \"checksum\": str(get_node(\"/root/Spine/SimHost\").get_checksum())}"}}
```

Expect an object with `pid`, a `project` path containing `mindustry-godot`, a growing `tick`, and a 16-hex `checksum`.

### 2. Load the golden scenario and step to 60 (engine ↔ headless parity)

```
godot_exec {"action":"eval","params":{"code":"var host = get_node(\"/root/Spine/SimHost\")\nvar loaded = host.load_scenario(\"res://scenarios/spine_place_break.json\")\nvar tick = host.step(60)\nreturn {\"pid\": OS.get_process_id(), \"loaded\": loaded, \"tick\": tick, \"checksum\": str(host.get_checksum())}"}}
```

Require `loaded: true`, `tick: 60`, and `checksum == "a1a7b96167c9718d"` (recorded in `scenarios/spine_place_break.json`; canonical FNV-1a `Checksum`, plan 05 M8). Then pause: `godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"set_paused","args":[true]}}`.

### 3. API place/break (Rust `#[func]` path)

```
godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"place_block","args":[3, 5, "stone-wall"]}}   # -> true
godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"break_block","args":[3, 5]}}                 # -> true
```

`true` is not a placement result: the live runtime can reject an occupied/blocked tile and still return true (only a `[D] place command rejected at (x, y)` log line remains). Read tiles back from `get_state_json()`. Odd-size blocks anchor on the given tile; even-size blocks use it as the top-left.

### 4. Mouse place/break (real input path)

1. Resolve the tile center: `godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/World/Camera2D\").tile_to_screen(7, 7)"}}` → `{x, y}` viewport coords.
2. Left click to place the selected block (default `stone-wall`):
   `godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}`
   then flush in the same eval (`Input.flush_buffered_events()`) and assert the tile — unflushed input sits in the buffer and looks dead. Select via `MindInput.select_block_by_name` first (the `SimHost` selector does not switch the input controller's block).
3. Assert tile `(7,7)` via `get_state_json` (recipe 6); right-click (`MOUSE_BUTTON_RIGHT`) to break. While paused, call `step(1)` afterwards to prove the pump advances (`get_tick()` must increase by exactly one).

### 5. Pause + step

`godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"set_paused","args":[true]}}` then `godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").step(10)"}}` → new tick. `step(0)` is a no-op that returns the current tick. Chunk large advances (e.g. 6–10 × `step(600)`) — one huge step can exceed the 15 s eval budget, and a timed-out eval keeps running and blocks later calls.

### 6. `get_state_json` assertions

`godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").get_state_json()"}}` returns the canonical dump (same schema as `mind-headless --dump`): `{format, tick, phase, checksum, world:{width,height,sparse,tiles:[{x,y,block,team,rot,build_id}]}, entities, events, commands_applied}`. `world.tiles` is sparse (non-air only) and sorted by `(y, x)` — assert on parsed JSON, not substring hunts. The full dump is heavy (megabytes on a loaded sector: ~6.6 MB on Ground Zero — never return it through the bridge or parse it in-eval, or the reply trips the editor websocket; stash with `set_meta("dump", s)` and read slices with `String.find/substr`). The inspector label (`/root/Spine/Ui/StateInspector/Label`) refreshes from the cheap `get_tick`/`get_checksum`/`get_group_counts` accessors every 250 ms and on `state_changed(tick, checksum)` while visible, and its text contains `tick`, `checksum` and the selected block.

### 7. Camera

`screen_to_tile(x, y) -> Vector2i`, `tile_to_screen(x, y) -> Vector2`, `center_on_tile(x, y)`; wheel zooms 0.25–4.0; WASD/arrows (or edge pan) move. After `load_scenario`, re-center and re-read the transform before trusting resolved coordinates.

### 8. Screenshot + logs

- `godot_screenshot {"action":"game"}` → PNG on disk (auto-rotated, last 50). The grid renders in the windowed editor game; a dummy/headless renderer has no viewport image.
- `godot_log {"action":"errors"}` must be empty for a clean run; `godot_log {"action":"get","params":{"count":200}}` shows the `[I] MindSimHost ready (...)` startup line and the `[D]/[I]/[W]/[E]` Rust bridge lines. `godot_log {"action":"clear"}` resets the shared buffer.

### 9. Automated smoke (do this first when in doubt)

`tools/mcp-smoke.sh` spawns a fresh stdio MCP server, verifies identity, then automates §7c steps 1–6 and 9: open + play spine, pid stamp, `load_scenario` + `step(60)` golden checksum, pause, API place/break, camera-resolved mouse place/break, log checks, teardown. It exits non-zero on any mismatch and prints the exact editor launch command when the bridge is down. The bridge accepts multiple MCP clients, so it can run while opencode's own MCP connection is attached — no need to close anything.

## When a recipe fails

1. Run `tools/mcp-smoke.sh` first — it isolates bridge/identity/checksum issues from your recipe.
2. `godot_log {"action":"errors"}` (clear first for a clean window), then `godot_health check`, then `godot_game {"action":"status"}`.
3. The headless oracle is the cheaper ground truth: `cargo run -p mind-headless -- run spine_place_break --json` and `... run spine_determinism --json` from the repo root.
4. A wedged game: check `godot_debugger sessions` first — an eval that errored or timed out trips the editor break-on-error (`paused: true`), pinning the frame loop while `godot_exec` still answers; `resume` re-breaks. Recover with `godot_game {"action":"stop"}` then play again. Do not kill the editor unless the bridge itself is unresponsive; relaunch it with the command from Session start.
5. `godot_exec` failing with `RUNTIME_NOT_CONNECTED` means no game process is connected — play first (recipes 1–2 need the game, editor-only tools do not); an eval issued in the same round-trip as `play` races the attach, so read `status` and retry.
6. **`godot_game play` with no `scene` param looks successful but does not attach the runtime** (`status` → `runtime_connected: false`, `instance_count: 0`). Always play `res://scenes/game.tscn` explicitly (§Session start 3); this is the usual cause of a "broken" MCP session.
7. **Screenshots that never change**: a `frames_drawn` pin while `Time` advances usually means frozen rendering (restart stop+play rather than forcing a repaint) — but for the editor-embedded game the counter proves nothing either way (pinned at 2 with live captures seen, and stale boot-menu captures with healthy status fps seen). Always cross-check capture content against structured state. Prefer `get_window().grab_focus()` before a capture; `move_to_foreground()` is deprecated and logs an ERROR.

# Part 2 — SpacetimeDB CLI (`mindustry_godot`)

Server-side setup and inspection only. Module/crate name: **`mindustry_godot`**; the local database is **`mindustry`** and integration tests use **`mindustry-it`**. Defaults live in `server/spacetime.json`; the flow is local-first (`spacetime start` in a login shell once, then publish).

## Core commands

All commands run from the repo root (`spacetime` is on PATH in login shells):

```bash
server/build.sh              # publish mindustry + regenerate Rust bindings (wipes dev data)
server/build.sh --check      # drift gate: regenerate to a temp dir + diff, never publishes
spacetime describe mindustry --server local  # tables/reducers/schema
spacetime logs mindustry --server local      # module logs — reducers return no data, errors show here
spacetime sql mindustry --server local "SELECT * FROM player"
```

Current P0 surface (plan 01 grows it): table `player` (`identity` primary key, `username`, `last_seen`) and reducer `set_username` (`<= 40` chars, `Vars.maxNameLength`); `init` seeds a marker, `client_connected`/`client_disconnected` maintain the row.

## Rules and gotchas

- **Gameplay goes through MCP, never CLI reducers.** The CLI has its own identity; a reducer call with no client attached proves nothing about the game.
- **Every publish wipes the database** (`--delete-data=always`); seeds re-run from module code. Connected clients drop during the republish.
- **Never hand-edit generated bindings** (`client/rust/mind-stdb/src/module_bindings/`); regenerate with `server/build.sh` and let the `--check` drift gate verify them.
- A newly added reducer/table does not exist until `server/build.sh` publishes; `cargo check` alone is not enough.
- The CLI is pinned to 2.10.1 (crates/bindings match). `spacetime version install 2.10.1 --use` if the host has another default.
- `spacetime generate` needs the `wasm32-unknown-unknown` Rust target (`rustup target add wasm32-unknown-unknown`).
- `spacetime sql` is a small SQL subset: plain `SELECT ... WHERE ... LIMIT`; `BETWEEN`/`ORDER BY` are rejected.

# Part 3 — Plan-23 MCP parity catalog (machine-readable companion)

The in-engine scenarios are catalogued in `parity/mcp_catalog.json` (plan 23 §6.6) and
the headless goldens they compare against are in `parity/scenario_catalog.json` +
`parity/golden_manifest.json`. `parity mcp-parity --suite T0` resolves the headless
half of every entry against its committed golden; the in-engine capture half runs
against the single running editor (bridge 6970). `tools/parity.sh --mcp`
runs the headless half locally.

## Catalog by phase

| Phase | Plan | Scenario id | Headless parity anchor | Screenshot name(s) |
|---|---|---|---|---|
| P0 | 00 | `mcp_spine` | `spine_place_break` golden `a1a7b96167c9718d` | `00_spine_place_break_01.png` |
| P1 | 01 | `mcp_stdb_relay_2p` | `stdb_command_order` | `01_stdb_net_page.png` |
| P1 | 02 | `mcp_content` | `content_audit` (counts golden) | `02_content_inspector.png` |
| P1 | 03 | `mcp_assets` | `assets_regions` | `03_assets_region.png`, `03_assets_icon.png` |
| P1 | 04 | `mcp_io_roundtrip` | `io_roundtrip` (`C0 == C1`) | `04_io_wave5.png` |
| P2 | 05 | `mcp_sim_pause` | `sim_core_boot` golden `eaf9472d8cf4dfd5` | `05_sim_inspector.png` |
| P3 | 06 | `mcp_world_gen` | `world_gen_serpulo` | `06_world_terrain.png` |
| P3 | 07 | `mcp_blocks` | `blocks_place_construct_destroy` | `07_blocks_place.png` |
| P3 | 08 | `mcp_logistics` | `logistics_smoke` | `08_logistics_before.png`, `08_logistics_after.png` |
| P3 | 09 | `mcp_power_split` | `power_graph_split_merge` | `09_power_before_split.png`, `09_power_after_merge.png` |
| P4 | 10 | `mcp_combat_duo` | `combat_basic` + `combat/duo_dummy_worksheet.json` | `10_combat_preshot.png`, `10_combat_postshot.png` |
| P4 | 11 | `mcp_units_move` | `units_spawn_path_arrive` | `11_units_move.png` |
| P4 | 12 | `mcp_campaign_cycle` | `campaign_sector_cycle` | `12_campaign_sector.png` |
| P5 | 13 | `mcp_logic_display` | `logic_draw` / `logic_arith` | `13_logic_display.png` |
| P5 | 14 | `mcp_ui_sweep` | `ui_text` + dialog manifest | per-dialog `14_ui_<name>.png` |
| P5 | 15 | `mcp_input_place` | `placement_validation_table` | `15_place_line_preview.png`, … |
| P6 | 16 | `mcp_render_layers` | `render_layer_order` | per-layer `16_layer_<name>.png` |
| P6 | 17 | `mcp_fx_impact` | `fx_lifecycle` / `fx_program` | `17_fx_pre.png`, `17_fx_shot_t2.png`, `17_fx_hit.png` |
| P7/P8 | 19–22 | `TBD by 19…22` | `editor_*`, `mods_*`, `mp_*` (landed) | `parity/screenshots/baseline/` |

## Naming, pid and pose rules (plan 23 §3.5)

- **Naming.** Screenshots are saved as `parity/screenshots/<plan>_<scenario>_<step>.png`
  (or `<plan>_<scenario>.png` for a single capture) and recorded in
  `parity/screenshots/manifest.json`; the baseline oracle is committed under
  `parity/screenshots/baseline/`. A capture is only promotable once it is
  non-blank-checked and (when a baseline exists) diffed at the same pose with the
  plan-23 tolerance (`parity screenshots --diff-a A --diff-b B`: ≤ 12/255 per
  channel, ≤ 0.5% changed pixels; NUD-37).
- **Pid stamping.** Every verification eval must return `OS.get_process_id()` and be
  compared against `godot_game instances`; on a pid change, re-establish the camera,
  pause state and loaded scenario before interpreting the next result.
- **Fixed poses.** Use `MindCamera2D.center_on_tile(tx, ty)` + zoom (or
  `MindRender.set_camera_pose`) and record the pose in the catalog entry so two
  captures are only ever compared at the same pose.
- **Checksums over pixels.** Prefer `SimHost.get_checksum()`/`get_state_json()` equality
  to the committed golden; screenshots are the fallback for view-only systems (16/17).
- **Clean teardown.** `godot_log errors` must be empty and every toggled flag
  (pause, camera) restored before the next catalog entry.
