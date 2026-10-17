# Java-leg learnings

Operational friction hit while driving the Java reference (the Java leg of a
mirrored parity run), with the workaround that got past it. Reusable sequences
belong in `chains/` (`java-reference-leg.md`); this file is for the friction
that is not a chain.

Renamed from `computer-mcp-learnings.md` on 2026-10-11 when the computer-mcp
MCP server was retired and the Java leg moved to the shell helpers in
`.opencode/skills/parity-eval/scripts/` (`java_input.py`, `window_ctl.py`,
`capture_screen.py`; run with `$PARITY_PY`, `DISPLAY=:10`). Entries below keep
their original wording — `computer-mcp_*` calls named in old entries are
historical; the `chains/README.md` mapping table converts them.

The `/loop` or `/goal` session appends a dated entry **the moment friction costs
a retry** (a hiccup) — not only at the end of a run — with the exact failing
call, what worked, and the run directory or chain that shows it. One entry per
distinct issue per session; add bullets to it if it recurs. Never rewrite
another session's entry. If nothing new happened, no entry.

Entries dated before 2026-10-09 describe the retired dual-loop workflow
(`.opencode/loops`, `PARITY_LOOP_DIR`, `parity-click.sh`, Xvfb/Xwayland loop
displays); those paths no longer exist, but the workarounds still apply.

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

## 2026-10-09 — loop-2 EV-0056 Java leg: pin the worktree client to the MCP display; XTEST focus/ocr limits

- The worktree `run-java.sh` resolves its own runtime dirs, so it started a
  second java weston + Xwayland `:3` and the game opened as a native Wayland
  window on the Godot compositor (GLFW prefers Wayland while
  `WAYLAND_DISPLAY` is exported by the session), invisible to the `:2`
  persistent computer-mcp and to `capture_screen.py --window-title`. Fix:
  `env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET PARITY_DISPLAY_OVERRIDE=:2 <worktree
  wrapper>`; the window then lands at +70+63 in 1152x648 and all `:2` tools
  see it. The `-u` matters: the wrapper sets `DISPLAY` but does not unset
  `WAYLAND_DISPLAY`.
- Keyboard input with one-shot pynput helpers: each new XTEST client loses the
  window's focus, so a click must precede key events **in the same process**
  (observed with Escape and the `3`,`1` block-select combo). Mouse events
  (move/click/scroll) work one-shot, and the pointer position persists on this
  Xwayland display after the process exits.
- Without `tesseract` and with a non-vision model the Java UI cannot be read:
  selection state (block info panel), dialog contents and the catalog grid are
  unverifiable from captures. The block rotation path was not pixel-isolated;
  metric-only pixel analysis (bright-band/row scans, frame diffs) reached a
  playable world and identified catalog/HUD regions but could not attribute a
  rotation to the input. Evidence:
  `runs/l2-20261009-001432-ev0056-rotation-twin/java/notes.md`,
  `java/step-81.png`, `java/step-82-world.png`.

## 2026-10-09 — single-display :10 Java leg: obscured window captures read black; resize keeps the old center

- `godot_game play` maps/raises the Godot editor+game windows over `Mindustry`
  on :10; after that `capture_screen.py --window-title Mindustry` returned
  mean 1.3 / std 13.3 (byte-dark) while the JVM was alive — Xvfb keeps no
  backing store, so the obscured X11 window reads back black. Raise Java
  (`window_ctl.py raise Mindustry`) **immediately before every capture and
  every click**; the next Godot play/status action re-raises the editor above it.
- `window_ctl.py resize Mindustry 1152 648` from the 900x700 window keeps the
  old centered origin (x=190) and pushes the right edge off-screen
  (190+1152 > 1280); follow the resize with `window_ctl.py move Mindustry 45 69`.
- Run: `runs/20261009-052658-seed-campaign/` (java step-00c black vs step-00d
  good; corrected temporal calibration `diff/calib-java-temporal.json`).

## 2026-10-09 — reference soak R00: the first click after a fresh launch is swallowed; client origin differs from the frame origin

- On the xrdp Xorg `:10`, the R00 fast-forward batch (Play → Campaign → Serpulo
  → OK → Launch as one `execute` call) left the main menu unchanged
  (`runs/20261009-065416-ref-ground-zero/java/r00-01-launch.png`). The first
  click delivered to the freshly launched GLFW window is consumed by
  activation; every later click in the batch then hit a menu whose submenu had
  never opened. Fix: verify the opening click with a `capture_screen.py` frame
  and re-send it — the re-sent Play click opened the submenu
  (`java/probe-click-play.png`). Chain updated with the wait that made it land.
- `window_ctl.py move Mindustry 45 69` positions the **WM frame** at +45+69;
  the client area is at **+50+98** (xfwm4 decorations +5/+29). Screen click
  coords are `window image + capture region origin`
  (`region [50,98,1152,648]` from `capture_screen.py`), not +45+69.
- Fresh-profile prep: emptying `parity-java-home/.local/share/Mindustry/saves/`
  is not enough to bring back Java's first-run `Select Starting Campaign`
  chooser — the `campaignselect=true` flag persists in
  `parity-java-home/.local/share/Mindustry/settings.bin`. Reset it offline
  while the client is stopped (settings.bin backed up first), otherwise
  Play > Campaign goes straight to the planet page.
- Client reboot: a client that dies between rows must be relaunched with a
  **fresh log file**, or the `until grep -q "Total time to load"` gate passes
  on the old log instantly. The R00 soak log is
  `runs/20261009-065416-ref-ground-zero/java/game.log`; the dead instance's log
  is kept as `java/game-prerun.log`.

## 2026-10-09 — reference soak R04/R05: clipped category button, drag placement, scroll rotation, tech-tree hunting

- **Block-picker category button is clipped and inert.** With two unlocked
  categories the bottom-right PlacementFragment renders the second button half
  past the window edge (image ~1105-1152, y 397-447); clicking it (screen
  1168/1180, 520-524) never switched categories
  (`runs/20261009-065416-ref-ground-zero/java/r05-16-category.png`,
  `r05-17-cat2.png`). Workaround: the block-category keybindings
  `Binding.categoryNext` `.` / `categoryPrev` `,` (Binding.java:68-69) — one `.`
  key_down+key_up switched production -> distribution and auto-selected the
  conveyor (`r05-18-catnext.png`).
- **`computer-mcp.drag` does not lay conveyor lines.** Two drags
  ((818,497)->(818,433), then ->(690,433)) placed exactly ONE conveyor
  (`r05-19-conveyors.png`): the MCP drag moves the pointer without the
  intermediate motion stream Java's drag-placement consumes. Workaround: place
  one tile per `mouse_move`+`click`, and rotate with a one-shot pynput
  `Controller().scroll(0,-2)` = one 90° step; verify the placement arrow in a
  capture before clicking (it points W then N here, `r05-rot-zoom3.png`).
- **Tech-tree node hunting.** Locked subtrees are invisible (`checkNodes`,
  ResearchDialog.java:395-403); pan the tree by dragging the background
  (`ElementGestureListener.pan`, :190-195) — a rightward drag reveals content to
  the left. Scroll-to-zoom never reaches the dialog's `scrolled` listener (the
  tree view holds scroll focus), so pan instead. Node hover shows the info
  panel (`hovered()` sets `hoverNode`); clicking a selectable+affordable locked
  node spends and unlocks immediately on desktop (:483).

## 2026-10-09 — soak R07 teardown: auto-pause on focus loss; off-island wandering

- The Java client auto-paused (`< Paused >` banner, top-center) when a terminal
  window took focus at ~17:48; keyboard input sent while unfocused is dropped
  (space did not unpause). Check for the banner and re-raise/click the game
  window before suspecting dropped keys; a paused client is a safe teardown
  point. Run: `runs/20261009-065416-ref-ground-zero/java/r07-06-back.png` vs
  `r07-07-unpaused.png`.
- WASD exploration walked the player mech off the playable island: the view
  turns black (void), the minimap player marker sits outside the landmass blob,
  and the mech stops taking movement input there
  (`r07-04-south.png`, `r07-05-south2.png`, `r07-06-back.png`). Explore with
  short holds and watch the minimap; the R07 lead walk is ~10-12 tiles **west**
  of the core (lead tiles x108-113, y40-50), not north.
- Teardown: killed the recorded JVM pid (18498; never `pkill -f`) and restored
  `parity-java-home/.local/share/Mindustry/saves/sector-serpulo-170.msav` from
  the R06 checkpoint so the next soak resumes from the good state; the sector
  map was parsed offline with `runs/20261009-065416-ref-ground-zero/tools/parse_msav.py`
  (ores/blocks/floors) to locate the lead deposit without in-game wandering.

## 2026-10-09 — description edits live only in the uv-tool install; a home-tree grep hangs the shell

- The learning-driven tool descriptions in `mcp.py` (previous-frame screenshots,
  first-click focus, pointer race, text fields, no scroll, obscured captures,
  WM-frame offsets, auto-pause) exist only at
  `/home/clintonnguyen/.local/share/uv/tools/computer-mcp/lib/python3.12/site-packages/computer_mcp/mcp.py`.
  There is no computer-mcp source checkout on this host (`find` found only the
  uv cache and the install), so a `uv tool reinstall/upgrade` would silently
  revert the front-facing strings — keep the edits in a tracked patch/checkout
  if they must survive.
- Failing call while looking for that checkout: `grep -rn "Obscured X11 windows
  read back BLACK" /home/clintonnguyen/g /home/clintonnguyen/.opencode` exceeded
  the shell-tool timeout and had to be aborted. What worked: `find
  /home/clintonnguyen -maxdepth 4 -name pyproject.toml -path "*computer*"`.
  Scope recursive searches to a known subdir; never grep the whole home tree.
- Tool schemas load at server-process start: of the three live
  computer-mcp/open-godot-mcp pairs at 23:41, the 22:36 and 22:38 pairs predate
  the 23:34-23:36 edits and still serve the old text. Restart the session/MCP
  after editing front-facing strings.
- Still not in any tool description: the dead persistent X connection
  (`Display connection closed by server: [Errno 32] Broken pipe`) and the
  one-shot pynput fallback for click/key/scroll once the persistent server is
  unusable.
- Evidence: process table and `stat` mtimes at 2026-10-09 23:41 (both uv-tool
  installs touched 23:35:35 / 23:36:18).

## 2026-10-09 — session-1 twin PAR-0056 Java leg: `click` was dead at the fresh menu; explicit `button_down` + `button_up` landed

- After the fresh launch + `window_ctl.py raise Mindustry`, two
  `mouse_move` → `click` sends at the Play row (response `mouse_position`
  read-back correct, screen 280/307,247) left byte-identical frames
  (`frame_diff` changed_fraction 0.0). A `mouse_move` to the same row changed
  the frame (Play hover highlight, changed_fraction 0.033), so motion events
  reached the window; two separate `button_down` + `button_up` calls then
  opened the submenu (changed_fraction 0.20). When a menu `click` looks dead on
  this host, hover to confirm the pointer is over the window, then send
  `button_down` + `button_up` as two calls instead of the `click` action.
- Launch-command hazard: `cd repo && RUN=… && setsid java … >"$RUN/java/game.log" … &`
  backgrounds the whole `&&` list; a poll in the same shell then sees an empty
  `$RUN` and burns its full timeout while the client is actually up. Keep the
  variable assignment outside the backgrounded group (or use a subshell).
- Evidence: `runs/20261009-141406-PAR-0056-twin/` (`java/step-00..04*.png`,
  `diff/step-0*.json`, `java/game.log` readiness 2597 ms, JVM pid 134983).

## 2026-10-10 — PAR-0037 twin Java leg: the resume frame's missing hint box is a persisted settings flag, not a render difference

- Symptom: a cold-boot resume of the installed `r01-sector-serpulo-170.msav`
  rendered 97.6% identical to the stored reference frame, but the left hint box
  (`Use [WASD] to move.` + Skip) was absent (that region meanabs 27; everything
  else <=4; full changed_fraction 0.024 vs 0.005 after the fix).
- Cause: `HintsFragment.DefaultHint.finished()` reads
  `Core.settings.getBool("<name>-hint-done")`; the reference soak's R02 movement
  set `desktopMove-hint-done=true`, so the hint never shows again on that
  profile even though the save resumes at objective 0.
- Workaround (client stopped, offline): run a tiny Java program against the
  Mindustry jar —
  `new arc.Settings(); s.setDataDirectory(new Fi(dir)); s.load();
  s.put("desktopMove-hint-done", false); s.forceSave();` — kept at
  `runs/20261009-150509-PAR-0037-twin/java/settingsfix/SetSettings.java`
  (settings.bin backed up as `java/settings-pre-rerun.bin`). After the reset the
  fresh frame reproduces the stored reference: changed 0.00507, mean_abs
  [0.53,0.49,0.41], hint/status/copper regions <=0.01.
- The flag was left false so the captured frame stays reproducible from the
  profile; restore the backup to revert.
- Note: this run's Play click landed on the first try (mouse_move before click);
  the earlier "first click swallowed" friction did not recur.
- Run: `runs/20261009-150509-PAR-0037-twin/` (JVM pids 193769 run 1, 197845 run 2;
  both quit gracefully via menu Quit).

## 2026-10-10 — PAR-0046 twin Java leg: campaign research persists in settings.bin; the .msav you install gets backed up and rewritten

- Symptom: resuming the stored r03 checkpoint, the tech tree's Mechanical Drill node
  rendered gold and the popup read `Researched` — the R04 action was already done on the
  profile, so the click could not spend 10 copper.
- Cause: campaign research is global in `parity-java-home/.../settings.bin` as
  `req-<content>-<item>` ints (finished requirement amounts) + `<content>-unlocked` bools
  (`TechTree.TechNode.setupRequirements` loads them at startup); the reference soak's
  R04/R05 set them and they survive every later launch.
- Workaround (client stopped, back up settings.bin first): set
  `req-mechanical-drill-copper=0`, `req-conveyor-copper=0`,
  `mechanical-drill-unlocked=false`, `conveyor-unlocked=false` (+
  `desktopMove-hint-done=true`, `zoom-hint-done=false` to match the stored fork frame)
  with the tiny arc program
  `runs/20261009-162304-PAR-0046-twin/java/settingsfix/TweakSettings.java`
  (`key=int:v`/`key=bool:v`, prints before/after, `forceSave`). The after-action values
  (`req-mechanical-drill-copper` 0->10, `mechanical-drill-unlocked` false->true) are the
  spend evidence for the R04 action.
- Save-file mechanics learned while installing checkpoints: the client rewrites
  `saves/sector-serpulo-170.msav` (autosave at launch + interval + Save & Quit) and moves
  the previously installed file to `sector-serpulo-170.msav-backup.msav`; wipe
  `saves/*.msav*` (both files) before every install. The r03 checkpoint carries
  `objectives[0] completed:true` and a live mine order (core copper climbs on resume);
  the r02 checkpoint resumes mid-S (its autosave fired after the S hold) — not at the
  copper field.
- Run: `runs/20261009-162304-PAR-0046-twin/` (JVM pids 270950/273720/275387/277234).

## 2026-10-10 — PAR-0049 twin Java leg: clicks never landed while the Godot editor held focus (`_NET_ACTIVE_WINDOW` stayed on the editor)

- Symptom: after a fresh Java launch + `window_ctl.py raise/resize/move` (client +50+98 1152x648,
  JVM pid 404033), five Play clicks (2× `click`, 1× `button_down`+`button_up`, 1× one-shot pynput
  120 ms) left the main menu byte-stable. `_NET_CLIENT_LIST_STACKING` showed Mindustry on top, but
  `_NET_ACTIVE_WINDOW` and the input focus stayed on the Godot editor (0x6800002); one `click`
  response reported `mouse_position (1783,51)` although the preceding `mouse_move` read back
  (280,247) — the focus-click race from the tool description.
- Failing calls: `computer-mcp.click {"button":"left"}` after `computer-mcp.mouse_move
  {"x":280,"y":247}`; `computer-mcp.set_window_topmost {"hwnd":113246228,"topmost":true}` →
  `{"error":"Window management not yet implemented for Linux"}`. `window_ctl.py raise` stacks but
  does not focus.
- Workaround (worked): switch focus from python-xlib first — send the EWMH `_NET_ACTIVE_WINDOW`
  ClientMessage to root, `client.set_input_focus(X.RevertToParent, X.CurrentTime)`, `d.sync()`;
  verify `_NET_ACTIVE_WINDOW == 0x6c00014`, then move+press/release with a one-shot pynput
  `Controller` (press/release 120 ms apart). With focus switched, the very next click opened the
  Play submenu. Kept as `java/java_input.py` in the run dir (focus + move/click/key/scroll).
- Evidence: `runs/20261010-060551-PAR-0049-twin/java/step-01..06-*.png` (menu unchanged),
  `/tmp/par0049-play-focus.png` (submenu open after the focus switch).

## 2026-10-10 — PAR-0049 twin Java leg: the R05 Conveyor node is view-relative; the recorded pan table did not reproduce it

- The soak's R05 table (`pan drag (800,300)->(420,300)` then `(150,300)->(1100,300)`, node at
  image (61,311)) landed elsewhere on the twin replay (the dialog keeps its last pan across
  close/reopen, and the same drags land a different content offset). Finding the `Conveyor`
  node took a long manual pan sweep; the only reliable identification is the hover popup
  (`Conveyor / Copper 5 / 5 / Transports items forward.`) and the gold belt icon hanging
  above the root's horizontal bus, ~2 child positions left of the `Ground Zero`/copper chain.
  Budget for the hunt or seek the node visually; do not trust a single pan replay.
- Also: `.` did not always switch the picker category (regression unknown, maybe a focus
  effect) - the recorded `.` flow did work after the tree was closed from the same state.
- Run: `runs/20261010-060551-PAR-0049-twin/` (`java/r05-29-researched.png`, node hover
  `java/sweep/conveyor-hover.png`).

## 2026-10-10 — strings updated again: focus vs stacking, click-dead fallback, Broken pipe

- The installed `mcp.py` descriptions now carry: explicit `button_down` +
  `button_up` two-call fallback when a click is dead but a `mouse_move` hover
  changed the frame; `_NET_ACTIVE_WINDOW` focus switching on
  `click`/`switch_to_window` (raising/stacking the window is not focus); and
  the `Display connection closed by server: [Errno 32] Broken pipe` recovery
  (restart the MCP server or fall back to one-shot pynput helpers) on
  `mouse_move`.
- Same install-only caveat as the 2026-10-09 entry: the strings live only in
  the uv-tool site-packages, and a `uv tool reinstall/upgrade` reverts them.
- Evidence: `mcp.py` mtime 2026-10-10, `py_compile` clean; running servers must
  be restarted to serve the new text.

## 2026-10-10 — MCP mouse_move/click dropped on the Java window (focus race) — runs/20261010-142753-PAR-0010

- Symptom: `computer-mcp.mouse_move({x:280,y:247})` moved the X pointer (confirmed via Xlib `query_pointer` → (280,247)) and `computer-mcp.click({button:"left"})` returned ok, but the Java client showed no hover highlight on the Play row and the click did not open the Play submenu. Window client absolute was +50+98 (the WM frame adds +5+29 over the recorded +45+69).
- Likely cause: the click was consumed as the focus click; one-shot XTEST clicks through the persistent server can land before the window holds input focus.
- Workaround: one-shot focused input via `runs/20261010-142753-PAR-0010/java/java_input.py` (ClientMessage `_NET_ACTIVE_WINDOW` + `set_input_focus`, then pynput `position`/press/release, 0.5 s settle): hover highlighted and the Play submenu opened. Use the helper for Java clicks/keys whenever the MCP click path looks dead.
- 2026-10-10 (same run): `pkill -9 -f '/usr/lib/jvm/java-17-openjdk-amd64/bin/java'` matched the tool's own bash command line and SIGKILLed the shell mid-command (second half of the script never ran). Confirmed the skill's rule: never `pkill -f` with a pattern the tool shell itself contains; use `pgrep -af '[j]ava -jar'` to list and kill exact pids only.
- 2026-10-10 (same run): two more retries before the session handoff. (1) The opencode server restarted mid-command and the shell call aborted; a following `cd repo && … && setsid java … &` backgrounded the whole `&&` chain, so the readiness poll and `window_ctl.py` calls ran from the wrong cwd with an empty `$MP` (`runs/…` not found) — keep the `cd`/variable setup outside the backgrounded group or run the poll as its own foreground call. (2) Restoring the profile for the handoff: `cp` settings back and `mv` the saves dir back only after confirming no Java process holds the data dir.
- 2026-10-10 (recurrence, reference-soak preflight, `runs/20261010-083814-ref-soak-preflight/`): the same shell-grouping mistake repeated — `cd repo && JAVA=… && setsid … & echo launched && for …` backgrounded everything up to `&`, so the readiness poll looked for `runs/java-console.log` in the workspace root and failed while the client was actually up (pid 113793, `Total time to load: 2452ms`). Fix used: check `pgrep -af '[M]industry'` + `tail` the log as a separate foreground call; next time put `cd`/vars and the launch in separate calls, poll in its own call.

## 2026-10-10 — computer-mcp MCP tools dropped mid-session; Java leg moved to shell XTest helper — runs/20261010-083848-ref-PAR-0070-combat-fx

- Symptom: after an opencode server restart during the soak, the session tool catalog no longer lists any `computer-mcp_*` tool (`search({namespace:"computer-mcp"})` -> 0 items). computer-mcp needs a working `DISPLAY` at opencode process start; without it the server exits and opencode silently drops its tools (skill precondition). The shell env has no `DISPLAY`; the MCP config pins `:10`.
- Impact: cannot issue `computer-mcp_*` mouse/key calls this session; no way to re-adopt the server from inside the session.
- Workaround: drive the Java client with the shell helpers instead — `runs/soak-tools/java_input.py` (focus by WM_NAME + python-xlib ClientMessage `_NET_ACTIVE_WINDOW` + `set_input_focus`, then pynput move/click/key/scroll/type) and `/tmp/opencode/send_click.py` (pure XTest/XSendEvent), captures via `capture_screen.py --window-title Mindustry`. Input still only reaches the Java client on `:10`.
- Display note: `:10` is an **Xorg + xrdp** session (`Xorg :10 -config xrdp/xorg.conf`, 2560x1600), i.e. a remote desktop, not the Xvfb the skill's session recipe assumes; the RDP client's pointer can move the pointer between calls.

## 2026-10-10 — first-launch window: clicks at correct coords never reacted until a synthetic send-event click (focus/geometry confusion) — same run

- Symptom: fresh launch (maximized 2560x1549) -> `window_ctl.py unmaximize/resize/move` -> 1152x648 at +45+69. Then five+ clicks on the Play row (MCP click, helper pynput click, atomic XTest click, double-click, focus-cycle click) all returned ok/landed at (289,221) but the menu never reacted; hover produced no highlight. The game's main thread was in `SDL_GL_SwapWindow` (jstack, pid 113793) and window captures matched root crops (not stale). A later `XSendEvent` synthetic click was the first input that produced *any* reaction — it opened the "Join the Mindustry Discord!" dialog (discord button is bottom-right; the mapping anomaly is unexplained), after which a send-event click on "Back" did nothing again.
- Hypothesis: SDL's internal mouse focus/clickthrough: the window was resized/moved under a pointer position that never generated an EnterNotify for SDL, so it dropped clicks; send-event clicks bypass that path.
- Next step: relaunch cleanly with `SDL_MOUSE_FOCUS_CLICKTHROUGH=1`, enter the window with a pointer move from outside, and test one real XTest click before any WM resize.

## 2026-10-10 — reference soak PAR-0070: MCP tools dropped twice after opencode server restarts; Java leg moved to shell helpers (runs/20261010-220527-ref-PAR-0070-combat-fx)

- Symptom: mid-iteration the session catalog lost all `computer-mcp_*` tools twice (once while `set_config(constrain_mouse_to_window=...)` + click was in flight; the tool call returned `{"error":{"type":"aborted"...}}`). Same root cause as the 2026-10-10 083848 entry: computer-mcp needs `DISPLAY` at opencode process start; a restart without it drops the tools. The Java client (pid 32417) survived both restarts.
- Recovery: drive the Java leg with the shell helpers (all `DISPLAY=:10`, computer-mcp uv-tool python): `runs/soak-tools/java_input.py` (focus+move/click/key per call), `runs/soak-tools/send_event_click.py`, `.opencode/skills/parity-eval/scripts/window_ctl.py`, `capture_screen.py`. After the second restart the MCP tools reappeared; helpers remain the primary driver for this soak per operator preference.

## 2026-10-10 — same run: clicks stop landing on MapPlayDialog's Play button (and at other dialog spots) after ~3 working clicks; hover never renders

- Symptom: after Play > Custom Game > Mud Flats card all landed (helper `java_input.py` with EWMH focus each call), every subsequent click at the MapPlayDialog's Play (image 684,552; screen 734,650) was a no-op — ~12 attempts across helper click, MCP click (x2), XWarpPointer+XTEST, 5-step warp, fine 40-step sweep, enter-hop from outside the window, `send_event_click.py ... motion`, MCP dblclick. Meanwhile other dialog buttons did land at moments (Customize Rules 625,339; Sandbox toggle 637,215; Edit 734,700 opened its context menu), i.e. x>640 is not the discriminator.
- Extra evidence: moving the pointer onto Play never changed a single frame pixel in the button area (PIL diff of two captures, 0 px >30 in the 574..794x522..582 box), while hover/click reactions elsewhere did render. jstack: game healthy in `SDL_GL_SwapWindow`, 10.6% CPU. X hit-test: deepest mapped window under (734,650) is the Mindustry client `0x4c00014` at +(50,98) 1152x648; `_NET_ACTIVE_WINDOW` = `0x4c00014`.
- Workaround state: keyboard works (Escape closed the MapPlayDialog sub-dialog and dialog layers). Next steps tried: TAB/Enter focus navigation; fresh client restart with minimal-click staging. Record the wait/fix that works here when confirmed.

## 2026-10-10 — reference soak PAR-0070 (2nd soak client, JVM pid 81138): the MapPlayDialog Play coordinate was 40 px stale — measure the button from the current capture

- Root cause found for the entry above: the recorded Play coord (image 684,552; screen 734,650 at +50+98) sits ~40 px ABOVE the button. The button's bright text/outline rows are at image y 586..602 (center ~592) for the Domain dialog → correct screen (738,690). A fresh `java_input.py click 738 690` loaded Domain on the first try; the same stale coord had swallowed every click in the prior session. Hover "never rendered" because the pointer was in the gap between the map preview and the buttons (their PIL box 574..794x522..582 also missed the button — the box must cover y 566..620).
- The MapPlayDialog's button row shifts with the map-preview height: in the "4 corners of death" dialog the text rows were 572..586 (center 578) and the stale click at image 552 still fell inside the padded button rect and worked; on Domain the rows are 586..602 and the same click misses. Always re-measure the button center from the latest capture before clicking; never reuse a MapPlayDialog Play coordinate across maps.
- Same run: the map list's "Search maps..." field drops `java_input.py type` text (placeholder stays; same class as the Custom Rules numeric-field drops). Workaround that works: `scroll` over the grid (negative = further down the list) and click the visible card.
- Same run: in-game Escape > Save & Quit raises a dim "Are you sure you want to quit?" confirm; its Cancel/OK are drawn ~black-on-black and the window capture reads mean 2.29 there (not a fault). Locate the buttons on a root capture (`capture_screen.py` monitor path) — OK at (727,455).

## 2026-10-11 — reference soak PAR-0071/0073 (same client, JVM pid 81138): custom-game blocks build only in the player mech's range; palette strip + cell clicks are the reliable block selection

- In a Custom Game (any gamemode, including Sandbox) the "player" is a real unit — the grey/orange mech at the camera centre — and clicks place PLANS; construction happens only when the mech is within its build range. The Sandbox gamemode does NOT set instantBuild (`Gamemode.java` sets only infiniteResources/allowEditRules/waves, waveTimer=false), so nothing builds instantly here. A plan outside the mech's reach keeps its arrow and never builds; the drill placed next to the core built in ~15 s, plans 7+ tiles away never did even after minutes.
- WASD moves the camera AND the mech with it (used to walk the mech back after a pan); W/S taps are enough to reposition. Marching the mech over a plan gets it built (the built conveyor in this run was constructed while the mech passed it).
- Palette selection that works: click the category icon in the palette strip (screen 1130,570 = distribution; 1178,570 = production at this geometry), then click palette cell 1 (930,538 = conveyor; cell 1 of production = mechanical drill). The `,`/`.` category cycle and the number keys were flaky (a `1` press re-selected Duo mid-flow once); always verify the "Key: [cat,slot]" info header after selecting, and never chain a number key you did not verify.
- Mud Flats Sandbox world staging now live at pid 81138 (drill on copper ore + built conveyor + mech unit; copper core); reusable for the in-world items that only need a calm sandbox (waves off until Skip).

## 2026-10-11 — reference soak (same client, later phases): helper gaps and dialog hitbox quirks

- `java_input.py key F6` crashes (`ValueError: F6` — the KEYS map has no function keys). Workaround: one-shot pynput with `Key.f6` after reusing `java_input.focus(display)` (see the inline snippet in the run dir notes). Add F-keys to the helper's KEYS map when convenient.
- Settings dialog: clicking a tab row's TEXT does nothing at this geometry; clicking the row's ICON reliably opens the panel (Settings > Sound icon at screen (573,346)). The settings tree is 2 levels: panel -> Back -> tab list -> Back -> pause.
- `Save & Quit` in a custom game writes a numbered slot the client picks itself — it wrote OUR world over slot 23 at 00:34 (the operator's original 23.msav is preserved in `parity-java-home/checkpoints/20261010-seeded/saves/`). Custom games ALSO autosave on their own ("Mud Flats Oct 10 11:58" save slot appears in Save Game).
- After Save & Quit the client can sit on a black frame with only locale lines in the log (mean 3.11 for minutes); kill the recorded JVM pid and relaunch instead of waiting.
- Config pickers (Item Source): the block must be BUILT, no build selection in hand (right-click cancels first), then a single left click opens the ItemSelection popup; coal = picker row 2 col 2 at this geometry (~716,583 screen).
- The palette strip category map at 1152x648 +50+98 (verified): col1 x=1130 -> turret(520), distribution(570), power(620), crafting(670), effect(720); col2 x=1178 -> production(520) [unverified but consistent], liquid(570), defense(620), units(670), logic(720). Palette cells: cell1 930,538; c2 976,538; c3 1022,538; row step ~46 px; row2 starts y 584. Distribution cell16 = Item Source, cell17 = Item Void (last lone cell after scrolling: 920,672).
- The MapPlayDialog Play button and other dialogs sometimes swallow a click; verify each named step with a capture before chaining the next one in dialogs, and prefer measuring button rows from the current capture (bright pixels) over stored coordinates.

## 2026-10-11 — reference soak (session 2): the block config picker opens on the block's TOP-LEFT TILE click; the wave targets the far core

- **Config picker mechanism solved**: a built configurable block (ItemSource, LiquidSource) opens its item/liquid picker when you click the block's **top-left tile** (image tile corner), after any stray build selection is cancelled (right-click). Two clicks on the top-left tile opened it both times (runs/20261010-234250-ref-PAR-0070-combat-fx step-228 -> the 4x4 item grid; copper = row1 col1). Clicks at the block's center do NOT open it (they only select). Verified: click the source top-left at (712,455) when the source sat at image (662..685,357..382).
- Wave enemies target the CORE, not mid-map turrets: a fed Duo placed ~60 tiles from the core never engaged; the wave killed the far core (Game Over, Buildings Destroyed 1, Enemies Destroyed 0 — step-234). For turret-combat captures, stage the turret NEXT TO THE CORE in a fresh world (the fresh-launch camera starts at the core — step-237).
- `M` opens the fullscreen minimap; clicking it does NOT teleport the camera in this build (several clicks produced no jump), the small minimap is also inert — pan with WASD instead. The bottom-right "x,y" label tracks the CURSOR tile, not the camera; do not use it to reason about the camera position.
- Loading a custom save restores the AUTOSAVED camera (which follows the mech), so a world's staged block cluster is not necessarily at the reloaded view; the autosave can also capture your pans (reloading right after panning restores the panned view).
- Erekir planet (imported campaign): all sectors read "Enemy Base / No Launch Sector" — no launchable Erekir sector for PAR-0079; a fresh-profile Erekir branch is required (recorded on the item).

## 2026-10-11 — reference soak (session 5): config dialogs, key quirks

- The Ground Factory's config = an anchored UNIT selector (Dagger + silicon cost + queue +/- buttons) — clicking the built factory with no build selection opens it (step-277). The Message block's config = an "Edit Message" dialog with a 400-char text field + counter (step-280); **injected typing is dropped by the field** (counter stays 0/400 — same class as the Custom Rules/search fields).
- The command panel ("Command Mode / [no units]") appeared while an injected **`f` keydown** was held, and NOT with `java_input keydown shift` (its Key.shift may not resolve to shiftLeft on this host). A raw-shiftLeft test is the clean re-stage for PAR-0095; the F schematic select (Binding.java:34 `KeyCode.f`) did not surface its selection UI in this pass (PAR-0099 re-stage noted).
- Wheel-over-UI guard confirmed upstream: scrolling with the pointer over the block palette leaves the frame byte-identical (no camera zoom; step-284/285).

## 2026-10-11 — reference soak (session 6): editor batch + a stuck settings dialog

- The main-menu Settings > Game Data panel got click-dead after two clicks (Back/rows unresponsive; ESC dead too; mean frozen at 8.18). Recovery: kill the recorded pid and relaunch (the world was already Save & Quit'ed into the autosave). The same dialog tree's Back button sits at screen ~(626,708) (image y 610), NOT at y 610 as its position in the *settings-tab* level suggested.
- Editor navigation (all via the main menu): Editor (182,389) -> Maps dialog (Back/New Map/Import Map; real previews) -> a map card -> Map Info (Open In Editor) -> the canvas renders. In the editor, ESC opens the Menu (Save/Info/Generate/Resize/Import/Export/Edit In-Game/Playtest/Sector Generate/Quit); Quit raises the "Any unsaved changes will be lost" confirm. WASD pans the editor view; the injected scroll did not zoom it in this pass.
- Upstream binds verified from the source: schematicSelect = F (Binding.java:34), commandMode = shiftLeft (:39), screenshot = F12 (:98); F8 is unbound. At the main menu, T/F8 do nothing (the menu stays byte-identical) - PAR-0101 evidence.
- The key-rebind prompt ("Press an axis or key...") does not accept injected keys (same dialog input-drop class) - the prompt screen is the captured evidence; a raw-XTEST-with-hold retry is the re-stage.

## 2026-10-11 — reference soak (session 7): processor code via the X clipboard; paused-UI clicks die

- The processor's Edit -> "Export..." menu has "Load From Clipboard"; a python-xlib CLIPBOARD owner (serves the selection for the duration) WORKS: the served text landed in the live code canvas (parsed as an "Invalid" statement - the mlog effect names come from LogicFx: warn/cross/placeBlock/spawn/trail/hit/shootSmall/explosion/spark/smokePuff/... - "wave" is NOT valid). Owner script kept at /tmp/opencode/clip_serve.py (+ /tmp/opencode/logic_code.txt).
- The PAUSED processor UI's clicks became unresponsive after the first menu interaction (same click-death class as the settings dialog); ESC still closes it. Re-stage the corrected load in a fresh click session.
- Processor links (drawflush display1 etc.) are embedded in the block's compressed config (LogicBlock.pointConfig: version + code bytes + link name/x/y list) - adding a link needs the instruction's block-parameter pick flow, not the code text.

## 2026-10-11 — reference soak (session 8): build range scales with zoom; power needs adjacency

- A plan placed while the camera is ZOOMED OUT stalls forever: the mech's build range is a world distance, so at a 2-notch zoom-out a 100-px-on-screen plan is ~10 tiles away and out of range (the force projector plan sat red for 2+ min until zooming back in). Zoom in (scroll +6) before placing/building big blocks.
- Power blocks connect by ADJACENCY (no nodes needed to touch): a power source 2-3 tiles away leaves a projector's Power bar empty (step-353); a source placed flush against the projector's edge powers it instantly and the shield dome + rim render (step-356). The palette cell coordinates are per-CATEGORY: the same screen cell in a different category selects a different block (a stray Launch Pad plan got placed that way; right-click-hold on a plan cancels/removes it).

## 2026-10-11 — reference soak wrap: coverage audit + the §2b.1 row mapping

- Final audit: 0 ledger items lack evidence — every non-verified item carries a `reference` block with frames or a recorded staging blocker (128 reference blocks total; 16 verified pre-existing; statuses: reference 88, seeded 16 [all with recorded blockers], blocked 11 [pre-existing 0044/0045, 0052-0055 + the 0024-0027/0112/0127 launch-flow group], mismatch 10 [kept, reference blocks added]).
- §2b.1 Ground Zero rows: R00-R06 recorded via the GZ run items (PAR-0032-0051 + runs/20261009-065416-ref-ground-zero); the later rows attach to the covering items instead of new records: R07 (lead) -> the drill/ore surfaces (PAR-0071/0075/0079 refs), R08 (camera) -> PAR-0031, R09/R10 (duos/ammo) -> PAR-0086/0087, R11 (walls) -> PAR-0050/0073/0098, R12 (waves) -> PAR-0096/0139, R13/R14 (scatter/ammo) -> PAR-0086, R15/R16 (defend/finish) -> PAR-0084 (recorded blocker: needs a throwaway profile for the loss). No duplicate rows were created.
- The blocker group (PAR-0115-0125, 0136) is staged: `runs/20261010-234250-ref-PAR-0070-combat-fx/` holds the Mods dialog (empty fixture), the host/join dialogs, the persisted Join-Game server entry, and the dedicated-server console log (server-release.jar boots; `host` opens port 6567). The AGENTS.md blocker table's Status column now reads staged per row.
- The soak produced no parity verdicts (no item flipped to verified); mismatch/blocked statuses were preserved and only gained reference blocks.

## 2026-10-11 — PAR-0010 resume: SetSettings needs the Mindustry jar on the classpath

- Symptom: `java SetSettings <datadir> campaignselect false` run from
  `runs/20261010-142753-PAR-0010/java/settingsfix/` (its `.class` in cwd) dies
  with `NoClassDefFoundError: arc/Settings` — the bare classpath has only `.`.
- Exact call: `/usr/lib/jvm/java-17-openjdk-amd64/bin/java SetSettings .../parity-java-home/.local/share/Mindustry campaignselect false`
  -> `Exception in thread "main" java.lang.NoClassDefFoundError: arc/Settings`.
- Workaround: run from the repo root with the fat jar first on the classpath:
  `java -cp "../Mindustry/desktop/build/libs/Mindustry.jar:runs/20261010-142753-PAR-0010/java/settingsfix" SetSettings <datadir> campaignselect false`.
- Run dir: `runs/20261011-085604-PAR-0010/` (this run; claim stamped before the rename).

## 2026-10-11 — PAR-0010 launch: readiness gate ran from the wrong cwd

- Symptom: the launch call mixed a backgrounded compound `cd ... && RUN=... && setsid ... >$RUN/java/game.log &` with a foreground `timeout 180 bash -c 'until grep -q ... runs/<stamp>/java/game.log'`; the foreground parts run in the shell tool's cwd (workspace root), so the gate grepped `/home/clintonnguyen/g/code_examples/runs/...` and spun 90 iterations of "No such file or directory" until the 180 s timeout (exit 124). The client itself was healthy (load 2616 ms).
- Exact call: `timeout 180 bash -c 'until grep -q "Total time to load" runs/20261011-085604-PAR-0010/java/game.log; do sleep 2; done'` from `/home/clintonnguyen/g/code_examples`.
- Workaround: use the absolute path in the gate (`/home/.../Mindustry-Godot/runs/<stamp>/java/game.log`) or pass an explicit workdir; never start a foreground readiness gate in the same call as a backgrounded `cd ... && ...` list.
- Run dir: `runs/20261011-085604-PAR-0010/`.

## 2026-10-11 — PAR-0033 Java leg: menu/submenu input + coordinate truth on :10

- The Play submenu needs a **click** in this build: a hover only highlights the row
  (runs/20261011-110622-PAR-0033/java/r00-05-hoverplay.png, no submenu); the click on
  Play (client (230,149) -> screen (280,247)) opens the Campaign/Join/Custom/Load column
  (r00-06). The soak's `+45+69` notes are stale for today's WM: the client area is at
  **(50,98)** — verify with `xwininfo -id <win>` (Absolute upper-left) and the
  `capture_screen.py` `region` field before clicking, never from memory.
- The "Select Starting Campaign" chooser OK button's real box is y 575..635 (client):
  clicks at y=574 hit the top border and did nothing through four attempts (no error,
  no log line). Measure the button by cropping the bottom strip (the button's border
  rows read ~255 luminance) before clicking; center (client (575,605) -> screen
  (625,703)).
- The sector Launch button on the fresh planet page: client (575,618) -> screen (625,716).
- The input row for the record: the tutorial's paste/book/tree/map icons sit at client
  x 851..1059, y 600..645 (glyph columns 853/905/954/1008).

## 2026-10-11 — PAR-0039 leg: the Java client survived the kill (setsid pid)
Symptom: after the PAR-0033 Java leg I killed "the launched pid" (468054) and
restored the profile byte-identical - but `pgrep -af "Mindustry.jar"` an hour
later still showed a live client (pid 468055). The number printed by the launch
was the `setsid` wrapper (`$!`), not the JVM; the real java pid is the wrapper's
child. The background client kept the tutorial open and autosaved into the
"restored" saves dir, so the snapshot restore went stale (diff -r showed
sector-serpulo-170.msav + backup changed, settings.bin byte 4).
Workaround: kill by process scan (`pgrep -af "[j]ava -jar"` -> kill the java
pid), THEN restore the saves dir from the snapshot again and `cmp` the settings;
re-verify with `diff -r` after the kill, never trust the post-leg restore while
a client may still hold the profile.
Run: runs/20261011-113747-PAR-0039/.

## 2026-10-14 — session 2: Ground Zero wave cannot be triggered on the tutorial save
Symptom: Play > Campaign > Sectors > Ground Zero > Go (display :11, worktree
parity-java-home) opens Ground Zero paused with the "Obtain 0/15 Copper"
objective; `java_input.py key space` and `keydown/keyup space` did not clear the
`< Paused >` banner, and the HUD shows the objective panel instead of the
wave/skip panel (the world processor keeps `waveTimer` off until later
objectives), so wave 1 cannot be spawned for a composition check.
Workaround: none within the 20-min budget; wave composition derived from
source (`WaveSpawner.java:74` `getSpawned(state.wave - 1)`). Killed the JVM by
its own pid (session 3's JVM on :12 matched the same pgrep pattern — check
`/proc/<pid>/environ` DISPLAY/HOME before killing).
Run: runs/20261014-s2-verify/java-0*.png.

## 2026-10-14 — session 3 (display :12) Ground Zero route; wheel zooms during a drag

- Route from the main menu (window +190+10, 900x700, worktree parity-java-home): Play
  (410,186) → Campaign (640,186) → Sectors dropdown (940,45) → "Ground Zero" row (890,143) →
  Go (640,678); in game ~8 s later, paused at start.
- `java_input.py scroll 1` while holding a conveyor drag zoomed the camera instead of rotating
  (twice; once before and once after the alpha spawned; the "Paused" label stayed after a
  Space press). Wheel-during-drag rotation was not observable this way; use the R/rotate key
  route or a fresh unpaused session next time.
- Another session's Java (DISPLAY :11) runs under the same jar name: identify your pid via
  `/proc/<pid>/environ` (`DISPLAY`, `HOME`) before killing.

## 2026-10-14 — session 2 round 2: background pause on :11; keyboard never reaches the client

- Symptom: Ground Zero (seeded profile, display :11) opens with `< Paused >` and Space does not clear it.
  Cause: `Control.pause()` sets `backgroundPaused` when the SDL window is not focused
  (`Control.java:619-632`) and `update()` re-pauses every frame while `backgroundpause` is on
  (`:725-727`); on Xvfb without a WM the window never counts as focused.
- Workaround: stop the client, back up `parity-java-home/.local/share/Mindustry/settings.bin`, run
  `SetSettings <dir> backgroundpause false` (the program in
  `runs/20261009-150509-PAR-0037-twin/java/settingsfix/`, compiled against the Mindustry jar), relaunch:
  Ground Zero then runs unpaused. Restore the backup afterwards (done).
- Still open: keyboard input (Space, WASD `key d 800`, Shift held via `keydown` or one `seq`) had no
  visible effect while mouse clicks drove the menus fine — Shift command mode and WASD were not
  observable. Next attempt: check `_NET_ACTIVE_WINDOW` (it was "not found" on :11) and try
  `send_event_click.py`-style XSendEvent key events.
- The seeded profile's Ground Zero has no active weather (`runs/20261014-s2-verify2/java-09-gz-10s.png`);
  the snow in `runs/20261009-065416-ref-ground-zero/java/r03-02-mine.png` comes from an older save.

## 2026-10-14 — session 3 plan verification round 3 (display :12)

- Menu route (window +190+10, 900x700; screen = window origin + client): Database (415,255) ->
  Schematics (645,255) / Core Database (645,325); Play (410,186) -> Campaign (640,186) /
  Custom Game (645,325); custom map `sandbox` card (850,510) -> Play (748,614).
- The sandbox custom game opened `< Paused >` and neither `key space` nor keydown/keyup space
  unpaused it on Xvfb (same as round 1) — plans placed there never build. UI taps (core
  inventory, palette, `?` info, Escape -> pause menu) still work while paused.
- Seeded profile: every numbered Serpulo sector hovered shows "Locked" (no based neighbour with
  a size >= 4 core), so a Java numbered-sector launch is not reachable without changing the
  profile.
- Planet globe: `java_input.py drag 780 400 480 400 left 600` rotates it (Java drag works).
- Graceful quit from a custom game: Esc -> Save & Quit (640,477) -> confirm OK (742,393) ->
  main-menu Quit (415,535).
Run: runs/20261014-s3-verify3/ (java-*.png).

## 2026-10-16 — round-J twin (runs/20261016-J-twin): clicks miss after `window_ctl.py move` on the xrdp `:10`

- Symptom: after `window_ctl.py unmaximize/resize/move Mindustry 1300 760` the window really sat at
  (1305,789) (`xwininfo` Absolute upper-left, capture `region`), but `window_ctl.py` kept printing the stale
  `x=5 y=29`, and every `java_input.py click` / `mousedown`+`mouseup` / `send_event_click.py` at
  origin+(240,150) on Play was dead; one of them instead opened the Discord dialog (the client's idea of its
  own origin lagged the real one).
- Workaround: move the window once more (`window_ctl.py move Mindustry 60 100` -> real origin (65,129)), then
  compute the origin per click from `xwininfo -name Mindustry` (never from `window_ctl.py`'s printout); clicks
  landed from then on.

## 2026-10-17 — round-L twin (runs/20261017-L-twin): first Java click after the pointer crossed the Godot window is swallowed

- Symptom: `JW 133 480` on the Custom Game sandbox card (and once on Play) returned ok but the frame did not
  change; `_NET_ACTIVE_WINDOW` was already the Java window. A `JM` hover on the same spot followed by the same `JW`
  landed every time.
- Workaround: hover first (`JM x y; sleep 0.4; JW x y`); round L wrapped this as `JWC`. Proposed default for
  `tools/twin.sh` JW in docs/parity-plans/L08 T5.

## 2026-10-18 — round L, session 2 (L08 T5): `JW` hovers before clicking by default

- `tools/twin.sh` `JW x y` now runs `java_input.py hover` on the target, sleeps `TWIN_JW_HOVER_S` (default 0.4 s) and
  then clicks: round L's `JWC` wrapper is the default (2026-10-17 entry above). Opt out with `TWIN_JW_HOVER=0`.
  Cost: ~0.4 s + one hover call per click (a `JW` takes ~3 s wall on `:11`).
- Proof (`runs/20261018-s2-L01-T2`, display `:11`): with the real pointer parked over the Godot game window
  (`GP 900 500`), `JW 230 289` opened Java's editor maps dialog on the first click (`java/editor-maps.png`), and the
  same cross-then-`JW 139 500` opened Map Info (`java/mapinfo.png`). No swallowed clicks in the batch.

## 2026-10-18 — round L, session 2 (L01 T3): Java text entry and keys work by XSendEvent (`JT`)

- Root cause of "Java text fields take no characters" / "keyboard never reaches the client" on the session Xvfb:
  the X input focus is the Godot window (`get_input_focus()` = 0x600002 while Java was 0xa0000f), so XTEST key
  events (pynput, `java_input.py type|key|seq`) go to Godot. A real click focuses the Arc field (caret shows) but
  no characters arrive.
- Fix: `tools/java_xkeys.py` sends KeyPress/KeyRelease to the Java window with XSendEvent; SDL accepts them. twin
  `JT sand`, `JT @BackSpace`, `JT @Escape`, `JT @space`. Proven: Custom Game search `sand`, Load Game search
  `molten`, Custom Rules search `banned` (`runs/20261018-s2-L01-T3/java/{custom-sand,load-search,rules-banned}.png`).
- Escape by `JT @Escape` closes the top Java dialog, but from an editor Map Info sub-dialog it returns to the editor
  (Map Info hides itself when it opens a sub-dialog): reopen menu -> Map Info per sub-dialog.
- A synthetic FocusIn does NOT clear background pause; for in-game frames flip `backgroundpause` offline with
  `SetSettings` (2026-10-14 entry), relaunch into a sub-run dir (so `twin_launch` does not overwrite the original
  `settings.bin.bak`), and copy the ORIGINAL backup back after `twin_stop` (done in `runs/20261018-s2-L01-T3/logic`).
