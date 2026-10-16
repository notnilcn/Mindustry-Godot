<!-- SPDX-License-Identifier: GPL-3.0-only -->
# J10 — Twin harness, residuals, re-walk — summary (session 5, wave A)

Workstream J10 (T1 + T2 item 6); T2 items 2–5 and T3 were **not** in this
workstream's scope. Branch `parity/session-5`, base `fe8fb0e4`, display `:14`,
Godot bridge 7010 (session 5 worktree `Mindustry-Godot-session5`).

## Per-task status

| Task | Status | Commit |
|---|---|---|
| T1 — commit the twin harness helpers | **done** | `574e085a` |
| T2 item 6 — harness hazards PAR-0345 / PAR-0347 | **done** | `78f42bf5` |
| T2 item 1 — bloom/linear settings (stretch) | **not done** (out of budget/appetite; left for a later session) | — |
| T2 items 2–5 (skip-wave double click, wheel scroll, hover over UI, FX/audio) | not in this workstream | — |
| T3 — final round-K twin re-walk | **NOT done** (round-K; runs after J01–J09 merge) | — |

## Commits

1. `574e085a` — `J10 T1: commit the twin-walk harness (tools/twin.sh + tools/twin_pointer.py)`
   Adds `tools/twin.sh` (sourced bash), `tools/twin_pointer.py` (pynput real-pointer
   warp) and a `tools/AGENTS.md` row.
2. `78f42bf5` — `J10 T2: gmcp.py eval indentation normaliser + stop/play watchdog`
   Adds `normalize_eval_indent` + `prepare_call` and the `--eval-timeout` /
   `--no-watchdog` / `--watchdog-scene` / `--watchdog-wait` eval watchdog to
   `tools/gmcp.py`.
3. (this file) — docs commit `docs/J10-twin-reverify-residuals_summary.md`.

Outside the repo: `.claude/skills/parity-eval/SKILL.md` gained §3c
"Twin-walk harness (`tools/twin.sh`)"; that file is shared and not
version-controlled, so it is recorded here instead of in a commit.

## T1 — twin harness

`tools/twin.sh` encodes the 2026-10-16 walk as script:

- **Java leg** (`JW` click, `JM` hover, `JD` drag, `JK` key/keydown/keyup,
  `JS` raise+capture): window-relative coordinates with the origin **re-read
  from `xwininfo -name Mindustry` on every call** (java-leg-learnings.md
  2026-10-16: `window_ctl.py`'s printout is stale after a move). Java actions
  raise the Java window first (`TWIN_RAISE_JAVA=0` disables).
- **Godot leg** (`GC` one-`sequence` dialog click, `GW` separate press/release
  world click, `GK` key by Godot name, `GS` screenshot, `GE` eval, `GP`
  real-pointer hover). `GP` resolves the game window by game pid +
  `WM_CLASS=Godot_Engine` — `xwininfo -name "Mindustry-Godot (DEBUG)"` returns
  the `--wid` host window, which the pointer never lands on.
- **Compare**: `SIDE` (frame_diff JSON + heatmap + side-by-side in
  `$TWIN_RUN/diff/`) and `ZOOM` (2x crop pair).
- **Lifecycle**: `twin_launch` backs up
  `parity-java-home/.local/share/Mindustry/settings.bin`, snapshots both
  profiles' saves, launches Java with `setsid`, gates on "Total time to load",
  normalizes to 1152x648 and places the window clear of the Godot game window
  (best effort, warns when the display cannot fit both); `twin_place_java`
  re-places after the game window appears; `twin_stop` stops Java, moves
  session-created saves (not in the launch snapshot) into
  `$TWIN_RUN/{java,godot}-session-saves/` and restores `settings.bin`.

Deliberate readings of the brief: gMCP goes through the sourcing checkout's
`tools/gmcp.py` (override `TWIN_GMCP`), so a branch can test changes to gmcp.py
with the harness — after the wave merges every checkout has the same file.
`twin_pointer.py` prefers a `python3` with pynput and falls back to
`$PARITY_PY` (the system `python3` on this host has no pynput).

Smoke (session 5, `runs/20261016-s5-J10-T1/notes.md`): `GP` hovered the exact
sidebar `Button#239897418595`; `GC`/`GK` opened and Escape-closed the settings
dialog (`dialog_stack` `[] -> ["settings"] -> []`); `GS`/`GE`/`SIDE`/`ZOOM`
produced artifacts; `JS` captured the Java menu and `JW` clicked Play
(submenu delta 0.199 vs the round-J reference 0.197); `twin_launch` →
`twin_stop` restored `settings.bin` byte-identical (`cmp` clean) and moved
0 (menu-only run) session saves. `JD` and `GW` were not exercised (thin
wrappers over already-tested helpers; noted as residual verification).

## T2 item 6 — harness hazards

**PAR-0345 (mixed-indentation evals hang the frame loop) — root cause + fix.**
The addon wraps eval bodies as `"extends Node\nfunc _eval():\n\t" +
code.replace("\n", "\n\t")` (`addons/open_godot_mcp/runtime/runtime_autoload.gd:691`),
so a space-indented line becomes tab+spaces; GDScript rejects the mixed
indentation, the parse error trips the editor break-on-error, the eval never
returns and the frame loop pins. Reproduced twice (frames frozen at 824/16168
across 2.3 s gaps while `Time.get_ticks_msec()` advanced; call times out at
15 s). `tools/gmcp.py` now normalises each body line's leading whitespace to
tabs (4-space unit) before sending; the valid space-indented body returns
`2.0` with the loop live afterwards. A genuinely malformed body stays invalid,
so it is caught by the watchdog instead of hanging silently.

**PAR-0347 (`$I.configure_building` on a door times out) — root cause.**
`$I` in an eval body is GDScript node shorthand `get_node("I")` on the
throwaway eval node, **not** `/root/Spine/Input` (probe:
`get_node_or_null("I")` = null; `/root/Spine/Input` = `MindInput`). The door is
incidental — the same door configured through the full node path
(`spine_place_break` + `place_block(6,4,"door")`, then
`configure_building(6,4,"bool","true")`) returns `true` and keeps the loop live
(frames 1190 → 1361), while the `$I` form times out with frames frozen (4627).
The watchdog in `tools/gmcp.py` closes the hazard class: on an eval
timeout/failure it issues `godot_game stop` + `play`, polls
`runtime_connected` and exits 1 with a `watchdog` JSON summary (verified: new
pid, frames 62 → 160, no manual `session-play.sh --play`).

## Exact test commands and results

```bash
bash -n tools/twin.sh                                  # OK
python3 -m py_compile tools/gmcp.py tools/twin_pointer.py   # OK
```

In-engine (session 5, bridge 7010; full transcript in the run notes):

```
GE 'var a = 1\nif a > 0:\n    a = 2\nreturn a'
  before: {"ok": false, "... timed out after 15.0s"}; frames pinned 824
  after:  [gmcp] normalised eval indentation (PAR-0345); {"ok": true, "result": 2.0}; frames live
GE '$I.configure_building(6, 4, "bool", "true")'
  before: timeout after 15.0s; frames frozen 4627
  after:  MCP timeout after 10s -> watchdog stop/play -> runtime_connected true (16.6 s)
GE 'return get_node("/root/Spine/Input").configure_building(6, 4, "bool", "true")'
  -> {"ok": true, "result": true}; frames live (door config is fine via the full path)
```

No Rust or GDScript was touched, so the cargo gates (fmt/clippy/tests) and the
Godot import gate were not run for this workstream; the touched surface is
`tools/` (bash/python), exercised in-engine above.

## In-engine evidence

- `runs/20261016-s5-J10-T1/` — `java/menu-java-00.png`, `menu-java-01-play.png`,
  `menu-00.png`, `godot/menu-00.png`, `diff/menu-00{,-side,-heatmap}.png`,
  `diff/menu-00-zoom.png`, `java-console.log`, `java-geometry.txt`,
  `saves-before.txt`, `settings.bin.bak`, `notes.md`.
- `runs/20261016-s5-J10-T2/` — `godot/menu-after-gmcp-change.png`, `notes.md`
  (repro + before/after transcripts).

Known smoke caveats recorded in the notes: the 1280x720 Xvfb cannot fit the
Java window (1152x648) and the Godot game window (1152x648) clear of each
other — placement warns and the raise protocol keeps actions landing; the Java
menu background is randomised per boot (my boot frame vs the round-J
`00-menu.png`: 83% different, while within-run menu frames differ 0.3%), so a
twin menu row must apply a fixed JVM seed or compare structurally.

## Deviations

- No gameplay behaviour changed; no Java file:line deviations. The two hazard
  fixes touch harness plumbing only (addon wrapper location cited above; the
  `$I` shorthand is plain GDScript semantics).
- T2 item 1 (bloom/linear settings) was left undone — see status table.
- `twin_pointer.py` runs on the parity-eval venv python when the system
  `python3` lacks pynput (this host), rather than unconditionally `python3`.

## Blockers

None for T1/T2-6. Residual verification gaps: `JD`/`GW` not exercised in this
smoke; the T2 item 1 stretch and T3 are untouched.

## Ledger changes to apply (coordinator; `chains.json`/`parity-ledger.json` untouched)

- **PAR-0345** (`MCP evals with mixed indentation hang the frame loop`):
  `seeded` → `verified`, `u: 2026-10-16`, `run:
  runs/20261016-s5-J10-T2/`, fix `78f42bf5` — root cause is the addon eval
  wrapper's tab injection; `tools/gmcp.py` now normalises body indentation to
  tabs, and the eval watchdog recovers any body that still errors (stop/play).
- **PAR-0347** (`$I.configure_building on a door can time out and stall the
  frame loop`): `seeded` → `verified`, `u: 2026-10-16`, `run:
  runs/20261016-s5-J10-T2/`, fix `78f42bf5` — root cause is `$I` being
  eval-node-relative (`get_node("I")`), not `/root/Spine/Input`; door config
  itself is fine via the full node path; the watchdog now recovers the pin.
- **J-INDEX §4 row** `PAR-0345/0347 | plan: harness hazards | J10 T2` →
  update to `landed in J10 T2 (78f42bf5); both verified in
  runs/20261016-s5-J10-T2/`.
- No other J-INDEX §4 pairs concern J10; T1 has no ledger item (tools-only).

## Done criteria (plan)

`tools/twin.sh` is committed and documented (skill §3c, AGENTS.md row) —
**yes**. The T2 items this workstream owned (item 6) are landed with root
causes and in-engine evidence — **yes**. Round-K re-walk (T3) and the summary
in `docs/K-round-twin-verification.md` — **not started (not this workstream)**.
## Wave B (2026-10-17) — T2 items 1–5

Branch base `3bd1abe8` (wave-A merged). Session 5, display `:14`, bridge 7010.

| Item | Status | Commit |
|---|---|---|
| T2.1 — inert graphics settings (`bloomintensity`/`bloomblur`/`linear`) | **done** (bloom pass implemented + both rows wired; in-engine verified) | `5325720c` |
| T2.2 — rapid skip-wave double click | **done / recorded**: both clients advance ONE wave (Godot measured; Java source chain) — no port debounce bug | (record only) |
| T2.3 — settings page wheel scroll (real pointer) | **done**: reproduced, root-caused (PAR-0100 regression), fixed in `MindInput` | `3afd8b70` |
| T2.4 — hover info over UI (real pointer) | **done / recorded**: MATCH (box shows over the world, hides over STOP UI) | (record only) |
| T2.5 — round-I FX/audio follow-ups | **record**: PAR-0346 kept as is; PAR-0348 + LegsComp emitter delegated to session 4 (J03 T7/T4) | (record only) |

### T2.1 details

Java sources: `Renderer.java:391-394` (bloom gate + intensity/blur),
`SettingsMenuDialog.java:531` (`bloom` checkbox), `SettingsMenuDialog.java:540-550`
(`linear` re-filters every atlas page), `ARC .../g2d/Bloom.java:128-168`
(threshold/blur/composite maths). Port: new `client/shaders/bloom.gdshader`,
`mind-core/src/render/bloom.rs::blur_radius`, `World/BloomGroup` in
`mind-gdext/src/render.rs` (bullet..effect band nodes baked through a
`CanvasGroup` with a camera-tracked `CaptureRect` defining the bake bounds;
`bloom`/`bloomintensity`/`bloomblur`/`linear` polled on the 30-frame settings
cadence).

Deviations: single-pass sqrt-scaled blur instead of Java's repeated quarter-res
ping-pong passes; `/root/Spine/MindFx` (FX host, sibling of the world renderer)
stays outside the capture; the pack-time bleed half of `linear`
(`Drawf.java:59`, `Mods.java:204`) remains a `mind-tools pack` concern.

Evidence: `runs/20261017-s5-J10-T2-1/` — uniforms follow the settings
(2.5→5.0, 1.4142→2.8284, gate on/off), bullet frames change 1.5–2.2 % of pixels
across bloom off/default/max, `linear` off/on changes 0.53 % at a fractional
camera offset.

### T2.2 details

`runs/20261017-s5-J10-T2-2/`: sandbox wave 1 + one 24-step sequence (two clicks
~300 ms apart) → wave 2. Gate = `hud_fragment.gd:_refresh_skip_button`
(`not spawning and enemies <= 0`), window ≈121 ticks
(`wave_spawner.rs::SPAWN_WINDOW_TICKS`). Java's button is disabled for the same
121 ticks (`HudFragment.java:509,1234-1236`; `WaveSpawner.java:62,108`) → also
one wave. Java live double click not run (deviation, budget); the source chain
is deterministic for that path.

### T2.3 details

Reproduced with the real pointer + real wheel (and injected): the settings
Graphics page never scrolled; a `Body.gui_input` probe showed the wheel never
reached the GUI. Root cause: `MindInput::input`'s Scroll branch marked the
wheel handled when `ui_scroll_at()` (PAR-0100's `!scene.hasScroll()` gate)
matched, so `DesktopInput.java:476-481`'s "skip the zoom" became "consume the
event". Fix: skip the zoom without consuming; the GUI now gets the wheel.

### T2.4 details

`runs/20261017-s5-J10-T2-4/`: real pointer over the core → hover box visible
(2 rows) with the HUD fragment hovered (non-STOP); over a palette button
(STOP) → hidden. Matches `Core.scene.hasMouse()`.

### T2.5 details

No code. See `runs/20261017-s5-J10-T2-5/`.

## Wave B — tests, gates and ledger changes

Per-commit gates (all clean): `cargo fmt --all -- --check`; `cargo clippy -p
mind-core -p mind-gdext --all-targets -- -D warnings`; `cargo test -p mind-core
--lib render::` (195 passed) and `-- shaders` (3 passed);
`bash tools/godot.sh --headless --path client --script res://tools/shaders_check.gd`
(29 shaders, 0 failed, includes `bloom.gdshader`); `bash tools/godot.sh
--headless --editor --quit --path client` after the `mind_widgets.gd` change.
No goldens moved.

### Ledger changes to apply (coordinator; chains.json / parity-ledger.json untouched)

- **PAR-0100** (`wheel-over-UI zooms the camera ...`): annotate the verified
  entry — the T8 guard consumed the wheel in `_input`, which regressed every UI
  scroll (settings Graphics page, map lists). Fixed in `3afd8b70`; the wheel is
  suppressed for the camera and delivered to the GUI. Run:
  `runs/20261017-s5-J10-T2-3/`.
- **New (J10 T2.1)**: I05 audit rows `bloomintensity`/`bloomblur`/`linear`
  ("still inert") are now wired and verified — commit `5325720c`, run
  `runs/20261017-s5-J10-T2-1/`, with the recorded deviations (single-pass
  sqrt-scaled blur; `/root/Spine/MindFx` outside the capture; pack-time linear
  bleed not ported).
- **New (J10 T2.3)**: settings/map-list wheel scroll — root cause PAR-0100
  regression + Godot sliders `scrollable`; fixed `3afd8b70`. Can fold into the
  PAR-0100 annotation above.
- **T2.2 (skip-wave double click)**: record that both clients advance one wave
  on a ~300 ms double click (Godot measured; Java `HudFragment.java:509,1234-6`
  + `WaveSpawner.java:62,108` disabled-button window). No port item needed.
- **T2.4 (hover info over UI)**: record MATCH — `placement_fragment.gd` gates on
  STOP-hovered UI exactly like `Core.scene.hasMouse()`. Run:
  `runs/20261017-s5-J10-T2-4/`.
- **PAR-0346** (power-node shimmer): keep as is → propose `wontfix` (the round-I
  claim was retracted in `docs/J-round-twin-verification.md` §2 item 8).
- **PAR-0348** (`FxEmit::square` 45-degree convention) and the **LegsComp
  leg-base emitter**: delegated to session 4 (J03 T7 / J03 T4); no J10 code
  touched those files this wave.
- **J-INDEX §4**: no new J10 row; the existing `PAR-0345/0347` row stays as the
  wave-A entry above.

### Wave-B deviations

- T2.2's live Java double click was not run (budget): the Java result is
  determined by the button gate chain above; recorded in
  `runs/20261017-s5-J10-T2-2/notes.md`.
- Bloom blur is a single wider pass rather than Java's repeated quarter-res
  passes; the FX host is outside the capture (both recorded in the commit and
  the T2.1 run notes).

### Done criteria (wave B)

T2 items 1–5 are landed or recorded with in-engine evidence. T3 (round-K
re-walk) remains **not started**.
