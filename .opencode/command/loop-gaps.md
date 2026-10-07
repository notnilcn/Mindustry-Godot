---
description: Run identify/implement/evaluate loops (max 5 cycles per finding) until the ledger is terminal.
agent: parity-orchestrator
---

Drive one identify -> implement -> evaluate loop per non-terminal finding.
Scope: $ARGUMENTS (optional area prefixes, e.g. `ui game`; empty means the whole
ledger).

You orchestrate only: claim one finding, launch one `gap-loop` subagent, and
launch the next the moment it returns. Never fix, evaluate, or call MCP
yourself.

Set up once, then keep these values in your own context and inline them into
every command you run (do not rely on shell variables surviving between bash
calls):

```bash
RF="${PARITY_MAIN:-.}/.opencode/skills/parity-eval/scripts/record_finding.py"
OWNER="loop-${MCP_SLOT_OWNER_KEY:-$PPID}"
AREA_ARGS=""   # --area ui --area game ... from $ARGUMENTS, else empty
EXCLUDES=""    # --exclude EV-#### for every item this run has already moved past
```

0. If `get_goal` shows no active goal, `create_goal` once with objective:
   "Drive every non-terminal findings.json item to a terminal status
   (verified-fixed or wontfix) through identify/implement/evaluate loops, one
   item at a time, with at most five cycles per item; stop when no unprocessed
   non-terminal item remains."
   If a goal already exists, continue it.

1. `python3 "$RF" reap`, then claim the next unprocessed item:

   ```bash
   python3 "$RF" claim --for loop --owner "$OWNER" $AREA_ARGS $EXCLUDES
   ```

   Exit 3 means this run has moved past everything claimable -> step 4.

2. Launch one `gap-loop` subagent with a self-contained brief: the claimed JSON
   (id, severity, area, title, expected, actual, repro, evidence, notes,
   `prev_status`, `owner`) and the cycle budget: up to five
   identify/implement/evaluate cycles for this item, then return. The agent
   loads its own protocol; do not paraphrase it.

3. When it returns, append `--exclude EV-####` for that id to `EXCLUDES` and go
   straight back to step 1. Never wait between items and never launch two
   `gap-loop` agents at once.

4. Nothing claimable left this run: run `python3 "$RF" summary` and decide.

   - `terminal == total`: `update_goal` complete (cite the summary JSON) and
     report.
   - `owned > 0`: other sessions are mid-item. `sleep 60`, `reap`, and retry
     from step 1 (keep `EXCLUDES`: this run does not retry items it already
     processed).
   - non-terminal items remain but none is claimable or owned: stop and report
     them. They are items this run already attempted and could not close (or
     items outside `AREA_ARGS`); a later run may retry them with fresh context,
     and `/fix-gaps` can always take them.

Items a `gap-loop` returned as `verified-unfixed` or `regression` keep that
status and stay in the fixer queue; the five-cycle budget is per item per run,
not a permanent give-up.
