# Parallel parity loops

Run several evaluator/implementer parity loops at once. Each loop gets its own
X display, its own git worktree/branch, its own Godot editor bridge port, and
isolated client user-data. `computer-mcp` and `open-godot-mcp` cannot share an
X display: XTEST input, focus, and full-screen capture are global per display,
and on Linux the MCP's window-targeting tools are not implemented. Distinct
displays are what makes the Java legs truly parallel.

## One-session orchestration (recommended)

Start a single `opencode --auto` in the main checkout and paste:

```
/parity-parallel 1=ui/menu 2=input --iterations 2
```

The primary agent is only an orchestrator: it calls
`.opencode/loops/bin/launch-parallel.sh`, which prepares each loop (Xvfb,
worktree, editor, MCP shims) and spawns one headless child process per loop:

```
opencode run --auto --agent loop-runner --dir <worktree> "<loop task>"
```

Each child is one opencode process with its own MCP servers (hence its own
display and bridge port), runs up to `--iterations K` iterations of the
parity loop (default 2), spawns its own `evaluator` subagents, and commits on
its own `parity/loop-N` branch. Nothing is merged or pushed. A plan can also
live in a file: `launch-parallel.sh --plan plans/current.plan`.

Monitor and stop:

```bash
.opencode/loops/bin/parallel-status.sh
.opencode/loops/bin/parallel-stop.sh 2        # or --all
```

The `loop-runner` agent is installed to `~/.config/opencode/agents/` by the
launcher so it is available in every worktree regardless of branch.

## Resource map

Loop ids are 1-based and encoded in environment variables (`lib/loop-vars.sh`):

| Loop | Display | Bridge port | Worktree | Branch |
|------|---------|-------------|----------|--------|
| 1 | current (`:10`) | 6970 | main checkout | main |
| 2 | `:11` (Xvfb) | 6980 | `../Mindustry-Godot-loop2` | `parity/loop-2` |
| 3 | `:12` (Xvfb) | 6990 | `../Mindustry-Godot-loop3` | `parity/loop-3` |
| N | `:(9+N)` (Xvfb) | `6970+10(N-1)` | `../Mindustry-Godot-loopN` | `parity/loop-N` |

All loops share the **main checkout's ledger** (`PARITY_LEDGER`). `record_finding.py`
flock-serializes writes, and `claim`/`release` make picking a finding atomic, so
two loops never fix the same `EV-####`.

## Start a loop

Tooling is verified in the main checkout before use. From the repo root:

```bash
# loop 1 (the main checkout, same behavior as before)
.opencode/loops/bin/start-loop.sh 1 --run

# loop 2, in a new terminal
.opencode/loops/bin/start-loop.sh 2 --run
```

`--run` execs `opencode` in the loop's worktree with `PARITY_*` exported and
`.opencode/loops/mcp-bin` prepended to `PATH`. The MCP entries in your global
config keep their plain names (`computer-mcp`, `open-godot-mcp`); the shims
translate `PARITY_DISPLAY`/`PARITY_BRIDGE_PORT` per session. Restarting
opencode is not needed.

Loop sessions launch with `opencode --auto` (auto-approve permission asks;
explicit deny rules still hold, e.g. the evaluator cannot edit game code or
commit). Pass `--no-auto` or set `PARITY_OPENCODE_AUTO=0` to keep approval
prompts. If you start opencode yourself instead of using `--run`, add `--auto`
the same way.

Inside the loop session, launch the clients per loop:

```bash
nohup .opencode/loops/bin/run-godot-editor.sh >"$PARITY_LOOP_DIR/logs/editor.log" 2>&1 &
.opencode/loops/bin/run-java.sh >"$PARITY_LOOP_DIR/logs/java.log" 2>&1 &
```

Then run `/parity-loop <scope>`. The parity-eval skill detects the loop env and
isolates everything per loop. Give each loop a disjoint scope (different
scenario ids or `--area` prefixes) so the work itself does not overlap.

## Isolation details

- **Display**: loops >= 2 run an Xvfb at 1280x720x24 (no WM needed). Captures
  via `capture_screen.py` are display-local. Do not run two clients on one
  display.
- **Bridge port**: the editor addon listens on `$PARITY_BRIDGE_PORT`
  (`OPEN_GODOT_MCP_PORT` wins over the shared EditorSettings); the MCP shim
  passes the same port via exact-port adoption. Do **not** pass `--projects` to
  the server and do **not** use `godot_instance launch_editor`: it allocates
  ports from its own local index (6970 first) and ignores the loop mapping.
- **User data**: `run-godot-editor.sh`/`run-java.sh` set per-loop `XDG_*` and a
  per-loop `HOME` for Java, so Godot screenshots (`user://mcp_screenshots`),
  editor settings, Mindustry settings/saves, and gameplay logs cannot collide.
  The real `~/.local/share/Mindustry` is copied in once as a seed.
- **Xvfb pointer quirk**: Xvfb resets the pointer to screen center when the
  XTEST client that moved it disconnects. The persistent computer-mcp MCP
  server keeps its connection, so MCP tool moves/clicks work; one-shot
  `computer-mcp mouse move` + separate `computer-mcp mouse click` does not.
  For scripted CLI clicks use `bin/parity-click.sh <x> <y> [button]`, which
  moves and clicks in one process.
- **Builds**: a new worktree seeds `client/bin/rust` and `client/.godot` from
  the main checkout, so `tools/build.sh` is incremental. Use
  `--no-seed-builds` to skip the copy.
- **DAP/LSP (6006/6005)**: Godot owns these via EditorSettings and the MCP env
  vars are inert; expect bind warnings when two editors run. The parity flows
  do not use them.
- **Multiplayer ENet port**: standalone games read it from project config; two
  loops running `mp_host_join` on the same host can collide. Keep multiplayer
  scenarios to one loop.

## Ledger and git

- `PARITY_LEDGER` points at the main checkout's `findings.json`; never edit the
  worktree copy. Evidence and reports live in `$PARITY_EVALS_DIR` with
  `PARITY_RUN_PREFIX` (`l2-…`) on run directories.
- The implementer claims a finding with
  `record_finding.py claim --area <scope> --owner loop-N` (atomic under lock),
  and `release`s it if the fix is abandoned. `add`/`verify` stay
  evaluator-only.
- Each loop commits on its own `parity/loop-N` branch. Never rewrite another
  loop's branch. Merge/landing is a human decision.

## Status and cleanup

```bash
.opencode/loops/bin/status-loops.sh
.opencode/loops/bin/stop-loop.sh 2                 # stop Xvfb
.opencode/loops/bin/stop-loop.sh 2 --remove-worktree --purge
```

`stop-loop.sh` never kills editors, games, or opencode sessions; quit those
from their sessions first. Never run `open-godot-mcp --shutdown-all`: it kills
every sibling loop's server on the host.

## Budget

The host renders with Mesa llvmpipe (CPU). Start with two loops and watch CPU
and FPS (16 cores / 14 GiB here); a third loop is likely to contend.
