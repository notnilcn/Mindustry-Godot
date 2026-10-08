---
description: Claim godot-pass findings and run the Java-vs-Godot twin evaluation pass for this loop
---

Run one twin-eval cycle for this loop session.

Arguments: $ARGUMENTS

If the arguments are empty, claim a batch (maximum of 10) of this session's
highest-severity `godot-pass` findings. Arguments may name areas (`ui`, `input`,
`game/campaign`) and/or a batch size. Run this in the loop session that owns
the `godot-pass` items and before `/merge-loops`, so the code under review is
this loop's worktree. You are the orchestrator in this session — read
`.opencode/agent/twin-orchestrator.md` and follow it exactly; never spawn a
subagent to run this command. In short:

1. Startup recovery: `reap --session <session> --older-than-minutes 0`, then
   collect this session's retryable `twin-unverified` items.
2. Acquire the global twin lease once. Exit 3 means the slot is held — stop
   without claiming anything.
3. Claim a batch of this session's `godot-pass` items for `twin`.
4. Spawn one `twin-evaluator` per claimed item, sequentially, refreshing the
   lease token between items; release any unattempted claims back to
   `godot-pass`.
5. Release the lease, final reap and summary, then your output contract.
