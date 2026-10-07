---
id: campaign-rules-dialog
title: Open the Campaign Difficulty dialog (UI rail button and MindUi API), verify body + liveness, close
status: verified
applies_when: Verifying EV-0041-family campaign difficulty/rules dialog behavior or any campaign_rules open/close check.
preconditions:
  - boot-and-identity completed (game in the main menu, runtime connected, pid stamped).
  - Godot extension build includes 5b5fa95 (MindCell align API); lib sha 84069d3451ebeb11 at HEAD 9efd068.
  - Resolve every click from a live eval; positions below are viewport coords from the 1152x648 editor-run window.
tools: [godot_exec, godot_input, godot_screenshot, godot_log, godot_debugger]
last_verified: 2026-10-08 9efd068 (runs/l2-20261007-160101-ev0041-campaign-rules-godot)
---

# campaign-rules-dialog

Two independent entry paths: the real UI rail button, and the MindUi API call
(the finding's exact repro). Run the UI path first, then a fresh boot for the API
cross-check, so a tool-induced freeze on the first cannot contaminate the second.

## Steps (UI variant)

1. Boot the spine and pid-stamp (see `boot-and-identity`).

2. Open the Play submenu. Resolve the Play button, click it, then resolve the
   Campaign button by label from the submenu (node names are auto-generated and
   shift between instances, so filter on the text label):

   ```
   godot_exec {"action":"eval","params":{"code":"var side = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu/Sidebar\")\nvar btns = side.find_children(\"*\", \"Button\", true, false)\nvar hit = btns.filter(func(b): return b.find_children(\"*\", \"Label\", true, false).any(func(l): return l.text == \"Campaign\"))\nvar p = (hit[0] as Control).get_global_rect().get_center()\nreturn {\"x\": int(p.x), \"y\": int(p.y), \"visible\": true}"}}
   ```

   Click via one sequence call (press+release, `coords: "viewport"`), then repeat
   for Campaign in `MenuGroup/menu/Submenu`. The Play button worked as
   `MenuGroup/menu/Sidebar/Buttons/@Button@254`, center (230,149); Campaign
   resolves to (460,149) once the submenu is fully faded (`Submenu.visible` true,
   `modulate.a == 1.0`).

3. On the "Select Starting Campaign" chooser, pick the Serpulo card and click OK:

   ```
   godot_exec {"action":"eval","params":{"code":"var p = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/planet\")\nvar cards = p.find_children(\"*\", \"Button\", true, false).filter(func(b): return b.is_visible_in_tree() and b.toggle_mode and b.custom_minimum_size.y > 100)\nvar p0 = (cards[0] as Control).get_global_rect().get_center()\nreturn {\"x\": int(p0.x), \"y\": int(p0.y), \"cards\": cards.map(func(b): return [str(b.name), b.button_pressed])}"}}
   ```

   Cards are ordered Serpulo, Erekir; Serpulo center was (412,194), OK (499,634).
   After the OK click the planet page (`PlanetDialog`) is up.

4. Click the `@campaign.difficulty` rail button. It is a code-instantiated
   `icon_button("book", <@campaign.difficulty>)` in the left rail; resolve it by
   its label:

   ```
   godot_exec {"action":"eval","params":{"code":"var p = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/planet\")\nvar b = p.find_children(\"*\", \"Button\", true, false).filter(func(x): return x.find_children(\"*\", \"Label\", true, false).any(func(l): return l.text == \"Difficulty\"))\nvar c = (b[0] as Control).get_global_rect().get_center()\nreturn {\"x\": int(c.x), \"y\": int(c.y)}"}}
   ```

   It sits at (175,136) when the rail has Erekir/Serpulo buttons above it.

5. Assert the dialog opened and has a body (no error log entry, no freeze):

   ```
   godot_exec {"action":"eval","params":{"code":"var ui = get_node(\"/root/MindUi\")\nvar d = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/campaign_rules\")\nvar diff = d.find_children(\"*\", \"Button\", true, false).filter(func(b): return b.toggle_mode and b.text != \"\")\nvar checks = d.find_children(\"*\", \"MindCheck\", true, false)\nreturn {\"pid\": OS.get_process_id(), \"frames\": Engine.get_process_frames(), \"dialog_visible\": d.visible, \"shown\": ui.call(\"is_dialog_shown\", \"campaign_rules\"), \"has_dialog\": ui.call(\"has_dialog\"), \"diff_count\": diff.size(), \"check_count\": checks.size()}"}}
   ```

   Expect `diff_count: 5`, `check_count: 7`, `shown/has_dialog: true`, frames
   advancing.

6. Liveness A-B (frames must advance; `host_paused` true is normal in the menu
   flow and does not mean a hang): read `Engine.get_process_frames()` twice with a
   sleep in between; then screenshot (`get_window().grab_focus()` before
   `godot_screenshot game`) and close with
   `ui.call("close_dialog", "campaign_rules")` → expect `true`.

## Steps (API cross-check, fresh boot)

1. Fresh `godot_game play`, pid-stamp, then the finding's exact repro:

   ```
   godot_exec {"action":"eval","params":{"code":"var ui = get_node(\"/root/MindUi\")\nvar t0 = Time.get_ticks_msec()\nvar opened = ui.call(\"open_dialog\", \"campaign_rules\", JSON.stringify({\"planet\": \"serpulo\"}))\nreturn {\"pid\": OS.get_process_id(), \"opened\": opened, \"elapsed_ms\": Time.get_ticks_msec() - t0, \"frames\": Engine.get_process_frames()}"}}
   ```

   Fixed build: returns in single-digit ms with `opened: true`. Pre-fix: the eval
   never returns (15 s MCP timeout) and the log has
   `Nonexistent function 'left' in base 'RefCounted (MindCell)'`.

2. Repeat step 5 assertions + step 6 liveness/close from the UI variant.

## MindUi signatures (Rust, `client/rust/mind-gdext/src/ui/ui_host.rs`)

- `open_dialog(name: String, ctx_json: String) -> bool`
- `close_dialog(name: String) -> bool`
- `is_dialog_shown(name: String) -> bool` — takes the dialog name; calling it
  with no args errors
- `has_dialog() -> bool`

## Success signals

- `open_dialog` returns true fast; dialog visible with 5 difficulty buttons
  (Normal pressed) + 7 MindCheck rule toggles + Back; error log has no MindCell
  entry and no re-bind panic; frames advance across the whole sequence; close
  returns true.

## Failure modes

- **An errored `godot_exec` eval pauses the game at the editor debugger**
  (break-on-error). Frames and tick freeze, later `godot_input sequence` calls
  time out, and the screen shows a frozen half-built frame — indistinguishable
  from the EV-0041 wedge unless you check
  `godot_debugger {"action":"sessions"}` (`paused: true`) and
  `godot_debugger {"action":"stack_trace"}` (frame in `gdscript://<id>.gd`).
  `godot_debugger resume` can re-break immediately; `godot_game stop` + `play`
  is the reliable recovery. Never put `await` or multi-line lambdas (`var x = …`
  with newlines inside a `func`) in an eval body.
- The dialog is a full-screen opaque panel; the planet page is not visible behind
  it. Use the DialogLayer node (`campaign_rules.visible`) for state, not pixels.
- `move_to_foreground()` logs a deprecation error on 4.7.2; use
  `get_window().grab_focus()` before screenshots so the error log stays clean.
- `MindUi` methods run on the Rust side; after any Rust panic the whole autoload
  is unusable (`Gd<T>::bind() failed`). A passing run must show later
  `has_dialog`/`close_dialog` calls answering normally.
