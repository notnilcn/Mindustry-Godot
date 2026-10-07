---
description: Spawn parallel parity loop agents from a plan and report launch status.
---

Orchestrate parallel Mindustry-Godot parity loops. Plan: $ARGUMENTS

You are only the orchestrator: never evaluate, fix, or edit game code yourself,
and never run a loop leg yourself.

Waves: code-only gap scans run through `/parity-gap` (no display, many in
parallel). Display-bound loops are the scarce resource — at most three — and
every MCP consumer takes a lease through
`.opencode/skills/parity-eval/scripts/mcp_slot.py` (`MCP_SLOT_LIMIT`, default
2): `gap-identifier`, `loop-runner` and `evaluator` are denied once both slots
are held, so a third loop stays code-only until one frees. Each runner follows
`parity-loop`: the `gap-identifier` scans and code-verifies, the runner fixes,
and the `evaluator` performs only the final in-engine verification of a
committed, code-verified fix.

1. If `$ARGUMENTS` is empty, stop and ask me for a plan in the form
   `<loop-id>=<scope> [<loop-id>=<scope> ...]` (for example
   `1=ui/menu 2=input`). Otherwise run:

   ```
   .opencode/loops/bin/launch-parallel.sh $ARGUMENTS
   ```

2. Report the launcher output: for each loop the scope, worktree, display and
   bridge port, child pid, and log path.

3. Tell me:
   - progress: `.opencode/loops/bin/parallel-status.sh`
   - stop: `.opencode/loops/bin/parallel-stop.sh [loop ...]`
   - each runner commits on its own `parity/loop-N` branch; nothing is merged
     or pushed, and I review branches before landing.

4. If the launcher reports a blocker, stop and report it verbatim.
