---
description: Claim open parity findings and run the writer/evaluator/twin fix cycle for this loop
agent: parity-orchestrator
subtask: true
---

Run one fix cycle for this loop session.

Arguments: $ARGUMENTS

If the arguments are empty, claim a small batch (3) of the highest-severity
`open` findings. Arguments may name areas (`ui`, `input`, `game/campaign`)
and/or a batch size. Follow your agent contract exactly:

1. Startup recovery: `reap --session <session> --older-than-minutes 0`, then
   dispatch this session's leftover `godot-open`/`godot-unverified` items.
2. Claim a writer batch and run each finding through `parity-writer` and
   `parity-evaluator`, at most 3 rounds per finding.
3. Twin phase: acquire the global twin lease, spawn `twin-evaluator`, re-check
   for leftover `godot-pass` items, release the lease.
4. Final reap and summary, then your output contract.
