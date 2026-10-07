# open-godot-mcp learnings

Operational friction hit while driving the Godot client through open-godot-mcp
and the `playtest` skill, with the workaround that got past it. Reusable
sequences belong in `.opencode/chains/`; this file is for everything that is
not a chain: environment quirks, tool errors, timing traps.

The `parity-evaluator` and `twin-evaluator` append a dated entry after a run
that taught them something. Never rewrite another session's entry; keep it to
what happened, what worked, and the run directory or chain that shows it. If
nothing new happened, no entry.

## 2026-10-07 — Seed notes from the playtest and parity-eval skills

- `godot_game play` without an explicit `params.scene` reports success but
  attaches nothing; every later `godot_exec` fails with
  `RUNTIME_NOT_CONNECTED`. Always pass `res://scenes/game.tscn`.
- Injected input sits in the accumulated-input buffer: after a `godot_input`
  press/release, flush with `Input.flush_buffered_events()` and assert the
  target state in the same eval.
- `godot_screenshot game` can return a stale presented frame when the editor
  covers the game window. Call `get_window().move_to_foreground()` first, or
  capture the X screen and crop.
- Eval bodies with `for`/`while` time out; a `null` result with `ok: true`
  usually means the body errored — check `godot_log errors` before retrying.
- An eval that errors stops in the editor Debugger (break-on-error) and freezes
  the frame loop; clear `godot_log` before a scenario and recover with
  `godot_game stop` + `godot_game play`.
