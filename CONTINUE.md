<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Continue the twin-verification sweep

The post-cutover re-verification of all 450 `TWIN-VERIFICATION.json` items is
paused at **23/450 terminal** (18 `reference-verified`, 5 `twin-verified`).
Next start: **PAR-0024**. Read
[`docs/TWIN-CONTINUATION.md`](docs/TWIN-CONTINUATION.md) first: it lists the
state, the fixes landed this session, the exact checker/fixer prompts, the
environment commands and the known residuals.

Suggested opening prompt:

> Continue the twin-verification sweep in
> `/home/clintonnguyen/g/code_examples/Mindustry-Godot` (branch main; the
> working tree intentionally contains this session's fixes — do not commit or
> revert them). Read `AGENTS.md`, `docs/TWIN-RUNS.md` and
> `docs/TWIN-CONTINUATION.md` first, then run the sequential checker loop
> described there starting at the first item whose status is not
> `reference-verified`/`twin-verified` (currently PAR-0024). Spawn one checker
> subagent at a time; each walks up to ~12 items Godot-only, stops at the first
> gap, spawns exactly one nested fixer (Java+Godot legs, fix, `twin-verified`),
> then the next checker continues. No parallel agents, no branches/worktrees,
> no commits. Keep replies tiny; write details to `runs/PAR-####/` and
> `TWIN-VERIFICATION.json`. Done when all 450 items are `reference-verified` or
> `twin-verified`.

Do not commit the uncommitted fixes or the session-state files under
`.opencode/parity-session/` and `.claude/worktrees/`. The detailed handoff for
the last batch is in `runs/TWIN-HANDOFF.md` (gitignored).
