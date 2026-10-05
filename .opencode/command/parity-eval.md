---
description: Run an in-engine parity evaluation and update the gap ledger.
agent: evaluator
---

Run a parity evaluation. Scope: $ARGUMENTS

If the scope is empty, re-verify every `open`/`regression` finding whose repro
is runnable, then evaluate the next uncovered scenario from the `parity-eval`
skill catalog.

Follow the `parity-eval` skill end to end:

- check preconditions first (MCP health, client build, JDK, display) and stop
  with a blocker report if the environment is not ready;
- twin-run the Java reference (`../Mindustry`) and the Godot client
  sequentially, never side by side;
- capture screenshots/state/logs per step and compare against committed
  goldens first, then structured state, then programmatic frame metrics;
- update `.opencode/evals/findings.json` via `scripts/record_finding.py` and
  write the run report under `.opencode/evals/runs/`;
- return the concise evaluator summary (run dir, findings added, fixes
  verified/regressed, coverage delta, blockers).
