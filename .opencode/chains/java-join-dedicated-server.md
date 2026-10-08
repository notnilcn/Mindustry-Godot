---
id: java-join-dedicated-server
title: Java reference — build/start a dedicated server, Join Game via LAN discovery, connect into it (EV-0060)
status: verified
applies_when: >-
  The twin/Java leg needs the reference client to actually join a server (not
  just attempt), or needs a reachable vanilla server on 6567 for
  discovery/version-gate checks.
preconditions:
  - JDK 17 (bootstrap.sh) and the reference checkout built
    (`../Mindustry/desktop/build/libs/Mindustry.jar`); `gradlew server:dist`
    for `../Mindustry/server/build/libs/server-release.jar`.
  - `DISPLAY` set at opencode start (computer-mcp); one client at a time on the
    loop display.
  - Port 6567 is host-global and shared across loops: check
    `ss -ltunp | grep 6567` before starting a server.
  - Client data dir is the worktree-local `.../loop-N/xdg-data/Mindustry`;
    settings edits happen while the client is stopped.
tools: [computer-mcp_mouse_move, computer-mcp_click, computer-mcp_key_down, computer-mcp_key_up, computer-mcp_type, bash]
last_verified: 2026-10-08 1d8f75b (runs/20261008-112749-ev0060-join-twin)
---

# java-join-dedicated-server

## Steps

1. Build the reference dedicated server (seconds once the desktop jar is
   cached):

   ```bash
   JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64 bash ../Mindustry/gradlew \
     -p ../Mindustry server:dist
   # -> ../Mindustry/server/build/libs/server-release.jar
   ```

2. Start it headless. The launcher idles after `Server loaded` unless a
   console `host` command arrives, and `< /dev/null` closes stdin without
   hosting — pipe the command instead:

   ```bash
   mkdir -p /tmp/opencode/<name>/xdg; cd /tmp/opencode/<name>
   (sleep 6; echo host; sleep 7200) | XDG_DATA_HOME=/tmp/opencode/<name>/xdg \
     setsid /usr/lib/jvm/java-17-openjdk-amd64/bin/java -jar \
     /abs/path/../Mindustry/server/build/libs/server-release.jar \
     >"$RUN/java/dedicated-server.log" 2>&1 &
   ```

   Wait for `Opened a server on port 6567` in the log, then confirm
   `ss -ltunp | grep 6567` (TCP LISTEN + UDP).

3. (Optional) Prefill the join-dialog address offline while the client is
   stopped — `new arc.Settings(); s.setDataDirectory(new Fi("<worktree>/.opencode/loops/run/loop-N/xdg-data/Mindustry")); s.load(); s.put("ip", "127.0.0.1"); s.forceSave();`
   compiled/run against `desktop/build/libs/Mindustry.jar`.

4. Launch the client and wait for the readiness line:

   ```bash
   setsid .opencode/loops/bin/run-java.sh -width 1152 -height 648 -maximized false \
     >"$RUN/java/game.log" 2>&1 </dev/null &
   until grep -q "Total time to load" "$RUN/java/game.log"; do sleep 2; done
   ```

   Capture with `capture_screen.py --window-title Mindustry`; screen coords =
   window origin (from the capture report's `region`) + element offset.

5. Menu `Play` (window ~255,149) → `Join Game` (window ~476,219). The dialog
   opens with the **Local Servers** row already discovered:
   `Server vCustom Build` / `0/30 players` / `Map: <map> / Survival` / `Nms`.

6. Connect by clicking the Local Servers row center (window ~568,250), not the
   bottom buttons. Success: `game.log` `Connecting to server: /<ip>:6567`
   then `Received world data: N kB`; `ss -tnp | grep 6567` ESTABLISHED both
   ways; server log `<name> has connected.`; in-game HUD with the chat line.

7. Teardown: `kill -TERM` the recorded client and server pids; confirm 6567 is
   free.

## Success signals

- Dedicated server log: `Opened a server on port 6567`; then `<name> has connected.`
- Client log: `Connecting to server: /<ip>:6567` + `Received world data: N kB`.
- The join dialog's Local Servers section lists the discovered row before any
  address entry (LAN discovery, `JoinDialog.refreshLocal`).

## Failure modes

- The dialog bottom row (`Back` / `Add Server` / `?`) is input-dead to
  synthetic clicks at 1152x648 (byte-identical captures; same class as the
  EV-0059 MapPlayDialog note). Drive the join through the discovered row.
- The Community Servers list overlaps the bottom row; a click at the Add Server
  position can hit a community card → `@servers.disclaimer` → `safeConnect` to
  a public server that rejects a custom build (`This server does not support
  custom builds`) — useful version-gate evidence, but not the local join.
- First open raises one-time `@join.info` and community-disclaimer popups that
  intercept the first clicks; dismiss with their OK buttons.
- Java TextFields take no injected characters on this host (docs pass through a
  caret only); use the offline settings prefill.
- A server started with `< /dev/null` prints `Server loaded` but never listens;
  feed `host` through a pipe/session that stays open.

## Evidence

- `runs/20261008-112749-ev0060-join-twin/` (EV-0060 twin-verified; JVM pid
  42858, server pid 41621; `java/step-05-join-dialog-clean.png`,
  `java/step-15-connect-local.png`, `java/step-16-in-game.png`, `java/game.log`,
  `java/dedicated-server.log`).
