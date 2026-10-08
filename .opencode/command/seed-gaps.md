---
description: Seed the parity ledger by surface-scanning both repos, then one deep-read subagent per candidate
---

Run one gap-seeding scan: a cheap surface pass over the Java reference and the
port, then one deep-read subagent per surviving candidate hypothesis to fill
the ledger.

Scope: $ARGUMENTS

If the scope is empty, pick the next uncovered area (campaign, HUD, input,
saves, content, multiplayer, editor, rendering, menus). Otherwise treat the
argument as an area prefix (e.g. `ui/menu`), a list of areas, or an explicit
instruction.

You are the orchestrator in this session — read
`.opencode/agent/gap-orchestrator.md` and follow it exactly; never spawn a
subagent to run this command. In short:

1. Phase 1, surface scan (no deep reading): dedupe against
   `.opencode/evals/findings.json` and the seed inventory in `.opencode/evals/`,
   read the AGENTS.md maps on both sides, compare file/type inventories, and
   run `.opencode/skills/parity-eval/scripts/surface_scan.py <path>` to dump
   declarations. Keep only falsifiable candidate hypotheses: an upstream
   surface with no plausible port counterpart, or a counterpart that looks
   stubbed, each with its upstream file:line and the port paths already
   checked.
2. Phase 2, one candidate per subagent: spawn a `gap-identifier` for each
   surviving hypothesis with only that hypothesis. It deep-reads the actual
   files on both sides and either records a new `EV-####` (`--source code
   --status open`, both sides' file:line in `--evidence`, a repro sketch, and
   an inline-Markdown fix plan), appends a dedupe note to an existing id, or
   rejects the hypothesis with the file:line that refutes it. Spawn in waves
   of at most 3 and never hand a subagent more than one candidate.
3. Phase 3, audit the new records (evidence, repro, plan, status) and report
   the output contract: ids added, ids deduped, hypotheses rejected, areas
   scanned, areas uncovered, blockers.

Do not fix anything and do not run in-engine verification.
