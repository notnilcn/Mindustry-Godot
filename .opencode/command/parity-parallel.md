---
description: Spawn parallel parity loop agents from a plan, a ledger-derived default, or a drain loop.
---

Orchestrate parallel Mindustry-Godot parity loops. Plan: $ARGUMENTS

This is the launch half of a campaign; for the full flow (code gap wave →
loops → final verification sweep) use `/parity-campaign`.

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

## 1. Resolve the plan

- **Loop specs** (`1=ui/menu 2=input [--iterations K]`): run

  ```
  .opencode/loops/bin/launch-parallel.sh $ARGUMENTS
  ```

- **`--every-eval`** (no specs): drain the ledger until no finding is left in
  `open`/`code-verified`/`regression` (`wontfix` and `verified-fixed` are
  terminal). Each freed runner gets the next-highest-priority scope from the
  ledger, skipping areas an active runner covers:

  ```
  nohup .opencode/loops/bin/drain-parallel.sh --slots 2 --iterations K \
    >.opencode/loops/run/drain.log 2>&1 &
  ```

  Do not combine it with explicit specs; pass `--slots`, `--ids`,
  `--iterations`, `--max-rounds` through. A runner that cannot launch (dirty
  worktree) stops the drain and reports — clean the worktree and re-run.

- **No arguments**: derive one scope from the ledger and launch loop 1:

  ```
  python3 .opencode/skills/parity-eval/scripts/record_finding.py next-scope
  ```

  It prints the area of the highest-priority pending finding (severity first,
  then oldest first). Exit 3 means nothing is pending — report "ledger drained"
  and stop. Otherwise run:

  ```
  .opencode/loops/bin/launch-parallel.sh 1=<scope> --iterations 2
  ```

Scope is a finding `area` prefix (`.opencode/evals/findings.json`), so the
areas `next-scope` prints map directly onto `claim --area` and loop specs.

## 2. Report the launcher output

For each loop: scope, worktree, display, bridge port, child pid, and log path.
For `--every-eval`, report the drain pid and log path instead.

## 3. Tell me

- progress: `.opencode/loops/bin/parallel-status.sh` (drain: tail the drain log)
- stop: `.opencode/loops/bin/parallel-stop.sh [loop ...]`
- each runner commits on its own `parity/loop-N` branch; nothing is merged or
  pushed, and I review branches before landing.

## 4. If the launcher reports a blocker, stop and report it verbatim.
