# Parity loops

One loop is one isolated client session: its own displays, editor bridge port,
git worktree (loops ≥ 2), user-data dirs, and run-dir prefix. Parallel loops
never share a display, and the shared ledger (`.opencode/evals/findings.json`)
is the only cross-loop coordination point.

Each loop runs two display servers. Xvfb (the `PARITY_DISPLAY` X display) backs
the Java reference and X11-only tooling. A headless `weston` compositor with
the GL renderer hosts the Godot editor and games on a Wayland socket
(`PARITY_WAYLAND_SOCKET`, e.g. `wayland-mind1`) with its runtime dir under
`PARITY_WAYLAND_DIR` (`run/loop-N/wayland`). Godot on Xvfb falls back to
llvmpipe software rendering because Xvfb has no DRI3; weston keeps Godot on the
GPU. Install weston once (`apt install weston`); without it the editor falls
back to Xvfb, and `PARITY_GODOT_DISPLAY=x11` forces that path.

## Usage

```bash
.opencode/loops/bin/start-loop.sh 1            # prepare loop 1, print env
.opencode/loops/bin/start-loop.sh 2 --run      # prepare + exec opencode --auto
.opencode/loops/bin/status-loops.sh            # per-loop processes/ports
.opencode/loops/bin/stop-loop.sh 2             # stop Xvfb/weston for one loop
```

`--run` launches opencode in the loop worktree with `--auto` (explicit deny
rules still hold). Pass `--no-auto` to keep permission prompts. The launch
helpers start the loop's Xvfb and weston when down; `run-godot-editor.sh` runs
the editor on the loop's weston (GPU) and `run-java.sh` runs the Java reference
on the X display, both with the loop's bridge port and user-data isolation.
`start-loop.sh` exports `WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR` for the session, so
a bare `godot` launch also lands on the GPU compositor.

## Loop map

| Loop | Xvfb display | Godot Wayland socket | Bridge | Worktree | Branch | Run prefix |
|---|---|---|---|---|---|---|
| 1 | inherited or `:10` | `wayland-mind1` | 6970 | the main checkout | current | `""` |
| N ≥ 2 | `:(9+N)` | `wayland-mindN` | `6970+10(N-1)` | `../Mindustry-Godot-loopN` | `parity/loop-N` | `lN-` |

`loop-vars.sh` exports `PARITY_LOOP`, `PARITY_DISPLAY`, `PARITY_WAYLAND_SOCKET`,
`PARITY_WAYLAND_DIR`, `PARITY_BRIDGE_PORT`, `PARITY_DAP_PORT`, `PARITY_LSP_PORT`,
`PARITY_WORKTREE`, `PARITY_EVALS_DIR`, `PARITY_LEDGER`, `PARITY_RUN_PREFIX`,
`PARITY_LOOP_DIR`, and `PARITY_MCP_BIN`. The `mcp-bin/` shims are prepended to
`PATH`, pinning computer-mcp to the loop X display and open-godot-mcp to the
loop bridge port, Wayland socket and XDG dirs.

## Workflow integration

- `/seed-gaps` seeds the shared ledger with code-sourced `open` candidates,
  each carrying a fix plan in its `plan` field, and does not need a loop.
- `/fix-gaps` runs per loop. The ledger `session` label is `$PARITY_SESSION`
  or `loop-$PARITY_LOOP`; a loop works its claimed batch one item at a time,
  and its `parity-writer` fixes each item and verifies it in the running Godot
  client before releasing it `godot-pass`.
- `/eval-gaps` runs in the same loop session before `/merge-loops`: it claims
  that session's `godot-pass` items for twin and drives one twin evaluator per
  item under the global lease (`.opencode/evals/twin-evaluator`, one item at a
  time across sessions).
- `/merge-loops` runs from the main checkout after a loop's fix cycle returns:
  it merges the loop branch into `main`, runs `tools/ci.sh`, syncs the loop
  worktree back from main, and tears the worktree and its runtime dir down
  with `stop-loop.sh --remove-worktree --purge` (the loop branch stays).
- A restarted loop recovers its own dead claims with
  `record_finding.py reap --session loop-N --older-than-minutes 0`.
- `start-loop.sh` refuses to launch when the worktree's workflow files
  (`.opencode/{agent,command,skills,plugin,loops}` or `AGENTS.md`) differ from
  the main checkout: the session loads commands and agents from the worktree,
  while `record_finding.py` and the shared ledger resolve to main, so drift
  makes a command call a helper with a different CLI. Commit workflow changes
  on main and merge main into the loop branch before starting; `--allow-drift`
  overrides the check.
- Launch clients only through `run-godot-editor.sh` / `run-java.sh`; never use
  `godot_instance launch_editor` (it ignores the loop port map) and never run
  `open-godot-mcp --shutdown-all` (it kills sibling loops).
- MCP is capped at two concurrent consumers (`MCP_SLOT_LIMIT`), so an active
  twin run plus one loop's Godot evaluation is the intended ceiling.
