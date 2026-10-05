---
description: Run one bounded parity iteration — evaluate if needed, fix one ledger finding, re-verify in-engine.
---

Run one iteration of the Mindustry-Godot parity feedback loop. Scope: $ARGUMENTS

Default scope: the highest-severity `open`/`regression` finding in
`.opencode/evals/findings.json`; if the ledger has no runnable open finding,
evaluate the next uncovered scenario from the `parity-eval` skill catalog.

The evaluator is a subagent; you (the primary agent) do the fixing. Never write
to `.opencode/evals/` yourself — the evaluator is the only writer. Never let the
evaluator edit game code.

## Iteration

1. **Evaluate if needed.** If the scope is a scenario or the ledger has no
   fresh finding for it, launch the `evaluator` subagent with the Task tool.
   Give it a self-contained brief: scope, why now, and the required return
   shape (`run dir`, findings added with ids/severity/titles, verified/regressed
   ids, coverage delta, blockers). Do not summarize the skill to it — it loads
   the `parity-eval` skill itself.

2. **Pick exactly one finding.** Read the ledger:

   ```bash
   python3 .opencode/skills/parity-eval/scripts/record_finding.py list --status open
   python3 .opencode/skills/parity-eval/scripts/record_finding.py list --status regression
   ```

   Order S1 > S2 > S3 > S4, oldest first within a severity, scoped to
   `$ARGUMENTS` when given. Quote the finding id in every update.

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
