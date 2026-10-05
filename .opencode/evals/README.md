# `.opencode/evals/` — parity evaluation ledger

Persistent state for the `evaluator` agent and the `/parity-eval` and
`/parity-loop` commands. Nothing here is game code; the evaluator is the only
writer.

## Layout

| Path | Contents |
|---|---|
| `findings.json` | Cumulative gap ledger. One record per player-visible parity gap. Machine-readable; only edited through `../skills/parity-eval/scripts/record_finding.py`. |
| `runs/<UTC-stamp>-<scenario>/` | One twin-run: `java/`, `godot/`, `diff/`, `report.md`, `run.json`. Local evidence; not committed. |

## Finding lifecycle

```
open ──fix claimed──► open ──evaluator re-runs repro──► verified-fixed
  ▲                                                        │
  └──────────────── regression ─────────────────────────────┘
```

- `open` — reproduced, no fresh evidence of a fix.
- `verified-fixed` — the exact repro was re-run in-engine and the expected
  behavior was observed; evidence path recorded.
- `regression` — previously verified-fixed, reproduced again.
- Severities: `S1` crash/hang/data loss; `S2` core flow broken; `S3`
  behavior/visual mismatch; `S4` polish.

Fixes are claimed in commit messages (`Fixes EV-0001`) and verified by the
evaluator. Implementers do not edit this ledger; they read it with
`record_finding.py list` and work one finding at a time. The `/parity-loop`
command drives that fix→verify iteration; `/parity-eval` runs the evaluation
half alone.

## Bootstrap

Environment setup is `../skills/parity-eval/scripts/bootstrap.sh`: the two MCP
servers are installed as `uv` tools (executables in `~/.local/bin`), and a
JDK 17 is used from `PATH`/`JAVA_HOME` or downloaded to
`~/.local/share/jdks/temurin-17`. Never trust a doc claim in a report; every
finding carries its own evidence.
