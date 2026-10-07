# MCP-cap / host capacity test — 2026-10-07

- Goal: find the real concurrent-agent ceiling on this 16 GB / 16-core host, so
  `MCP_SLOT_LIMIT` (default 2) and the "up to three loops" budget are
  data-backed.
- Method: sampler (`monitor.sh`, 3 s rows in `metrics.csv`) while adding
  isolated Godot editors one at a time (own Xvfb display, bridge port, XDG
  dirs), then one unpaused game process. No Java reference leg in this test.
- Host baseline during test: 3 opencode sessions + 6 MCP servers ≈ 5.3 GB used,
  0.8–1.7 load, 2.68 GB swap already in use (from earlier work).

## Steady-state levels

| Level | godot procs | avail MB | godot RSS MB | CPU idle % | swap MB | load1 |
|---|---|---|---|---|---|---|
| baseline | 0 | 9960 | 0 | 96 | 2684 | 1.2 |
| +1 editor | 1 | 8712 | 1481 | 95 | 2683 | 0.8 |
| +2 editors | 2 | 7452 | 2965 | 93 | 2683 | 2.6 |
| +3 editors | 3 | 6108 | 4440 | 91 | 2683 | 2.8 |
| +4 editors | 4 | 4790 | 5923 | 90 | 2683 | 2.2 |
| +1 game process | 5 | 4322 | 6757 | 52 | 2684 | 7.4→13 |
| after kill | 0 | 10094 | 0 | 96 | 2684 | 9.2→7.9 |

## Per-component cost (measured)

| Component | RAM | CPU (llvmpipe) |
|---|---|---|
| Idle Godot editor + Xvfb | ~1.26 GB | ~0.23 core |
| Unpaused game process (menu spine) | ~0.83 GB | **~5.6 cores** |
| opencode session (idle/active) | ~0.7–0.9 GB | ~0.03–0.35 core |
| MCP server pair (open-godot + computer) | ~0.09 GB | ~0 |
| Xvfb | ~0.06 GB | ~0 |

Extrapolated full parity loop (editor + game + opencode + MCP):
**~3.1 GB RAM, up to ~6.4 cores while its game plays**.

## Findings

- **RAM is the binding constraint, not CPU**: 4 editors + 1 game + the test's
  own sessions peaked at 11.0 GB used with zero swap growth (15.3 GB total).
  Four full loops would land at ~14–16 GB and swap; three land at ~11–13 GB.
- **CPU saturates per game, not per agent**: one unpaused llvmpipe game burns
  ~5.6 of 16 cores; three simultaneously-playing loops would contend, which is
  why active evaluation should stay serialized within a loop and spread across
  displays rather than stacked.
- **Cap set to 2 (2026-10-07).** Two slots keep the editor + Java/Godot legs
  of an evaluator twin-run comfortable while still allowing two loops to hold
  MCP; the guard denies `gap-identifier`, `loop-runner` and `evaluator` beyond
  that (`MCP_SLOT_GUARD_AGENTS` overrides), and the interactive session stays
  warn-and-allow. See the evaluator twin-run measurement below.

## Evaluator twin-run measurement (2026-10-07, two loops)

Setup: 2 Godot editors (the two loops) + both Java Mindustry clients running
simultaneously — the worst case when both MCP slots are evaluators in the Java
leg — on Xvfb `:18`/`:19`, plus the same 3 opencode sessions.

| Phase | avail MB | used MB | CPU idle % | swap MB | load1 |
|---|---|---|---|---|---|
| baseline (3 sessions) | ~10100 | ~5200 | 94 | 2684 | ~1 |
| + 2 editors | 7462 | 7822 | 90 | 2684 | 1.7 |
| + 2 Java legs | 6112 | 9172 | 21 | 2684 | 12.9 |
| after cleanup | 10128 | 5157 | 94 | 2684 | 11.2 |

- Java client: ~0.81 GB RSS, ~5.0 cores (llvmpipe, main menu, unpaused).
- Peak: **9.2 GB used of 15.3 GB, swap flat, 79% CPU busy**.
- **CPU is the ceiling for concurrent evaluator twin-runs**: two active
  clients cost ~10–13 cores; a third would exceed the 16 available. RAM leaves
  comfortable headroom at two.
- This is why the cap is 2: two loops can evaluate concurrently, and a third
  stays code-only until one releases.

## Artifacts

Raw run evidence (local, gitignored) under
`.opencode/evals/runs/20261007-mcp-cap-test/` and
`.opencode/evals/runs/20261007-evaluator-mem-test/`:

- `metrics.csv` — 3 s samples (ts, avail/used/swap MB, cpu idle, load, RSS sums).
- `monitor.sh` — sampler; `metrics-broken-cpu-parser.csv` — first sampler
  attempt with a vmstat column bug (superseded).
- `editor1..4.log`, `game1.log`, `xvfb1[4-7].log` — process logs.
