---
name: parity-eval
description: >-
  Twin-run parity evaluation for Mindustry-Godot: drive the Java Mindustry
  reference with computer-mcp and the Godot client with open-godot-mcp,
  capture evidence, compare, and update the gap ledger under .opencode/evals/.
  Use when evaluating parity, playtesting the running client, auditing a
  system against upstream, or verifying a claimed port fix in-engine.
---

# Skill: parity-eval — Godot and Java↔Godot twin-run evaluation

Three agents consume this skill: the `gap-identifier` for the code-level half
(candidate seeding and code-evidence triage, no run), the `parity-writer` for
the fix and its Godot leg (`godot-pass` / `godot-open` while it works the
item), and the `twin-evaluator` for the final Java↔Godot twin run
(`twin-verified` / `open`), spawned by `/eval-gaps`. Read
[`../playtest/SKILL.md`](../playtest/SKILL.md) first for the Godot MCP launch
flow, node map, pid-stamp rules, and eval pitfalls; this skill adds the Java
reference leg, the comparison protocol, and the evidence/ledger contract. Do
not duplicate the playtest skill here — where they disagree, the playtest skill
is authoritative for the Godot side. Project call sequences live in
`.opencode/chains/`; consult the matching chain before composing MCP calls.

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
| `.opencode/evals/` | Ledger + run artifacts + MCP slot leases. |
| `.opencode/chains/` | Project MCP call sequences; consult before composing calls, keep updated after a confirmed change. |
| `.opencode/skills/parity-eval/scripts/` | `bootstrap.sh`, `capture_screen.py`, `frame_diff.py`, `record_finding.py`, `mcp_slot.py`. |

Host installs are resolved through environment overrides (`GODOT_BIN`,
`MINDY_SRC`, `JAVA17_HOME`, `MCP_VENV`); never hardcode absolute paths.

## 0. Parallel loops — resource isolation

Sessions started with `.opencode/loops/bin/start-loop.sh <N> --run` export the
loop manifest and prepend `.opencode/loops/mcp-bin` to `PATH`, so bare
`computer-mcp` / `open-godot-mcp` (and the opencode MCP servers) are pinned to
this loop's displays and editor bridge port. Loop 1 is the main checkout;
loops >= 2 run in `../Mindustry-Godot-loopN` worktrees on branch `parity/loop-N`.
The launch helpers (and the MCP shims) start that loop's two headless weston
compositors when they are down: one with the Xwayland module backs the Java
reference's X display (GPU; its display number is discovered at startup and
exported as `PARITY_DISPLAY`), the other hosts Godot on the GPU. Xvfb is the
software fallback for the Java side (`PARITY_JAVA_DISPLAY=x11` forces it;
fallback display `:10`, `:11`, …). Read `.opencode/loops/README.md` before
starting or stopping a loop.

| Variable | Meaning |
|---|---|
| `PARITY_LOOP` | loop id |
| `PARITY_DISPLAY` | the loop's Java X display: the java weston's Xwayland display on the GPU (number discovered at startup), or Xvfb (`:10`, `:11`, …) on the software fallback |
| `PARITY_WAYLAND_SOCKET`, `PARITY_WAYLAND_DIR` | Godot's weston/Wayland socket (`wayland-mind1`, …) and its runtime dir |
| `PARITY_JAVA_WAYLAND_SOCKET`, `PARITY_JAVA_WAYLAND_DIR` | Java's weston/Wayland socket (`mind1-java`, …) and its runtime dir |
| `PARITY_BRIDGE_PORT` | editor addon listen port / MCP adopt port (6970, 6980, …) |
| `PARITY_WORKTREE` | checkout this loop runs in |
| `PARITY_EVALS_DIR`, `PARITY_LEDGER` | shared evals dir and flock-protected ledger |
| `PARITY_RUN_PREFIX` | prefix run dirs with this (`l2-…` for loop 2) |
| `PARITY_LOOP_DIR` | per-loop runtime dir (display pid/logs, client user data) |

Rules:

- Loops are isolated; within one loop, drive one client at a time (Java then
  Godot). Different loops may run concurrently — never share a display.
- **MCP is slot-tracked**: before the first MCP call in a session not covered
  by a loop process lease, run
  `.opencode/skills/parity-eval/scripts/mcp_slot.py acquire --owner <label>`.
  The cap is off by default (`MCP_SLOT_LIMIT=0`); with a positive limit, exit 3
  means the host is at that limit — stay code-only or stop. `refresh` between
  calls; when an MCP run ends, release the slot with `mcp_slot.py release`
  (bare form uses `$MCP_SLOT_OWNER_KEY`, `--token`/`--key` also work).
- Launch clients only through `.opencode/loops/bin/run-godot-editor.sh` and
  `.opencode/loops/bin/run-java.sh`; they set the Godot Wayland compositor /
  Java X display, bridge port, and per-loop user-data dirs. Never use
  `godot_instance launch_editor` (it
  allocates ports from its own local index, ignoring the loop map) and never
  run `open-godot-mcp --shutdown-all` (it kills sibling loops' servers).
- Ledger writes go to `$PARITY_LEDGER` (the scripts' default via env), which is
  shared by all loops and serialized by a file lock. Run dirs live under
  `$PARITY_EVALS_DIR/runs/${PARITY_RUN_PREFIX}<stamp>-<scenario>/`.
- On the Xvfb software fallback a one-shot `computer-mcp mouse move` does not
  persist: the pointer snaps back to screen center when the XTEST client
  disconnects. Drive the Java leg with the persistent computer-mcp MCP tools,
  or click with `.opencode/loops/bin/parity-click.sh <x> <y> [button]` (single
  process, works on both display servers).

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

**computer-mcp needs `DISPLAY` at opencode process start.** It imports
`pynput` at module load, so without an X connection the server exits and
opencode silently drops its tools. Confirm the `computer-mcp_*` tools are
present before attempting the Java leg; a displayless SSH session must export
`DISPLAY` before launching opencode (or set it in the global config
`environment`). The loop shims already supply `$PARITY_DISPLAY`, and
open-godot-mcp has no such requirement.

The Java reference needs a one-time build (also packs sprites); first run
downloads Gradle 9.x and Arc from jitpack. Use the `JAVA_HOME` printed by
`bootstrap.sh` (distro JDK: `/usr/lib/jvm/java-17-openjdk-amd64`; Temurin:
`$HOME/.local/share/jdks/temurin-17`):

```bash
JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64 bash ../Mindustry/gradlew \
  -p ../Mindustry tools:pack desktop:dist
```

**Display.** The Godot client renders on the loop's headless weston compositor
(`$PARITY_WAYLAND_SOCKET`, GL on the GPU), and the Java reference renders on
the loop's dedicated weston + Xwayland compositor (`$PARITY_DISPLAY`, GL on
the GPU; its display number is discovered at startup). Both launch wrappers
start their server when down. On Xwayland the X root window holds no GPU
pixels, so mss/root grabs (including `computer-mcp screenshot`) come back
black: capture the game window instead with
`capture_screen.py --window-title Mindustry` (a bare capture retries the
window automatically and says so in its report). `PARITY_JAVA_DISPLAY=x11` or
a missing weston/Xwayland falls back to Xvfb (llvmpipe, ~5–15 FPS), where
root captures work. `godot_screenshot game` needs a windowed game, not
`--headless`. Always check a capture is non-blank (`capture_screen.py` and
`frame_diff.py` report mean/stddev) before treating a black frame as a
finding.

**Vision.** If the session model cannot accept images, never write "the button
is missing" from an unseen PNG: run `frame_diff.py` for metrics and
`capture_screen.py --ocr` for text, and say the claim is metric-based.

## 2. Twin-run protocol

Within this loop, one scenario at a time, Java first, Godot second. Parallel
loops run the same protocol simultaneously on their own displays. The
`parity-writer` runs the Godot leg itself, as part of each fix round; the
`twin-evaluator`, spawned by `/eval-gaps`, runs both legs for `godot-pass`
items, and only one twin evaluator runs across all sessions at a time (global
twin lease).

```
pick scope → create run dir → JAVA leg (capture) → quit Java
           → GODOT leg (capture) → quit Godot → compare → ledger + report
```

Run directory and naming (`$PARITY_EVALS_DIR` is shared, so keep the loop
prefix; `$PARITY_RUN_PREFIX` is empty for loop 1 and `l<N>-` otherwise):

```
$PARITY_EVALS_DIR/runs/${PARITY_RUN_PREFIX}<YYYYMMDD-HHMMSS>-<scenario>/
  run.json          # scenario id, loop id, versions/commits, window size, host, outcome
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
(`godot_editor_read state` → `project_path` contains the loop worktree, not a
sibling checkout) → `godot_editor_edit open_scene` → `godot_game play` **with**
`{"scene":"res://scenes/game.tscn"}` → `godot_game status` with
`runtime_connected: true`. Launch the editor if the bridge is down, through the
loop wrapper (it starts weston and pins the Godot Wayland socket, also keeping
`$PARITY_DISPLAY` for Java and `$PARITY_BRIDGE_PORT`; the MCP shim adopts the
same port):

```bash
nohup .opencode/loops/bin/run-godot-editor.sh \
  >"$PARITY_LOOP_DIR/logs/editor.log" 2>&1 &
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
window state). There is no semantic API. The working launch → drive → capture
→ quit sequence lives in `.opencode/chains/java-reference-leg.md`; friction
that is not a reusable sequence goes to `computer-mcp-learnings.md` at the
repo root.

```bash
RUN="$PARITY_EVALS_DIR/runs/${PARITY_RUN_PREFIX}<stamp>-<scenario>"
# launch through the loop wrapper: it sets DISPLAY, a seeded per-loop HOME,
# and per-loop XDG dirs so settings/saves never race another loop
.opencode/loops/bin/run-java.sh >"$RUN/java/game.log" 2>&1 &
# readiness signal in the log, not a fixed sleep:
until grep -q "Total time to load" "$RUN/java/game.log"; do sleep 2; done
```

Drive with the computer-mcp MCP tools (`click`, `double_click`, `drag`,
`mouse_move`, `type`, `key_press`, `key_down`, `key_up`, `list_windows`,
`get_window_info`). Those tools go to the loop's persistent computer-mcp
server, so pointer state survives between calls. `computer-mcp screenshot`
grabs the X root through mss, which is black on the Xwayland display; capture
frames to disk with `capture_screen.py` instead. For scripted CLI clicks use
`.opencode/loops/bin/parity-click.sh <x> <y> [button]`; a bare
`computer-mcp mouse move` followed by a separate `computer-mcp mouse click`
does **not** work on an Xvfb display (the pointer resets to center when the
one-shot XTEST client exits). Capture to disk with:

```bash
"$MCP_VENV/bin/python" .opencode/skills/parity-eval/scripts/capture_screen.py \
  --window-title Mindustry --output "$RUN/java/step-01-menu.png"
```

`--window-title` reads the game window directly (XGetImage); a bare capture
tries the monitor first and retries the window when that grab is blank, so the
report's `capture`/`note` fields tell you which path ran.

Quit gracefully via the main menu Quit button; if the process lingers, kill the
pid recorded at launch, and note it in `run.json`.

Java gotchas:

- Settings live in the loop's seeded HOME (`$PARITY_LOOP_DIR/home`), copied
  once from `~/.local/share/Mindustry`; normalize them inside the loop (window
  size, UI scale, language, music/SFX volume) and record the values in
  `run.json`. `settings_backups/` keeps prior snapshots.
- For the Java leg each loop owns its X display (`$PARITY_DISPLAY`): Xwayland
  on the GPU with Weston's window manager by default, Xvfb (no WM) on the
  software fallback. Run one client at a time and never start a Java client on
  another loop's display.
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

`$PARITY_LEDGER` (default `.opencode/evals/findings.json`) is machine-readable,
shared by all parallel sessions, and protected by a file lock: `record_finding.py`
takes the lock around every read-modify-write, so `add`/`claim` from concurrent
sessions can never duplicate ids or lose entries. Use the script; never
hand-edit. In a loop worktree call it through the main checkout:
`RF="${PARITY_MAIN:-.}/.opencode/skills/parity-eval/scripts/record_finding.py"`.
Phase statuses: `open` (writer queue) → `godot-open` (writer fixing and
running its own Godot leg) → `godot-pass` (Godot leg agreed; waits for
`/eval-gaps`) → `twin-unverified` (twin in flight) → `twin-verified`
(terminal), plus `wontfix` (accepted deviation). `godot-unverified` is the
marker for a legacy interrupted Godot leg; the writer reclaims it. A claim is a
cross-session lock: a fresh `owner` hides the item from pickers; `release`
restores the pre-claim status and `reap` collects dead-session claims older
than `--older-than-minutes` (default 90). Every claim carries a `session`
label; `writer` retries and `twin` picks only see the same session's items, so
a loop never verifies another worktree's code. `godot-pass` items are
session-local by design; `/eval-gaps`, run in the same loop's session before
`/merge-loops`, claims them for twin.

The gap identifier owns `add --source code` and code-evidence `wontfix`; the
parity writer owns `godot-pass`/`godot-open` via `release`; the twin evaluator
owns `twin-verified`/`open` verdicts via `verify`; `/fix-gaps` claims writer
batches and `/eval-gaps` claims twin batches, each driven one item at a time.
Every candidate carries a fix plan in its `plan` field — inline Markdown (seam,
files, steps, check), not a plan number — so a claim payload hands the writer
the sketch without any extra file lookup; the writer's failed Godot leg or the
twin evaluator's failed run refreshes that plan from the run evidence before
the next writer round.

```bash
python3 "$RF" summary
python3 "$RF" list --status open --session "loop-${PARITY_LOOP:-1}"
python3 "$RF" claim --for writer --session "loop-${PARITY_LOOP:-1}" --count 3  # exits 3 when none
python3 "$RF" claim --for twin --session "loop-${PARITY_LOOP:-1}" --count 3    # exits 3 when none
python3 "$RF" release --id EV-0001 --status godot-pass \
  --note "expected behavior observed at <pid>" --evidence "runs/.../godot/step-04.png"
python3 "$RF" release --id EV-0001 --note "writer blocked: <reason>"
python3 "$RF" reap --older-than-minutes 90             # recover dead-session claims
python3 "$RF" reap --session "loop-2" --older-than-minutes 0  # a restarted loop's dead claims
python3 "$RF" add \
  --area ui/menu --severity S2 --title "Campaign button does not open the planet view" \
  --expected "Java: Play > Campaign opens Serpulo planet view" \
  --actual "Godot: click leaves the menu unchanged; no dialog bound" \
  --repro "scenario boot_menu step 4" \
  --plan "Seam: menu button emits no signal. Steps: bind Campaign to the planet dialog; hide the Workshop button. Check: headless boot_menu step 4." \
  --evidence "runs/${PARITY_RUN_PREFIX}<stamp>-boot_menu/godot/step-04.png"
python3 "$RF" set-plan --id EV-0001 --plan "<Markdown fix sketch>"
python3 "$RF" verify \
  --id EV-0001 --status twin-verified --note "twin run at <commit>" \
  --evidence "runs/${PARITY_RUN_PREFIX}<stamp2>-boot_menu/godot/step-04.png"
python3 "$RF" verify --id EV-0002 --status open --note "twin fail: <what differs>"
```

### Code-level candidates and triage

The gap identifier records candidates without running anything; `add` starts
them in the writer queue (`open`) with a fix plan in `--plan`:

```bash
python3 "$RF" add \
  --area game/campaign --severity S1 --source code --confidence high \
  --title "Launch never applies the sector rules" \
  --expected "World.java:265-330 applies the preset rules on launch" \
  --actual "campaign.rs:124 calls play_new_sector with Rules::default()" \
  --repro "start_sector('serpulo',170); eval rules.waves" \
  --plan "Seam: campaign.rs:124 skips the preset rules. Steps: resolve the preset, pass its Rules to play_new_sector, add the headless case. Check: cargo test -p mind-core campaign." \
  --evidence client/rust/mind-gdext/src/campaign.rs:124 \
  --evidence ../Mindustry/core/src/mindustry/core/World.java:265
python3 "$RF" set-plan --id EV-0001 --plan "<Markdown fix sketch>" \
  --note "plan added while sharpening the repro"
python3 "$RF" set-status --id EV-0001 --status wontfix \
  --note "handled by the shared Rules path" \
  --evidence client/rust/mind-gdext/src/campaign.rs:120
```

Rules for code-sourced records: both sides' file:line in `--evidence`, a repro
sketch the evaluator can execute later, a `plan` sketch (seam, files, steps,
check) in inline Markdown, one symptom per record, and dedupe notes on
existing ids instead of new records. Code evidence never closes a
finding as fixed; a fresh in-engine reproduction is the only path to
`godot-pass` and `twin-verified`, and only a reading that settles the gap may
set `wontfix`.

Titles name the player-visible symptom, not the presumed code cause. One
finding per independent symptom; link an existing id in notes instead of
duplicating. Agent `edit` permission covers `**/evals/**`; if the shared evals
dir is outside the session's worktree and the tool still refuses the path,
fall back to the script and bash (`record_finding.py` for the ledger,
`mkdir`/redirection for reports).

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
- Each loop's editor addon listens on `$PARITY_BRIDGE_PORT` (6970, 6980, …) and
  the MCP shim adopts exactly that port. Identity-check `project_path` every
  session: with several loops running, a wrong-port editor is easy to adopt.
- Two runtimes (editor game + stale instance) make a misrouted eval look
  successful; pid-stamp everything.
- `godot_instance launch_editor` allocates ports from its own local index and
  ignores the loop map; launch editors only through `run-godot-editor.sh`.
  Never run `open-godot-mcp --shutdown-all` — it kills sibling loops' servers.
- On the Xvfb software fallback, a one-shot `computer-mcp mouse move` snaps
  back to screen center when the process exits. Use the persistent
  computer-mcp MCP tools or `parity-click.sh`; the same applies to any helper
  that moves and clicks in separate processes.
- Eval bodies with `for`/`while` time out; split heavy expressions. A `null`
  result with `ok: true` usually means the body errored.
- `godot_screenshot burst` blocks the round-trip; keep bursts tiny or take
  individual captures.
- Don't re-run the headless goldens as "evaluation"; the harness already owns
  that. In-engine time is for the view/input/flow layers the harness cannot see.
- A displayless opencode session silently drops `computer-mcp`; the Java leg
  then has no tool to call. Relaunch with `DISPLAY` set (loop shims or the
  global config env) instead of improvising a headless Java leg.
- Never write a finding without an artifact path. Never mark a phase passed
  without a fresh repro. Never record a verdict from a code reading — code
  evidence can only seed a candidate or settle it as `wontfix`.
