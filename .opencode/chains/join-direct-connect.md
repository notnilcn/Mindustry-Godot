---
id: join-direct-connect
title: Main menu Join Game -> Add Server address -> OK -> MindNet.connect_to browsing (EV-0060)
status: verified
applies_when: Verifying the Join Game direct-connect path (dialog connect_requested -> UiRoot -> MindNet.connect_to + public match browser), or arranging a live online connector from the menu before hosting/joining.
preconditions:
  - boot-and-identity completed (editor bridge, res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - Local SpacetimeDB server running (`spacetime start`; 2.10.1) and the module published (`server/build.sh`, db `mindustry`). Without it the handshake fails and net stays offline.
  - The client boots MindNet.offline=true by design (single-player invariant); the dialog connect is what switches the connector online (`connect_to` accepts only Offline|Browsing).
tools: [godot_log, godot_exec, godot_input, godot_screenshot, godot_game, bash]
last_verified: 2026-10-08 afd380f (runs/20261008-184534-ev0060-join-godot); 2026-10-08 1d8f75b (runs/20261008-112749-ev0060-join-twin)
---

# join-direct-connect

## Steps

1. Clear the log while Playing, then pid-stamp and prove the loop is live
   (read frames twice; `godot_game status` fps is not a liveness probe):

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var net = get_node(\"/root/Spine/MindNet\")\nreturn {\"pid\": OS.get_process_id(), \"frames\": Engine.get_process_frames(), \"net_state\": str(net.call(\"session_state\")), \"menu_visible\": bool(get_node(\"/root/Spine/Ui/UiRoot/MenuGroup\").visible)}"}}
   godot_exec {"action":"eval","params":{"code":"return {\"frames_again\": Engine.get_process_frames(), \"pid\": OS.get_process_id()}"}}
   ```

   Require `menu_visible true`, `net_state "offline"`, and a growing frame
   count.

2. Click the top-level Play button, then Join Game in the second column.
   Resolve both centers, never hardcode (1152x648 put them at 230,149 and
   460,219):

   ```
   godot_exec {"action":"eval","params":{"code":"var play = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu/Sidebar/Buttons\").get_child(0)\nvar c = (play as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"play_center\": {\"x\": int(c.x), \"y\": int(c.y)}}"}}
   # mouse_motion -> mouse_button pressed:true -> mouse_button pressed:false at play_center
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar submenu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu/Submenu\")\nvar join = submenu.get_node(\"Buttons\").get_child(2)\nvar c = (join as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"submenu_visible\": bool(submenu.visible), \"join_text\": str(join.get_child(0).get_child(1).text), \"join_center\": {\"x\": int(c.x), \"y\": int(c.y)}}"}}
   # click at join_center
   ```

   Submenu child order: 0 spacer, 1 Campaign, 2 Join Game, 3 Custom Game,
   4 Load Game.

3. Assert the dialog opened and both intents are wired (this is the EV-0060
   seam; `connect_requested` must reach `UiRoot._on_connect_requested`):

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar join = ui_root.get_node(\"DialogLayer/join\")\nvar net = get_node(\"/root/Spine/MindNet\")\nreturn {\"pid\": OS.get_process_id(), \"stack\": str(get_node(\"/root/MindUi\").call(\"dialog_stack\")), \"join_visible\": bool(join.visible), \"connect_wired\": bool(join.is_connected(\"connect_requested\", Callable(ui_root, \"_on_connect_requested\"))), \"join_wired\": bool(join.is_connected(\"join_requested\", Callable(ui_root, \"_on_join_requested\"))), \"net_state\": str(net.call(\"session_state\"))}"}}
   ```

   Require `stack ["join"]`, both `*_wired` true, `net_state "offline"`.

4. Click `Add Server` (`join.get("buttons").get_child(1)`; child 0 is Back,
   child 2 is the `?` info button) and assert the address form:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar join = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/join\")\nvar ip = join.get(\"_ip_field\")\nreturn {\"pid\": OS.get_process_id(), \"panel_visible\": bool(join.get(\"_add_panel\").visible), \"ip_focus\": bool(ip.has_focus()), \"ip_text\": str(ip.text)}"}}
   ```

   Require `panel_visible true`, `ip_focus true`.

5. Type the address into the focused field. `godot_input action=text` emits one
   event object parsed twice in the same frame (Godot error) and its unicode-only
   event does not land; `godot_input action=key` sets no unicode. Use the
   documented eval fallback — one fresh down+up `InputEventKey` per char with
   `keycode` + `physical_keycode` + `unicode` through
   `get_viewport().push_input`:

   ```
   godot_exec {"action":"eval","params":{"code":"var join = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/join\")\nvar ip = join.get(\"_ip_field\")\nvar vp = get_viewport()\nfor ch in \"127.0.0.1\":\n\tvar down := InputEventKey.new()\n\tdown.keycode = OS.find_keycode_from_string(ch)\n\tdown.physical_keycode = down.keycode\n\tdown.unicode = ch.unicode_at(0)\n\tdown.pressed = true\n\tvp.push_input(down)\n\tvar up := InputEventKey.new()\n\tup.keycode = down.keycode\n\tup.physical_keycode = down.keycode\n\tup.pressed = false\n\tvp.push_input(up)\nreturn {\"pid\": OS.get_process_id(), \"ip_text\": str(ip.text), \"ip_focus\": bool(ip.has_focus())}"}}
   ```

6. Click OK in the add panel (`join.get("_add_panel").get_child(1).get_child(0)`)
   and read the outcome back:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar net = get_node(\"/root/Spine/MindNet\")\nvar join = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/join\")\nreturn {\"pid\": OS.get_process_id(), \"net_state\": str(net.call(\"session_state\")), \"public_matches\": str(net.call(\"get_public_matches_json\")), \"panel_visible\": bool(join.get(\"_add_panel\").visible)}"}}
   ```

   `_submit_add` emits `connect_requested` and hides the panel; UiRoot calls
   `MindNet.connect_to(address)` with `host[:port][/db]` (defaults
   `http://<host>:3000/<db_name>`).

7. Structured/OCR-free oracle before pixels:

   ```
   godot_log {"action":"get","params":{"count":60}}
   godot_log {"action":"errors","params":{"max":20,"include_warnings":true}}
   ```

   Optional server-side confirmation:
   `spacetime sql mindustry --server local "SELECT match_id, map_id, status, player_count FROM relay_match"`.

## Success signals

- After OK: `net_state "browsing"` (left offline), panel hidden, dialog open.
- Log: `[D] Client handshake done.` and
  `[I] MindNet.connect_to: browsing \`http://127.0.0.1:3000/mindustry\`` plus
  `subscribed base wave` / `subscribed lobby wave` / `on_applied`.
- `get_public_matches_json()` returns the live `all_matches` rows when a public
  relay match exists (match_id, map_id, status Lobby, player_count).
- `godot_log errors` empty.

## Failure modes

- `godot_input text`/`key` silently fail on a `LineEdit` (see step 5) and the
  text action also emits a Godot error; use `push_input` with unicode.
- An eval lambda that touches a childless node (`b.get_child(0)` on the `?`
  button) errors and pauses the debugger; the frame counter then pins. Recover
  with `godot_game stop` + `play` (`godot_debugger resume` reports ok but does
  not unpause).
- No local server: handshake fails, `MindNet.connect_to failed: …` in the log,
  net stays offline.
- The browser list only rebuilds on `shown()`/refresh, so connecting with the
  dialog already open leaves the dialog rendering its empty state even though
  `get_public_matches_json()` has rows — judge on the JSON, not the frame.
- Both the `connect_requested` and `join_requested` listeners exist only after
  `UiRoot._connect_net()`; assert `is_connected` before blaming the dialog.
- After OK the dialog keeps rendering its pre-connect list: `_global_list` only
  rebuilds in `shown()`. Calling `_rebuild_public()` from an eval and reading
  `_global_list` children in the same body can error (`Index p_index = 0 is out
  of bounds`) and trips break-on-error; never `await` in an eval body (wedges the
  frame loop). The reliable visible-list check is: click `Back`, then
  Play > Join Game again — the reopened dialog rebuilt 2 rows (`spine survival
  1/8`) in the twin run (`step-07-public-list.png`).

## Variants

- A local-list row calls `_connect_to`, which emits the same
  `connect_requested` intent.
- Launch the game with `--db` (or `--pN`) to boot online and skip the dialog.

## Evidence

- `runs/20261008-184534-ev0060-join-godot/` (EV-0060 godot-pass; pid 50570;
  `godot/state-05-after-connect.json`, `godot/game.log`).
- `runs/20261008-112749-ev0060-join-twin/` (EV-0060 twin-verified; pids
  51583/54225/56882; `godot/state-connect-56882.json`,
  `godot/step-07-public-list.png`; errors `[]` in the final pid's window).
