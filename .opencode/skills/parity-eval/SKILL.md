---
name: parity-eval
description: >-
  Twin-run parity evaluation for Mindustry-Godot: drive the Java Mindustry
  reference with computer-mcp and the Godot client with open-godot-mcp,
  capture evidence, compare, and update the gap ledger under .opencode/evals/.
  Use when evaluating parity, playtesting the running client, auditing a
  system against upstream, or verifying a claimed port fix in-engine.
---

# Skill: parity-eval — Java↔Godot twin-run evaluation

The evaluator agent is the only consumer. Read
[`../playtest/SKILL.md`](../playtest/SKILL.md) first for the Godot MCP launch
flow, node map, pid-stamp rules, and eval pitfalls; this skill adds the Java
reference leg, the comparison protocol, and the evidence/ledger contract. Do
not duplicate the playtest skill here — where they disagree, the playtest skill
is authoritative for the Godot side.

**Anything a player does goes through a game client.** `spacetime call` never
counts as a gameplay check. The `spacetime` CLI is only for arranging and
inspecting the server side, exactly as described in the playtest skill.

## Repository map

Run everything from the `Mindustry-Godot` repo root. The parent workspace also
holds the reference checkout and the root `.opencode` symlink that loads this
skill:

| Path | Role |
|---|---|
| `client/` | Godot 4.7 project (`res://`). |
| `../Mindustry` | Java reference (upstream `2cd7aeec…`, per `parity/upstream.lock`). |
| `parity/mcp_catalog.json` | Authoritative in-engine scenario queue (ids, scenes, steps, screenshots). |
| `parity/golden_manifest.json`, `parity/scenario_catalog.json` | Simulation goldens/checksums. |
| `.opencode/evals/` | Ledger + run artifacts (your only write target). |
| `.opencode/skills/parity-eval/scripts/` | `bootstrap.sh`, `capture_screen.py`, `frame_diff.py`, `record_finding.py`. |

Host installs are resolved through environment overrides (`GODOT_BIN`,
`MINDY_SRC`, `JAVA17_HOME`, `MCP_VENV`); never hardcode absolute paths.

## 1. Preconditions

Run the bootstrap check before any scenario. If anything fails, stop and report
the blocker; do not improvise system installs (there is no passwordless sudo).

```bash
MCP_VENV="${MCP_VENV:-$HOME/.local/share/mcp-venv}"
".opencode/skills/parity-eval/scripts/bootstrap.sh" --check
```

`bootstrap.sh` provisions, idempotently:

- `computer-mcp` (PyPI, 0.0.7, pinned with `--with "mcp<2"` because its
  `@server.list_tools()` API was removed in MCP SDK 2.x) and `open-godot-mcp`
  (`notnilcn/Open-Godot-MCP`, installed as `uv` tools so both executables land
  in `~/.local/bin`, where the global opencode `mcp` config calls them by bare
  name). The installed server version must match the vendored addon
  `client/addons/open_godot_mcp/plugin.cfg` (`0.1.10`).
- A JDK 17: an existing Java 17 on `PATH`/`JAVA_HOME`/`/usr/lib/jvm` is used;
  otherwise Temurin is downloaded into `$JAVA17_HOME`
  (`$HOME/.local/share/jdks/temurin-17`). The repo's `settings.gradle` enforces
  17 and `/tmp` JDKs are ephemeral.
- Optional host tools. `xdotool`/`wmctrl` are only needed for window-geometry
  automation; without them, capture the full screen and run one client at a
  time. `tesseract` enables OCR text comparison; it is optional.

Additional preconditions:

```bash
tools/build.sh                       # mind-gdext + mind-headless; syncs scenarios
"${GODOT_BIN:-godot4}" --version     # this host ships the binary as `godot`
java -version                        # must be 17.x; bootstrap.sh prints JAVA_HOME
tools/mcp-smoke.sh                   # when in doubt: isolates bridge/identity/checksum issues
```

The Java reference needs a one-time build (also packs sprites); first run
downloads Gradle 9.x and Arc from jitpack. Use the `JAVA_HOME` printed by
`bootstrap.sh` (distro JDK: `/usr/lib/jvm/java-17-openjdk-amd64`; Temurin:
`$HOME/.local/share/jdks/temurin-17`):

```bash
JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64 bash ../Mindustry/gradlew \
  -p ../Mindustry tools:pack desktop:dist
```

**Display.** The host runs X on `DISPLAY=:10.0` with Mesa `llvmpipe` (software
GL): expect ~5–15 FPS. `godot_screenshot game` needs a windowed game, not
`--headless`. Always check a capture is non-blank (`capture_screen.py` and
`frame_diff.py` report mean/stddev) before treating a black frame as a finding.

**Vision.** If the session model cannot accept images, never write "the button
is missing" from an unseen PNG: run `frame_diff.py` for metrics and
`capture_screen.py --ocr` for text, and say the claim is metric-based.

## 2. Twin-run protocol

One scenario at a time, Java first, Godot second.

```
pick scope → create run dir → JAVA leg (capture) → quit Java
           → GODOT leg (capture) → quit Godot → compare → ledger + report
```

Run directory and naming:

```
.opencode/evals/runs/<YYYYMMDD-HHMMSS>-<scenario>/
  run.json          # scenario id, versions/commits, window size, host, outcome
  java/  step-<nn>-<name>.png | game.log | notes.md
  godot/ step-<nn>-<name>.png | game.log | state-<nn>.json
  diff/  step-<nn>.json | step-<nn>-heatmap.png
  report.md
```

Fixed comparison setup (record it in `run.json`): same screen/window size
(e.g. `DisplayServer.window_set_size(Vector2i(1280,720))` on Godot; normalize
the Java window in its settings), UI scale 1.0, language `en`, same sector/seed,
and the same camera pose per capture step.

## 3. Godot leg

Follow the playtest skill exactly: `godot_health check` → editor identity
(`godot_editor_read state` → `project_path` contains `mindustry-godot`) →
`godot_editor_edit open_scene` → `godot_game play` **with**
`{"scene":"res://scenes/game.tscn"}` → `godot_game status` with
`runtime_connected: true`. Launch the editor if the bridge is down:

```bash
nohup "${GODOT_BIN:-godot4}" --editor --path client >>"$RUN/godot/editor.log" 2>&1 &
```

Useful evaluator moves, all pid-stamped:

- Golden check: `load_scenario` + `step(60)` in one loop-free eval, then
  compare `get_checksum()` to the committed golden before pausing.
- Debug step: `MindSimHost.set_paused(true)` then `step(n)` for exact ticks;
  `_input` still applies while paused.
- State: `get_state_json()` for tiles/entities/events; assert on parsed JSON,
  never substrings.
- HUD/UI: `MindHud`/`MindUi` properties and JSON endpoints, then
  `godot_screenshot game`.
- Input: `godot_input` with `coords: "viewport"` from
  `Camera2D.tile_to_screen(x, y)`; never hardcode click points.
- Logs: `godot_log clear` before a step, `godot_log errors` after; a scenario
  only passes with an empty error log.

Clean teardown: restore pause/camera, `godot_game stop`, then record the log.

### Input and capture gotchas (local host)

- **Prefer discrete `godot_input` calls over `sequence`.** The awaited
  `sequence` handler awaits `process_frame` inside a debugger call and can hit
  the 15 s `call_runtime` timeout; `mouse_motion`/`mouse_button`/`key` return
  immediately.
- **Injected events sit in the accumulated-input buffer.** After a
  `godot_input` press/release, run an eval that calls
  `Input.flush_buffered_events()` and then asserts the target state
  (`visible`, `get_tree().root.gui_get_hovered_control()`, dialog stack). A
  click that is never flushed looks like a dead UI.
- **Eval fallback when `godot_input` still does not land** (validated): build
  the events in eval and call `get_viewport().push_input(down)` /
  `push_input(up)` synchronously; resolve the point from
  `(control as Control).get_global_rect().get_center()`. Record in `run.json`
  that the fallback was used, and never hardcode coordinates.
- **Screenshot staleness.** With the editor maximized over the game window,
  `godot_screenshot game` can return the last presented frame — identical bytes
  across real state changes. Call `get_window().move_to_foreground()` in an
  eval first (validated), or capture the X screen with computer-mcp and crop
  with `capture_screen.py --rect` using `DisplayServer.window_get_position()`
  and `DisplayServer.window_get_size()`.
- **Error breakpoints.** An eval body that errors stops in the editor Debugger;
  `godot_log errors` and clear the buffer before the scenario so stale errors
  are not attributed to the run.

## 4. Java leg

The reference is driven at the OS level with computer-mcp (mouse, keyboard,
screenshot, window state). There is no semantic API.

```bash
RUN=.opencode/evals/runs/<stamp>-<scenario>
JAVA17="${JAVA17_HOME:-$(dirname "$(dirname "$(readlink -f "$(command -v java)")")")}"  # printed by bootstrap.sh
JAVA_HOME="$JAVA17" "$JAVA17/bin/java" -jar \
  ../Mindustry/desktop/build/libs/Mindustry.jar >"$RUN/java/game.log" 2>&1 &
# readiness signal in the log, not a fixed sleep:
until grep -q "Total time to load" "$RUN/java/game.log"; do sleep 2; done
```

Drive with the computer-mcp tools (`click`, `double_click`, `drag`,
`mouse_move`, `type`, `key_press`, `key_down`, `key_up`, `screenshot`,
`list_windows`, `get_window_info`), or the CLI directly (`computer-mcp mouse
click --x 640 --y 360`; run `--help` for exact flags). Capture to disk with:

```bash
"$MCP_VENV/bin/python" .opencode/skills/parity-eval/scripts/capture_screen.py \
  --output "$RUN/java/step-01-menu.png"
```

Quit gracefully via the main menu Quit button; if the process lingers, kill the
pid recorded at launch, and note it in `run.json`.

Java gotchas:

- Settings persist in `~/.local/share/Mindustry`; normalize them once
  (window size, UI scale, language, music/SFX volume) and record the values in
  `run.json`. `settings_backups/` keeps prior snapshots.
- There is a single X display and usually no WM tooling: run one client at a
  time so it owns focus.
- Prefer keyboard shortcuts and menu paths that exist in both clients; when a
  click coordinate is needed, derive it from the current Java screenshot at the
  recorded window size and store the coordinate in the run notes.

## 5. Comparison order

Cheapest and most trustworthy first:

1. **Committed oracle.** Godot `get_checksum()` / `get_state_json()` against
   `parity/scenario_catalog.json` / `golden_manifest.json`. A mismatch is an
   automatic S1/S2 finding with the exact tick and checksum in evidence.
2. **Structured state.** Non-visual behavior that both clients expose
   (HUD item counts, wave number, building config, prices) — compare values,
   not pixels.
3. **Text.** `capture_screen.py --ocr` or dialog text / bundle keys; missing or
   mistranslated strings are findings with the OCR output as evidence.
4. **Frames.** Only when size and pose match:

   ```bash
   "$MCP_VENV/bin/python" .opencode/skills/parity-eval/scripts/frame_diff.py \
     --java "$RUN/java/step-03-game.png" --godot "$RUN/godot/step-03-game.png" \
     --out-json "$RUN/diff/step-03.json" \
     --out-heatmap "$RUN/diff/step-03-heatmap.png"
   ```

   The script reports size mismatch, blank frames, mean/std, changed-pixel
   ratio, coarse-layout diff, and palette distance. A layout-region difference
   is a presentation finding; a palette-only difference is usually a shader or
   asset finding. Software-GL timing jitter means never diff mid-animation
   frames — pause both clients for captures whenever possible.

Record the strongest evidence class that shows the mismatch, plus a human
repro. "Looks different" is not a finding.

## 6. Ledger and reports

`.opencode/evals/findings.json` is machine-readable and single-writer. Use the
script; never hand-edit:

```bash
python3 .opencode/skills/parity-eval/scripts/record_finding.py list --status open
python3 .opencode/skills/parity-eval/scripts/record_finding.py add \
  --area ui/menu --severity S2 --title "Campaign button does not open the planet view" \
  --expected "Java: Play > Campaign opens Serpulo planet view" \
  --actual "Godot: click leaves the menu unchanged; no dialog bound" \
  --repro "scenario boot_menu step 4" \
  --evidence runs/<stamp>-boot_menu/godot/step-04.png \
  --plan 14
python3 .opencode/skills/parity-eval/scripts/record_finding.py verify \
  --id EV-0001 --status verified-fixed --note "re-ran boot_menu at <commit>" \
  --evidence runs/<stamp2>-boot_menu/godot/step-04.png
```

Titles name the player-visible symptom, not the presumed code cause. One
finding per independent symptom; link an existing id in notes instead of
duplicating. If `edit` is denied for an evals path, fall back to the script and
bash (`record_finding.py` for the ledger, `mkdir`/redirection for reports).

Write `report.md` with this shape:

```markdown
# <scenario> — <date> (<godot commit>, Java <upstream short sha>)

- Result: <parity | gaps | blocked>
- Window/setup: <size>, <pose>, <settings snapshot ref>

## Findings
| id | sev | step | symptom | evidence |
|----|-----|------|---------|----------|

## Verified
| id | status | re-run | evidence |
|----|--------|--------|----------|

## Blocked / not covered
- <step>: <exact blocker>

## Raw artifacts
- java/: ...
- godot/: ...
```

## 7. Scenario catalog (grow as coverage increases)

Start with the high-value flows; `parity/mcp_catalog.json` is the long-form
queue and should win where ids overlap.

| id | goal | oracle |
|---|---|---|
| `boot_menu` | Boot to main menu; logo, background, button tree, submenus | frame + OCR |
| `settings_ui` | Settings dialog tabs/values/apply; language, video, controls | OCR + frame |
| `campaign_launch` | Play → Campaign → Serpulo → Ground Zero → launch | frame + state |
| `hud_ingame` | Core items HUD, wave counter, catalog, minimap, toolbar | state JSON + frame |
| `placement_basic` | Select, place, rotate, configure, break a block | state JSON + frame |
| `logistics_flow` | Drill → conveyor → core item movement | HUD values + state |
| `power_grid` | Generator → consumer graph, battery charge | state JSON |
| `combat_wave` | Summon wave, turret fire, HP/damage, death FX | frame + state |
| `units_move` | Spawn/move/attack-order a unit; pathing and animation | frame + state |
| `logic_program` | Place processor, edit code, run message/arithmetic | frame + state |
| `ui_dialogs` | Each manifest dialog opens and renders (sweep) | OCR + frame |
| `input_zoom_pan` | Camera zoom/pan bounds, place-line, selection box | frame + camera JSON |
| `save_load` | Save slot, reload, checksum/state equality | state JSON |
| `editor_basic` | Map editor open, paint, save | frame + state |
| `mp_host_join` | Host/join over STDB (server leg via `spacetime` CLI) | server rows + client state |

Before scripting a new scenario, check whether an entry already exists in
`parity/mcp_catalog.json` and reuse its id, scene, steps, and screenshot names.

## 8. Pitfalls

- `godot_game play` without the explicit `params.scene` looks successful but
  attaches nothing; every later `godot_exec` fails with `RUNTIME_NOT_CONNECTED`.
- The MCP bridge binds one editor at `127.0.0.1:6970`; a sibling Godot project
  can steal it. Identity-check `project_path` every session.
- Two runtimes (editor game + stale instance) make a misrouted eval look
  successful; pid-stamp everything.
- Eval bodies with `for`/`while` time out; split heavy expressions. A `null`
  result with `ok: true` usually means the body errored.
- `godot_screenshot burst` blocks the round-trip; keep bursts tiny or take
  individual captures.
- Don't re-run the headless goldens as "evaluation"; the harness already owns
  that. In-engine time is for the view/input/flow layers the harness cannot see.
- Never write a finding without an artifact path. Never mark fixed without a
  fresh repro. Never edit game code.
