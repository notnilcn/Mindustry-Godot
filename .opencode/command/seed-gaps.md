---
description: Seed the parity findings ledger with code-sourced candidates and fix plans (status open)
agent: gap-identifier
subtask: true
---

Seed the shared parity ledger with code-sourced candidate findings, each
carrying a fix plan.

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
- Every candidate carries a fix plan in its `plan` field: a short Markdown
  sketch of the seam, the files to touch with file:line, the ordered fix
  steps, and the check that proves it, written from the code you already read.
  Pass it on `add --plan "<text>"`; when you sharpen or refresh an existing
  `open`/`godot-open` item that still has no plan (or only an old plan
  number), give it one with the `set-plan` command.
- Do not fix anything and do not run in-engine verification.
- End with the output contract from your agent definition: ids added, ids
  deduped, `wontfix` decisions with the file:line that settles them, areas
  scanned, areas uncovered, blockers.
