# Parallel parity loops

Run several gap-identifier/evaluator/implementer parity loops at once. Each loop gets its own
X display, its own git worktree/branch, its own Godot editor bridge port, and
isolated client user-data. `computer-mcp` and `open-godot-mcp` cannot share an
X display: XTEST input, focus, and full-screen capture are global per display,
and on Linux the MCP's window-targeting tools are not implemented. Distinct
displays are what makes the Java legs truly parallel.

## Session orchestration

Start one loop session per worker (each in its own terminal) and run a workflow
command inside it:

- `/loop-gaps` — identify -> implement -> evaluate per finding, up to five
  cycles per item, until no unprocessed non-terminal item remains. This is the
  main driver; run several sessions for parallel throughput.
- `/evaluate-gaps` — drain the evaluator queue (`needs-evaluation`) one finding
  at a time. Run it alongside `/loop-gaps`; it waits while fixers are in flight.
- `/fix-gaps` — one swarm of `parity-writer` agents partitioned by area, each
  claiming findings until its areas are drained.
- `/seed-gaps` — one swarm of `gap-identifier` agents seeding new code-sourced
  candidates.

Every command launches a `parity-orchestrator` primary agent that only claims
findings and launches worker subagents (`gap-identifier`, `gap-loop`,
`parity-writer`, `evaluator`). `/loop-gaps` nests two levels
(orchestrator -> `gap-loop` -> leg worker), so the opencode config needs
`"subagent_depth": 2`; the default of 1 blocks the `gap-loop` worker chain with
"Subagent depth limit reached". Parallel sessions coordinate through the one
shared ledger: `claim` is atomic under the ledger flock, a finding with a fresh
`owner` is invisible to every other session, and `reap` restores claims left by
dead sessions. Worker loops run one item at a time per session, so the host
resources stay bounded by the number of loop sessions, not by the swarm size.

The workflow files (`/seed-gaps`, the new agents, `record_finding.py`) live in
`.opencode/` on `main`; a loop worktree only sees them after `main` is merged
into its `parity/loop-N` branch (or the worktree is recreated).

## Resource map

Loop ids are 1-based and encoded in environment variables (`lib/loop-vars.sh`):

| Loop | Display | Bridge port | Worktree | Branch |
|------|---------|-------------|----------|--------|
| 1 | inherited or `:10` (Xvfb) | 6970 | main checkout | main |
| 2 | `:11` (Xvfb) | 6980 | `../Mindustry-Godot-loop2` | `parity/loop-2` |
| 3 | `:12` (Xvfb) | 6990 | `../Mindustry-Godot-loop3` | `parity/loop-3` |
| N | `:(9+N)` (Xvfb) | `6970+10(N-1)` | `../Mindustry-Godot-loopN` | `parity/loop-N` |

All loops share the **main checkout's ledger** (`PARITY_LEDGER`). `record_finding.py`
flock-serializes writes, and `claim`/`release` make picking a finding atomic, so
concurrent loops never fix the same `EV-####`.

## Start a loop

Tooling is verified in the main checkout before use. From the repo root:

```bash
# loop 1 (main checkout; starts Xvfb :10 when the display is down)
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

Then run `/loop-gaps` (or `/evaluate-gaps`) in the session. The parity-eval
skill detects the loop env and isolates everything per loop. Scope arguments
(different `--area` prefixes or EV ids) are optional: the shared ledger already
keeps sessions off each other's findings, so disjoint scopes are a throughput
choice, not a correctness requirement.

## Isolation details

- **Display**: every loop runs on its own display; `start-loop.sh`, the
  `run-*.sh` wrappers and the MCP shims start an Xvfb at 1280x720x24 when the
  display is down (no WM needed). Loop 1 prefers an inherited `DISPLAY`, else
  `:10`; loops >= 2 always use `:(9+N)`. Captures via `capture_screen.py` are
  display-local. Do not run two clients on one display.
- **Bridge port**: the editor addon listens on `$PARITY_BRIDGE_PORT`
  (`OPEN_GODOT_MCP_PORT` wins over the shared EditorSettings); the MCP shim
  passes the same port via exact-port adoption. Do **not** pass `--projects` to
  the server and do **not** use `godot_instance launch_editor`: it allocates
  ports from its own local index (6970 first) and ignores the loop mapping.
- **MCP slots**: every process that uses MCP takes a lease through
  `../skills/parity-eval/scripts/mcp_slot.py` (`MCP_SLOT_LIMIT`, default 2,
  shared across worktrees through the git common dir); the guard denies
  `gap-identifier`, `gap-loop`, `parity-orchestrator` and `evaluator` once both
  slots are held, so a further consumer stays code-only until one frees.
  `/evaluate-gaps` checks `mcp_slot.py status` and waits rather than claiming
  work it cannot evaluate. `computer-mcp` imports
  `pynput` at module load, so
  the loop shims supply `DISPLAY=$PARITY_DISPLAY`; a plain SSH opencode session
  needs the display in its process env (or the global config `environment`) or
  the server is dropped silently.
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
  `PARITY_RUN_PREFIX` (`l2-…`) on run directories. In a loop worktree always
  call the helper through the main checkout:
  `RF="${PARITY_MAIN:-.}/.opencode/skills/parity-eval/scripts/record_finding.py"`.
- Queue statuses: `open` / `regression` / `verified-unfixed` are the fixer
  queue, `needs-evaluation` is the evaluator queue, `claimed` is the
  cross-session lock (a fresh `owner` hides the item from every picker),
  `verified-fixed` and `wontfix` are terminal. `verify` records the evaluator's
  engine verdicts (`verified-fixed` / `verified-unfixed` / `regression` /
  `wontfix`); `set-status` moves a claimed item between queues without dropping
  the claim; `release` drops it; `reap` recovers stale claims.
- Each loop commits on its own `parity/loop-N` branch (its `parity-writer`
  subagents commit with `Fixes EV-####`). Never rewrite another loop's branch.
  Merge/landing is a human decision.

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

`start-loop.sh` supports any number of loop ids. The host renders with Mesa
llvmpipe (CPU); run up to three loops and watch CPU, RAM and FPS (16 cores /
15.3 GiB here). Drop back a loop if an editor or game stops responding, if FPS
collapses, or if the host starts swapping. The MCP slot cap
(`MCP_SLOT_LIMIT`, default 2) bounds concurrent MCP consumers independently of
loop count; code-only gap-identifier and fixer agents run outside it.

Measured 2026-10-07 (`.opencode/evals/20261007-mcp-cap-test.md`):
idle editor ≈ 1.3 GB / 0.25 core, unpaused llvmpipe game ≈ 0.8 GB / ~5.6
cores, so a full loop ≈ 3.1 GB. Four editors + one game + three agent sessions
peaked at 11.0 GB used with no swap growth. Two evaluator Java legs plus two
editors peak at 9.2 GB used and 79% CPU busy, so CPU — not RAM — is what caps
concurrent twin-runs at two. The guard denies `gap-identifier`, `gap-loop`,
`parity-orchestrator` and `evaluator` at the cap by default
(`MCP_SLOT_GUARD_AGENTS` overrides); the interactive session is warn-and-allow.
