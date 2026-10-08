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

## 2026-10-08 — Java leg on the GPU: java weston + Xwayland; root captures are black

- The Java reference now runs on a dedicated headless `weston` + Xwayland
  compositor (`mindN-java` socket; `PARITY_DISPLAY` is the discovered Xwayland
  display, e.g. `:1`), so the client logs `AMD Radeon 780M (radeonsi ...)` and
  8.4 GB VRAM instead of llvmpipe. Xvfb stays as the software fallback
  (`PARITY_JAVA_DISPLAY=x11` or missing weston/Xwayland).
- On Xwayland the X root window holds no GPU pixels: `mss`/`computer-mcp
  screenshot` and root crops (`--rect`) come back pure black. Window capture
  works: `capture_screen.py --window-title Mindustry` finds the window through
  python-xlib (no xdotool) and reads the live client area with XGetImage; a
  bare capture retries the window when the monitor grab is blank, and the
  report carries `capture`/`region`/`note`/`window_id`.
- XTest input (computer-mcp and `parity-click.sh`) lands unchanged under
  Xwayland, and Weston's XWM manages the window. The Xvfb "first key press only
  focuses" caveat is scoped to the Xvfb fallback.
- Sessions started before the change still have computer-mcp pinned to the old
  Xvfb `:10`; restart loop sessions (`start-loop.sh`) so the shim resolves the
  Xwayland display. `status-loops.sh` prints the resolved display and server
  (`xway(pid N)` vs `xvfb(pid N)`).

## 2026-10-08 — EV-0059 twin Java leg: stale-pointer clicks, dead MapPlay bottom row, no text input

- **Pointer-move/click race.** `parity-click.sh` (pynput `position = ...`,
  ~50 ms sleep, `click`) and early `computer-mcp_click` sequences can click at
  the *previous* cursor position when Xwayland is busy: a click aimed at the
  Archipelago card (357,235 window) opened Debris Field, and several "dead
  buttons" were this race, not dead UI. Workaround: read the cursor back
  (`pynput.mouse.Controller().position`), retry the move until it reports the
  target within 1 px, then press/release with ~120 ms gaps (the
  `/tmp/opencode/click.py` pattern). Evidence:
  `runs/20261008-105728-ev0059-host-twin/java/retry-*.png`,
  `step-24d-load.png`.
- **MapPlayDialog's bottom `Back`/`Play` row did not respond to synthetic
  clicks at 1152x648.** With the Custom Game map dialog open, the rendered row
  at window y~555 never took hover/pressed state or activation even with a
  verified pointer; the dialog's upper content (gamemode buttons, Customize
  Rules) did respond. Reach an in-game state through `Play -> Load Game -> save
  card` instead (the whole slot card is clickable and loads). Evidence:
  `runs/20261008-105728-ev0059-host-twin/java/step-20-after-play.png` vs
  `step-24d-load.png`.
- **Java text fields take no characters.** `computer-mcp_type` and individual
  `key_press` move the focus caret but never append text (same class as the
  known Custom Rules dialog friction). To set the player name, write it into
  `xdg-data/Mindustry/settings.bin` offline with a small Java program against
  the Arc jar (`new arc.Settings(); s.setDataDirectory(new Fi(dir)); s.load();
  s.put("name", "tester"); s.forceSave();`) while the client is stopped, then
  relaunch. Evidence:
  `runs/20261008-105728-ev0059-host-twin/java/step-27-host-dialog.png`.
- **Never `pkill -f "<jar name>"`.** The pattern matches the executing shell's
  own command line; `pkill -TERM -f Mindustry.jar` killed the tool shell
  mid-teardown (the command "timed out" while its shell was already dead) and
  can hit sibling loops' clients. Kill only recorded pids; probe with
  bracket patterns (`pgrep -f 'Mindustry[.]jar'`).
- Evidence run dir: `runs/20261008-105728-ev0059-host-twin`.

## 2026-10-08 — EV-0060 twin Java leg: join-dialog bottom row dead, one-time popups, dedicated-server recipe

- The JoinDialog bottom row (`Back` / `Add Server` / `?`) did not respond to
  synthetic clicks at 1152x648: three clicks at the rendered centers produced
  byte-identical captures (`sha256 6e117fad…`, `step-12/13b/14`), while dialog
  content above (Local Servers row, community cards, collapse glyph) responded.
  Same class as the EV-0059 MapPlayDialog note. Workaround: connect through the
  discovered **Local Servers** row (window ~568,250), which drives the same
  `safeConnect` → `connect` path.
- The populated Community Servers list overlaps the bottom row: a click at the
  Add Server position hit a community card instead, raising the one-time
  `@servers.disclaimer` and `safeConnect`-ing to a public server that rejects a
  custom build (`This server does not support custom builds`). The first open
  also raises `@join.info`; both popups intercept the first clicks.
- A vanilla dedicated server is buildable and joinable in seconds:
  `gradlew server:dist` → `server-release.jar`, started with `(sleep 6; echo
  host; sleep 7200) | XDG_DATA_HOME=… java -jar server-release.jar` (with
  `< /dev/null` it prints `Server loaded` but never listens). The client's LAN
  discovery found it and the Local row listed `Server vCustom Build / 0/30 /
  Map: Triad / Survival / 8ms`; connecting logged `Connecting to server:
  /192.168.20.213:6567` + `Received world data: 29.4 kB`, server log `tester
  has connected.`, `ss` ESTABLISHED. Sequence in
  `chains/java-join-dedicated-server.md`.
- The `ip` prefill for the join dialog uses the same offline Arc Settings edit
  as the player name (`SetIp.java`: `s.put("ip", "127.0.0.1")`).
- Evidence: `runs/20261008-112749-ev0060-join-twin/` (`java/notes.md`,
  `step-05-join-dialog-clean.png`, `step-15-connect-local.png`,
  `step-16-in-game.png`, `dedicated-server.log`).

## 2026-10-08 — loop-1 Java leg: SDL_GL_SwapWindow freeze on a stale Xwayland compositor; compositor restart kills the MCP server's X connection

- The loop-1 `mind1-java` weston/Xwayland (up since 20:54) rendered a few
  minutes of the Mindustry menu, then froze: the window stopped changing and
  ignored clicks/F12 while the process stayed alive at ~3% CPU. `jstack`
  showed the main thread RUNNABLE in
  `arc.backend.sdl.jni.SDL.SDL_GL_SwapWindow`
  (`arc.backend.sdl.SdlApplication.loop`). A one-shot `xev` on the same display
  received XTest clicks during the freeze, so injection was fine — the buffer
  swap was stuck. Killing the client + the stale compositor and relaunching
  through `run-java.sh` fixed it: pid 72553 animated continuously and accepted
  every click for the rest of the leg.
- Killing the compositor took the persistent computer-mcp server's X connection
  with it: `computer-mcp_mouse_move` returned
  `Display connection closed by server: [Errno 32] Broken pipe`. `parity-click.sh`
  (one process per click) and small pynput scripts kept working on the new
  display and completed the whole Java leg. Do not kill a loop compositor
  mid-leg unless MCP can be restarted; otherwise switch to one-shots.
- Click coordinates: an attached PNG can be scaled in the model's view, so a
  remembered image y can be ~50 px off (a click meant for `Go` opened the
  `Stats` dialog). Locate buttons by moving the pointer to a candidate and
  diffing two window captures (hover highlight) — a small PIL changed-pixel
  bbox pins the real rect; `xwininfo -root -children` +
  `capture_screen.py --window-title Mindustry` give screen = window + origin.
- Evidence: `runs/20261008-215055-ev0061-game-over-twin/` (`java/notes.md`,
  `java/game.log`), sequence in `chains/java-campaign-loss.md`.

## 2026-10-08 — loop-1 EV-0038 twin Java leg: server already Broken pipe; first click after launch only focuses

- The persistent computer-mcp server's X connection was dead before the first
  call of the leg: every `computer-mcp_mouse_move` returned
  `Display connection closed by server: [Errno 32] Broken pipe`, while the loop
  display `:2` was alive (`capture_screen.py --window-title Mindustry` window
  grabs were non-blank, `xwininfo -root -children` answered). This is the
  documented after-effect of a compositor restart between sessions
  (`computer-mcp-learnings.md` 2026-10-08 EV-0061 entry); there is no in-leg
  restart, so `parity-click.sh` (one process per click) plus pynput one-shots
  for moves completed the whole Java leg.
- A freshly launched Java window consumed the first click of a new UI element
  as focus/hover only: the first `Editor` click painted the row highlight but
  did not open the Maps grid, and the first `Archipelago` click hovered the
  card without opening Map Info; the second click on the same coordinate acted.
  Budget one focus click after launch, or send each first click twice.
- Evidence: `runs/20261008-121029-ev0038-editor-row-twin/` (`java/game.log`,
  `java/step-02-after-editor-click.png`, `java/step-03-editor-maps.png`,
  `java/step-05-archipelago-info.png`); flow now in
  `chains/java-reference-leg.md`.

## 2026-10-08 — loop-1 EV-0039 twin Java leg: Broken pipe again; submenu row needed two clicks

- The persistent computer-mcp server was Broken pipe again at leg start on
  loop-1 `:2` (`computer-mcp_mouse_move` -> `Display connection closed by
  server: [Errno 32] Broken pipe`) while the display and window captures stayed
  alive. All input went through `/tmp/opencode/click.py` (pynput move with
  pointer read-back within 1 px, 0.25 s settle, click) and one-shot pynput
  scrolls; `capture_screen.py --window-title Mindustry` gave every frame.
- New quirk: a single verified click at the resolved `Custom Game` submenu row
  (window 450,289 -> screen 520,352) closed the submenu without opening the map
  grid. After reopening the submenu, two clicks 120 ms apart at the same point
  reached the grid and then a MapPlayDialog (title `Mud Flats`). Send two
  clicks when entering a submenu row and verify by capture, not by the returned
  frame (the tool response lags one action, as documented).
- Submenu rows do **not** show a hover highlight (main-menu buttons do), so the
  hover-diff bbox probe came back byte-identical at the row; locate submenu rows
  by scanning the light-text pixel bands of the window capture instead.
- Evidence: `runs/20261008-221916-ev0039-twin/` (`java/step-05-custom-game.png`
  back at the menu after the single click, `java/step-07-customgame-click2.png`
  grid + MapPlayDialog, `java/step-08-custom-grid.png` grid at top);
  sequence in `chains/java-reference-leg.md`.

## 2026-10-08 — loop-1 EV-0051 twin Java leg: Broken pipe again; PIL bbox fixes a scaled-PNG coordinate

- The persistent computer-mcp server was Broken pipe at the first call again on
  loop-1 `:2` (`computer-mcp_mouse_move` -> `Display connection closed by
  server: [Errno 32] Broken pipe`); the display and window captures stayed
  alive. All clicks used `/tmp/opencode/click.py` (pynput move + pointer
  read-back + click); Escape used a one-shot pynput `press`/`release`. This is
  the fourth loop-1 leg today with a dead persistent server — plan for
  one-shots from the first call.
- Surprise fix for the scaled-attachment problem: `python3` on this host now
  has **PIL 10.2.0** (the earlier "no PIL/numpy anywhere" notes are stale for
  plain `python3`; numpy is still absent). Locating the Java planet panel's
  `Launch` button by bright-pixel row bands in the window PNG gave its real
  rect (window y 602-630) while the model-view estimate was ~38 px high; the
  click at the measured screen position (643,679) launched the sector.
- 1152x648 menu coordinates confirmed: Play (310,216), submenu Campaign
  (550,216) — first click on a new row only highlights, second acts; pause
  `Save & Quit` (645,474), confirm `OK` (747,420).
- Evidence: `runs/20261008-223232-ev0051-campaign-live-views-twin/`
  (`java/step-06-sector-click2.png`, `java/step-12-after-launch.png`,
  `java/step-19-gz-panel-after.png`, `java/notes.md`); flow appended to
  `chains/java-reference-leg.md`.

## 2026-10-08 — loop-1 EV-0053 twin Java leg: Broken pipe again; two-click map row

- The persistent computer-mcp server was Broken pipe at the first call again on
  loop-1 `:2` (`computer-mcp_mouse_move` -> `Display connection closed by
  server: [Errno 32] Broken pipe`); display and window captures stayed alive.
  The whole leg ran through `parity-click.sh` plus one-shot pynput helpers.
- The custom-game submenu row needs two clicks 120 ms apart in one process: a
  single `parity-click.sh` click only highlighted `Custom Game` and left the
  submenu open. `/tmp/opencode/click2.py` (position -> 0.25 s -> click ->
  0.12 s -> click) opened the map list.
- Map-list card targeting at 1152x648: the click at screen (816,325) landed on
  Debris Field (column 3), not the Domain preview assumed from the model view.
  Measure the card columns from the capture instead of assuming an even pitch;
  Debris Field carried spawns + survival rules and was kept.
- Evidence: `runs/20261008-225009-ev0053-hud-twin/` (`java/step-05-maplist.png`,
  `java/step-07-ingame.png`, `java/notes.md`); flow appended to
  `chains/java-custom-survival-wave.md`.
