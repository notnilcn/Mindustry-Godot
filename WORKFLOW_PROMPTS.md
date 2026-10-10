# Workflow prompts — parity hunt

Canonical paste-text for the `@bybrawe/opencode-goal` (`/goal`) and
`@bybrawe/opencode-loop` (`/loop`) sessions. The durable spec is
`.opencode/skills/parity-eval/SKILL.md`, and the queue, proof record and
re-runnable sequences are `parity-ledger.json` (each item embeds its own
`path`/`reproduce`/`steps`, plus a `reference` block when Java evidence has
been recorded). If a prompt below and the skill disagree, the skill wins —
update this file with it.

Run from the workspace root (`code_examples/`), or from `Mindustry-Godot/` with
the `Mindustry-Godot/` path prefix dropped.

Over SSH (no display), first run `Mindustry-Godot/tools/parity-session.sh`
(add `--gpu-godot` for a Wayland/GL editor; `--run` starts opencode for you).
It brings up the session display (`:10` for session 1) plus the editor bridge
(`6970`), prints the session summary, and `--stop` tears it down — then the
prompts below work unchanged. `--session N` (N >= 2) starts a parallel session
on the same host: display `:(9+N)`, bridge `6970+10(N-1)`, a git worktree
`Mindustry-Godot-sessionN` with its own build caches and Java data dir, and
session-local MCP wiring; the ledger and `runs/` stay shared with the main
checkout through symlinks, so claims and reference frames are visible to every
session.

Three rules the prompts carry: **Godot-first** — only the Godot client is
booted by default; the Java reference is opened only when the item's stored
reference evidence is missing or not good enough (or the operator asks for a
twin check); **scope** — the ledger's top-level `focus` wins over the global
queue; and **hiccups** — friction is written to the matching learnings file
(`java-leg-learnings.md` / `open-godot-mcp-learnings.md`) the moment it happens,
not at the end of the run.

The **reference soak** prompt tops up Java evidence for items that lack it —
exactly the fallback trigger the primary hunt looks for.

## /goal — the parity hunt (primary). Run it from this workspace root so Mindustry-Godot/... paths resolve; if you start inside Mindustry-Godot, drop that prefix:

```
/goal Reach 1:1 parity between the Java Mindustry reference and the Godot port — Godot-first. The port is Mindustry-Godot/, the reference is Mindustry/. Before acting, load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md) and the playtest skill; they are the operating manual. The work queue and proof record is Mindustry-Godot/parity-ledger.json; each item embeds its re-runnable sequence (path/reproduce/steps) and, when reference-backed, the stored Java evidence (reference.run / reference.frames / reference.settle / reference.java_path).

Client rule — the Godot client only, by default. An item with adequate `reference` evidence is a Godot-only walk (skill §3b) judged against its stored reference frames (the `frames` block — the fork frame plus the action/result frame; key names vary by soak) from `reference.run`; never boot Java to mirror an item that already has it. Open the Java reference only when the stored evidence is missing or not good enough — no `reference` block or frames, a fork-only block with no action/result frame, a blank or mid-animation capture, a frame from a different fork/state, a reference that no longer reproduces for the action under test — or when the operator asks for an independent twin check. Whenever the Java leg runs, recapture the reference and store it on the item, and record the reason it was needed in run.json; never leave Java up for an item that does not need it.

Scope: parity-ledger.json's top-level "focus" wins over the default queue. A focus item runs that PAR-####; an area/path focus runs the first non-verified item whose surface/title/parent lineage/path.to matches the focus target/path tokens, or seeds that area from its fork when no items exist yet. If the operator names a new item/area, update "focus" first (null resumes the global queue) and stamp it in run.json.

Each iteration:
0. Timeout rule: never issue an unbounded command. Wrap every shell call in `timeout N` (`timeout 30` helper/capture, `timeout 90` a sleep or poll chunk, `timeout 180` a launch readiness gate), and poll readiness with `timeout 180 bash -c 'until grep -q "Total time to load" <log>; do sleep 2; done'`. A timed-out call is friction: record the hiccup (exact call, log tail), kill only the recorded pid if a client half-died, then continue or mark the item blocked — never re-issue the same hung call unchanged.
1. Hiccup rule: the moment anything costs a retry (tool error, maximized window, blank capture, input that did not land, stale editor binary, bridge drop, moved dialog path), append a dated entry to Mindustry-Godot/java-leg-learnings.md (Java/X leg) or Mindustry-Godot/open-godot-mcp-learnings.md (Godot/bridge leg) — symptom, exact failing call, workaround, run dir — before continuing.
2. Pick and claim under flock: take the exclusive flock on Mindustry-Godot/parity-ledger.json.lock, re-read the ledger, then take the first non-verified item in scope (focus wins; otherwise the first non-verified item in parity-ledger.json) that no live session holds — write status "in-progress" and claim {by: <session label>, at: ISO8601}, stamp runs/<stamp>-<item>/run.json, release the lock. Skip items another live session holds; reap claims older than 90 minutes. Read the item's path/reproduce/steps and any reference/fix.plan; an item that already carries a fix.plan resumes from it instead of re-planning.
3. Bring up only what the item needs. Default: the Godot client — the session's editor + bridge (tools/parity-session.sh already started them; health-check via open-godot-mcp and start res://scenes/game.tscn) — stamped in run.json; a running editor has the old libmind_gdext.so mmap'd, so relaunch it after tools/build.sh. Java leg (only per the client rule): launch the reference the way the soak did — the ledger's `environment.java.launch` with `DISPLAY="$DISPLAY"` and `HOME`/`XDG_*` pointed at `parity-java-home/`, run from the Mindustry-Godot repo root, log under runs/<run>/java/game.log — gate on "Total time to load" with a bounded poll, normalize the window (window_ctl.py unmaximize Mindustry, then resize 1152 648, then raise before every capture), and write the fresh evidence into the item's `reference` block (captured, profile, run, frames, settle, expected, java_path).
4. Fast-forward, then mirror only the action under test. Replay the item's Godot path to the fork as a single execute chain — one execute call issuing every call back-to-back with explicit waits for the unskippable animations (boot log gate, dialog fades, loading bar, prompt transitions) — then confirm the fork with the structured check in path.confirm. (A code-scan item whose path.godot is not yet a call list: derive the Godot fast-forward from the reference's java_path/fix.plan and store it on the item.) From the fork onward, mirror step by step: Godot action with godot_* and capture with godot_screenshot; the Java side of the diff is the reference's action/result frame from `reference.run` (or the fresh capture when the Java leg just ran) — diff each pair with frame_diff.py into runs/<run>/diff/. When the Java leg runs, replay path.java as one chain and mirror the same action once (the Java-leg helpers run with `$PARITY_PY`: raise the window with window_ctl.py, act with java_input.py, capture with capture_screen.py --window-title Mindustry). The path replay is never captured or diffed; navigation is not evidence. A blank or mid-animation capture is retaken, never recorded.
5. Match gate: size_match, changed_fraction <= 0.005, each mean_abs_rgb <= 12, coarse_layout.max_channel_mean <= 6, palette_distance <= 0.05, no blank frames. Calibrate only with recorded evidence.
6. First mismatch: STOP the run. Record the step, both frames (the Java side is the reference frame or the fresh capture), the diff JSON and metrics, and the exact reproduction tool calls for both legs in the ledger item; before editing any code, write or refresh fix.plan (seam, files, approach, narrow check, round, evidence) so an interrupted run resumes from it. Then edit the Godot side — a missing prerequisite the item needs to run at all is in scope, and any newly revealed surface gets its own item — run the narrow check, tools/build.sh, relaunch the editor, and re-run the same chain from the start (path replay + full steps) against adequate Java evidence. An item is verified only when the whole chain passes.
7. Whenever a fork reveals new interactive surface, append one item per control/action (parent, surface, path, discovered_by) before moving past it.
8. After every item: update parity-ledger.json (status, updated, path/reproduce/steps and path.last_replayed with the exact calls; refresh the reference block whenever the Java leg ran); check the learnings files carry every hiccup of this run (log any missed one now); write runs/<run>/report.md; commit the code fix as "Fixes PAR-####" and the docs in a separate commit. Settle: a chain that passes against adequate reference frames closes as verified {mode: "reference", run, reference}; a live both-leg pass closes as mode "chain"; when the operator asks for independent verification, spawn one fresh twin-verifier subagent (the only other Java boot) and settle as mode "twin".

Success: every parity-ledger.json item is verified (seed/chain/reference/twin) or explicitly wontfix/blocked with evidence; every verified item carries its exact re-runnable sequence in the ledger (path/reproduce/steps, plus reference.java_path/reference.settle on the Java side), and every discovered interactive element has a ledger item; every interrupted item carries a resumable fix.plan.

--continuous
--name parity-hunt
--constraint "Gameplay input only through the game clients; the spacetime CLI never counts as a gameplay check"
--constraint "Godot only by default: boot the Java reference only when the item's stored reference is missing or not good enough, or when the operator asks for a twin check; record the reason in run.json"
--constraint "No status change without fresh artifacts: a fresh Godot chain gated against adequate Java reference frames settles as mode reference, a live both-leg pass as mode chain, and a twin-verifier subagent's fresh pair only when the operator asks"
--constraint "Log every hiccup to the matching learnings file the moment it happens; a hiccup that lives only in the session log is unrecorded"
--constraint "Never run an unbounded shell command: every call carries an explicit timeout and readiness polls are bounded log-gate waits; a timed-out call is a hiccup, not a retry loop"
--constraint "The ledger focus wins over the global queue until it is cleared"
--constraint "Never widen the comparison gate silently; record calibration evidence in the item"
--constraint "Claim, reap and write the ledger only under the parity-ledger.json.lock flock; one item in flight per session"
--constraint "Never two clients on one display or one data dir; a parallel session owns its display, bridge, worktree and Java data dir via tools/parity-session.sh"
--constraint "Write fix.plan into the ledger before the first code edit and refresh it after every failed round"
--constraint "Commit a code fix only after the narrow check and the re-run pass"
--success "Every parity-ledger.json item is verified (seed/chain/reference/twin) or explicitly wontfix/blocked with evidence"
--success "Every verified item carries its exact re-runnable sequence in the ledger (path/reproduce/steps, plus reference.java_path/reference.settle on the Java side), and every discovered interactive element has a ledger item"
--success "Interrupted items carry a resumable fix.plan in the ledger"
```

## /goal — reference soak (top up Java evidence for items that lack it)

Run this in its own session (goal locks are exclusive; not while the parity
hunt goal is active). Run it from the Mindustry-Godot repo root (or prefix the
paths with `Mindustry-Godot/` from the workspace root). It records the Java side
of the basic in-game actions once, so the hunt's Godot-only walks have reference
frames; use it when the hunt finds an item whose stored reference is missing or
not good enough. It produces no verdicts.

```
/goal Top up Mindustry-Godot/parity-ledger.json with Java-only reference evidence for open items whose `reference` block is missing or not good enough, so later Godot runs can fast-forward against stored frames instead of a live Java leg. The reference is Mindustry/; the port is Mindustry-Godot/. Before acting, load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md): §2b is the recording protocol and §2b.1 the action table; the ledger items' reference.java_path blocks carry the boot recipe. The Godot client and all Godot code are out of scope for this goal, and this goal produces no parity verdicts.

Each iteration:
0. Every shell call carries an explicit timeout (`timeout 30` helper/capture, `timeout 90` a sleep or poll chunk, `timeout 180` a launch gate); a timed-out call is a hiccup, never an unbounded retry loop. Launch the reference (skill §0): display `$DISPLAY` (session 1: `:10`), poll for "Total time to load" under the bounded gate, unmaximize/resize to 1152x648, raise before every capture, stamp pid/profile in Mindustry-Godot/runs/<stamp>-ref-<area>/run.json. Keep one Java client for the whole soak; reboot between rows only when the row needs a fresh profile (back up parity-java-home/.local/share/Mindustry/saves/ first and say so in run.json).
1. Pick the next row: a skill §2b.1 basic action without reference evidence, or any open item flagged with missing/inadequate reference frames. If a sibling item already carries the action's reference (e.g. the R00-R06 frames under runs/20261009-065416-ref-ground-zero/), cross-link its frames instead of re-capturing. Never duplicate an existing item.
2. Fast-forward: replay the row's prefix from its checkpoint as one execute call issuing every Java-leg shell call back-to-back, with explicit waits for the unskippable animations (boot log gate, dialog fades, loading bar, prompt transitions; wave timers in <=60 s sleep chunks, each chunk its own timed call). No per-step captures or diffs.
3. Action under test, once: perform it with `$PARITY_PY` `.opencode/skills/parity-eval/scripts/java_input.py`; wait for the objective/prompt to settle (sleep, then capture; a mid-animation frame is retaken, never recorded); capture the reference frame with `capture_screen.py --window-title Mindustry` after raising the window. Record the exact calls and the settle wait that made the action land.
4. Checkpoint after the action, best effort in order: pause-menu save slot, else copy the fresh parity-java-home/.local/share/Mindustry/saves/*.msav and verify its mtime changed, else record that the next replay must use the full prefix. Store the surviving path in the item.
5. Record in Mindustry-Godot/parity-ledger.json, on the existing item: the Java fast-forward calls into path/reproduce where they are stubs, and a reference block (captured, profile, run, frames fork/action, expected bundle keys/text, settle, checkpoint, java_path); status stays "reference" (or keeps its current status if a mismatch/blocked item is getting a refreshed reference). Commit docs (ledger, learnings) as one docs commit per segment; never commit code.
6. Fork rule: append an item for any interactive element newly revealed before moving past it.
7. Hiccups immediately: dropped input, a click that only focused, a maximized window, a blank capture, a save slot that never lands — dated entry in Mindustry-Godot/java-leg-learnings.md, plus the wait that fixed it into the item's sequence.

End condition: the queue is empty — every open item that needs Java evidence carries an adequate `reference` block (frames, settle waits, checkpoint or a full-prefix note), or records a staging blocker. The soak stops when the queue is covered; it does not chase mismatches and does not run the Godot leg.

Parallelism: one agent is the default (a single Java client is the mutable resource). A second agent needs its own session via Mindustry-Godot/tools/parity-session.sh --session N (own display, bridge, worktree, Java data dir), a disjoint item range, and the parity-ledger.json.lock flock for every ledger write; never two clients on one display or one data dir.

Constraints: gameplay input only through the Java client (shell helpers); every shell call carries an explicit timeout; reference capture is not a parity verdict (no item becomes verified under this goal); record the settle wait that made each action land; retake mid-animation frames instead of recording them; never wipe or re-import the Java profiles.

Success: every open item without reference evidence carries a reference block with frames and settle waits (or a recorded staging blocker); the ledger and java-leg-learnings.md carry every recorded sequence, wait and workaround.

--continuous
--name reference-soak
--constraint "Java client only; the Godot client and Godot code are out of scope"
--constraint "Gameplay input only through the Java client; the spacetime CLI never counts"
--constraint "Reference capture is not a parity verdict: no item becomes verified under this goal"
--constraint "Every shell command carries an explicit timeout; readiness polls are bounded log-gate waits; a timed-out call is a hiccup, never a retry loop"
--constraint "Record the settle wait that made each action land; retake mid-animation frames instead of recording them"
--constraint "Never wipe or re-import the Java profiles"
--success "Every open parity-ledger item without reference frames carries a reference block, or a recorded staging blocker"
--success "The ledger's reference blocks and java-leg-learnings.md carry every recorded sequence, wait and workaround"
```

## /loop — idle-driven form

Run this instead of the goal, in its own session (a prompt-loop is blocked while
a goal is active in the same session).

```
/loop --name parity-hunt --ask-never --safe --no-overlap --max-failures 3 Each idle turn: do one iteration of the Mindustry-Godot parity hunt, Godot-first. Load the parity-eval skill (Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md) and follow Mindustry-Godot/parity-ledger.json; its "focus" wins over the global queue (an item focus runs that PAR-####; an area/path focus runs the first non-verified item matching it, or seeds that area from its fork when none exist yet; null = global queue). Client rule: only the Godot client by default — an item with adequate `reference` evidence is a Godot-only walk (skill §3b) judged against its stored reference frames (the `frames` block: fork plus action/result frame; keys vary by soak) from reference.run; boot the Java reference only when the stored evidence is missing or not good enough (no reference/frames, a fork-only block with no action/result frame, a blank or mid-animation capture, a wrong fork/state, or a reference that no longer reproduces) or when the operator asks for a twin check; recapture the reference on the item and record the reason in run.json whenever Java runs. Claim the picked item under the parity-ledger.json.lock flock (re-read, status "in-progress", claim {by, at}; skip items a live session holds; reap claims older than 90 minutes; resume from a stored fix.plan instead of re-planning). Hiccup rule: the moment anything costs a retry (tool error, maximized window, blank capture, dropped input, stale binary, bridge drop), append a dated entry to Mindustry-Godot/java-leg-learnings.md or Mindustry-Godot/open-godot-mcp-learnings.md — failing call, workaround, run dir — before continuing. First seed if needed: when seeding.last_sweep is null or the focus area has no items, sweep one surface — fast-forward to the focus fork, snapshot both, enumerate the live controls, append one item per element/action (surface, parent, path, discovered_by), classify each with one action per leg + frame_diff gate (match -> verified.mode seed; divergence -> mismatch with both frames and calls) — then pick the first non-verified item in scope and run it from its stored sequence in the ledger: replay its path.godot (navigation to the fork) as one execute chain issuing every call back-to-back with explicit waits for the unskippable animations (boot log gate, dialog fades, loading bar, prompt transitions) — no per-step captures or diffs — confirm the fork with path.confirm, then mirror only the action under test: Godot action + godot_screenshot, frame_diff.py against the reference's action/result frame (or the fresh Java capture) with the skill's gate; a blank or mid-animation capture is retaken, never recorded. On the first mismatch, stop, record both frames + diff + exact tool calls in the ledger item, write or refresh fix.plan before touching code, fix the Godot side, rebuild (tools/build.sh), relaunch the editor, re-run the sequence (path replay + full steps), then update the item (path/reproduce/steps, updated, last_replayed), the learnings files and runs/<run>/report.md, and commit ("Fixes PAR-####" for code, docs separately). Settle: a passing fresh chain against adequate reference frames closes as verified.mode reference (a code fix whose full re-run is green included); a live both-leg pass as chain; an operator-requested twin-verifier subagent's fresh pair as twin. Append an item per newly revealed control before moving past any fork. If blocked, record why and move on. Gameplay only through the clients; never claim a result without fresh artifacts gated as above.
```

## Mid-session nudge — retarget an already-running session

Paste when a live session needs the new rules without a restart (works for
`/goal` and `/loop`):

```
Workflow update — re-read Mindustry-Godot/.opencode/skills/parity-eval/SKILL.md (§0 Hiccups, §3b reference-backed runs) and Mindustry-Godot/parity-ledger.json before the next iteration.
1. Godot-first: boot the Java reference only when the picked item's stored `reference` evidence is missing or not good enough (no frames, a blank/mid-animation capture, a wrong fork, a reference that no longer reproduces) or when an independent twin check is wanted; recapture the reference on the item and record the reason in run.json whenever Java runs.
2. Hiccups: the moment any friction costs a retry (e.g. the Java window opening maximized), append a dated entry to Mindustry-Godot/java-leg-learnings.md or Mindustry-Godot/open-godot-mcp-learnings.md with the exact failing call, the workaround and the run dir, then continue. Do not wait until the end of the item.
3. Scope: work only on items under parity-ledger.json's "focus". If none exist yet, seed that area first.
4. Fast-forward: replay the Godot path to the fork as one execute chain with the unskippable-animation waits inline; mirror only the action under test and diff against the reference's action/result frame (or the fresh Java capture) under the §4 gate. Never capture or diff the path replay.
5. Settle: a passing fresh chain against adequate reference frames closes as verified.mode "reference" (after a code fix and a green full re-run too); a live both-leg pass as "chain"; an operator-requested twin-verifier subagent's fresh pair as "twin".
```
