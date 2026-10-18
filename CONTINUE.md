<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Continue round L

Round L is paused with a clean continuation point. Read
[`docs/L-round-continuation-handoff.md`](docs/L-round-continuation-handoff.md)
first; it lists what landed, the remaining work in order, the exact gates, and
harness pitfalls.

Suggested opening prompt:

> Continue round L from `docs/L-round-continuation-handoff.md`. Run
> `bash tools/build.sh` once, start session 1, and work §4 in order. Start with
> the live acceptance batch; then L04 T1.

Do not commit the operator's archived-docs deletions or the session-state files
under `.opencode/parity-session/` and `.claude/worktrees/`.
