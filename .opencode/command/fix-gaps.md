---
description: Claim open parity findings and run the writer fix-and-verify cycle for this loop
---

Run one fix cycle for this loop session.

Arguments: $ARGUMENTS

If the arguments are empty, claim a batch (maximum of 10) of the highest-severity
`open` findings. Arguments may name areas (`ui`, `input`, `game/campaign`)
and/or a batch size. You are the orchestrator in this session — read
`.opencode/agent/parity-orchestrator.md` and follow it exactly; never spawn a
subagent to run this command. In short:

1. Startup recovery: `reap --session <session> --older-than-minutes 0`, then
   dispatch this session's leftover `godot-open`/`godot-unverified` items.
2. Claim a batch of writer items, then work it one item at a time: spawn one
   `parity-writer` for the current item and wait. The writer fixes the code,
   runs the narrow check, commits `Fixes EV-####`, verifies the fix in the
   running Godot client itself (at most 3 rounds), and releases the item. Each
   subagent gets only that one item's JSON from `findings.json` — never the
   whole batch.
3. Final reap and summary. `godot-pass` items wait for `/eval-gaps`; run that
   command in this loop's session before `/merge-loops` and report how many
   items are waiting.
