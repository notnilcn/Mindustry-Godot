# computer-mcp learnings

Operational friction hit while driving the Java reference through computer-mcp
(the twin evaluator's Java leg), with the workaround that got past it. Reusable
sequences belong in `.opencode/chains/` (`java-reference-leg.md`); this file is
for the friction that is not a chain.

The `twin-evaluator` appends a dated entry after a run that taught it
something. Never rewrite another session's entry; keep it to what happened,
what worked, and the run directory or chain that shows it. If nothing new
happened, no entry.

## 2026-10-07 — Seed notes from the parity-eval skill

- computer-mcp imports `pynput` at module load: without `DISPLAY` at opencode
  start the server exits and opencode silently drops `computer-mcp_*` tools.
  The Java leg cannot run; report the blocker instead of improvising a headless
  leg.
- On Xvfb a one-shot `computer-mcp mouse move` snaps back to screen center when
  the XTEST client disconnects. Use the persistent MCP tools, or
  `.opencode/loops/bin/parity-click.sh <x> <y> [button]` (single process).
- Xvfb has no window manager: run one client at a time on the loop display and
  derive click coordinates from the current screenshot at the recorded window
  size — never hardcode them.
- The reference settings live in the loop's seeded HOME
  (`$PARITY_LOOP_DIR/home`); normalize window size, UI scale and language, and
  record the values in `run.json`.
- Readiness is a log line (`Total time to load`), not a fixed sleep.
