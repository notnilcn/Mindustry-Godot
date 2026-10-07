---
description: Merge finished parity loop branches into main
---

Merge a finished parity loop branch into `main`.

Arguments: $ARGUMENTS

If the arguments name a loop (a number like `2`, or a branch like
`parity/loop-2`), merge that one; otherwise merge each `parity/loop-*` branch
that has commits not on `main`, one at a time.

In the main checkout, merge the branch after inspecting
`git log main..<branch>` and
`python3 .opencode/skills/parity-eval/scripts/record_finding.py list --session
loop-<N>`. First commit any pending main workflow/ledger changes and the
uncommitted `.opencode/chains/` edits in `../Mindustry-Godot-loop<N>`; then
`git merge --no-ff <branch>`, resolve conflicts, run `tools/ci.sh`, and finally
`git -C ../Mindustry-Godot-loop<N> merge main`. Do not push. Report the commit
list, conflicts, and check result.
