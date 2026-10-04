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

Typical flow: `server/build.sh` (local publish + bindings) → `godot_game play` on `res://scenes/spine.tscn` → verify via pid-stamped `godot_exec` evals and/or `spacetime sql` when the check is purely server-side.

Generic UI-driving mechanics (CLICK/DRAG coordinates, eval pitfalls, stall diagnosis) live in the MCP itself — **follow them, don't re-derive here**:

- `playtest` MCP prompt — the interactive + deterministic workflow with exact tool JSON.
- The `open_godot_mcp` addon docs under `client/addons/open_godot_mcp/` (`docs/`, handler headers) — short reminders at the call site.

The MCP server is a stdio process (`open-godot-mcp`, binary `~/.local/bin/open-godot-mcp`, bridge default **ws://127.0.0.1:6970**). Every tool takes `action` plus an optional `params` object; action-specific arguments go inside `params` (e.g. `godot_editor_edit {"action":"open_scene","params":{"path":"res://..."}}`).

# Part 1 — Godot MCP recipes (project-specific)

## Session start

1. **`godot_health check` first.** If it reports `BRIDGE_NOT_CONNECTED` (or errors with "No Godot instance connected"), launch the editor yourself on the Linux host; the MCP bridge auto-loads with the editor and the native display hosts the window:

   ```bash
   nohup godot4 --editor --path client \
     >/tmp/mind-editor.log 2>&1 &
   ```

   Wait ~20 s, then `godot_instance list` (the instance appears adopted) and `godot_health check` (port 6970). The editor never exits; a second, headless editor run during CI may briefly bind 6971 — ignore it.

2. **Identity preflight before any `godot_game` call.** The bridge is `127.0.0.1:6970`; if another Godot editor (the sibling `main/` project) already holds it, the MCP server talks to *that* project. Close the other editor, then verify: `godot_editor_read state` → `project_path` must contain `mindustry-godot`. (The `godot_exec` runtime identity check only works once a game is running; the smoke asserts it after play via `ProjectSettings.globalize_path("res://")`.)

3. **Always run `res://scenes/spine.tscn`** — the only P0 entry point. `godot_editor_edit {"action":"open_scene","params":{"path":"res://scenes/spine.tscn"}}`, then `godot_game {"action":"play","params":{"scene":"res://scenes/spine.tscn"}}` with the scene passed explicitly. Never play whatever scene happens to be open. Wait for `godot_game {"action":"status"}` → `runtime_connected: true`.

4. **Node map** (all static nodes declared in the `.tscn`):

   | Path | Role |
   |---|---|
   | `/root/Spine/SimHost` | `MindSimHost`: sim owner, fixed 60 Hz pump, API (`place_block`, `break_block`, `get_state_json`, `get_checksum`, `get_tick`, `is_paused`, `set_paused`, `step`, `load_scenario`, `selected_block`, `select_block`, `capture`) |
   | `/root/Spine/World/TileGrid` | grid + block quads (`MindTileGrid`) |
   | `/root/Spine/World/Camera2D` | `screen_to_tile`, `tile_to_screen`, `center_on_tile` |
   | `/root/Spine/Ui/StateInspector/Label` | GDScript inspector overlay text |

## Project standing rules

- **Pid-stamp every verification eval** (`OS.get_process_id()`), compare against `godot_game {"action":"instances"}` / the pid from the first eval. If a pid stamp changes across calls, re-establish every assumed state before interpreting the next result. Two runtimes (an editor game + a stale instance) can make a misrouted call look successful.
- **The sim is a fixed 60 Hz pump.** While Playing, the tick advances with wall time between calls; use `set_paused(true)` for deterministic API/input work and `step(n)` for exact advancement. `set_paused` does *not* disable `_input`: mouse place/break still applies while paused.
- **Do not pause before the scenario step.** `load_scenario` + `step(60)` must run in `Playing`: the checksum hashes `phase`, so pausing first yields `bf2ab23165daa498`, not the golden (corrected §7c order: load → step in Playing → pause → API/input). Keep `load_scenario` + `step(60)` in **one** loop-free eval so the pump cannot advance between them.
- **Loads reset the sim**: `load_scenario` replaces the whole world/player, so re-read the pid/tick and re-establish the camera before interpreting later results.
- **`tile_to_screen` returns viewport coordinates** (tile center). Feed it to `godot_input` with `coords: "viewport"`; the runtime lifts it to window space correctly. Never hardcode a click point.
- **Loop-free evals**: the MCP eval wrapper times out on `for`/`while` bodies; split heavy expressions.

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

Require `loaded: true`, `tick: 60`, and `checksum == "e53c9277bb8c28d1"` (recorded in `scenarios/spine_place_break.json`). Then pause: `godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"set_paused","args":[true]}}`.

### 3. API place/break (Rust `#[func]` path)

```
godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"place_block","args":[3, 5, "stone-wall"]}}   # -> true
godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"break_block","args":[3, 5]}}                 # -> true
```

### 4. Mouse place/break (real input path)

1. Resolve the tile center: `godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/World/Camera2D\").tile_to_screen(7, 7)"}}` → `{x, y}` viewport coords.
2. Left click to place the selected block (default `stone-wall`):
   `godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}`
3. Assert tile `(7,7)` via `get_state_json` (recipe 6); right-click (`MOUSE_BUTTON_RIGHT`) to break. While paused, call `step(1)` afterwards to prove the pump advances (`get_tick()` must increase by exactly one).

### 5. Pause + step

`godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"set_paused","args":[true]}}` then `godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").step(10)"}}` → new tick. `step(0)` is a no-op that returns the current tick.

### 6. `get_state_json` assertions

`godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").get_state_json()"}}` returns the canonical dump (same schema as `mind-headless --dump`): `{format, tick, phase, checksum, world:{width,height,sparse,tiles:[{x,y,block,team,rot,build_id}]}, entities, events, commands_applied}`. `world.tiles` is sparse (non-air only) and sorted by `(y, x)` — assert on parsed JSON, not substring hunts. The inspector label (`/root/Spine/Ui/StateInspector/Label`) polls this every 250 ms and on `state_changed(tick, checksum)`; its text contains `tick`, `checksum` and the selected block.

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
4. A wedged game: `godot_game {"action":"stop"}` then play again. Do not kill the editor unless the bridge itself is unresponsive; relaunch it with the command from Session start.
5. `godot_exec` failing with `RUNTIME_NOT_CONNECTED` means no game process is connected — play first (recipes 1–2 need the game, editor-only tools do not).

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
