# `.opencode/evals/` — parity evaluation ledger

Persistent state for the parity workflow: the `gap-identifier` writes
code-sourced candidates, the `evaluator` writes engine-sourced findings and
engine verdicts, and workers claim/release through `record_finding.py`. Nothing
here is game code.

## Layout

| Path | Contents |
|---|---|
| `findings.json` | Cumulative gap ledger. One record per player-visible parity gap. Machine-readable; only edited through `../skills/parity-eval/scripts/record_finding.py`. Records written before this format exist without `source`/`confidence`; both are optional on read. |
| `mcp-slots/` | Live MCP slot leases (`mcp_slot.py`), shared across worktrees through the git common dir. Gitignored. |
| `runs/<UTC-stamp>-<scenario>/` | One twin-run: `java/`, `godot/`, `diff/`, `report.md`, `run.json`. Local evidence; not committed. |
| `20261006-player-facing-gap-inventory.md` | Historical code-audit seed list (candidates, not findings). |

## Finding lifecycle

```
open ──claim --for fix──► claimed ──fixer commits──► needs-evaluation
                                                          │
                                          claim --for evaluation
                                                          ▼
                                                       claimed
                                                          │
                             evaluator re-runs the exact repro in-engine
                                                          ▼
      verified-fixed (expected behavior) ──re-repro──► regression
      verified-unfixed (gap still present after a fix attempt)
      needs-evaluation (blocked/deferred; released back to the queue)
      wontfix (accepted deviation)
```

- `open` — a candidate: code-sourced (file:line + repro sketch) or
  engine-sourced; no fresh evidence of a fix. Fixer queue.
- `needs-evaluation` — queued for the evaluator: seeded for in-engine triage, a
  fixer just committed, or a loop died before evaluation finished. Evaluator
  queue.
- `claimed` — **the cross-session lock.** One owner, a `claimed_at` stamp, and
  the `prev_status` to restore. A finding with a fresh owner is invisible to
  every picker; `reap` releases claims older than the TTL (default 90 min).
  A finding can carry a live owner while its status reads `needs-evaluation`
  (the crash-safe handoff before an evaluation leg).
- `verified-fixed` — evaluator re-ran the exact repro and observed the expected
  behavior; evidence path recorded. Terminal.
- `verified-unfixed` — evaluator re-ran the repro and confirmed the gap is
  still present after a fix attempt. Fixer queue.
- `regression` — a previously verified-fixed finding reproduced again. Fixer
  queue.
- `wontfix` — accepted deviation, platform limitation, duplicate, or code
  evidence shows the gap is closed; the note says which. Terminal.
- Severities: `S1` crash/hang/data loss; `S2` core flow broken; `S3`
  behavior/visual mismatch; `S4` polish.
- Legacy `in-progress` and `code-verified` records are normalized to
  `needs-evaluation` on load; `record_finding.py migrate` persists that mapping.

## Writers

| Command | Who |
|---|---|
| `add --source code` (status `open` or `needs-evaluation`), `set-status ... wontfix` | `gap-identifier` |
| `add --source engine`, `verify` (engine verdicts), `release` on a blocker | `evaluator` |
| `claim --for fix`, `set-status needs-evaluation`, `release` | `parity-writer` |
| `claim --for evaluation`, `reap` | `parity-orchestrator` (evaluate loops) |
| `claim --for loop`, `set-status`, `release` | `parity-orchestrator` + `gap-loop` |

Fixes are claimed in commit messages (`Fixes EV-0001`) and verified by the
evaluator in-engine. Commands: `/seed-gaps` fans out code-only discovery,
`/evaluate-gaps` drains the evaluator queue one finding at a time, `/fix-gaps`
runs one area-partitioned fixer swarm, and `/loop-gaps` drives
identify → implement → evaluate per finding (max five cycles per item) until
the ledger is terminal.

## Bootstrap

Environment setup is `../skills/parity-eval/scripts/bootstrap.sh`: the two MCP
servers are installed as `uv` tools (executables in `~/.local/bin`), and a
JDK 17 is used from `PATH`/`JAVA_HOME` or downloaded to
`~/.local/share/jdks/temurin-17`. `computer-mcp` imports `pynput` at module
load and needs `DISPLAY` at opencode process start; open-godot-mcp does not.
MCP use is slot-gated by `mcp_slot.py`. Never trust a doc claim in a report;
every finding carries its own evidence.
