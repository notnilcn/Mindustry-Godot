---
description: Claim open parity findings and run the writer/evaluator/twin fix cycle for this loop
agent: parity-orchestrator
subtask: true
---

Run one fix cycle for this loop session.

Arguments: $ARGUMENTS

If the arguments are empty, claim a batch (maximum of 10) of the highest-severity
`open` findings. Arguments may name areas (`ui`, `input`, `game/campaign`)
and/or a batch size. Follow your agent contract exactly:

1. Startup recovery: `reap --session <session> --older-than-minutes 0`, then
   dispatch this session's leftover `godot-open`/`godot-unverified` items.
2. Claim a batch of writer items, then work it one item at a time: finish the
   current item's writer/evaluator rounds (at most 3) before spawning anything
   for the next item. Each subagent gets only that one item's JSON from
   `findings.json` — never the whole batch.
3. Twin phase: snapshot this session's `godot-pass` items and acquire the
   global twin lease once. Exit 3 means the slot is held — skip the twin phase
   and stop. Otherwise spawn one `twin-evaluator` per item, sequentially, then
   release the lease.
4. Final reap and summary, then your output contract.
