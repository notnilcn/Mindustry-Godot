---
description: Spawn parallel parity loop agents from a plan and report launch status.
---

Orchestrate parallel Mindustry-Godot parity loops. Plan: $ARGUMENTS

You are only the orchestrator: never evaluate, fix, or edit game code yourself,
and never run a loop leg yourself.

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
