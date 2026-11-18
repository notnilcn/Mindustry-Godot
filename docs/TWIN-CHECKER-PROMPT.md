<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Twin checker prompt (Mindustry-Godot)

The canonical checker brief is the packaged template at
[`tools/ai-tools/twin/prompts/checker.md`](../tools/ai-tools/twin/prompts/checker.md).
Do not fork the workflow text; fill its placeholders from this page and
[`tools/TWIN_COUNTERPART.md`](../tools/TWIN_COUNTERPART.md), then send the
result to exactly one checker subagent.

## Fill-in values

| Placeholder | Value |
|---|---|
| `{{REPO}}` | the main checkout (currently `/home/c/g/code_examples/Mindustry-Godot`) |
| `{{COUNTERPART}}` | `tools/TWIN_COUNTERPART.md` |
| `{{TWIN_DIR}}` | `tools/ai-tools/twin` |
| `{{REGISTRY}}` | `TWIN-VERIFICATION.json` |
| `{{RUNS}}` | `runs` |
| `{{RESOURCE_GATE}}` | `tools/ai-tools/resource-gate/run-resource-intensive-task.sh` |
| `{{START}}` | output of `python3 tools/ai-tools/twin/twin_verify.py next --root .` |
| `{{BATCH}}` | 10–12 items |

Export the leg names before any capture so the packaged helper writes the
repo's folders:

```bash
export TWIN_REFERENCE_LEG=java TWIN_PORT_LEG=godot
```

## Repo additions to the generic brief

- Work only in the main checkout. Never switch branches or worktrees, commit,
  revert the working tree, or run two checkers.
- Read `AGENTS.md`, `docs/TWIN-RUNS.md`, `docs/TWIN-CONTINUATION.md` (or
  `docs/TWIN-WINDOWS.md` on Windows), `parity/AGENTS.md`, and the item record.
- The Godot leg is the port. Use the platform's MCP/editor recipe; capture with
  the packaged helper:
  `tools/ai-tools/twin/twin_item.sh capture port <ITEM> <label> -- ...`
  (the Windows page gives the concrete command).
- The Java leg is the reference. A checker does **not** launch or build Java;
  the nested fixer does when a gap needs a fresh pair.
- PASS → `twin_verify.py set-status <ITEM> reference-verified --note "…"`, write
  the run report, continue.
- GAP → stop, leave `mismatch`, write the report, spawn exactly one nested fixer
  with `tools/ai-tools/twin/prompts/fixer.md`, wait, verify, then stop.
- Finish with `python3 tools/ai-tools/twin/twin_verify.py check --root .` and
  the tiny reply from the packaged template.

The nested fixer must also read `AGENTS.md`, the counterpart page and the
port's contributor rules, use the resource queue for every build, rebuild and
restart the port client after a Rust change, and set `twin-verified` only when
fresh Java and Godot evidence match.
