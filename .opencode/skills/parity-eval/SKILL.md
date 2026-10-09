---
name: parity-eval
description: >-
  Java↔Godot parity hunting for Mindustry-Godot: launch both clients on the
  shared display :10, seed the ledger by walking the live surface of both
  clients, fast-forward each item's navigation as one tool-call chain per leg,
  mirror only the action under test with capture/diff, and on the first
  mismatch stop, fix the Godot side, and document the item. Use when running
  the /loop or /goal parity process, seeding the coverage ledger, evaluating
  parity, auditing a system against upstream, or verifying a port fix
  in-engine.
---

# Skill: parity-eval — Java↔Godot parity hunting

One session drives both clients **concurrently on the same display** (`:10`).
The Java reference is operated at the OS level with computer-mcp; the Godot
client is driven through open-godot-mcp. Work comes in two iteration types:

- **Seed sweep** — explore both clients from a known state, record every
  interactive element and player action as a ledger item, and close the ones
  that already match (`verified.mode: "seed"`). Sweeps never fix.
- **Item run** — replay the item's navigation to its fork as one tool-call
  chain per leg (the fast-forward, not evidence), then mirror **only the action
  under test** step by step, capture both windows, diff the frames, and when a
  diff crosses the gate **stop, fix the Godot side, re-run, and record
  everything**.

The work is driven by the `@bybrawe/opencode-loop` (`/loop`) and
`@bybrawe/opencode-goal` (`/goal`) plugins. A goal or prompt-loop owns one
autonomous session; `parity-ledger.json` is its durable work queue and proof
record.

## Document map

| Path | Role |
|---|---|
| `parity-ledger.json` | Machine-readable queue + proof + coverage record: items, `path` fast-forwards, `focus` scope, `seeding.last_sweep`/`frontier`. Read before picking work; update after every step, mismatch, and fix. |
| `WORKFLOW_PROMPTS.md` | Canonical `/goal` and `/loop` paste-text (scope + hiccup rules). Keep in sync with this skill; the skill wins on conflict. |
| `chains/` | Canonical per-flow MCP call sequences with a `## Path (fast-forward)` section (navigation, not evidence) and `## Steps (under test)` (the mirrored part); schema in `chains/README.md`. Consult before composing calls; update/bump after a confirmed run. |
| `runs/<stamp>-<item>/` | Run artifacts: `java/`, `godot/`, `diff/`, `run.json`, `report.md`. Gitignored. |
| `computer-mcp-learnings.md`, `open-godot-mcp-learnings.md` | Friction + workarounds that are not chains. Append a dated entry when a run taught you something. |
| `parity/mcp_catalog.json`, `parity/scenario_catalog.json`, `parity/golden_manifest.json` | Committed oracle data (harness-owned). Do not duplicate it here. |
| `.opencode/skills/playtest/SKILL.md` | Godot MCP mechanics. Read it first; this skill adds the Java leg, the seed sweep, the path fast-forward, the comparison gate, and the documentation contract. |

**Anything a player does goes through a game client.** `spacetime call` never
counts as a gameplay check; the CLI is only for arranging and inspecting the
server side, exactly as the playtest skill describes.

## 0. Session setup

### Hiccups: log them when they happen (do this before Preconditions)

A **hiccup** is any friction that costs a retry: a tool error, a window that
opened maximized, input that did not land, a blank capture, a stale editor
binary, a bridge drop, a dialog path that moved. The moment it happens — before
continuing the run — append a dated entry to the matching learnings file:

- `computer-mcp-learnings.md` — Java leg, `window_ctl.py` / `capture_screen.py`,
  X display, pynput workarounds.
- `open-godot-mcp-learnings.md` — Godot editor/bridge, `godot_*` calls, frame
  loop, screenshots, node/eval traps.

Each entry: symptom, the exact failing tool call + params, the workaround that
worked (or the blocker if none), and the run dir or chain that shows it. One
entry per distinct issue per session; add a bullet to it if it recurs with more
detail. The end-of-item step only verifies nothing was missed — a hiccup that
exists only in the session log was not recorded. Never rewrite another
session's entry.

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
text through structured state (see §5).

### SSH / no-display sessions

`tools/parity-session.sh` prepares the workflow's hardcoded environment
(display `:10`, editor bridge `6970`, runtime state under the gitignored
`.opencode/parity-session/`): it starts Xvfb `:10`, optionally a headless
weston for the Godot editor (`--gpu-godot`), launches the editor and waits for
the bridge port. `--run` then execs opencode at the workspace root, and
`--stop` tears the session down. `--session N` (N >= 2) starts a parallel
session on the same host: display `:(9+N)`, bridge `6970+10(N-1)`, a worktree
`../Mindustry-Godot-sessionN` with seeded build caches and its own Java data
dir, and session-local MCP wiring (PATH shims plus an `opencode.json` project
override); the ledger and `runs/` are symlinked to the main checkout so claims
and reference frames are shared. Run it before a remote `/goal` or `/loop`
session; the Java client is still launched by the session itself (below).
`ssh` has no `DISPLAY`, and the computer-mcp MCP entry is pinned to
`DISPLAY=:10.0` in the global opencode config — keep session 1 on `:10` (the
other sessions override the MCP endpoints themselves).

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
  `DISPLAY=:10 "$MCP_PY" .opencode/skills/parity-eval/scripts/window_ctl.py unmaximize Mindustry`,
  then `... resize Mindustry 1152 648`, then `... move Mindustry 40 40`. A
  maximized window ignores `resize` — drop the WM state first. The embedded
  Godot game is fixed at 1152x648 — never try to resize it.
- Java saves/settings live in `parity-java-home/`; keep that directory stable
  so scenarios start from the same state. Delete its `saves/` only to test a
  fresh profile, and say so in `run.json`.
- Both windows may be maximized/overlapping; `godot_screenshot` reads the game
  viewport texture, but Java captures read the X window, so **raise it first**:

  ```bash
  DISPLAY=:10 "$MCP_PY" .opencode/skills/parity-eval/scripts/window_ctl.py raise Mindustry
  ```

## 1. Picking work

Read `parity-ledger.json` first; its top-level `focus` is the operator's scope
and outranks the default queue.

1. **Operator instruction** — "work on `Play > Campaign`", "do PAR-0012", "seed
   the editor" updates `focus` (when it is meant to persist), is stamped in
   `run.json`, and is followed immediately.
2. **Ledger `focus`** — resolve it against the items:
   - `kind: "item"` → run that `PAR-####` next.
   - `kind: "area"` | `"path"` → an item is in scope when `surface`, `title`,
     `chain`, `parent` lineage, or `path.to` matches the focus `target`/`path`
     tokens (case-insensitive). Run the first in-scope non-`verified` item.
   - No in-scope items exist yet → seed that area (§2): replay the focus fork's
     `path` first, sweep its surface, and create the items there.
3. **No focus** — seed sweep when `seeding.last_sweep` is `null` or the frontier
   has unrecorded surfaces (§2); otherwise the first non-`verified` item
   (`seeded`, `reference`, `in-progress`, `mismatch`, `fixing`, or `blocked`
   with a clear next check) in ledger order (§3). A `reference` item runs the
   Godot-only walk in §3b.

A named item or area always beats the global queue; the global queue resumes
when `focus` is `null`. One sweep surface or one item per iteration. Never
combine a fix and a fresh hunt step; commit a fix (`Fixes PAR-####`) before
moving on.

## 2. Seed sweep: the live frontier

A seed sweep turns "what is there to compare?" into ledger items backed by
fresh artifacts from both legs. It explores, classifies, and records — it does
**not** fix.

1. Reach the sweep's surface: boot both clients fresh and, when the sweep is
   scoped to a deeper fork (`focus`), replay the focus `path` (§3 step 3)
   before the first snapshot. Boot + focus replay is the `path` of every item
   the sweep discovers. Create `runs/<stamp>-seed-<area>/` with `seed/` for
   frame pairs and stamp the pids/commits in `run.json`.
2. Snapshot the current surface on both sides and record the pair
   (`seed/step-NN-<surface>-java.png`, `seed/step-NN-<surface>-godot.png`):
   - **Java**: capture the window; the capture *is* the enumeration — read the
     visible labels, buttons, rows and affordances from the image.
   - **Godot**: `godot_screenshot` plus a control-listing eval so every visible
     `Control` gets a node path and center (recipe in `chains/seed-survey.md`).
   `frame_diff.py` the pair under the §4 gate; that is the surface-render check.
3. Write one ledger item per interactive element/action found — in-game actions
   too (move, shoot, mine, place, drag, rotate, select, pause, skip wave), not
   just buttons. Each item gets a player-visible `title`, `surface`, `parent`
   (the fork item that revealed it), `path` (exact replay to this fork),
   `status: "seeded"`, a `chain` file (`chains/seed-survey.md` is fine until the
   item needs its own), and `discovered_by` = this sweep.
4. Classify each new element in the same sweep when it is cheap: act once per
   leg (Java first: `raise` → action → capture; Godot: action →
   `godot_screenshot`), `frame_diff.py` the pair, apply §4.
   - Pass → `status: "verified"`, `verified: {mode: "seed", date, run}`; the
     item is closed and skips the hunt queue.
   - Fail → `status: "mismatch"` with the step, both frames, the diff JSON and
     the exact calls from each leg. Leave the fix to the normal item run — a
     seed mismatch is already step-1 evidence.
   Return both clients to the fork before the next element (Java: Escape/Back;
   Godot: dialog close/Back) and confirm the fork state.
5. **Fork rule.** An action that reveals a new surface (Play → submenu;
   Campaign → planet page; sector → in-game HUD) queues that surface in
   `seeding.frontier` and appends its controls as items whose `path` extends the
   current one. No newly revealed fork is ever left unrecorded — in a sweep
   *or* a normal item run.
6. Bound the sweep to one surface (or one depth level) per iteration unless the
   operator says otherwise; deeper forks stay queued as `seeded` items with
   re-runnable paths. Finish by setting `seeding.last_sweep`, clearing the swept
   frontier entry, writing the run report, and committing docs only (never a
   code fix in a sweep).

## 2b. Reference soak (Java-only pre-load)

A **reference soak** records what the Java client does for one player action so
a later Godot run can be judged against **stored** evidence: one client per run,
no live second leg, no window juggling. It is the fast path into flows the port
does not implement yet. A soak records — it never fixes, diffs, or issues a
parity verdict.

Per action it produces three artifacts:

- reference frame(s) in `runs/<stamp>-ref-<area>/java/`;
- the re-runnable sequence (checkpoint -> fork -> action + settle waits) in
  `chains/java-ref-<area>.md`;
- one ledger item with a `reference` block and `status: "reference"`.

### 2b.1 Action table (Ground Zero basics)

The Ground Zero tutorial is an objective graph stored inside
`../Mindustry/core/assets/maps/serpulo/groundZero.msav` (bundle keys `gz.*`,
plus `hint.desktopMove` and `objective.*`). Rows in practical play order:

| # | action | expected Java state | oracle keys |
|---|---|---|---|
| R00 | fresh profile: Play > Campaign > Serpulo > OK > Launch | planet page -> loading -> tutorial start | `gz.mine`, `hint.desktopMove` |
| R01 | resume: Save & Quit after a checkpoint, then Play > Campaign > Ground Zero hex > Go | tutorial resumes at the current objective | objective HUD text |
| R02 | move (WASD) near the copper ore | hint `Use [WASD] to move.`; camera follows | `hint.desktopMove` |
| R03 | click the copper ore to begin mining | mining starts; `Obtain: 0/15 Copper` | objective 0 (`Item 15`, `gz.mine`) |
| R04 | tech tree -> research Mechanical Drill -> select it in the production menu -> place on copper | drill placed; objective clears | objective 1 (`BuildCount mechanical-drill`, `gz.research`) |
| R05 | research and place conveyors so the drill feeds the core | core receives 5 | objective 2 (`CoreItem 5`, `gz.conveyors`) |
| R06 | place more drills; mine 100 copper | core reaches 100 | objective 3 (`CoreItem 100`, `gz.drills`) |
| R07 | drill lead; mine 25 lead | core reaches 25 lead | objective 9 (`CoreItem lead 25`, `gz.lead`) |
| R08 | move the camera up to the defense area | camera pans north | `gz.moveup` marker on objective 6 |
| R09 | research and place 2 Duo turrets | 2 duos built | objective 6 (`BuildCount duo 2`, `gz.turrets`) |
| R10 | supply the Duos with copper (conveyors) | `duoAmmo` flag set | objective 15 (`Flag duoAmmo`, `gz.duoammo`, `gz.supplyturret`) |
| R11 | place 6 copper walls around the turrets | 6 walls built | objective 7 (`BuildCount copper-wall 6`, `gz.walls`) |
| R12 | wait out the wave timers: 300 s, 180 s, 180 s (zone3 spawns the wave) | wave announced/started | objectives 4/5/8 (`gz.zone1..3`) |
| R13 | after the wave, research and place a Scatter turret | scatter built | objective 11 (`BuildCount scatter`, `gz.aa`) |
| R14 | supply the Scatter with lead | `scatterAmmo` flag set | objective 16 (`Flag scatterAmmo`, `gz.scatterammo`) |
| R15 | defend: survive the assault | enemies destroyed | objectives 12/13 (`gz.defend`, `objective.enemiesapproaching`) |
| R16 | finish: keep defending into the capture timer | `allowWaves` timer runs | objective 14 (`gz.finish`) |

Amounts and parents come from the map's parsed objective graph (2026-10-09);
rows overlap in play, so a soak records the first practical pass in prompt
order. R00/R01 and the tutorial fork may already have items from a seed sweep
(PAR-0028/0029/0030/0031) — attach the `reference` block to those instead of
duplicating them.

### 2b.2 Recording rules

- **Java only.** Boot and drive the reference as in §0; the Godot client stays
  down. `capture_screen.py --window-title Mindustry` is the camera; raise the
  window before every capture.
- **One continuous client.** Boot once per soak and keep the state across
  iterations instead of replaying from boot per row. Reboot only when R00 needs
  a fresh profile (back up
  `parity-java-home/.local/share/Mindustry/saves/` first and say so in
  `run.json`).
- **Fast-forward in one call.** The prefix from the row's checkpoint to its
  fork is one `execute` call issuing all calls back-to-back — no per-step
  captures or diffs.
- **Wait inside the chain.** Unskippable animations need explicit waits between
  calls: the boot log gate (`until grep -q "Total time to load"`), a short
  `sleep` after each menu/dialog input (fades), a retake after Launch (loading
  bar), and chunked `sleep`s for the R12 timers (300/180/180 s). Long waits bank
  write-up time and poll in <=60 s chunks so no tool call has to hold for
  minutes. Record the wait that worked per row in the chain's `settle`.
- **Mid-animation captures are retaken, never recorded.** A frame captured
  before the objective/prompt settles is retaken after the recorded wait; a
  retake is not a finding.
- **Checkpoint after every action.** Best effort, in order: the pause menu's
  save slot; else copy the fresh
  `parity-java-home/.local/share/Mindustry/saves/*.msav` and verify its mtime
  changed; else fall back to replaying the full prefix next time. Record the
  surviving path in `reference.checkpoint`.
- **Fork rule.** Any interactive element seen in passing gets a ledger item
  before the row closes (§2 step 5).

### 2b.3 Item shape

```json
{
  "id": "PAR-00NN", "title": "Tutorial: mine copper by clicking the ore",
  "surface": "world", "parent": "PAR-00MM", "status": "reference",
  "chain": "chains/java-ref-ground-zero-tutorial.md",
  "discovered_by": "runs/<stamp>-ref-ground-zero/",
  "path": {"to": "Ground Zero tutorial, objective 'Obtain 0/15 Copper'",
           "java": ["<checkpoint + fork calls>"], "godot": [],
           "confirm": {"java": "java/step-03-fork.png"}},
  "reference": {
    "captured": "<ISO8601>", "profile": "fresh|resumed",
    "run": "runs/<stamp>-ref-ground-zero/",
    "frames": {"fork": "java/step-03-fork.png", "action": "java/step-03-mine.png"},
    "settle": [{"after": "click ore", "wait_ms": 500}],
    "expected": {"bundle": ["gz.mine"], "objective": "Obtain: 0/15 Copper"},
    "chain": "chains/java-ref-ground-zero-tutorial.md",
    "checkpoint": "parity-java-home/.local/share/Mindustry/saves/<file>.msav"
  },
  "reproduce": {"java": ["<action calls>", "capture_screen.py ..."],
                "godot": [], "compare": []},
  "steps": [], "mismatch": null, "fix": {"status": "open"}, "verified": null
}
```

`status: "reference"` means Java evidence recorded, Godot walk still open. It
is not a verdict: §3b either closes the item as `verified.mode: "reference"` or
opens a mismatch with a fresh Java pair.

### 2b.4 Parallelism

One agent is the default and stays fast because the Godot leg is absent; the
mutable resource is the single Java client, not the model, and the tutorial is
linear — a second agent only helps if it can start from a checkpoint of the
first agent's state. If the operator runs one, it must have its own
loop/display/data dir (`../.opencode/loops/bin/start-loop.sh N --run` from the
workspace root: own display, bridge, worktree, Java data dir), start from a
copy of the checkpoint that precedes its first row, and own a disjoint row
range; the two serialize `parity-ledger.json` writes with an exclusive `flock`
on `parity-ledger.json.lock` (re-read, apply, write, release). Never run two
clients on one display or one data dir.

### 2b.5 Hiccups

§0 applies unchanged: a click that only focused the window, a dropped key, a
maximized window, a blank capture, a field that drops input, a save slot that
never lands — one dated entry in `computer-mcp-learnings.md` immediately, and
the wait that fixed it into the chain.

## 3. Item run: fast-forward, then mirror

Pick the first non-`verified` item in `parity-ledger.json` (or the item the
goal names). For that item:

1. Read its chain in `chains/` and the ledger entry; note `path` and `steps`.
2. Reset both clients to the same boot state (restart or return both to the
   main menu) and stamp the pids/commits in `run.json`.
3. **Fast-forward to the fork.** Replay the item's `path` once per leg as a
   single tool-call chain — one `execute` call per leg issuing all of that
   leg's calls in order. Do **not** capture or diff navigation steps; a wrong
   turn is a path bug to fix in the chain, not a parity mismatch. Confirm the
   fork once per leg (Godot: the structured check in `path.confirm`; Java: one
   capture), then start `steps` with both clients at the same fork.
4. **Mirror the action under test.** For each step of `steps`, alternately:
   - **Java**: `raise Mindustry` → perform the action with `computer-mcp_*` →
     `capture_screen.py --window-title Mindustry` → `runs/<run>/java/step-NN.png`.
   - **Godot**: perform the equivalent action with `godot_*` → capture the game
     with `godot_screenshot` (or the viewport) → `runs/<run>/godot/step-NN.png`.
   - **Compare**: `frame_diff.py` the two captures → `runs/<run>/diff/step-NN.json`;
     apply the gate in §4.
5. All steps match → mark the item `verified` (`mode: "chain"`), bump the chain
   (`last_verified`, steps, and its path if navigation proved fragile), continue
   to the next item.
6. First mismatch → **stop the item run** and switch to fix mode (§5).

### 3b. Reference-backed run (Godot only)

When the picked item carries a `reference` block (`status: "reference"`), the
Java side is the recorded reference instead of a live client:

1. Keep the Godot client as in step 2; the Java client stays down. Stamp the
   Godot pid/commit in `run.json` and verify `reference.frames.action` exists
   and is non-blank.
2. Fast-forward the item's **Godot** path to the same fork and mirror the action
   as in step 4. Compare with §4 using `reference.frames.action` as the Java
   side.
3. Pass → `verified: {mode: "reference", run: <this run>, reference: <ref run>}`
   and bump the chain's `last_verified`. Record the step with `java_frame`
   pointing at `<reference.run>/<reference.frames.action>` and the fresh
   `godot_frame`/`diff`.
4. Fail → re-run the Java leg once from `reference.checkpoint` and recapture the
   same frame. If the fresh Java frame reproduces the reference, treat the Godot
   leg as the mismatch (§5) with the fresh pair; if it does not, refresh the
   reference item (frame, waits) and retry — never file a mismatch against a
   stale reference.

Hard rules:

- **The path is not evidence.** Navigation replays back-to-back and is never
  diffed step by step. Only `steps` at the fork are mirrored one action pair at
  a time; the failure must be reproducible at a named step there.
- Several items can share a fork: each item still stores its own exact `path`
  calls and replays them independently. Never assume a client is at a fork
  because a previous item left it there.
- Only compare stable frames. Menus/dialogs are naturally stable. In-game
  scenes are not: pause the Java client and `SimHost.set_paused(true)` the
  Godot client (or `step()` both to the same tick) before capturing. Never diff
  mid-animation.
- If a capture comes back blank (metrics `blank: true`) or the action clearly
  did not land, re-take it — do not classify a capture failure as a parity
  mismatch. Log the retake in `run.json`.
- The tool response of a Java `click`/`mouse_move`/`key_press` shows the
  previous frame; always capture separately before judging what it did.
- Every run also seeds: when the fork reveals interactive surface with no
  ledger item, append the items (see §2 step 5) before moving past it.

## 4. Comparison gate

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

A reference-backed run is evidence class 4 with recorded Java provenance: the
Java side is `reference.frames.action` from `reference.run` (window capture at
1152x648), the Godot side is the fresh capture. Name both paths in the run
report and the diff JSON so the comparison stays reproducible. Do not mix a
reference pass into `chain` runs: if a live Java frame is captured, the item is
a normal §3 run.

## 5. Mismatch → fix → verify

On the first mismatch:

1. **Record the stop point in `parity-ledger.json` before touching code**: step
   number, the two screenshots, the diff JSON, the metric values, and the exact
   MCP call sequences for both legs up to that step (copy from the run log; the
   chain has the canonical full sequence). A mismatch recorded by a seed sweep
   already has this evidence: reproduce it once (path replay + the failing
   step) so the fix starts from a fresh stop, then treat the item like any
   other.
2. Diagnose on the Godot side with the playtest skill (structured state first:
   checksum, `get_state_json`, HUD JSON, dialog nodes). If pixels disagree but
   structured state agrees, the finding is presentation-layer; if state
   disagrees, find the sim/UI seam.
3. Implement the fix in `client/` (GDScript) or `client/rust/` (Rust).
4. Run the narrow check (`cargo test -p mind-core ...`, `cargo clippy`,
   `tools/build.sh` as applicable).
5. After `tools/build.sh`, **relaunch the editor** — a running editor has the
   old `libmind_gdext.so` mmap'd.
6. Re-run the item from the top — `path` replay (fast-forward), then the full
   `steps` under the gate. The item becomes `verified` (`mode: "chain"`) only
   when the whole chain passes; a fixed step that reveals the next mismatch
   stays in the same item (record the new stop point).
7. Update the docs (§6) and commit: code fix first (`Fixes <PAR-ID>`), docs in
   a separate commit.

A code reading can seed a candidate or settle one as `wontfix`, but only a
fresh in-engine repro can mark an item fixed.

## 6. Documentation contract

### `parity-ledger.json`

The single source of progress. Shape (one `items[]` entry per item):

```json
{
  "id": "PAR-0002", "title": "player-visible symptom", "status": "verified",
  "surface": "menu", "parent": "PAR-0001",
  "chain": "chains/enter-campaign.md", "updated": "2026-10-09T12:00:00Z",
  "discovered_by": "runs/20261009-110000-seed-menu/",
  "path": {
    "to": "fresh main menu on both clients",
    "java": [{"tool": "computer-mcp_click", "params": {"x": 310, "y": 216, "button": "left"}}],
    "godot": [{"tool": "godot_input", "params": {"action": "mouse_button", "params": {"button": "MOUSE_BUTTON_LEFT", "position": {"x": 0, "y": 0}, "coords": "viewport", "pressed": true}}}],
    "confirm": {"godot": "MenuGroup.visible and DialogLayer empty", "java": "seed/step-01-menu-java.png"},
    "last_replayed": "runs/20261009-120000-enter-campaign/"
  },
  "run": "runs/20261009-120000-enter-campaign/",
  "reproduce": {
    "java": [{"tool": "computer-mcp_click", "params": {"x": 310, "y": 216, "button": "left"}},
             {"tool": "capture_screen.py", "params": {"--window-title": "Mindustry"}}],
    "godot": [{"tool": "godot_input", "params": {"action": "mouse_button", "params": {"button": "MOUSE_BUTTON_LEFT"}}},
              {"tool": "godot_screenshot", "params": {"action": "game"}}],
    "compare": [{"tool": "frame_diff.py", "params": {"--java": "java/step-01.png", "--godot": "godot/step-01.png", "--out-json": "diff/step-01.json"}}]
  },
  "steps": [{"n": 1, "action": "click Play", "java_frame": "java/step-01.png", "godot_frame": "godot/step-01.png", "diff": "diff/step-01.json", "verdict": "match"}],
  "mismatch": {"step": 1, "signals": {"changed_fraction": 0.21},
               "java_frame": "java/step-01.png", "godot_frame": "godot/step-01.png"},
  "fix": {"status": "closed", "commit": null, "summary": null},
  "verified": {"mode": "chain", "date": "2026-10-09T12:00:00Z", "run": "runs/20261009-120000-enter-campaign/"}
}
```

Update `updated`, `path.last_replayed`, `steps`, `mismatch`, `fix`, `verified`
after every run; a seed sweep also maintains `seeding.last_sweep`/`frontier`.
Append new items with the next `PAR-####` id and set `parent`/`surface`/
`discovered_by`. Statuses: `seeded` (discovered or compiled, no fresh matched
run), `reference` (Java evidence recorded by a soak, §2b; the Godot walk is
still open), `in-progress`, `mismatch`, `fixing`, `verified` (closed —
`verified.mode` is `seed` for a sweep observation, `chain` for a full both-leg
pass, `reference` for a Godot-only pass against a stored Java frame, or `twin`
for a fresh Java+Godot pair captured by the twin-verifier subagent, the only
mode that marks a reference item settled), `blocked`, `wontfix`.
A `verified.mode: "seed"` item may be reopened if a later chain run contradicts
it. Its `chain` may point at `chains/seed-survey.md` until the item is re-run,
fixed, or needs item-specific preconditions, at which point write its own file.

### Chains

After a confirmed run, update (or create) the matching `chains/<name>.md`
following `chains/README.md`: exact tool + action + parameter JSON per step, no
hardcoded click coordinates unless measured from the current capture, status
`verified` only with a run dir + pid/commit in `last_verified`. A chain splits
into `## Path (fast-forward)` — navigation, replayed back-to-back per leg, not
evidence — and `## Steps (under test)` — the mirrored part behind the gate.
Java-leg sequences go in `java-*.md` files (soak sequences are
`java-ref-*.md`); a mixed item names both servers in `tools`.

### Learnings

Friction that is not a reusable chain goes to the repo-root learnings files as
one dated entry, written **the moment the hiccup happens** (§0): what happened,
the exact failing call, what worked, the run or chain that shows it. Java/X-leg
friction in `computer-mcp-learnings.md`; Godot/bridge friction in
`open-godot-mcp-learnings.md`. Never rewrite another session's entry.

### Run report

`runs/<stamp>-<item>/report.md`:

```markdown
# <item> — <date> (<godot commit>, Java <upstream short sha>)

- Result: <parity | mismatch | blocked | seeded>
- Setup: display :10, windows <java 1152x648 @ x,y> <godot embedded 1152x648>, settings snapshot ref

## Path
- replayed: <yes/no>; fork: <state>; confirm: <godot check / java capture>

## Steps
| # | action | java frame | godot frame | diff json | verdict |

## Mismatch
- step: <n>; signals: <metrics>; heatmap: <path>

## Fix
- commit: <sha>; check: <command>; re-run: <run dir>

## Raw artifacts
- java/: ... godot/: ... diff/: ...
```

## 7. Java-leg gotchas (computer-mcp)

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

## 8. Godot-leg gotchas (open-godot-mcp)

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

## 9. Scenario queue

Growing coverage; `parity/mcp_catalog.json` wins where ids overlap. Seed live
surface with `seed-survey`; reuse the matching chain where one exists.

| id | goal | primary oracle |
|---|---|---|
| `seed-survey` | boot both clients → enumerate a surface → one item per element → classify with one action pair | frame + node list |
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

## 10. Known limits

- No OCR (`tesseract` absent): text parity is judged from structured state and
  bundle keys; screenshots stay metric-based.
- Java has no semantic API; every Java assertion is OS-level (click, key,
  window capture) or a screenshot metric.
- The two clients will never be byte-identical (font rasterization, shaders).
  The gate is the tolerance above, not equality; a local difference smaller
  than the gate is not a finding.
- If the session model cannot see images, judge from `frame_diff.py` metrics,
  the heatmap path, and structured state — and say the claim is metric-based.
  A seed sweep then cannot read the Java enumeration visually: fall back to the
  Java source for element labels and to hover-highlight frame probes for
  coordinates (or run `seed-survey` with a vision-capable session), and record
  the enumeration method in the run report.
