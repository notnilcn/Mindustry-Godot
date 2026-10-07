---
description: Fix every fixable finding with a single swarm of parity-writer agents partitioned by area.
agent: parity-orchestrator
---

Run one fix swarm over the fixer queue (`open`, `regression`,
`verified-unfixed`). Arguments: $ARGUMENTS — optional max concurrent writers
(default 4) and/or area prefixes.

You are the orchestrator: never edit code, never commit, never claim a finding
for yourself, never call MCP. The `parity-writer` subagents do all of it.

1. List the fixable areas:

   ```bash
   RF="${PARITY_MAIN:-.}/.opencode/skills/parity-eval/scripts/record_finding.py"
   python3 "$RF" areas --for fix
   ```

   Partition those areas into at most N disjoint groups (N = the number in
   `$ARGUMENTS`, else 4), keeping each `area/` family (`ui/`, `game/`,
   `input/`, `net/`, `content/`, ...) whole. Every area with fixable work must
   belong to exactly one group.

2. Launch one `parity-writer` subagent per group in a single message (parallel
   Task calls). Each brief carries:

   - the group's exact area prefixes and the rule that it claims and fixes only
     findings inside them (shared plumbing files may need minimal cross-area
     edits; keep them small and never touch another group's finding);
   - the claim loop, run from the writer's own shell, one finding at a time
     until exit 3 (its group is drained):

     ```bash
     RF="${PARITY_MAIN:-.}/.opencode/skills/parity-eval/scripts/record_finding.py"
     OWNER="fix-<group>-${MCP_SLOT_OWNER_KEY:-$PPID}"
     python3 "$RF" claim --for fix --owner "$OWNER" --area <a> [--area <b> ...]
     ```

     exit 3 means done; `release --id EV-#### --note "<reason>"` when a fix is
     abandoned;
   - the `parity-writer` protocol: read the nearest `AGENTS.md`, minimal
     parity-pinned fix, narrow test/repro, commit ending `Fixes EV-####`, then
     `set-status --status needs-evaluation`, then claim the next finding;
   - do not use MCP; no pushes; one commit per finding;
   - required return: per group, fixed ids + commit hashes, abandoned ids with
     reasons, blockers.

3. When the swarm returns, run `python3 "$RF" summary` and report: per group,
   findings fixed (id + commit), released/abandoned, still-fixable leftovers
   (new areas that appeared during the run), and blockers. If new fixable areas
   appeared and the groups were not covering them, either launch a small
   follow-up swarm for those areas or list them for the next run — do not
   silently drop them.

Fixes stay unverified until an evaluator runs their repro; `/evaluate-gaps` or
`/loop-gaps` owns that.
