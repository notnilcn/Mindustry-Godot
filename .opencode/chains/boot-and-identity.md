---
id: boot-and-identity
title: Editor bridge → game scene → runtime connected → pid stamp
status: seeded
applies_when: Start of any MCP session; after an editor, game, or branch/worktree change.
preconditions:
  - Godot editor running with the open_godot_mcp addon bridge up.
  - No other editor holds this loop's bridge port (PARITY_BRIDGE_PORT, default 6970).
  - open-godot-mcp needs no DISPLAY on the opencode side; the editor does.
tools: [godot_health, godot_editor_read, godot_editor_edit, godot_game, godot_exec]
last_verified: not yet (seeded from .opencode/skills/playtest/SKILL.md §Session start)
---

# boot-and-identity

## Steps

1. Bridge check:

   ```
   godot_health {"action":"check"}
   ```

   Require `bridge_connected: true`. If the bridge is down, launch the editor
   through the loop wrapper (`.opencode/loops/bin/run-godot-editor.sh`) inside a
   loop, or `nohup godot4 --editor --path client` for a plain session, wait
   ~20 s, then retry.

2. Identity check:

   ```
   godot_editor_read {"action":"state"}
   ```

   `project_path` must contain `mindustry-godot`; inside a loop it must be the
   loop's worktree, not a sibling checkout.

3. Open the entry scene:

   ```
   godot_editor_edit {"action":"open_scene","params":{"path":"res://scenes/game.tscn"}}
   ```

4. Play with the scene passed explicitly:

   ```
   godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
   ```

5. Confirm the runtime attached:

   ```
   godot_game {"action":"status"}
   ```

   Require `runtime_connected: true`.

6. Pid stamp plus cheap state:

   ```
   godot_exec {"action":"eval","params":{"code":"return {\"pid\": OS.get_process_id(), \"project\": ProjectSettings.globalize_path(\"res://\"), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick(), \"checksum\": str(get_node(\"/root/Spine/SimHost\").get_checksum())}"}}
   ```

   Save the returned pid. Re-stamp whenever the pid may have changed.

## Success signals

- `bridge_connected` and `runtime_connected` are both true.
- The pid is stable across calls; `project` contains the expected checkout.
- `tick` grows while playing and `checksum` is 16 hex chars.

## Failure modes

- `play` without `params.scene` looks successful but attaches nothing; every
  later eval fails with `RUNTIME_NOT_CONNECTED`.
- A wrong-port editor (sibling loop) is silently adopted; check the worktree
  path in step 2.
- Two runtimes (editor game plus a stale instance) make a misrouted eval look
  successful; compare pids against `godot_game {"action":"instances"}`.
