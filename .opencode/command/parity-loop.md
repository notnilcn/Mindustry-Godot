---
description: Run one bounded parity iteration — gap-identify, fix one ledger finding, code-verify, then final in-engine verification.
---

Run one iteration of the Mindustry-Godot parity feedback loop. Scope: $ARGUMENTS

Default scope: the highest-severity `open` finding in `$PARITY_LEDGER`
(default `.opencode/evals/findings.json`) matching `$ARGUMENTS`; if the ledger
has no runnable open finding, have the gap identifier scan the scope and add
code-sourced candidates first. Parallel loops are expected: each loop runs in
its own worktree/display/bridge port and must take a disjoint scope.

Roles: the `gap-identifier` subagent finds candidates and code-verifies fixes;
you (the primary agent) fix; the `evaluator` subagent is the final in-engine
gate and the only judge of `verified-fixed`. Never write `add`/`code-verify`/
`verify` content yourself — the subagents own those. The `claim`/`release`
coordination commands are the exception: run them through `record_finding.py`,
which holds the ledger's file lock. Never let the gap identifier edit game
code, and never let the fixer mark its own finding verified.

## Iteration

1. **Find work.** If the scope has no fresh open finding, launch the
   `gap-identifier` subagent with the Task tool. Give it a self-contained
   brief: scope/area, the dedupe requirement (read the ledger and the code-audit
   inventory first), and the required return shape (candidate ids, severities,
   confidences, evidence, blockers). Do not summarize its agent contract to it.
   Code-sourced candidates are enough to start fixing — no engine run here.

2. **Claim exactly one finding atomically.** The shared ledger is written by
   several loops, so never "read and pick" by hand:

   ```bash
   python3 .opencode/skills/parity-eval/scripts/record_finding.py claim \
     --owner "loop-${PARITY_LOOP:-1}" ${SCOPE_AREA:+--area "$SCOPE_AREA"}
   ```

   The command locks the ledger, picks the highest severity (S1 > S2 > S3 > S4)
   then oldest open finding matching the scope, marks it `in-progress`, prints
   the finding JSON, and exits 3 when nothing matches. If it exits 3, run step
   1 instead. If a claim fails hard mid-iteration (fix abandoned, blocker, the
   candidate is not real), return it with
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

5. **Claim the fix.** Parity fix commits are pre-authorized: commit directly
   without asking, with a message that ends in `Fixes EV-XXXX` — that is the
   repo's fix-claim convention. Never push, and never edit the ledger to mark it
   fixed.

6. **Code-verify.** Launch the `gap-identifier` subagent again: re-read the fix
   diff against the upstream behavior and `code-verify` EV-XXXX, or `release`
   it with the exact reason the diff does not close the gap. This is code
   inspection only; it does not replace the in-engine repro.

7. **Final in-engine verification.** Only now — fix committed and marked
   `code-verified` — launch the `evaluator` subagent to re-run the finding's
   exact repro, update the ledger (`verified-fixed` or `regression`) with fresh
   evidence, and report the artifact path. The evaluator frees its MCP slot at
   the end of the twin-run (`mcp_slot.py release`), so a waiting loop can take
   it. No other scope; batch only findings whose repro shares one
   scenario/state.

8. **Report and stop.** One finding per invocation: id, what changed, checks
   run, code-verify result, evaluator verdict, and the next recommended
   finding. Re-run this command to continue the loop.

MCP use is slot-gated (`gap-identifier`, `loop-runner` and `evaluator` are
denied at the cap; the interactive session is warn-and-allow) through
`.opencode/skills/parity-eval/scripts/mcp_slot.py`, `MCP_SLOT_LIMIT` default 2;
computer-mcp additionally needs `DISPLAY` at opencode process start. If any
step is blocked (MCP slot refused, MCP bridge down, Java reference not built,
display missing), stop and report the exact blocker — do not improvise installs
or skip the final verification.
