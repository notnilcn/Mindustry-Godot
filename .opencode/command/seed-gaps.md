---
description: Seed findings.json with code-only parity gap candidates via a swarm of gap-identifier agents.
agent: parity-orchestrator
---

Run a code-only parity gap seed wave. Areas: $ARGUMENTS

You are the orchestrator: never edit code or the ledger yourself, never fix,
never run a twin run, and never call MCP. The `gap-identifier` subagents do the
scanning and write their own candidates through `record_finding.py`.

1. Pick disjoint areas. If `$ARGUMENTS` is non-empty, treat it as the area list
   (one per whitespace-separated token, e.g. `ui/menu input net`). Otherwise
   use these default groups and cover all five:

   - `ui` — menus, dialogs, HUD, settings, campaign UI
   - `game` — campaign, rules, waves, objectives, sim runtime
   - `input` — placement, commands, keybinds, RTS selection
   - `world` — world/terrain, blocks, content, logistics, power
   - `combat` + `net` — combat runtime, multiplayer relay, assets pipeline

   Cap the wave at five concurrent agents; merge groups if a list is longer.

2. Launch one `gap-identifier` subagent per area in a single message (parallel
   Task calls). Each brief must be self-contained and carry:

   - the area (and explicit subareas it owns); it must not report outside it;
   - the dedupe rule: read the ledger and `.opencode/evals/` seed material
     first, append notes to existing `EV-####` records instead of adding
     duplicates, and never re-report `verified-fixed`/`wontfix` areas without
     fresh code evidence;
   - its ledger commands: `add --source code` with `--status open` by default,
     or `--status needs-evaluation` when the candidate cannot be settled by
     code reading and needs an in-engine look; every candidate needs upstream
     and port `file:line` evidence plus a repro sketch;
   - code-only rule: no MCP by default; if it tries and `mcp_slot.py` exits 3,
     it stays code-only and says so;
   - the required return shape: candidate ids/severities/confidences/titles,
     dedupe notes, blockers.

3. Do not duplicate the subagents' writes or renumber ids. When the wave
   returns, run `python3 "${PARITY_MAIN:-.}/.opencode/skills/parity-eval/scripts/record_finding.py" summary`
   and report: candidates added/deduped per area
   (ids + titles), areas still uncovered, MCP slots refused, and the
   recommended order (S1/S2 first, oldest first).

Candidates stay unverified until the evaluator runs their repro in-engine;
that is the evaluator's gate, not this command's.
