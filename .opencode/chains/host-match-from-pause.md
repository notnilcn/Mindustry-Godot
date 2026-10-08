---
id: host-match-from-pause
title: Host a match from the pause menu (host dialog port field -> host_requested -> MindNet.create_match)
status: verified
applies_when: Verifying the pause @hostserver entry, HostDialog port field/runHost, or that hosting actually creates a match (EV-0059 class).
preconditions:
  - boot-and-identity completed (editor bridge, res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - Local SpacetimeDB server running (`spacetime start`; 2.10.1) and the module published (`server/build.sh`, db `mindustry`).
  - The host dialog opens from the pause menu; the pause @hostserver entry is only enabled while `MindNet.session_state() == "offline"` (upstream `disabled(b -> net.active())`).
  - The game boots with `MindNet.offline = true` (single-player invariant). Arrange the online connector before pressing Host: call `MindNet.connect_to("127.0.0.1")` (the Join Connect path) or launch with `--db`.
tools: [godot_log, godot_exec, godot_input, godot_screenshot, godot_game, bash]
last_verified: 2026-10-08 afd380f (runs/20261008-083744-ev0059-host-godot); 2026-10-08 1d8f75b twin (runs/20261008-105728-ev0059-host-twin)
---

# host-match-from-pause

## Steps

1. Clear the log after the game attaches, then pid-stamp and confirm the frame
   loop is live (the tick grows while Playing):

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar net = get_node(\"/root/Spine/MindNet\")\nreturn {\"pid\": OS.get_process_id(), \"frames\": Engine.get_process_frames(), \"tick\": get_node(\"/root/Spine/SimHost\").get_tick(), \"net_state\": str(net.call(\"session_state\")), \"menu_visible\": ui_root.get_node(\"MenuGroup\").visible, \"window\": {\"x\": DisplayServer.window_get_size().x, \"y\": DisplayServer.window_get_size().y}}"}}
   ```

2. Leave the standalone menu and open the pause dialog; Escape is ignored while
   `MenuGroup.visible` (UiRoot `_on_escape` returns false):

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/Ui/UiRoot","method":"set_menu_visible","args":[false]}}
   godot_input {"action":"key","params":{"key":"Escape","pressed":true}}
   godot_input {"action":"key","params":{"key":"Escape","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar paused = ui_root.get_node(\"DialogLayer/paused\")\nvar host = paused.find_child(\"hostserver\", true, false)\nvar c = (host as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"stack\": str(ui.call(\"dialog_stack\")), \"paused_visible\": paused.visible, \"host_found\": host != null, \"host_disabled\": (host == null or bool(host.disabled)), \"host_center\": {\"x\": int(c.x), \"y\": int(c.y)}, \"net_state\": str(get_node(\"/root/Spine/MindNet\").call(\"session_state\"))}"}}
   ```

   Resolve the center; do not hardcode it (1152x648 put it at 697,503). The
   button is runtime-created (`owner == null`) — the `ui-control-click` chain
   covers the click path.

3. Click the `hostserver` entry (one discrete call per event) and read the
   dialog back. The HostDialog exposes `_name_field`, `_port_field` and
   `_host_button` as script properties:

   ```
   godot_input {"action":"mouse_motion","params":{"position":{"x":697,"y":503},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":697,"y":503},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":697,"y":503},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar host = ui_root.get_node(\"DialogLayer/host\")\nvar host_btn = host.get(\"_host_button\")\nvar port_field = host.get(\"_port_field\")\nif not host.has_meta(\"host_emit_count\"):\n\thost.set_meta(\"host_emit_count\", 0)\n\thost.host_requested.connect(func(_n: String, _m: String) -> void: host.set_meta(\"host_emit_count\", int(host.get_meta(\"host_emit_count\")) + 1))\nvar c = (host_btn as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"stack\": str(ui.call(\"dialog_stack\")), \"host_visible\": host.visible, \"port_text\": str(port_field.text), \"port_valid\": bool(host.call(\"_port_valid\")), \"host_button_disabled\": bool(host_btn.disabled), \"host_button_center\": {\"x\": int(c.x), \"y\": int(c.y)}, \"signal_connected\": host.is_connected(\"host_requested\", Callable(ui_root, \"_on_host_requested\"))}"}}
   ```

   Require `stack == ["paused","host"]`, `port_text == "6567"`,
   `port_valid == true`, `signal_connected == true`.

4. Arrange the online connector (see preconditions) and wait for the
   handshake; a row appears in `all_players`:

   ```
   godot_exec {"action":"eval","params":{"code":"var host = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/host\")\nvar name_field = host.get(\"_name_field\")\nname_field.text = \"tester\"\nvar net = get_node(\"/root/Spine/MindNet\")\nvar ok = bool(net.call(\"connect_to\", \"127.0.0.1\"))\nreturn {\"pid\": OS.get_process_id(), \"name_text\": str(name_field.text), \"connect_to\": ok, \"net_state_after_connect\": str(net.call(\"session_state\"))}"}}
   # then, from the shell, confirm the client identity:
   # spacetime sql mindustry --server local "SELECT identity, username FROM all_players"
   ```

5. Click the dialog Host button at its resolved center, then read the outcome
   back. `HostDialog.runHost` closes the dialog and the UiRoot listener calls
   `MindNet.create_match(map_id, 1, mode, "public", 8, rules)`; the session
   moves to `in_lobby`:

   ```
   godot_input {"action":"mouse_motion","params":{"position":{"x":468,"y":503},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":468,"y":503},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":468,"y":503},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar host = ui_root.get_node(\"DialogLayer/host\")\nvar net = get_node(\"/root/Spine/MindNet\")\nreturn {\"pid\": OS.get_process_id(), \"frames\": Engine.get_process_frames(), \"stack\": str(ui.call(\"dialog_stack\")), \"host_visible\": host.visible, \"emit_count\": int(host.get_meta(\"host_emit_count\")), \"net_state\": str(net.call(\"session_state\")), \"public_matches\": str(net.call(\"get_public_matches_json\")).substr(0, 200)}"}}
   ```

6. Confirm the match server-side (structured oracle before pixels):

   ```
   spacetime sql mindustry --server local "SELECT match_id, map_id, map_seed, mode_name, visibility, created_by, host, status, player_count, max_players FROM relay_match"
   ```

7. Teardown: `godot_game {"action":"stop"}`. The pause host entry is disabled
   while `net.active()`; `leave_match()` only works once a `my_match` row has
   set the session match id.

## Success signals

- `stack ["paused","host"]` with `port_text "6567"` and a connected listener.
- After the Host click: `emit_count 1`, `host_visible false`, `net_state
  "in_lobby"`; `relay_match` holds the row with the client identity as
  `created_by == host`; the client sees it in `get_public_matches_json()`;
  `godot_log errors` is empty.
- The `match_id` is new on every run (`1` in the writer leg, `4097` in the twin
  leg); assert the row by map/mode/created_by and its presence in the public
  list, not a fixed id.

## Failure modes

- `port_text` empty with placeholder `"6567"` (pre-afd380f): `MindWidgets.field()`
  sets `placeholder_text`, so a field built as `MindWidgets.field(str(port))`
  never satisfies `_port_valid()`; `_host()` returns before emitting and the
  Host click looks dead. The field text must be set.
- Clicking Host with an empty name is upstream-correct: `@noname` info toast,
  no emit.
- With a default offline `MindNet` (no `--db`), `create_match` logs
  `connector is offline` and the session stays offline even though the click
  emitted — arrange online first.
- The dialog only opens while `net.active()` is false; with a match already
  active the pause entry is disabled.
- A parse-error eval body freezes the game loop (frames constant); check
  `Engine.get_process_frames()` twice and `godot_debugger sessions` before
  trusting clicks, then `godot_game stop` + `play`.

## Variants

- `--db` launch: the connector is online from boot; skip step 4's `connect_to`
  and the pause entry stays enabled until hosting starts.
- Campaign sector: `_current_map_id` becomes `planet-sector` and the rules come
  from `MindCampaign.get_rules_json()`; the default fallback is map `spine`.
