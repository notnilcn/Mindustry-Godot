---
description: Run an in-engine parity evaluation and update the gap ledger.
agent: evaluator
---

Run a parity evaluation. Scope: $ARGUMENTS

If the scope is empty, re-verify every `open`/`regression` finding whose repro
is runnable, then evaluate the next uncovered scenario from the `parity-eval`
skill catalog. In parallel mode, evaluate only this loop's scope; sibling loops
own the other areas.

Follow the `parity-eval` skill end to end:

- check preconditions first (MCP health, client build, JDK, display) and stop
  with a blocker report if the environment is not ready;
- use the loop's isolated resources (`$PARITY_DISPLAY`, `$PARITY_BRIDGE_PORT`,
  `$PARITY_WORKTREE`); launch clients through
  `.opencode/loops/bin/run-godot-editor.sh` and `run-java.sh`;
- twin-run the Java reference (`../Mindustry`) and the Godot client
  sequentially within this loop, never side by side;
- capture screenshots/state/logs per step and compare against committed
  goldens first, then structured state, then programmatic frame metrics;
- update `$PARITY_LEDGER` (default `.opencode/evals/findings.json`) via
  `scripts/record_finding.py` and write the run report under
  `$PARITY_EVALS_DIR/runs/${PARITY_RUN_PREFIX}<stamp>-<scenario>/`;
- return the concise evaluator summary (run dir, findings added, fixes
  verified/regressed, coverage delta, blockers).
