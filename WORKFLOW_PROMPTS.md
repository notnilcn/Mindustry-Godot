# Workflow prompts — parity hunt

Canonical paste-text for the `@bybrawe/opencode-goal` (`/goal`) and
`@bybrawe/opencode-loop` (`/loop`) sessions. The durable spec is
`.opencode/skills/parity-eval/SKILL.md`, the queue/proof record is
`parity-ledger.json`, and the re-runnable sequences are in `chains/`. If a
prompt below and the skill disagree, the skill wins — update this file with it.

Run from the workspace root (`code_examples/`), or from `Mindustry-Godot/` with
the `Mindustry-Godot/` path prefix dropped.

## /goal — the parity hunt (primary)

```
/goal — the parity hunt (primary). Run it from this workspace root so Mindustry-Godot/... paths resolve; if you start inside Mindustry-Godot, drop that prefix:
/goal Reach 1:1 parity between the Java Mindustry reference and the Godot port. The port is Mindustry-Godot/, the reference is Mindustry/. Before acting, load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md) and the playtest skill; they are the operating manual. The work queue and proof record is Mindustry-Godot/parity-ledger.json. Exact MCP call sequences for both legs live in Mindustry-Godot/chains/ (schema in its README).

Each iteration:
0. Seed when needed: if parity-ledger.json has no seed sweep (seeding.last_sweep is null) or the frontier has unswept surfaces, sweep one surface first (skill §2): boot both clients on :10, snapshot the surface pair, enumerate the interactive elements on both sides, append one ledger item per element/action (surface, parent, path, discovered_by), then classify each with one action per leg + frame_diff gate — match → verified (verified.mode "seed"), divergence → mismatch with both frames and exact calls. Return both clients to the fork between elements; never fix during a sweep.
1. Pick the first non-verified item in parity-ledger.json. Read its chain.
2. Bring both clients up concurrently on display :10 (Java via computer-mcp; Godot editor + bridge 6970 via open-godot-mcp), normalize the Java window to 1152x648, stamp pids/commits in Mindustry-Godot/runs/<stamp>-<item>/run.json.
3. Fast-forward: replay the item's path (navigation to the fork) as a single MCP tool call chain per leg — one execute call per leg, no per-step captures or diffs — then confirm the fork once per leg (Godot structured check, Java one capture). Only from the fork onward is the item mirrored action by action: Java action with computer-mcp_*, raise the Java window (window_ctl.py raise Mindustry), capture with capture_screen.py; equivalent Godot action with godot_* and capture with godot_screenshot; diff each pair with frame_diff.py into runs/<run>/diff/.
4. Match gate: size_match, changed_fraction <= 0.005, each mean_abs_rgb <= 12, coarse_layout.max_channel_mean <= 6, palette_distance <= 0.05, no blank frames. Calibrate only with recorded evidence.
5. First mismatch: STOP the sweep. Record the step, both frames, the diff JSON and metrics, and the exact reproduction tool calls for both legs in the ledger item. Then edit the Godot side, run the narrow check, tools/build.sh, relaunch the editor, and re-run the same chain from the start (path replay + full steps). An item is verified only when the whole chain passes.
6. Whenever a fork reveals new interactive surface, append one item per control/action (parent, surface, path, discovered_by) before moving past it.
7. After every item: update parity-ledger.json; update or create the chain with both legs' exact calls and last_verified; append any new friction as a dated entry in computer-mcp-learnings.md or open-godot-mcp-learnings.md; write runs/<run>/report.md; commit the code fix as "Fixes PAR-####" and the docs in a separate commit.

Constraints: gameplay only through the game clients (never spacetime call for gameplay); no verdict without fresh artifacts from both legs; one mirrored action per step once at the fork (the path replay is the only batching); one item at a time; if blocked (client will not launch, bridge down, blank captures after retries) mark the item blocked with the failing tool call and move to the next item.

Success: every ledger item is verified or explicitly wontfix/blocked with evidence, and chains/ plus the learnings files carry every sequence and workaround.

--continuous
--name parity-hunt
--constraint "Gameplay input only through the game clients; the spacetime CLI never counts as a gameplay check"
--constraint "No status change without fresh run artifacts from both legs"
--constraint "Never widen the comparison gate silently; record calibration evidence in the item"
--constraint "Commit a code fix only after the narrow check and the re-run pass"
--success "Every parity-ledger.json item is verified or explicitly wontfix/blocked with evidence"
--success "chains/ contains an exact re-runnable sequence for each verified item, and every discovered interactive element has a ledger item"
```

## /loop — idle-driven form

Run this instead of the goal, in its own session (a prompt-loop is blocked while
a goal is active in the same session).

```
/loop --name parity-hunt --ask-never --safe --no-overlap --max-failures 3 Each idle turn: do one iteration of the Mindustry-Godot parity hunt. Load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md) and follow Mindustry-Godot/parity-ledger.json. Bring both clients up on display :10 (Java via computer-mcp, Godot via open-godot-mcp bridge 6970). First seed if needed: when seeding.last_sweep is null or the frontier has unswept surfaces, sweep one surface — snapshot both, enumerate the live controls, append one item per element/action (surface, parent, path, discovered_by), classify each with one action per leg + frame_diff gate (match → verified.mode seed; divergence → mismatch with both frames and calls) — then pick the first non-verified ledger item, read its chain in Mindustry-Godot/chains/, and run it: replay its path (navigation to the fork) as one MCP tool call chain per leg — no per-step captures or diffs — confirm the fork once per leg, then mirror only the actions after the fork one at a time: Java action + capture_screen.py, equivalent Godot action + godot_screenshot, frame_diff.py with the gate from the skill. On the first mismatch, stop, record both frames + diff + exact tool calls in the ledger item, fix the Godot side, rebuild (tools/build.sh), relaunch the editor, re-run the chain (path replay + full steps), then update the ledger, the chain, the learnings files and runs/<run>/report.md, and commit ("Fixes PAR-####" for code, docs separately). Append an item per newly revealed control before moving past any fork. If blocked, record why and move on. Gameplay only through the clients; never claim a result without fresh artifacts from both legs.
```
