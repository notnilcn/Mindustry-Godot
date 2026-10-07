# `.opencode/evals/` — parity evaluation ledger

Persistent state for the parity loop: the `gap-identifier` writes code-sourced
candidates, the `evaluator` writes engine-sourced findings and final verdicts,
and implementers claim/release. Nothing here is game code.

## Layout

| Path | Contents |
|---|---|
| `findings.json` | Cumulative gap ledger. One record per player-visible parity gap. Machine-readable; only edited through `../skills/parity-eval/scripts/record_finding.py`. Records written before this format exist without `source`/`confidence`; both are optional on read. |
| `mcp-slots/` | Live MCP slot leases (`mcp_slot.py`), shared across worktrees through the git common dir. Gitignored. |
| `runs/<UTC-stamp>-<scenario>/` | One twin-run: `java/`, `godot/`, `diff/`, `report.md`, `run.json`. Local evidence; not committed. |
| `20261006-player-facing-gap-inventory.md` | Historical code-audit seed list (candidates, not findings). |

## Finding lifecycle

```
open ──fixer claims──► in-progress ──gap identifier reads diff──► code-verified
  ▲                                                                    │
  │                                          evaluator re-runs repro   │
  └──────────── release ────────────────┬─────────────────────────────┘
                                        ▼
                                 verified-fixed ──► regression (re-repro)
```

- `open` — a candidate: code-sourced (file:line + repro sketch) or
  engine-sourced; no fresh evidence of a fix.
- `in-progress` — claimed by one owner; released if the fix is abandoned.
- `code-verified` — the gap identifier read the committed diff and it closes
  the code seam; no in-engine evidence yet.
- `verified-fixed` — the evaluator re-ran the exact repro in-engine and the
  expected behavior was observed; evidence path recorded.
- `regression` — previously verified-fixed, reproduced again.
- `wontfix` — accepted deviation, platform limitation, duplicate, or the code
  reading shows the gap is closed; the note says which.
- Severities: `S1` crash/hang/data loss; `S2` core flow broken; `S3`
  behavior/visual mismatch; `S4` polish.

## Writers

| Command | Who |
|---|---|
| `add --source code`, `code-verify`, `wontfix` | `gap-identifier` |
| `add --source engine`, `verify` (final verdict) | `evaluator` |
| `claim`, `release` | implementers / loop runners |

Fixes are claimed in commit messages (`Fixes EV-0001`) and verified by the
evaluator. Implementers read the ledger with `record_finding.py list` and work
one finding at a time. `/parity-gap` fans out discovery, `/parity-loop` drives
one identify → fix → code-verify → final-verify iteration, and `/parity-eval`
runs the evaluation half alone.

## Bootstrap

Environment setup is `../skills/parity-eval/scripts/bootstrap.sh`: the two MCP
servers are installed as `uv` tools (executables in `~/.local/bin`), and a
JDK 17 is used from `PATH`/`JAVA_HOME` or downloaded to
`~/.local/share/jdks/temurin-17`. `computer-mcp` imports `pynput` at module
load and needs `DISPLAY` at opencode process start; open-godot-mcp does not.
MCP use is slot-gated by `mcp_slot.py`. Never trust a doc claim in a report;
every finding carries its own evidence.
