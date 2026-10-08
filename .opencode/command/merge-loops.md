---
description: Merge finished parity loop branches into main
---

Merge a finished parity loop branch into `main`.

Arguments: $ARGUMENTS

Arguments may name a loop (a number like `2`, or a branch like `parity/loop-2`)
and/or pass `--skip-ci`. With no loop named, merge each `parity/loop-*` branch
that has commits not on `main`, one at a time.

In the main checkout, inspect `git log main..<branch>` and
`python3 .opencode/skills/parity-eval/scripts/record_finding.py list --session
loop-<N>` before merging. First commit any pending main workflow/ledger changes
and the uncommitted `.opencode/chains/` edits in `../Mindustry-Godot-loop<N>`.
A branch with zero commits past main is already merged: report that and skip
the `git merge --no-ff <branch>` step; otherwise merge and resolve conflicts.
Then run `tools/ci.sh` unless `--skip-ci` was passed — skipping is acceptable
when the merge carries no compiled code (workflow or ledger commits only) —
and state the skip in the report.

Sync the branch back from main with
`git -C ../Mindustry-Godot-loop<N> merge main`: a restarted loop recreates its
worktree from the branch, and `start-loop.sh` refuses to start while its
`.opencode`/`AGENTS.md` files drift from main. When the worktree is already
gone and the branch still needs the sync, `git branch -f parity/loop-<N> main`
updates it in place (only while the branch is not checked out).

Do not stop the loop or remove its worktree; that is the loop session's or
operator's call. Do not push. Report the commit list, conflicts, and check
result (or the CI skip).
