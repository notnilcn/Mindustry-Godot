---
description: Seed the parity findings ledger with code-sourced candidates (status open)
agent: gap-identifier
subtask: true
---

Seed the shared parity ledger with code-sourced candidate findings.

Scope: $ARGUMENTS

If the scope is empty, pick the next uncovered area (campaign, HUD, input,
saves, content, multiplayer, editor, rendering, menus). Otherwise treat the
argument as an area prefix (e.g. `ui/menu`), a list of areas, or an explicit
instruction.

Rules:

- Dedupe against `.opencode/evals/findings.json` and the seed inventory in
  `.opencode/evals/`; append a note to an existing `EV-####` instead of adding
  a duplicate symptom.
- Record every new candidate through the ledger helper with `--source code`
  and `--status open`, both sides' file:line in `--evidence`, and a repro
  sketch the evaluators can run later.
- Do not fix anything and do not run in-engine verification.
- End with the output contract from your agent definition: ids added, ids
  deduped, `wontfix` decisions with the file:line that settles them, areas
  scanned, areas uncovered, blockers.
