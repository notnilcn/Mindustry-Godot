<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Twin verification on Windows (2026-11-02 handoff)

The Linux environment in [`TWIN-CONTINUATION.md`](TWIN-CONTINUATION.md) no longer
exists: no X display, no Xvfb, no `window_ctl.py`, no `runs/archive/**`. This page
is the Windows equivalent of that page's **Environment** section. The workflow,
statuses and evidence layout are unchanged ([`TWIN-RUNS.md`](TWIN-RUNS.md)).

Repo root: `C:\Users\Clinton\g\code_examples\Mindustry-Godot` (branch `main`).

## Godot client (the only leg a checker runs)

- Binary: `C:\Users\Clinton\.local\bin\godot.exe` (4.7.2), project `client/`.
- **Editor bridge is port 6971, not 6970.** Port 6970 is held by a stale
  socket that no process owns; the plugin auto-advances, and the MCP server
  adopts whatever port `godot_instance launch_editor` reports.
- Restart the editor with the fixed recipe (keeps the port at 6971 so every MCP
  server, yours included, reconnects on demand):

  ```powershell
  Get-Process godot -ErrorAction SilentlyContinue | Stop-Process -Force
  $env:OPEN_GODOT_MCP_PORT = "6971"
  Start-Process -FilePath "C:\Users\Clinton\.local\bin\godot.exe" `
    -ArgumentList '--path','C:\Users\Clinton\g\code_examples\Mindustry-Godot\client','-e'
  godot_health check            # bridge_connected must be true (~20 s)
  ```

  `.mcp.json` pins `OPEN_GODOT_MCP_PORT=6971`, so a fresh MCP server adopts the
  running bridge by itself. `godot_instance launch_editor` also works but drifts
  the port on every call (6971 → 6980 → …) and would strand the other servers:
  use it only if the recipe above fails. Never run two editors on the project and
  never use `tools/parity-session.sh` (Xvfb/`DISPLAY`).
- Play: `godot_game play {"scene":"res://scenes/game.tscn"}` (explicit scene),
  then wait ~10 s for the `Loading…` screen to clear. There is no
  `DISPLAY`/`xdotool`/`twin_pointer.py` path: input goes through `godot_input`
  (`mouse_button`, `mouse_motion`, `key`, `text`, `action`) with
  `"coords": "viewport"`.
- Screenshots: `godot_screenshot game` (omit `max_width` to get the native
  1152x648 frame). The response carries `path`; copy it into the item folder.
- Eval: `godot_exec eval` with a GDScript snippet that `return`s a value.
  Never eval in the same round-trip as `godot_game play`.
- Errors: `godot_log errors` must be empty for a passing item; `godot_log get`
  is the tail.
- Runtime state: `godot_runtime_state digest`, `inspect`, `watch`.

### Rebuild after a Rust change

```
cargo build --manifest-path client/rust/Cargo.toml -p mind-gdext -p mind-headless --target-dir client/bin/rust
```

(~2 min). The running editor and game keep `mind_gdext.dll` mapped, so **stop the
editor first** (`godot_instance terminate` or `Stop-Process godot`), rebuild, then
`godot_instance launch_editor` again and replay. `tools/build.sh` itself is a bash
script; the cargo line above is the Windows equivalent.

### Godot user data

`%APPDATA%\Godot\app_userdata\Mindustry-Godot` — `saves/`, `config/settings.bin`,
`logs/`, `mcp_screenshots/`. A `saves/sector-serpulo-170.msav` campaign slot
already exists. Back the folder up before profile surgery and restore it after.

## Java reference leg (fixers only)

- Jar: `C:\Users\Clinton\g\code_examples\Mindustry\desktop\build\libs\Mindustry.jar`
  (built 2026-11-02 from `parity/upstream.lock` commit `2cd7aeec…`).
- Source: `C:\Users\Clinton\g\code_examples\Mindustry` (junction to
  `E:\Tools\mindustry-dump`; it has `git` — use `git -C <dir> show` for upstream
  reads). Arc dependency checkout: `C:\Users\Clinton\g\code_examples\Arc`.
- JDK 17: `C:\Program Files\Eclipse Adoptium\jdk-17.0.20.101-hotspot`
  (`java` is on PATH).
- Rebuild: `.\gradlew.bat desktop:dist --no-daemon` in the Mindustry tree
  (~2 min; the jar lands in `desktop/build/libs/Mindustry.jar`).
- Launch detached, no console, then fix the geometry:

  ```powershell
  python tools\windows\java_leg.py launch     # java -jar <jar> -maximized false -width 1152 -height 648
  python tools\windows\java_leg.py move       # client area exactly 1152x648 at (0,0), physical pixels
  python tools\windows\java_leg.py shot --out runs\PAR-0024\java\20261102-153000-01-menu.png
  python tools\windows\java_leg.py click --x 240 --y 152
  python tools\windows\java_leg.py key --keys "esc,ctrl+a"
  python tools\windows\java_leg.py text --text "hello"
  python tools\windows\java_leg.py stop
  ```

  `tools/windows/java_leg.py` is the Windows replacement for
  `window_ctl.py`/`java_input.py`/`capture_screen.py`. It declares per-monitor DPI
  awareness before touching a window — **without that the host's 150 % scaling
  makes the capture crop the top-left third of the frame** (this bit the first
  attempt at this host).
- The Java data dir on Windows is `%APPDATA%\Mindustry` (created on first run):
  `config/settings.bin`, `saves/`. Back it up before profile surgery and restore
  it after, exactly like the old `parity-java-home` rule.
- The reference pose is 1152x648 at window position (0,0), the geometry every
  archived frame under `parity/twin/PAR-####/` uses.

## Evidence

- `runs/PAR-####/godot/<yyyyMMdd-HHmmss>-<label>.png`, `java/` for a twin pair,
  report at `runs/PAR-####/<yyyyMMdd-HHmmss>-run.md`.
- PowerShell stamp: `Get-Date -Format 'yyyyMMdd-HHmmss'`.
- Archived pairs are AVIF at 1152x648; Pillow 12.1.1 reads them:
  `python -c "from PIL import Image; Image.open(p).convert('RGB').save(out)"`.
- `tools/twin_item.sh` is bash. Only `dir`/`note`/`log` are portable (and only
  under Git Bash, `C:\Program Files\Git\bin\bash.exe` — **never** the WSL
  `C:\Windows\system32\bash.exe`, which sees a different filesystem);
  `godot-shot`/`java-shot` assume the Linux helpers, so create the folders and
  copy frames directly instead.
- `python tools\twin_verification.py check` must stay `ok: 450 items …` after
  every status edit.

## Guardrails

No commits, no branches/worktrees, no parallel agents, no GDScript (all
non-addon code is `.rs`/`.tscn`), SPDX header plus upstream file citation on every
edited source file, and `runs/archive/**` does not exist here — compare against
`parity/twin/PAR-####/` or the item's recorded acceptance criteria instead.
