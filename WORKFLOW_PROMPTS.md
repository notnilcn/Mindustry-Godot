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
`--session N` (N >= 2) starts a parallel session on the same host: display
`:(9+N)`, bridge `6970+10(N-1)`, a git worktree `Mindustry-Godot-sessionN`
with its own build caches and Java data dir, and session-local MCP wiring; the
ledger and `runs/` stay shared with the main checkout through symlinks, so
claims and reference frames are visible to every session.

Two rules the prompts carry: **scope** — the ledger's top-level `focus` wins
over the global queue; and **hiccups** — friction is written to the matching
learnings file the moment it happens, not at the end of the run.

The **reference soak** prompt records the Java side of the basic in-game
actions once (skill §2b); the primary hunt then walks those items Godot-only
(§3b), so the slow both-legs path stays for items without reference evidence.
The **godot walk** prompt goes further: each reference item is recreated in
Godot, planned and fixed in the ledger, chained, and settled by a spawned
twin-verifier subagent that re-runs both legs fresh.

## /goal — the parity hunt (primary). Run it from this workspace root so Mindustry-Godot/... paths resolve; if you start inside Mindustry-Godot, drop that prefix:

```
/goal Reach 1:1 parity between the Java Mindustry reference and the Godot port. The port is Mindustry-Godot/, the reference is Mindustry/. Before acting, load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md) and the playtest skill; they are the operating manual. The work queue and proof record is Mindustry-Godot/parity-ledger.json. Exact MCP call sequences for both legs live in Mindustry-Godot/chains/ (schema in its README).

Scope: parity-ledger.json's top-level "focus" wins over the default queue. A focus item runs that PAR-####; an area/path focus runs the first non-verified item whose surface/title/chain/parent lineage/path.to matches the focus target/path tokens, or seeds that area from its fork when no items exist yet. If the operator names a new item/area, update "focus" first (null resumes the global queue) and stamp it in run.json.

Each iteration:
0. Hiccup rule: the moment anything costs a retry (tool error, maximized window, blank capture, input that did not land, stale editor binary, bridge drop, moved dialog path), append a dated entry to Mindustry-Godot/computer-mcp-learnings.md (Java/X leg) or Mindustry-Godot/open-godot-mcp-learnings.md (Godot/bridge leg) — symptom, exact failing call, workaround, run dir — before continuing.
1. Seed when needed: if parity-ledger.json has no seed sweep (seeding.last_sweep is null) or the focus area has no items, sweep one surface first (skill §2): boot both clients on :10, fast-forward to the focus fork (one tool-call chain per leg), snapshot the surface pair, enumerate the interactive elements on both sides, append one ledger item per element/action (surface, parent, path, discovered_by), then classify each with one action per leg + frame_diff gate — match → verified (verified.mode "seed"), divergence → mismatch with both frames and exact calls. Return both clients to the fork between elements; never fix during a sweep.
2. Pick the first non-verified item in scope (focus wins; otherwise the first non-verified item in parity-ledger.json). Read its chain.
3. Bring both clients up concurrently on display :10 (Java via computer-mcp; Godot editor + bridge 6970 via open-godot-mcp), normalize the Java window (window_ctl.py unmaximize Mindustry, then resize 1152 648), stamp pids/commits in Mindustry-Godot/runs/<stamp>-<item>/run.json. An item with a `reference` block needs only the Godot client (skill §3b).
4. Fast-forward: replay the item's path (navigation to the fork) as a single MCP tool call chain per leg — one execute call per leg, no per-step captures or diffs — then confirm the fork once per leg (Godot structured check, Java one capture). Only from the fork onward is the item mirrored action by action: Java action with computer-mcp_*, raise the Java window (window_ctl.py raise Mindustry), capture with capture_screen.py; equivalent Godot action with godot_* and capture with godot_screenshot; diff each pair with frame_diff.py into runs/<run>/diff/. For a reference-backed item the Java side of the diff is `reference.frames.action` from `reference.run`.
5. Match gate: size_match, changed_fraction <= 0.005, each mean_abs_rgb <= 12, coarse_layout.max_channel_mean <= 6, palette_distance <= 0.05, no blank frames. Calibrate only with recorded evidence.
6. First mismatch: STOP the run. Record the step, both frames, the diff JSON and metrics, and the exact reproduction tool calls for both legs in the ledger item. Then edit the Godot side, run the narrow check, tools/build.sh, relaunch the editor, and re-run the same chain from the start (path replay + full steps). An item is verified only when the whole chain passes.
7. Whenever a fork reveals new interactive surface, append one item per control/action (parent, surface, path, discovered_by) before moving past it.
8. After every item: update parity-ledger.json; update or create the chain with both legs' exact calls and last_verified; check the learnings files carry every hiccup of this run (log any missed one now); write runs/<run>/report.md; commit the code fix as "Fixes PAR-####" and the docs in a separate commit.

Constraints: gameplay only through the game clients (never spacetime call for gameplay); no verdict without fresh artifacts from both legs; one mirrored action per step once at the fork (the path replay is the only batching); one item at a time; if blocked (client will not launch, bridge down, blank captures after retries) mark the item blocked with the failing tool call and move to the next item. A reference-backed item may close as verified.mode "reference" against its stored Java frame (skill §3b); a code fix still needs a fresh Java pair.

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

## /goal — reference soak (fast Java pre-load of the Godot basics)

Run this in its own session (goal locks are exclusive; not while the parity
hunt goal is active). It records the Java side of the basic in-game actions
once, so the hunt's reference-backed runs (§3b) need only the Godot client.
Stop it once every row of skill §2b.1 has reference evidence; it produces no
verdicts.

```
/goal Pre-load Mindustry-Godot/parity-ledger.json with Java-only reference items for the basic in-game actions, so later Godot runs can fast-forward against stored evidence instead of a live Java leg. The reference is Mindustry/; the port is Mindustry-Godot/. Before acting, load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md): its §2b is the recording protocol and its action table (R00-R16) is the work queue; Mindustry-Godot/chains/java-ref-ground-zero-tutorial.md carries the boot recipe. The Godot client and all Godot code are out of scope for this goal, and this goal produces no parity verdicts.

Each iteration:
0. Keep one Java client for the whole soak. If none is up, launch the reference (skill §0): display :10, wait for "Total time to load", unmaximize/resize to 1152x648, raise before every capture, stamp pid/profile in Mindustry-Godot/runs/<stamp>-ref-ground-zero/run.json. Do not reboot between rows; reboot only when R00 needs a fresh profile (back up parity-java-home/.local/share/Mindustry/saves/ first and say so in run.json).
1. Pick the next skill §2b.1 row (R00..R16) without reference evidence; rows already covered by an existing item get their reference block attached instead of a duplicate.
2. Fast-forward: replay the row's prefix from its checkpoint as one execute call issuing every computer-mcp/shell call back-to-back, with explicit waits for the unskippable animations in between (boot log gate, dialog fades, loading bar, prompt transitions; the R12 wave timers 300/180/180 s run in <=60 s sleep chunks). No per-step captures or diffs.
3. Action under test, once: perform it with computer-mcp_*; wait for the objective/prompt to settle (sleep, then capture; a mid-animation frame is retaken, never recorded); capture the reference frame with capture_screen.py --window-title Mindustry after raising the window. Record the exact calls and the settle wait that made the action land.
4. Checkpoint after the action, best effort in order: pause-menu save slot, else copy the fresh parity-java-home/.local/share/Mindustry/saves/*.msav and verify its mtime changed, else record that the next replay must use the full prefix. Store the surviving path in the item.
5. Record in Mindustry-Godot/parity-ledger.json: one item per row with parent, surface, path (the Java fast-forward calls), a reference block (run dir, frames, expected bundle keys/text, settle, checkpoint, chain), status "reference"; create or update Mindustry-Godot/chains/java-ref-ground-zero-tutorial.md with the exact calls, waits and coordinate notes. Commit the docs (ledger, chain, learnings) as one docs commit per segment; never commit code.
6. Fork rule: append an item for any interactive element newly revealed before moving past it.
7. Hiccups immediately: dropped input, a click that only focused, a maximized window, a blank capture, a save slot that never lands — dated entry in Mindustry-Godot/computer-mcp-learnings.md, plus the wait that fixed it into the chain.

End condition: every skill §2b.1 row has Java reference evidence — the fresh and resumed launches and the full Ground Zero tutorial chain (move, mine copper, research/place the Mechanical Drill, conveyors, 100 copper, lead, move up, Duo turrets + ammo, walls, zone timers, Scatter + ammo, defend, finish). The soak stops when the table is covered; it does not chase mismatches and does not run the Godot leg.

Parallelism: one agent is the default (the tutorial is linear; a second agent only helps from a checkpoint copy). A second agent must have its own loop/display/data dir (../.opencode/loops/bin/start-loop.sh N --run), start from a copy of the checkpoint preceding its first row, own a disjoint row range, and serialize parity-ledger.json writes with flock; never two clients on one display or one data dir.

Constraints: gameplay input only through the Java client (computer-mcp); the spacetime CLI never counts; no item may be marked verified by this goal (reference capture is evidence, not a verdict); never capture mid-animation; never launch the Godot client.

Success: every basic in-game action has a reference frame, a chain, and a ledger item; chains/ and computer-mcp-learnings.md carry every sequence, wait and workaround.

--continuous
--name reference-soak
--constraint "Java client only; the Godot client and Godot code are out of scope"
--constraint "Gameplay input only through the Java client; the spacetime CLI never counts"
--constraint "Reference capture is not a parity verdict: no item becomes verified under this goal"
--constraint "Record the settle wait that made each action land; retake mid-animation frames instead of recording them"
--success "Every basic in-game action row (skill §2b.1) has reference frames, a chain, and a ledger item"
--success "chains/ and computer-mcp-learnings.md carry every recorded sequence, wait and workaround"
```

## /goal — godot walk (Godot side of the reference soak)

Run this in its own session (goal locks are exclusive). It walks the
`reference` (Java-only) items in `parity-ledger.json` through the Godot client —
recreate, plan into the ledger, fix, Godot leg pass, chain — and spawns a fresh
twin-verifier subagent per item for the final Java-vs-Godot verdict. It is
parallelizable: claim one item under flock and own your session's resources.

```
/goal Turn Mindustry-Godot/parity-ledger.json's Java-reference items (status "reference") into verified items by walking each one through the Godot client: claim it, recreate it Godot-side against its stored reference frame, persist a fix plan to the ledger before touching code, execute the plan (implementing prerequisite features where the port lacks them), write the Godot-side chain, then spawn a twin-verifier subagent that re-runs both legs fresh and settles the item. The port is Mindustry-Godot/, the reference is Mindustry/. Before acting, load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md) and the playtest skill; §3b is the reference-backed walk, §5 the mismatch-fix-verify cycle, §6 the documentation contract. The queue and proof record is Mindustry-Godot/parity-ledger.json; exact MCP sequences live in Mindustry-Godot/chains/ (schema in its README).

Each iteration:
0. Hiccup rule: the moment anything costs a retry (tool error, blank capture, stale editor binary, bridge drop, input that did not land, a fork that fails to reproduce), append a dated entry to Mindustry-Godot/open-godot-mcp-learnings.md (Godot/bridge leg) or Mindustry-Godot/computer-mcp-learnings.md (Java leg) — symptom, exact failing call, workaround, run dir — before continuing.
1. Claim under flock: take the exclusive flock on Mindustry-Godot/parity-ledger.json.lock, re-read the ledger, then claim the earliest in-scope item in skill §2b.1 play order with status "reference" (PAR-0037, PAR-0044, PAR-0045, PAR-0046, PAR-0049, PAR-0051 first as the soak grows; a non-null focus wins) or resume the earliest item with status "in-progress" and a fix.plan. Write status "in-progress", claim {by: <session label>, at: ISO8601}, stamp runs/<stamp>-<item>/run.json, release the lock. Skip items another live session holds; reap claims older than 90 minutes back to their prior status. An item that already carries a fix.plan resumes from it instead of re-planning.
2. Recreate Godot-side (skill §3b): keep the Java client down; bring the Godot client up (bridge 6970; after tools/build.sh relaunch the editor — a running editor has the old libmind_gdext.so mmap'd); stamp pid/commit; verify reference.frames.action exists and is non-blank; fast-forward the item's Godot path as one execute chain to the fork; confirm the fork with the structured check; mirror the action under test once with godot_*; capture with godot_screenshot; frame_diff.py against reference.frames.action under the §4 gate. Record steps, frames and diff metrics in the run dir and the ledger; a mid-animation or blank frame is retaken, never recorded.
3. First mismatch → plan first: before editing any code, write fix.plan into the ledger item (seam, files, approach, prerequisites, narrow check, round, the mismatch evidence) so an interrupted run resumes from it; then execute: implement minimally in client/ (GDScript) or client/rust/ (Rust), keeping content IDs, bundle keys and sprite names append-only. A missing prerequisite the item needs to run at all — e.g. a player entity behind WASD movement and click-mining — is in scope: implement it and append ledger items for any newly revealed surface. Run the narrow check (cargo test -p ..., cargo clippy, tools/build.sh as applicable); on a red check fix the fix, never run the Godot leg. When the stored reference cannot reproduce (stale frame, moved path), re-run the Java leg once from reference.checkpoint on the session's display and refresh the reference item before filing a mismatch against it.
4. Godot leg passes: set fix.status "godot-pass" with the run dir, frames and step evidence (the item stays "in-progress" until the twin), write or update the Godot-side chain in Mindustry-Godot/chains/ with the exact calls and last_verified, commit the code fix as "Fixes PAR-####" (docs separately), then spawn exactly one fresh twin-verifier subagent with only this item's JSON and the step-5 contract.
5. Twin verification (subagent; the twin-evaluator agent when registered, else general): re-run the item from the fork fresh on both legs — Java via computer-mcp and Godot via the bridge on the session's display, same fixed 1152x648 — mirroring the action once per leg, and frame_diff the fresh pair under the §4 gate. Settle: match → verified {mode: "twin", writer_run, twin_run, java_frame, godot_frame} and bump the chain's last_verified with both legs; mismatch → status "mismatch" with the fresh both-leg frames/metrics and a fix.plan refreshed from the twin evidence (the item re-enters the writer queue); blocked → leave the item unverified with the exact failing call. The spawn prompt must carry the item JSON, the skill/chain paths, this settle rule, and the constraints (one item, no code edits, no further subagents, report with artifacts).
6. Resume rule: an interrupted run leaves claim + fix.plan in the item; the next iteration re-reads them, validates the plan against the current code, and continues instead of re-discovering. Released or reaped items keep the last valid plan.
7. Fork rule: append an item for any interactive element newly revealed before moving past it; never leave a surface unrecorded.
8. After each item: update parity-ledger.json (claim settled, fix, verified, paths, updated), runs/<stamp>-<item>/report.md, and the learnings files; add the "twin" verified mode to the skill's §6 mode list and to the ledger's conventions.status_notes in the same docs commit so the vocabulary stays consistent; commit docs separately from the code fix.

End condition: no "reference" item is unresolved — each is verified (mode twin), blocked with its failing call, or wontfix with evidence — and every settled item has a Godot-side chain, a twin run, and a report. When the queue is empty the walk idles and re-checks; it never marks an item verified from its own Godot leg alone.

Parallelism: several instances may run concurrently on disjoint items. Each instance owns its session resources — display, editor bridge, worktree and Java data dir — brought up with Mindustry-Godot/tools/parity-session.sh; never reuse another session's display, bridge, worktree or parity-java-home. All ledger writes go through the parity-ledger.json.lock flock (re-read, apply, write, release), and an item is only claimed or reaped inside the lock. One item in flight per session; the writer session holds its item's claim while its twin-verifier spawn runs.

Constraints: gameplay input only through the game clients; the spacetime CLI never counts as a gameplay check. Never mark an item verified from code reading, a build log or the writer's own Godot leg — only the twin subagent's fresh both-leg pair settles it. Write fix.plan into the ledger before the first code edit and refresh it after every failed round. Keep fixes minimal and parity-pinned; prerequisite features are in scope but get their own ledger items. Commit "Fixes PAR-####" only after the narrow check and the Godot leg pass; docs separately. Log every hiccup immediately. Never two clients on one display or one data dir.

Success: every reference item is verified (twin) or explicitly blocked/wontfix with evidence; chains/ carries the Godot-side sequence and the twin both-leg sequence for each settled item; every interrupted item carries a resumable fix.plan.

--continuous
--name godot-walk
--constraint "Gameplay input only through the game clients; the spacetime CLI never counts as a gameplay check"
--constraint "No verdict without fresh artifacts; only a twin-verifier subagent's fresh Java+Godot pair marks an item verified"
--constraint "Write fix.plan into the ledger before the first code edit; refresh it after every failed round so an interrupted run resumes"
--constraint "One item in flight per session; claim, reap and write the ledger only under the parity-ledger.json.lock flock"
--constraint "Log every hiccup to the matching learnings file the moment it happens, with the failing call and workaround"
--constraint "Never two clients on one display or one data dir; a parallel session owns its own display, bridge, worktree and Java data dir"
--success "Every reference item is verified (mode twin), blocked, or wontfix with evidence"
--success "chains/ carries the Godot-side sequence and the twin both-leg sequence for each settled item"
--success "Interrupted items carry a resumable fix.plan in the ledger"
```

## /loop — idle-driven form

Run this instead of the goal, in its own session (a prompt-loop is blocked while
a goal is active in the same session).

```
/loop --name parity-hunt --ask-never --safe --no-overlap --max-failures 3 Each idle turn: do one iteration of the Mindustry-Godot parity hunt. Load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md) and follow Mindustry-Godot/parity-ledger.json; its "focus" wins over the global queue (an item focus runs that PAR-####; an area/path focus runs the first non-verified item matching it, or seeds that area from its fork when none exist yet; null = global queue). Bring both clients up on display :10 (Java via computer-mcp, Godot via open-godot-mcp bridge 6970), normalizing the Java window (window_ctl.py unmaximize Mindustry, then resize 1152 648); an item with a `reference` block needs only the Godot client (skill §3b). Hiccup rule: the moment anything costs a retry (tool error, maximized window, blank capture, dropped input, stale binary, bridge drop), append a dated entry to Mindustry-Godot/computer-mcp-learnings.md or Mindustry-Godot/open-godot-mcp-learnings.md — failing call, workaround, run dir — before continuing. First seed if needed: when seeding.last_sweep is null or the focus area has no items, sweep one surface — fast-forward to the focus fork, snapshot both, enumerate the live controls, append one item per element/action (surface, parent, path, discovered_by), classify each with one action per leg + frame_diff gate (match → verified.mode seed; divergence → mismatch with both frames and calls) — then pick the first non-verified item in scope, read its chain in Mindustry-Godot/chains/, and run it: replay its path (navigation to the fork) as one MCP tool call chain per leg — no per-step captures or diffs — confirm the fork once per leg, then mirror only the actions after the fork one at a time: Java action + capture_screen.py, equivalent Godot action + godot_screenshot, frame_diff.py with the gate from the skill. On the first mismatch, stop, record both frames + diff + exact tool calls in the ledger item, fix the Godot side, rebuild (tools/build.sh), relaunch the editor, re-run the chain (path replay + full steps), then update the ledger, the chain, the learnings files and runs/<run>/report.md, and commit ("Fixes PAR-####" for code, docs separately). Append an item per newly revealed control before moving past any fork. If blocked, record why and move on. Gameplay only through the clients; never claim a result without fresh artifacts from both legs.
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
