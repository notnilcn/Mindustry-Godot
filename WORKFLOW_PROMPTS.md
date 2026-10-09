# Workflow prompts — parity hunt

Canonical paste-text for the `@bybrawe/opencode-goal` (`/goal`) and
`@bybrawe/opencode-loop` (`/loop`) sessions. The durable spec is
`.opencode/skills/parity-eval/SKILL.md`, the queue/proof record is
`parity-ledger.json`, and the re-runnable sequences are in `chains/`. If a
prompt below and the skill disagree, the skill wins — update this file with it.

Run from the workspace root (`code_examples/`), or from `Mindustry-Godot/` with
the `Mindustry-Godot/` path prefix dropped.

Over SSH (no display), first run `Mindustry-Godot/tools/parity-session.sh`
(add `--gpu-godot` for a Wayland/GL editor; `--run` starts opencode for you).
It brings up display `:10` plus the editor bridge `6970`, prints the session
summary, and `--stop` tears it down — then the prompts below work unchanged.

Two rules the prompts carry: **scope** — the ledger's top-level `focus` wins
over the global queue; and **hiccups** — friction is written to the matching
learnings file the moment it happens, not at the end of the run.

## /goal — the parity hunt (primary). Run it from this workspace root so Mindustry-Godot/... paths resolve; if you start inside Mindustry-Godot, drop that prefix:

```
/goal Reach 1:1 parity between the Java Mindustry reference and the Godot port. The port is Mindustry-Godot/, the reference is Mindustry/. Before acting, load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md) and the playtest skill; they are the operating manual. The work queue and proof record is Mindustry-Godot/parity-ledger.json. Exact MCP call sequences for both legs live in Mindustry-Godot/chains/ (schema in its README).

Scope: parity-ledger.json's top-level "focus" wins over the default queue. A focus item runs that PAR-####; an area/path focus runs the first non-verified item whose surface/title/chain/parent lineage/path.to matches the focus target/path tokens, or seeds that area from its fork when no items exist yet. If the operator names a new item/area, update "focus" first (null resumes the global queue) and stamp it in run.json.

Each iteration:
0. Hiccup rule: the moment anything costs a retry (tool error, maximized window, blank capture, input that did not land, stale editor binary, bridge drop, moved dialog path), append a dated entry to Mindustry-Godot/computer-mcp-learnings.md (Java/X leg) or Mindustry-Godot/open-godot-mcp-learnings.md (Godot/bridge leg) — symptom, exact failing call, workaround, run dir — before continuing.
1. Seed when needed: if parity-ledger.json has no seed sweep (seeding.last_sweep is null) or the focus area has no items, sweep one surface first (skill §2): boot both clients on :10, fast-forward to the focus fork (one tool-call chain per leg), snapshot the surface pair, enumerate the interactive elements on both sides, append one ledger item per element/action (surface, parent, path, discovered_by), then classify each with one action per leg + frame_diff gate — match → verified (verified.mode "seed"), divergence → mismatch with both frames and exact calls. Return both clients to the fork between elements; never fix during a sweep.
2. Pick the first non-verified item in scope (focus wins; otherwise the first non-verified item in parity-ledger.json). Read its chain.
3. Bring both clients up concurrently on display :10 (Java via computer-mcp; Godot editor + bridge 6970 via open-godot-mcp), normalize the Java window (window_ctl.py unmaximize Mindustry, then resize 1152 648), stamp pids/commits in Mindustry-Godot/runs/<stamp>-<item>/run.json.
4. Fast-forward: replay the item's path (navigation to the fork) as a single MCP tool call chain per leg — one execute call per leg, no per-step captures or diffs — then confirm the fork once per leg (Godot structured check, Java one capture). Only from the fork onward is the item mirrored action by action: Java action with computer-mcp_*, raise the Java window (window_ctl.py raise Mindustry), capture with capture_screen.py; equivalent Godot action with godot_* and capture with godot_screenshot; diff each pair with frame_diff.py into runs/<run>/diff/.
5. Match gate: size_match, changed_fraction <= 0.005, each mean_abs_rgb <= 12, coarse_layout.max_channel_mean <= 6, palette_distance <= 0.05, no blank frames. Calibrate only with recorded evidence.
6. First mismatch: STOP the run. Record the step, both frames, the diff JSON and metrics, and the exact reproduction tool calls for both legs in the ledger item. Then edit the Godot side, run the narrow check, tools/build.sh, relaunch the editor, and re-run the same chain from the start (path replay + full steps). An item is verified only when the whole chain passes.
7. Whenever a fork reveals new interactive surface, append one item per control/action (parent, surface, path, discovered_by) before moving past it.
8. After every item: update parity-ledger.json; update or create the chain with both legs' exact calls and last_verified; check the learnings files carry every hiccup of this run (log any missed one now); write runs/<run>/report.md; commit the code fix as "Fixes PAR-####" and the docs in a separate commit.

Constraints: gameplay only through the game clients (never spacetime call for gameplay); no verdict without fresh artifacts from both legs; one mirrored action per step once at the fork (the path replay is the only batching); one item at a time; if blocked (client will not launch, bridge down, blank captures after retries) mark the item blocked with the failing tool call and move to the next item.

Success: every ledger item is verified or explicitly wontfix/blocked with evidence, and chains/ plus the learnings files carry every sequence and workaround.

--continuous
--name parity-hunt
--constraint "Gameplay input only through the game clients; the spacetime CLI never counts as a gameplay check"
--constraint "No status change without fresh run artifacts from both legs"
--constraint "Log every hiccup to the matching learnings file the moment it happens; a hiccup that lives only in the session log is unrecorded"
--constraint "The ledger focus wins over the global queue until it is cleared"
--constraint "Never widen the comparison gate silently; record calibration evidence in the item"
--constraint "Commit a code fix only after the narrow check and the re-run pass"
--success "Every parity-ledger.json item is verified or explicitly wontfix/blocked with evidence"
--success "chains/ contains an exact re-runnable sequence for each verified item, and every discovered interactive element has a ledger item"
```

## /loop — idle-driven form

Run this instead of the goal, in its own session (a prompt-loop is blocked while
a goal is active in the same session).

```
/loop --name parity-hunt --ask-never --safe --no-overlap --max-failures 3 Each idle turn: do one iteration of the Mindustry-Godot parity hunt. Load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md) and follow Mindustry-Godot/parity-ledger.json; its "focus" wins over the global queue (an item focus runs that PAR-####; an area/path focus runs the first non-verified item matching it, or seeds that area from its fork when none exist yet; null = global queue). Bring both clients up on display :10 (Java via computer-mcp, Godot via open-godot-mcp bridge 6970), normalizing the Java window (window_ctl.py unmaximize Mindustry, then resize 1152 648). Hiccup rule: the moment anything costs a retry (tool error, maximized window, blank capture, dropped input, stale binary, bridge drop), append a dated entry to Mindustry-Godot/computer-mcp-learnings.md or Mindustry-Godot/open-godot-mcp-learnings.md — failing call, workaround, run dir — before continuing. First seed if needed: when seeding.last_sweep is null or the focus area has no items, sweep one surface — fast-forward to the focus fork, snapshot both, enumerate the live controls, append one item per element/action (surface, parent, path, discovered_by), classify each with one action per leg + frame_diff gate (match → verified.mode seed; divergence → mismatch with both frames and calls) — then pick the first non-verified item in scope, read its chain in Mindustry-Godot/chains/, and run it: replay its path (navigation to the fork) as one MCP tool call chain per leg — no per-step captures or diffs — confirm the fork once per leg, then mirror only the actions after the fork one at a time: Java action + capture_screen.py, equivalent Godot action + godot_screenshot, frame_diff.py with the gate from the skill. On the first mismatch, stop, record both frames + diff + exact tool calls in the ledger item, fix the Godot side, rebuild (tools/build.sh), relaunch the editor, re-run the chain (path replay + full steps), then update the ledger, the chain, the learnings files and runs/<run>/report.md, and commit ("Fixes PAR-####" for code, docs separately). Append an item per newly revealed control before moving past any fork. If blocked, record why and move on. Gameplay only through the clients; never claim a result without fresh artifacts from both legs.
```

## Mid-session nudge — retarget an already-running session

Paste when a live session needs the new rules without a restart (works for
`/goal` and `/loop`):

```
Workflow update — re-read Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md (§0 Hiccups, §1 focus) and Mindustry-Godot/parity-ledger.json before the next iteration.
1. Hiccups: the moment any friction costs a retry (e.g. the Java window opening maximized), append a dated entry to Mindustry-Godot/computer-mcp-learnings.md or Mindustry-Godot/open-godot-mcp-learnings.md with the exact failing call, the workaround and the run dir, then continue. Do not wait until the end of the item.
2. Scope: work only on items under parity-ledger.json's "focus". If none exist yet, seed that area first: boot both, fast-forward to the focus fork as one tool-call chain per leg, enumerate both sides, append one item per element/action (surface, parent, path, discovered_by), and classify each with one action pair + frame_diff gate (match → verified.mode "seed"; divergence → mismatch with both frames and calls). Then run the first non-verified in-scope item.
3. Maximized Java window: window_ctl.py unmaximize Mindustry, then resize 1152 648, then raise.
```
