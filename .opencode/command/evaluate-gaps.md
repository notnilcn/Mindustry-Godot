---
description: Evaluate every needs-evaluation finding one at a time until the evaluator queue is empty.
agent: parity-orchestrator
---

Drive the evaluator queue. Scope: $ARGUMENTS (optional area prefixes, e.g.
`ui input`; empty means the whole ledger).

You orchestrate only: claim one finding, launch one `evaluator` subagent, launch
the next the instant it returns. Never evaluate, never call MCP, never edit.

Set up once, then keep these values in your own context and inline them into
every command you run (do not rely on shell variables surviving between bash
calls):

```bash
RF="${PARITY_MAIN:-.}/.opencode/skills/parity-eval/scripts/record_finding.py"
SLOTS="${PARITY_MAIN:-.}/.opencode/skills/parity-eval/scripts/mcp_slot.py"
OWNER="eval-${MCP_SLOT_OWNER_KEY:-$PPID}"
AREA_ARGS=""   # --area ui --area input ... from $ARGUMENTS, else empty
DEFERRED=""    # space-separated ids this run could not evaluate yet
```

0. If `get_goal` shows no active goal, `create_goal` once with objective:
   "Evaluate every findings.json item in the evaluator queue through an
   in-engine twin run until no unevaluated item remains, leaving each an engine
   verdict (verified-fixed / verified-unfixed / regression / wontfix) and
   waiting while other sessions' fixes are in flight."
   If a goal already exists, continue it.

1. Every cycle: `python3 "$RF" reap`, then check slot capacity with
   `python3 "$SLOTS" status` (JSON, field `free`). If `free` is 0, other
   evaluators own the slots: `sleep 120`, clear `DEFERRED`, and repeat this
   step.

2. Claim the next item:

   ```bash
   python3 "$RF" claim --for evaluation --owner "$OWNER" $AREA_ARGS $DEFERRED
   ```

   `--exclude` is repeatable; pass one flag per deferred id. Exit 3 means
   nothing claimable -> step 5.

3. Launch exactly one `evaluator` subagent with a self-contained brief: the
   claimed JSON (id, title, expected, actual, repro, evidence, owner), the run
   directory convention, and these constraints:

   - re-run the finding's exact repro in a fresh twin run (Java then Godot) at
     the loop's display/bridge when `PARITY_*` is set; never skip to code
     reading;
   - record the verdict only through
     `python3 "${PARITY_MAIN:-.}/.opencode/skills/parity-eval/scripts/record_finding.py" verify --id EV-#### --status <verdict> --note ... --evidence ...`
     where the verdict is `verified-fixed` (expected behavior observed),
     `verified-unfixed` (gap still present after a fix attempt), `regression`
     (a previously verified-fixed item reproduced again), or `wontfix` (accepted
     deviation);
   - on MCP slot refusal, missing display, or a missing precondition:
     `python3 "${PARITY_MAIN:-.}/.opencode/skills/parity-eval/scripts/record_finding.py" release --id EV-#### --status needs-evaluation --note "<blocker>"`
     and return `deferred` with the exact blocker;
   - never edit game code or the ledger outside those calls, and never commit.

4. Append the result: terminal/deferred verdict plus the evidence path. On
   `verified-fixed`/`verified-unfixed`/`regression`/`wontfix` go straight back
   to step 1; on `deferred`, add the id to `DEFERRED` and go straight back to
   step 1.

5. Nothing claimable: run `python3 "$RF" summary` and decide.

   - `terminal == total`: `update_goal` complete (cite the summary JSON) and
     report.
   - `owned > 0` or `free == 0`: other sessions are mid-work. `sleep 120`,
     clear `DEFERRED`, `reap`, retry from step 1.
   - non-terminal items remain but `owned == 0` and nothing evaluable is
     claimable: wait one grace cycle (`sleep 300`, `reap`, retry from step 1),
     then if still empty stop and report. The unevaluated leftovers are fixer
     work (`open`, `regression`, `verified-unfixed`) for `/fix-gaps` or
     `/loop-gaps`, not evaluator work.

Never stop while evaluator work or a live claim exists, and never hold a
finding idle between the claim and the evaluator launch.
