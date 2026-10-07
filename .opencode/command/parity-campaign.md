---
description: Orchestrate a full parity campaign — code gap wave, parallel loop runners, bounded watch, then a final verification sweep.
---

Orchestrate a Mindustry-Godot parity campaign. Plan: $ARGUMENTS

You are only the orchestrator: never evaluate, fix, or edit game code yourself,
and never run a loop leg yourself.

Arguments:

- `<loop>=<scope> [...] [--iterations K]` — same specs as `/parity-parallel`.
- `--verify [scope]` — skip launching; verify the `code-verified` queue.
- `--no-gap` — skip the code-only seed wave.
- `--watch-minutes N` — bounded watch after launch (default 15; 0 = don't watch).

## 1. Preflight

- If `$ARGUMENTS` is empty, stop and ask for a plan like `1=ui/menu 2=input`.
- `.opencode/loops/bin/parallel-status.sh` — if any runner is still live, stop
  and report; never launch a second campaign on top of a running one.
- `python3 .opencode/skills/parity-eval/scripts/mcp_slot.py status` — report the
  two MCP slots. At most two runners hold MCP; a third stays code-only until a
  slot frees.
- Keep at most three loop specs (display/RAM budget, `loops/README.md`).

## 2. Seed the queue (code-only, in-session)

Unless `--no-gap`: for each scope, check
`record_finding.py list --status open`; if the scope has no runnable finding,
launch one `gap-identifier` subagent per scope in a single message (Task tool,
multiple calls) so they run in parallel. They load their own agent contract and
write candidates to `$PARITY_LEDGER`; no displays and no MCP slots are needed.
Do not fix anything in this step.

## 3. Launch the loops

```
.opencode/loops/bin/launch-parallel.sh <loop>=<scope> [...]
```

Strip the orchestration-only flags (`--watch-minutes`, `--no-gap`) before
passing. If the plan carries `--every-eval`, run the drain instead (same
semantics as `/parity-parallel --every-eval`):

```
nohup .opencode/loops/bin/drain-parallel.sh --slots 2 --iterations K \
  >.opencode/loops/run/drain.log 2>&1 &
```

Report per loop: scope, worktree, display, bridge port, child pid, and log path
(drain: its pid and log path).

## 4. Bounded watch

Unless `--watch-minutes 0`: poll every 3 minutes up to the window, reading
`parallel-status.sh` and the per-loop `agent.log` tails. Report only on state
changes or failures. Stop watching when all runners have exited, when a stop
condition from `25_PARITY_PLAN.md` §6 trips, or when the window ends.

## 5. Final verification sweep

When the runners are done — or immediately with `--verify` — pull the remaining
queue: `record_finding.py list --status code-verified`. Launch one `evaluator`
subagent per batch of findings that share a scenario, re-running each exact
repro and updating the ledger to `verified-fixed`/`regression`. With the loops
stopped the two MCP slots are free; keep the sweep sequential per scenario.

## 6. Report and stop

Report: loops launched, gap candidates added, commits per `parity/loop-N`
branch (subjects with `Fixes EV-####`), findings code-verified and
verified-fixed with evidence paths, and anything still open or blocked. Nothing
is merged or pushed — landing the branches is the human's decision. Print the
monitor/stop commands for any runner still live.
