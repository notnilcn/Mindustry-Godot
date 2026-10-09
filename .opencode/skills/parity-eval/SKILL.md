---
name: parity-eval
description: >-
  Mirrored Java↔Godot parity runs for Mindustry-Godot: launch both clients on
  the shared display :10, drive the Java reference with computer-mcp and the
  Godot client with open-godot-mcp, mirror every action, screenshot-compare
  until the first mismatch, then stop and fix the Godot side and document the
  item. Use when running the /loop or /goal parity process, evaluating parity,
  auditing a system against upstream, or verifying a port fix in-engine.
---

# Skill: parity-eval — mirrored Java↔Godot parity hunting

One session drives both clients **concurrently on the same display** (`:10`).
The Java reference is operated at the OS level with computer-mcp; the Godot
client is driven through open-godot-mcp. Every scenario is a lock-step mirror:
do the same action on both sides, capture both windows, diff the frames, and
when a diff crosses the gate **stop the sweep, fix the Godot side, re-run, and
record everything**.

The work is driven by the `@bybrawe/opencode-loop` (`/loop`) and
`@bybrawe/opencode-goal` (`/goal`) plugins. A goal or prompt-loop owns one
autonomous session; `parity-ledger.json` is its durable work queue and proof
record.

## Document map

| Path | Role |
|---|---|
| `parity-ledger.json` | Machine-readable queue + proof. Read before picking work; update after every step, mismatch, and fix. |
| `chains/` | Canonical per-flow MCP call sequences (schema in `chains/README.md`). Consult before composing calls; update/bump after a confirmed run. |
| `runs/<stamp>-<item>/` | Run artifacts: `java/`, `godot/`, `diff/`, `run.json`, `report.md`. Gitignored. |
| `computer-mcp-learnings.md`, `open-godot-mcp-learnings.md` | Friction + workarounds that are not chains. Append a dated entry when a run taught you something. |
| `parity/mcp_catalog.json`, `parity/scenario_catalog.json`, `parity/golden_manifest.json` | Committed oracle data (harness-owned). Do not duplicate it here. |
| `.opencode/skills/playtest/SKILL.md` | Godot MCP mechanics. Read it first; this skill only adds the Java leg, the mirror loop, the comparison gate, and the documentation contract. |

**Anything a player does goes through a game client.** `spacetime call` never
counts as a gameplay check; the CLI is only for arranging and inspecting the
server side, exactly as the playtest skill describes.

## 0. Session setup

### Preconditions

Run from the `Mindustry-Godot` repo root. Check once per session; stop and
report a blocker instead of improvising system installs.

```bash
MCP_PY="${MCP_PY:-$HOME/.local/share/uv/tools/computer-mcp/bin/python}"  # mss+PIL+python-xlib
ls ../Mindustry/desktop/build/libs/Mindustry.jar                        # Java reference
tools/build.sh          # only when Rust sources changed; syncs scenarios
tools/mcp-smoke.sh      # when the Godot bridge is in doubt
```

`bootstrap.sh --check` still validates JDK 17, the MCP servers and the optional
host tools. `tesseract` is **not** installed, so OCR is unavailable — compare
text through structured state (see §3).

### Launch both clients on `:10`

Shell commands need `DISPLAY=:10` for any X tool (`capture_screen.py`,
`window_ctl.py`, `xwininfo`); the MCP servers already have it from the global
opencode config.

Java reference (detached; `setsid` survives the shell-tool timeout where plain
`nohup ... &` does not):

```bash
JAR=../Mindustry/desktop/build/libs/Mindustry.jar
JAVA=/usr/lib/jvm/java-17-openjdk-amd64/bin/java
JHOME="$PWD/parity-java-home"                    # persistent, gitignored
mkdir -p "$JHOME/.local/share" "$JHOME/.config" "$JHOME/.cache"
setsid env DISPLAY=:10 HOME="$JHOME" \
  XDG_DATA_HOME="$JHOME/.local/share" XDG_CONFIG_HOME="$JHOME/.config" \
  XDG_CACHE_HOME="$JHOME/.cache" \
  "$JAVA" -jar "$JAR" >>"runs/java-console.log" 2>&1 &
# readiness is a log line, not a fixed sleep (first launch is slow):
until grep -q "Total time to load" runs/java-console.log; do sleep 2; done
```

Godot editor (only when `godot_health check` says the bridge is down; it stays
up across runs and must be relaunched after every `tools/build.sh`):

```bash
OPEN_GODOT_MCP_PORT=6970 setsid "${GODOT_BIN:-godot}" --editor \
  --path client >>runs/editor-console.log 2>&1 &
```

Then follow the playtest skill: `godot_health check` → `godot_editor_read
state` (`project_path` contains `mindustry-godot`) → `godot_editor_edit
open_scene res://scenes/game.tscn` → `godot_game play` **with**
`{"scene":"res://scenes/game.tscn"}` → `godot_game status` shows
`runtime_connected: true` → pid-stamp.

### Normalize before comparing

Record the values in `run.json`; all of them persist in `parity-java-home/`:

- Java window **1152x648**, windowed, UI scale 1.0, language `en`, music/SFX
  volume 0. Normalize once interactively; if text fields drop injected input
  (known), resize/move the window instead:
  `DISPLAY=:10 "$MCP_PY" .opencode/skills/parity-eval/scripts/window_ctl.py resize Mindustry 1152 648`
  and `... move Mindustry 40 40`. The embedded Godot game is fixed at
  1152x648 — never try to resize it.
- Java saves/settings live in `parity-java-home/`; keep that directory stable
  so scenarios start from the same state. Delete its `saves/` only to test a
  fresh profile, and say so in `run.json`.
- Both windows may be maximized/overlapping; `godot_screenshot` reads the game
  viewport texture, but Java captures read the X window, so **raise it first**:

  ```bash
  DISPLAY=:10 "$MCP_PY" .opencode/skills/parity-eval/scripts/window_ctl.py raise Mindustry
  ```

## 1. The mirror loop

Pick the first non-`verified` item in `parity-ledger.json` (or the scenario the
goal names). For that item:

1. Read its chain in `chains/` and the ledger entry; note both legs' sequences.
2. Reset both clients to the same boot state (restart or return both to the
   main menu) and stamp the pids/commits in `run.json`.
3. For each step of the chain, alternately:
   - **Java**: `raise Mindustry` → perform the action with `computer-mcp_*` →
     `capture_screen.py --window-title Mindustry` → `runs/<run>/java/step-NN.png`.
   - **Godot**: perform the equivalent action with `godot_*` → capture the game
     with `godot_screenshot` (or the viewport) → `runs/<run>/godot/step-NN.png`.
   - **Compare**: `frame_diff.py` the two captures → `runs/<run>/diff/step-NN.json`;
     apply the gate in §2.
4. All steps match → mark the item `verified`, bump the chain (`last_verified`,
   steps), continue to the next item.
5. First mismatch → **stop the sweep** and switch to fix mode (§4).

Hard rules:

- One mirrored action pair per step. Never leap several UI states ahead on one
  side and then "catch up" the other; the failure must be reproducible at a
  named step.
- Only compare stable frames. Menus/dialogs are naturally stable. In-game
  scenes are not: pause the Java client and `SimHost.set_paused(true)` the
  Godot client (or `step()` both to the same tick) before capturing. Never diff
  mid-animation.
- If a capture comes back blank (metrics `blank: true`) or the action clearly
  did not land, re-take it — do not classify a capture failure as a parity
  mismatch. Log the retake in `run.json`.
- The tool response of a Java `click`/`mouse_move`/`key_press` shows the
  previous frame; always capture separately before judging what it did.

## 2. Comparison gate

```bash
DISPLAY=:10 "$MCP_PY" .opencode/skills/parity-eval/scripts/frame_diff.py \
  --java  runs/<run>/java/step-NN.png \
  --godot runs/<run>/godot/step-NN.png \
  --out-json runs/<run>/diff/step-NN.json \
  --out-heatmap runs/<run>/diff/step-NN-heatmap.png \
  --out-side-by-side runs/<run>/diff/step-NN-side.png
```

Default gate (plan-23 tolerance), all must hold:

| metric | match when |
|---|---|
| `size_match` | `true` (both 1152x648) |
| `diff.changed_fraction` | `<= 0.005` |
| `diff.mean_abs_rgb` (each channel) | `<= 12` |
| `coarse_layout.max_channel_mean` | `<= 6` |
| `palette_distance` | `<= 0.05` |
| stats | neither frame `blank` |

Anything else is a **mismatch**. Calibration is allowed only with evidence:
record the original gate, the new numbers, the reason (e.g. known font
rasterization), and the calibration frame pair in the ledger entry. Never
widen silently; never widen past the plan-23 numbers except for a documented
Java-vs-Godot rendering difference that is itself tracked as an item.

Work in the strongest evidence class available (cheapest first):

1. **Committed oracle** — Godot `get_checksum()`/`get_state_json()` vs
   `parity/scenario_catalog.json` / `golden_manifest.json`.
2. **Structured state** — both clients' non-visual values (HUD item counts,
   wave number, prices, config).
3. **Text** — bundle keys / dialog text read from structured state (no OCR).
4. **Frames** — the gate above; a layout-region difference is a presentation
   finding, a palette-only difference usually a shader/asset finding.

## 3. Mismatch → fix → verify

On the first mismatch:

1. **Record the stop point in `parity-ledger.json` before touching code**: step
   number, the two screenshots, the diff JSON, the metric values, and the exact
   MCP call sequences for both legs up to that step (copy from the run log; the
   chain has the canonical full sequence).
2. Diagnose on the Godot side with the playtest skill (structured state first:
   checksum, `get_state_json`, HUD JSON, dialog nodes). If pixels disagree but
   structured state agrees, the finding is presentation-layer; if state
   disagrees, find the sim/UI seam.
3. Implement the fix in `client/` (GDScript) or `client/rust/` (Rust).
4. Run the narrow check (`cargo test -p mind-core ...`, `cargo clippy`,
   `tools/build.sh` as applicable).
5. After `tools/build.sh`, **relaunch the editor** — a running editor has the
   old `libmind_gdext.so` mmap'd.
6. Re-run the scenario from the top. The item becomes `verified` only when the
   whole chain passes; a fixed step that reveals the next mismatch stays in the
   same item (record the new stop point).
7. Update the docs (§5) and commit: code fix first (`Fixes <PAR-ID>`), docs in
   a separate commit.

A code reading can seed a candidate or settle one as `wontfix`, but only a
fresh in-engine repro can mark an item fixed.

## 4. Documentation contract

### `parity-ledger.json`

The single source of progress. Shape (one `items[]` entry per item):

```json
{
  "id": "PAR-0001", "title": "player-visible symptom", "status": "mismatch",
  "chain": "chains/enter-campaign.md", "updated": "2026-10-09T12:00:00Z",
  "run": "runs/20261009-120000-enter-campaign/",
  "reproduce": {
    "java": [{"tool": "computer-mcp_click", "params": {"x": 310, "y": 216, "button": "left"}},
             {"tool": "capture_screen.py", "params": {"--window-title": "Mindustry"}}],
    "godot": [{"tool": "godot_input", "params": {"action": "mouse_button", "params": {"button": "MOUSE_BUTTON_LEFT"}}},
              {"tool": "godot_screenshot", "params": {"action": "game"}}]
  },
  "steps": [{"n": 1, "action": "click Play", "diff": "diff/step-01.json", "verdict": "match"}],
  "mismatch": {"step": 4, "signals": {"changed_fraction": 0.21},
               "java_frame": "java/step-04.png", "godot_frame": "godot/step-04.png"},
  "fix": {"status": "open", "commit": null, "summary": null},
  "verified": null
}
```

Update `updated`, `steps`, `mismatch`, `fix`, `verified` after every run; append
new items with the next `PAR-####` id. Statuses: `seeded`, `in-progress`,
`mismatch`, `fixing`, `verified`, `blocked`, `wontfix`.

### Chains

After a confirmed run, update (or create) the matching `chains/<name>.md`
following `chains/README.md`: exact tool + action + parameter JSON per step, no
hardcoded click coordinates unless measured from the current capture, status
`verified` only with a run dir + pid/commit in `last_verified`. Java-leg
sequences go in `java-*.md` files; a mixed item names both servers in `tools`.

### Learnings

Friction that is not a reusable chain goes to the repo-root learnings files as
one dated entry: what happened, what worked, the run or chain that shows it.
Never rewrite another session's entry.

### Run report

`runs/<stamp>-<item>/report.md`:

```markdown
# <item> — <date> (<godot commit>, Java <upstream short sha>)

- Result: <parity | mismatch | blocked>
- Setup: display :10, windows <java 1152x648 @ x,y> <godot embedded 1152x648>, settings snapshot ref

## Steps
| # | action | java frame | godot frame | diff json | verdict |

## Mismatch
- step: <n>; signals: <metrics>; heatmap: <path>

## Fix
- commit: <sha>; check: <command>; re-run: <run dir>

## Raw artifacts
- java/: ... godot/: ... diff/: ...
```

## 5. Java-leg gotchas (computer-mcp)

- `list_windows`/`get_window_info` are not implemented on Linux; use
  `window_ctl.py list` or `xwininfo -root -children`, and
  `window_ctl.py raise Mindustry` before every Java capture.
- The screenshot attached to a `click`/`mouse_move`/`key_press` response is the
  previous frame. Capture separately.
- `key_press` holds long enough for X auto-repeat: for toggles split
  `key_down` + `key_up` across two calls. Send an explicit `button_up` before
  expecting a fresh click; a world `button_down` can stay logically held.
- No scroll action exists: one-shot pynput from `$MCP_PY`
  (`from pynput.mouse import Controller; Controller().scroll(0,-6)`).
- Timed key combos (the in-game block-select combo inside 400 ms) need one
  persistent pynput process, not separate MCP calls.
- Java text fields frequently drop injected input; prefer menu paths and
  shortcuts that exist in both clients, or write settings offline (see
  `computer-mcp-learnings.md`, EV-0059) while the client is stopped.
- Fresh Java data dir: Play → Campaign shows a `Select Starting Campaign`
  dialog (Serpulo photo → OK) before the planet page.
- Never `pkill -f "<jar>"`: the pattern matches the tool shell itself. Record
  the pid from `pgrep -f 'Mindustry[.]jar'` and kill only that.
- Quit gracefully via the main menu; if the process lingers, kill the recorded
  pid and note it in `run.json`.

## 6. Godot-leg gotchas (open-godot-mcp)

- `godot_game play` without `params.scene` attaches nothing; always pass
  `res://scenes/game.tscn`.
- Injected input sits in the accumulated-input buffer: flush with
  `Input.flush_buffered_events()` in an eval and assert the target state.
- An errored or timed-out `godot_exec` can trip break-on-error and pin the
  frame loop while evals still answer; recover with `godot_game stop` + `play`
  (`resume` re-breaks). Keep eval bodies loop-free and never `await` in them.
- `frames_drawn` pinning is not proof of stale captures, and liveness is not
  proof of fresh ones: cross-check capture content against structured state.
- Rust methods without `#[func]` are not eval-reachable; use a `#[func]`
  sibling or the real input path.
- `SimHost.place_block` returns true even when the runtime rejects the tile;
  assert placed tiles from `get_state_json()`.
- STOP Controls swallow world clicks (paused banner at viewport center, hints
  panel, `LoadingLayer`); probe `gui_get_hovered_control()` before concluding
  an input is dead.
- Camera transforms update one frame after `center_on_tile`/`pan_to`; resolve
  click coordinates in a later eval. Edge-pan drifts the camera while the
  pointer idles at a screen edge — park the pointer or disable camera process.
- The editor-embedded game cannot be resized (fixed 1152x648).

## 7. Scenario queue

Growing coverage; `parity/mcp_catalog.json` wins where ids overlap. Seed each
item in the ledger from the matching chain.

| id | goal | primary oracle |
|---|---|---|
| `boot-and-identity` | boot → main menu; logo, background, button tree, submenus | frame |
| `ui-control-click` | click runtime buttons; verify via read-back | frame + node state |
| `enter-campaign` | Play → Campaign → planet → sector → launch | frame + state |
| `sector-preset-rules` | launch sector; assert preset rules | state JSON |
| `ground-zero-fresh-launch` | fresh Ground Zero: core/entity assertions | state JSON |
| `hud-wave-enemies-skip` | HUD wave counter, skip button, toasts | state JSON + frame |
| `rts-select-orders` / `placement-rotation` | command mode, select, order; place/rotate/break | state JSON + frame |
| `ground-zero-production-probe` | drill → core item delivery | state JSON |
| `pause-step-interact` | pause menu, step, API and mouse place/break | state JSON |
| `campaign-save-load` / `campaign-save-restore` | save, reload, equality | state JSON + frame |
| `editor-maps-row-click` / `custom-game-map-list` | editor/custom game map flows | frame + editor state |
| `settings-gamedata-actions` / `database-grid-audit` | settings dialogs, database grid | frame + state |
| `join-direct-connect` / `host-match-from-pause` | join/host flows | server rows + client state |

## 8. Known limits

- No OCR (`tesseract` absent): text parity is judged from structured state and
  bundle keys; screenshots stay metric-based.
- Java has no semantic API; every Java assertion is OS-level (click, key,
  window capture) or a screenshot metric.
- The two clients will never be byte-identical (font rasterization, shaders).
  The gate is the tolerance above, not equality; a local difference smaller
  than the gate is not a finding.
- If the session model cannot see images, judge from `frame_diff.py` metrics,
  the heatmap path, and structured state — and say the claim is metric-based.
