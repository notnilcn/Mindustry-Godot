---
description: Fan out code-only gap-identifier agents over disjoint areas and merge their candidates into the ledger.
---

Run a code-level parity gap scan wave. Areas: $ARGUMENTS

You are the orchestrator: never edit code or the ledger yourself, never fix
anything, and never run a twin run.

1. Split `$ARGUMENTS` into disjoint areas. Default split: `game/campaign`,
   `ui/hud`, `ui/dialogs`, `input`, `io/save`, `world/blocks`, `combat`,
   `net/multiplayer`, `ui/editor`. With no arguments, cover the milestone areas
   that have no open or code-verified findings.

2. Launch one `gap-identifier` subagent per area in a single message (Task
   tool, multiple calls) so they run concurrently. Each brief must carry: the
   area, the dedupe rule (read the ledger and `.opencode/evals/` inventory
   first, append notes to existing ids instead of re-adding), the ledger
   commands it owns (`add --source code`, `code-verify`, `wontfix`), the MCP
   slot rule (code-only by default; `mcp_slot.py` exit 3 means stay off MCP),
   and the required return shape (candidate ids/severities/confidences, dedupe
   notes, blockers).

3. The subagents write candidates to `$PARITY_LEDGER` themselves through
   `record_finding.py`; do not duplicate their writes and do not renumber ids.

4. Report: per area, candidates added/deduped (ids + titles), MCP slots that
   were refused, and the recommended fix order (S1/S2 first, oldest first).

Code-sourced candidates stay unverified until the evaluator runs the repro
in-engine; that is the final gate, not this command.
