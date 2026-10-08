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

## 2026-10-08 — loop-1 twin Java leg (EV-0048): custom-game rules dialog input, lagging call screenshots, pynput scroll

- The Custom Rules dialog drops injected keyboard input: typing `10` into the
  "Initial Wave Spacing" number field (and into the dialog's own search box)
  produced no visible change, while the Custom Game map-search field had accepted
  typed text earlier in the same session. Workaround: leave numeric rule fields
  alone and trigger the wave with the HUD skip button (survival). A single
  unverified click on the "Waves" checkbox silently turned waves off; only the
  empty in-game status pane revealed it — read the in-game HUD after any rules
  edit.
- The screenshot attached to `computer-mcp_click` / `mouse_move` / `key_press`
  is the previous frame (menus/submenus/highlights appear one call late). Always
  take a separate `computer-mcp_screenshot` before acting on what a call
  returned.
- computer-mcp has no scroll tool and `xdotool` is absent; zooming the Java
  camera worked with a one-shot pynput scroll from the computer-mcp venv —
  `DISPLAY=:N "$MCP_VENV/bin/python" -c "from pynput.mouse import Controller;
  Controller().scroll(0,-6)"` (discrete scroll events do not need the persistent
  pointer that one-shot moves do).
- Pre-check a Java wave reference before driving: `maps/serpulo/groundZero.msav`
  decompresses (zlib) to rules with `waveTimer:false, waveSending:false`; the
  campaign sector's waves are world-processor/objective-flag gated and the HUD
  skip button is disabled. Reusable custom-survival sequence:
  `chains/java-custom-survival-wave.md`.
- Evidence: `runs/20261008-040830-ev0048-live-unit-runtime-twin/java/notes.md`.

## 2026-10-08 — loop-1 twin Java leg (EV-0062): held Escape auto-repeats and re-toggles the pause dialog

- Opening the Java pause dialog: the first `key_press escape` after a mouse
  click was swallowed by window focus (known), the second opened the dialog,
  but `computer-mcp_key_press` held the key long enough for X auto-repeat to
  fire another Escape, closing the dialog again before the next capture (the
  game was running ~3 s later). Split `computer-mcp_key_down` +
  `computer-mcp_key_up` in two calls opened the dialog and it stayed open.
- The screenshot attached to `click`/`mouse_move` is still the previous frame
  (re-confirmed); always capture separately before judging what a click did.
- Evidence: `runs/20261008-143432-ev0062-twin/java/` (`notes.md`; step-06 is
  the open pause dialog after the split key events, step-07 the settings
  dialog). Ground-truth path added to `chains/java-reference-leg.md`.

## 2026-10-08 — loop-2 twin Java leg (EV-0044): `--window-title` capture needs xdotool; first-run campaign dialog

- `capture_screen.py --window-title Mindustry` printed `window not found via
  xdotool; captured full monitor` (xdotool/wmctrl absent on this host) and
  returned the whole 1280x720 screen. Workaround that worked: read
  `DISPLAY=:11 xwininfo -root -children` for `"Mindustry": 900x700+190+10`, then
  capture with `--rect 190,10,900,700`. `chains/java-reference-leg.md` step 3
  now records this.
- On a **fresh** Java data dir, Play -> Campaign opens a `Select Starting
  Campaign` dialog (Serpulo/Erekir planet photos) before the planet page; the
  click must land on the planet photo (Serpulo at (477,310)), then OK (640,673),
  then the usual Ground Zero Launch (640,677). Missing the photo leaves Erekir
  selected. Route recorded in `chains/java-reference-leg.md`.
- Split `key_down`/`key_up` calls for a plain `shift` hold/release worked; the
  response screenshot still lags one frame (known).
- Evidence: `runs/l2-20261008-150205-input_controls-twin/java/notes.md`
  (JVM pid 190927, 900x700).

## 2026-10-08 — loop-2 twin Java leg (EV-0055): timed key combos, no scroll tool, catalog index vs grid

- The in-game number-key block-select combo (`PlacementFragment.updatePick`)
  requires the category key and the tens/units keys inside a 400 ms window;
  `computer-mcp_key_press` round trips routinely exceed it, so a combo fired as
  three separate calls degenerates into three successive *category* selections
  (observed: 3 -> distribution, 1 -> turret, 4 -> liquid). Workaround: one
  persistent pynput process sending the whole combo with 40/70 ms gaps
  (`runs/l2-20261008-051657-ev0055-rts-select-twin/java/key_seq.py`), the same
  persistent-connection idea as `parity-click.sh` for clicks.
- computer-mcp exposes no scroll-wheel action, so a Java ScrollPane cannot be
  scrolled directly. The block catalog's hidden entries were reachable with the
  number combo instead; the Payload Source item picker has its own search field
  (ItemSelection adds one when the list is long), where "flare" returned 0 hits
  and "stell" 1 (the Erekir-planet map filters serpulo content). Select by combo
  and read the top table's block name — the visible catalog grid may be scrolled
  from a previous session (`blockPane.setScrollYForce`) and cannot be indexed
  by eye.
- The response screenshot still lags one action (known); every judgment in this
  run used a separate `capture_screen.py` crop.
- Evidence: `runs/l2-20261008-051657-ev0055-rts-select-twin/java/notes.md`.
