---
description: Run one bounded parity iteration — evaluate if needed, fix one ledger finding, re-verify in-engine.
---

Run one iteration of the Mindustry-Godot parity feedback loop. Scope: $ARGUMENTS

Default scope: the highest-severity `open` finding in `$PARITY_LEDGER`
(default `.opencode/evals/findings.json`) matching `$ARGUMENTS`; if the ledger
has no runnable open finding, evaluate the next uncovered scenario from the
`parity-eval` skill catalog. Parallel loops are expected: each loop runs in its
own worktree/display/bridge port and must take a disjoint scope.

The evaluator is a subagent; you (the primary agent) do the fixing. Never write
findings content (`add`/`verify`/`regress`) yourself — the evaluator is the only
writer. The `claim`/`release` coordination commands are the exception: run them
through `record_finding.py`, which holds the ledger's file lock. Never let the
evaluator edit game code.

## Iteration

1. **Evaluate if needed.** If the scope is a scenario or the ledger has no
   fresh finding for it, launch the `evaluator` subagent with the Task tool.
   Give it a self-contained brief: scope, why now, and the required return
   shape (`run dir`, findings added with ids/severity/titles, verified/regressed
   ids, coverage delta, blockers). Do not summarize the skill to it — it loads
   the `parity-eval` skill itself.

2. **Claim exactly one finding atomically.** The shared ledger is written by
   several loops, so never "read and pick" by hand:

   ```bash
   python3 .opencode/skills/parity-eval/scripts/record_finding.py claim \
     --owner "loop-${PARITY_LOOP:-1}" ${SCOPE_AREA:+--area "$SCOPE_AREA"}
   ```

   The command locks the ledger, picks the highest severity (S1 > S2 > S3 > S4)
   then oldest open finding matching the scope, marks it `in-progress`, prints
   the finding JSON, and exits 3 when nothing matches. If it exits 3, evaluate
   the next uncovered scenario instead. If a claim fails hard mid-iteration
   (fix abandoned, blocker, evaluation says it is not real), return it with
   `record_finding.py release --id EV-XXXX --note "<why>"`.

3. **Fix that finding only.** Read the nearest `AGENTS.md` to the code you
   touch and follow it. Keep the change minimal and parity-pinned (content IDs,
   checksums, ABI). Add or update the smallest test that would have caught it.
   If the fix needs a golden/checksum re-record, do it in the same change per
   the root `AGENTS.md` rules.

4. **Verify cheaply before handing off.** Run the narrowest applicable check
   (`cargo test -p <crate>`, `tools/ci.sh` for cross-cutting changes) and, when
   possible, the finding's headless repro. A red check means fix the fix, not
   claim the finding.

5. **Claim the fix.** Ask the user before committing (normal commit rules).
   Once approved, commit with a message that ends in `Fixes EV-XXXX` — that is
   the repo's fix-claim convention. Never edit the ledger to mark it fixed.

6. **Re-verify in-engine.** Launch the `evaluator` subagent again with the
   task: verify finding `EV-XXXX` by re-running its exact repro, update the
   ledger (`verified-fixed` or `regression`) with fresh evidence, and report
   the artifact path. No other scope.

7. **Report and stop.** One finding per invocation: id, what changed, checks
   run, verification result from the evaluator, and the next recommended
   finding. Re-run this command to continue the loop.

If any step is blocked (MCP down, Java reference not built, display missing),
stop and report the exact blocker — do not improvise installs or skip the
in-engine verification.
