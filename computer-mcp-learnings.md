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

## 2026-10-08 — loop-2 twin Java leg: worktree-local loop dir, detached launch, no window management on Linux

- The loop wrappers resolve `PARITY_LOOP_DIR` against the **worktree-local**
  `.opencode/loops`, so the Java client's data lives under
  `<worktree>/.opencode/loops/run/loop-N/xdg-data/Mindustry` (Arc honours
  `XDG_DATA_HOME`) — not the `$PARITY_LOOP_DIR` exported into the session
  environment, whose `home/.local/share/Mindustry` copy is unused. Save files
  were looked for in the wrong tree until `find` showed the real path.
- Launching the reference with `nohup .opencode/loops/bin/run-java.sh &` inside a
  shell-tool command does not detach: the tool timeout kills the process group
  and the JVM with it. `setsid .opencode/loops/bin/run-java.sh
  >"$RUN/java/game.log" 2>&1 </dev/null &` survives; the java pid is the
  `pgrep -f Mindustry.jar` match, not the wrapper pid.
- `computer-mcp_list_windows` / window-geometry actions are not implemented on
  Linux (`{"error":"Window management not yet implemented for Linux"}`). Use
  `DISPLAY=:N xwininfo -root -children` for the window origin/size and
  `capture_screen.py --window-title Mindustry` (run with the computer-mcp uv
  tool's python, which has mss) for window-cropped captures. Click coords are
  screen coords = window origin + element offset read from the screenshot.
- The Xvfb Java window needs a key press to take focus first: the first Escape/J
  is a no-op, send the binding twice. A `button_down` on the world can also stay
  logically held across later calls (manual mining kept adding copper; placement
  clicks were swallowed by the ongoing drag) — send an explicit `button_up`
  before expecting a fresh click to act.
- Desktop campaign pause menu has no Save Game (Save & Quit only); the sector
  save is written by `playNewSector` at launch. `kill -TERM` after capture left
  the loop display clean (only loop-1's client remained).
- Evidence: `runs/l2-20261007-163042-twin-java-ref/` (sequence now in
  `chains/java-reference-leg.md`), plus the three settled twin runs
  `l2-20261007-170406-ev0037-core-launch-twin`,
  `l2-20261007-171145-ev0052-research-twin`,
  `l2-20261007-172706-ev0041-campaign-rules-twin`.

## 2026-10-08 — twin sweep 2 (loop-2): no new Java-leg friction

- No Java client was launched this sweep: EV-0054 reused the fresh first-sweep
  Java leg (`runs/l2-20261007-163042-twin-java-ref`) because it covers the
  expected behavior, and EV-0047 could not add a placed-drill Java capture
  (this session's model cannot accept images and tesseract is absent, so the
  OS-driven client could not be driven to a blind placement/read-back). No new
  computer-mcp difficulty to report; the existing `java-reference-leg` chain
  notes still hold.
- Settlement runs: `runs/l2-20261008-034014-ev0054-load-game-twin`,
  `runs/l2-20261008-034337-ev0047-production-twin` (both twin-verified).
